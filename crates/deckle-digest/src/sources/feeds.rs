// SPDX-License-Identifier: GPL-3.0-or-later
//! RSS / Atom feeds: the Local section (a Google News search plus local papers) and one
//! section per `[[feeds]]` entry. Feed HTML is reduced to text paragraphs (v0).

use std::collections::HashSet;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{LazyLock, Mutex};

use chrono::{DateTime, Local, Utc};
use regex::Regex;

use crate::config::{Config, FeedConfig, FullText};
use crate::fetch::{Net, encode};
use crate::publisher::{Access, Walled};
use crate::{Article, Section, article, esc};

/// Paragraphs kept per article.
const MAX_PARAGRAPHS: usize = 40;

#[derive(Clone, Debug, PartialEq)]
pub struct Item {
    pub title: String,
    pub source: Option<String>,
    pub published: Option<DateTime<Utc>>,
    pub paragraphs: Vec<String>,
    pub link: Option<String>,
}

/// A Google News search limited to the last `max_age_hours` (rounded up to days): without
/// `when:` the results are by relevance and mostly months old.
pub fn google_news_url(
    query: &str,
    max_age_hours: u32,
    hl: Option<&str>,
    gl: Option<&str>,
    ceid: Option<&str>,
) -> String {
    let query = if query.contains("when:") {
        query.to_owned()
    } else {
        format!("{query} when:{}d", max_age_hours.div_ceil(24).max(1))
    };
    format!(
        "https://news.google.com/rss/search?q={}&hl={}&gl={}&ceid={}",
        encode(&query),
        encode(hl.filter(|s| !s.is_empty()).unwrap_or("en-US")),
        encode(gl.filter(|s| !s.is_empty()).unwrap_or("US")),
        encode(ceid.filter(|s| !s.is_empty()).unwrap_or("US:en")).replace("%3A", ":")
    )
}

thread_local! {
    /// Set while [`quietly`] runs on this thread: a panic here is caught and reported as an
    /// unreadable feed, so the panic hook stays silent.
    static QUIET: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
}

/// `f`, with a panic caught and not printed; the error is the panic's message.
///
/// The app shows the last lines of a Daily run's stderr as its log, so a parser panic Deckle
/// catches must not print there. The global hook is replaced once, by one that stays silent on
/// a thread inside `quietly` and passes every other panic to the hook it replaced. Swapping the
/// hook around each call instead would race with feeds parsed on other threads at once. A hook
/// set later by anyone else replaces this one, and caught panics then print again.
///
/// So that a parser bug can still be found, the message travels in the feed's "unreadable"
/// error: the payload text alone, on one line, cut to 200 characters. The file and line, which
/// name the dependency's path on this machine, stay out. A payload that is not text says only
/// that the parser failed.
fn quietly<T>(f: impl FnOnce() -> T + std::panic::UnwindSafe) -> Result<T, String> {
    static HOOK: std::sync::Once = std::sync::Once::new();
    HOOK.call_once(|| {
        let previous = std::panic::take_hook();
        std::panic::set_hook(Box::new(move |info| {
            if !QUIET.with(std::cell::Cell::get) {
                previous(info);
            }
        }));
    });
    let was = QUIET.with(|q| q.replace(true));
    let result = std::panic::catch_unwind(f);
    QUIET.with(|q| q.set(was));
    result.map_err(|payload| {
        let message = payload
            .downcast_ref::<&str>()
            .copied()
            .or_else(|| payload.downcast_ref::<String>().map(String::as_str));
        let Some(message) = message else {
            return "the feed's parser failed".to_owned();
        };
        // One line, and at most 200 characters: a message that quotes the feed must not fill
        // the Daily's log.
        let mut text = message.split_whitespace().collect::<Vec<_>>().join(" ");
        if text.chars().count() > 200 {
            text = text.chars().take(199).chain(['…']).collect();
        }
        format!("the feed's parser failed: {text}")
    })
}

/// The error for a feed whose parser panicked, `parser` from [`quietly`].
fn unreadable(parser: &str) -> String {
    format!("this feed could not be read ({parser})")
}

/// Why `feed-rs` must not read this feed, or `None` (nesting-depth design, feeds).
///
/// `feed-rs` recurses once per element level before it rejects anything, so the depth is
/// counted first. A scan of the raw bytes cannot agree with it: quick-xml decodes the bytes
/// first (by byte-order mark, then by the declaration's `encoding`), and a byte of a Shift_JIS
/// or ISO-2022-JP character can look like markup. So the feed is read here by the same reader,
/// set up as `feed-rs` 3 sets it up (`feed_rs::xml::ElementSource::new`, `fetch_next`): a
/// quick-xml `NsReader` over its `DecodingReader`, switching encoding at the declaration. This
/// reader does not recurse. It stops at the reader's first error, where `feed-rs` stops too.
fn refusal(bytes: &[u8]) -> Option<String> {
    use quick_xml::events::{BytesStart, Event};
    use std::io::BufRead;

    let mut input = std::io::BufReader::new(bytes);
    // As `feed_rs::parser::Parser::parse`: JSON when `{` comes before `<` in the first buffer.
    let first = input
        .fill_buf()
        .ok()?
        .iter()
        .copied()
        .find(|b| matches!(b, b'<' | b'{'));
    if first != Some(b'<') {
        return None;
    }
    let read = quietly(move || {
        let mut reader =
            quick_xml::NsReader::from_reader(quick_xml::encoding::DecodingReader::new(input));
        let config = reader.config_mut();
        config.expand_empty_elements = true;
        config.trim_markup_names_in_closing_tags = true;
        config.trim_text(false);
        let mut buf = Vec::new();
        let (mut depth, mut declarations) = (0usize, 0);
        loop {
            buf.clear();
            match reader.read_resolved_event_into(&mut buf) {
                Ok((_, Event::Decl(decl))) => {
                    // A second declaration naming another encoding trips an assert in
                    // quick-xml; neither it nor a doubled `encoding` has a reading to trust.
                    declarations += 1;
                    if declarations > 1 {
                        return Some("it has more than one XML declaration".to_owned());
                    }
                    let start = BytesStart::from_content(&*decl, 3);
                    let mut attributes = start.attributes();
                    attributes.with_checks(false);
                    let encodings = attributes
                        .map_while(Result::ok)
                        .filter(|a| a.key.as_ref() == "encoding")
                        .count();
                    if encodings > 1 {
                        return Some("its XML declaration gives the encoding twice".to_owned());
                    }
                    if let Some(encoding) = decl.encoder() {
                        // quick-xml asserts here when the declaration ends past the bytes it
                        // decoded first, or a second one names another encoding.
                        let switch = std::panic::AssertUnwindSafe(|| {
                            reader.get_mut().set_encoding(encoding)
                        });
                        if let Err(parser) = quietly(switch) {
                            return Some(format!(
                                "its XML declaration names an encoding it cannot switch to \
                                 ({parser})"
                            ));
                        }
                    }
                }
                Ok((_, Event::Start(_))) => {
                    depth += 1;
                    if depth > deckle_limits::MAX_DEPTH {
                        return Some(format!(
                            "it is nested more than {} levels deep",
                            deckle_limits::MAX_DEPTH
                        ));
                    }
                }
                Ok((_, Event::End(_))) => depth = depth.saturating_sub(1),
                Ok((_, Event::Eof)) | Err(_) => return None,
                Ok(_) => {}
            }
        }
    });
    // Any other panic is quick-xml's, in a call `feed-rs` makes the same way.
    match read {
        Ok(why) => why.map(|why| format!("this feed could not be read: {why}")),
        Err(parser) => Some(unreadable(&parser)),
    }
}

/// Feed title and its items, newest first, without items older than `max_age_hours` (items
/// without a date are kept).
pub fn parse(
    bytes: &[u8],
    now: DateTime<Utc>,
    max_age_hours: u32,
) -> Result<(Option<String>, Vec<Item>), String> {
    if let Some(message) = refusal(bytes) {
        return Err(message);
    }
    // A panic in the parser (quick-xml asserts on some declarations) costs this feed, not the
    // edition.
    let feed = quietly(|| feed_rs::parser::parse(bytes))
        .map_err(|parser| unreadable(&parser))?
        .map_err(|e| e.to_string())?;
    let cutoff = now - chrono::Duration::hours(i64::from(max_age_hours));
    let mut items: Vec<Item> = feed
        .entries
        .into_iter()
        .filter_map(|e| {
            let title = clean_line(&e.title.map(|t| t.content).unwrap_or_default());
            if title.is_empty() {
                return None;
            }
            let published = e.published.or(e.updated);
            if published.is_some_and(|p| p < cutoff) {
                return None;
            }
            let html = e
                .content
                .and_then(|c| c.body)
                .or(e.summary.map(|s| s.content))
                .unwrap_or_default();
            Some(Item {
                title,
                source: None,
                published,
                paragraphs: html_paragraphs(&html),
                link: e.links.first().map(|l| l.href.clone()),
            })
        })
        .collect();
    items.sort_by_key(|i| std::cmp::Reverse(i.published));
    Ok((feed.title.map(|t| clean_line(&t.content)), items))
}

/// Google News items: "Headline - Source", and a description that only repeats the link.
pub fn as_briefs(items: Vec<Item>) -> Vec<Item> {
    items
        .into_iter()
        .map(|mut i| {
            if let Some((head, source)) = i.title.rsplit_once(" - ") {
                i.source = Some(source.trim().to_owned());
                i.title = head.trim().to_owned();
            }
            i.paragraphs.clear();
            i
        })
        .collect()
}

static DROP_BLOCKS: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"(?is)<(script|style|figure|iframe)\b.*?</(script|style|figure|iframe)>").unwrap()
});
static BREAKS: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(
        r"(?i)<br\s*/?>|</?(p|div|li|ul|ol|h[1-6]|blockquote|tr|table|section|article)\b[^>]*>",
    )
    .unwrap()
});
static TAGS: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"(?s)<[^>]*>").unwrap());
static SPACES: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"[ \t\r\f\u{a0}]+").unwrap());
static PARAS: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"\n\s*\n|\n").unwrap());

/// Feed HTML → plain-text paragraphs.
pub fn html_paragraphs(html: &str) -> Vec<String> {
    let s = DROP_BLOCKS.replace_all(html, "");
    let s = BREAKS.replace_all(&s, "\n\n");
    let s = TAGS.replace_all(&s, "");
    let s = html_escape::decode_html_entities(&s);
    PARAS
        .split(&s)
        .map(|p| SPACES.replace_all(p, " ").trim().to_owned())
        .filter(|p| !p.is_empty())
        .take(MAX_PARAGRAPHS)
        .collect()
}

fn clean_line(s: &str) -> String {
    let s = html_escape::decode_html_entities(s);
    SPACES
        .replace_all(&s.replace('\n', " "), " ")
        .trim()
        .to_owned()
}

/// A title reduced to its letters and digits, lowercase: how repeats are spotted.
pub fn title_key(title: &str) -> String {
    title
        .chars()
        .filter(|c| c.is_alphanumeric())
        .flat_map(char::to_lowercase)
        .collect()
}

fn when(t: DateTime<Utc>) -> String {
    t.with_timezone(&Local)
        .format("%b %-d, %-I:%M %p")
        .to_string()
}

pub(crate) fn host(url: &str) -> &str {
    url.split("://")
        .nth(1)
        .and_then(|r| r.split('/').next())
        .map(|h| h.trim_start_matches("www."))
        .unwrap_or(url)
}

/// The story as the feed gave it, with a plain source line and no outbound link. `note` says
/// why the full story is missing, when it was looked for.
pub fn article(item: &Item, note: Option<&str>) -> Article {
    let meta = [item.source.clone(), item.published.map(when)]
        .into_iter()
        .flatten()
        .collect::<Vec<_>>()
        .join(" · ");
    let mut body: String = item
        .paragraphs
        .iter()
        .filter(|p| !p.contains("URL:"))
        .map(|p| format!("<p>{}</p>", esc(p)))
        .collect();
    if let Some(n) = note {
        body.push_str(&format!("<p class=\"note\">{}</p>", esc(n)));
    }
    if let Some(link) = &item.link {
        body.push_str(&format!(
            "<p class=\"source\">Source: {}</p>",
            esc(host(link))
        ));
    }
    Article {
        extra: Default::default(),
        title: item.title.clone(),
        meta: (!meta.is_empty()).then_some(meta),
        body,
        teaser: teaser(&item.paragraphs),
        photos: Vec::new(),
        url: item.link.clone(),
        outlet: item
            .source
            .clone()
            .or_else(|| item.link.as_deref().map(|l| host(l).to_owned())),
    }
}

/// The token a story's photo `n` carries in its body until the renderer names the file.
pub fn photo_token(n: usize) -> String {
    format!("__DECKLE_PHOTO_{n}__")
}

/// The full story behind `item` (design §5a), or the feed's version with a note.
pub fn full_article(item: &Item, cfg: &Config, net: &dyn Net) -> Article {
    let Some(link) = item.link.as_deref() else {
        return article(item, None);
    };
    let url = if article::is_google_news(link) {
        match article::resolve_google_news(link, net) {
            Some(u) => u,
            None => return article(item, Some(UNAVAILABLE)),
        }
    } else {
        link.to_owned()
    };
    let (page, access) = match net.get_page_as(&url) {
        Ok(got) => got,
        Err(_) => return article(item, Some(UNAVAILABLE)),
    };
    // A walled page is what a stranger sees: extract it as today (a free story reads in
    // full); only a page the site withholds falls back to the feed, with the reason.
    let Some(x) = article::extract(&String::from_utf8_lossy(&page), &url) else {
        let mut a = match &access {
            Access::Walled { site, why } => article(item, Some(&why.note(site))),
            _ => article(item, Some(UNAVAILABLE)),
        };
        a.extra.access = access;
        return a;
    };
    let outlet = item.source.clone().unwrap_or_else(|| host(&url).to_owned());
    let (mut body, photos, mut extra) = story_body(&x, &outlet, cfg, net);
    // A page the site withholds from this account (not in the plan, or the plan unreadable on
    // an encrypted page) is premium for certain: what extracted is its clear preview, so it
    // prints with the reason, as the headline-only fallback does (spec §9).
    if let Access::Walled {
        site,
        why: why @ (Walled::NotInPlan | Walled::PlanUnknown),
    } = &access
    {
        let note = format!("<p class=\"note\">{}</p>", esc(&why.note(site)));
        let at = body.rfind("<p class=\"source\">").unwrap_or(body.len());
        body.insert_str(at, &note);
    }
    if let Some(p) = item.published {
        extra.published = Some(p.to_rfc3339());
    }
    extra.access = access;
    let meta = [
        x.byline.as_deref().map(by_line),
        item.source.clone(),
        item.published.map(when),
    ]
    .into_iter()
    .flatten()
    .collect::<Vec<_>>()
    .join(" · ");
    Article {
        title: item.title.clone(),
        meta: (!meta.is_empty()).then_some(meta),
        body,
        teaser: x.teaser.or_else(|| teaser(&item.paragraphs)),
        photos,
        url: Some(url),
        outlet: Some(outlet),
        extra,
    }
}

/// "By …" unless the byline already says so.
pub(crate) fn by_line(b: &str) -> String {
    if b.to_lowercase().starts_with("by ") {
        b.to_owned()
    } else {
        format!("By {b}")
    }
}

/// An extracted story's body with its photos fetched and placed, and a source line; and what a
/// bundle carries besides (photo masters and captions, the byline, the page's publish time).
pub(crate) fn story_body(
    x: &article::Extracted,
    outlet: &str,
    cfg: &Config,
    net: &dyn Net,
) -> (String, Vec<Vec<u8>>, crate::Extra) {
    let fetched = if cfg.photos {
        // Kept in color when an export wants it; each EPUB grays them for e-ink.
        let as_ = match cfg.wants_color() {
            true => crate::config::Photos::Color,
            false => crate::config::Photos::Gray,
        };
        let masters = !cfg.bundles_wanted().is_empty();
        article::fetch_photos_with(&x.photos, x.lead, cfg.max_photos, net, as_, masters)
    } else {
        vec![None; x.photos.len()]
    };
    let extra = crate::Extra {
        photos: fetched
            .iter()
            .zip(&x.photos)
            .enumerate()
            .filter_map(|(k, (f, _))| {
                let f = f.as_ref()?;
                let meta = x.photo_meta.get(k).cloned().unwrap_or_default();
                Some(crate::PhotoExtra {
                    master: f.master.clone(),
                    caption: meta.caption,
                    credit: meta.credit,
                    alt: meta.alt,
                })
            })
            .collect(),
        byline: x.byline.clone(),
        published: x.published.clone(),
        access: crate::publisher::Access::Open,
    };
    let fetched: Vec<Option<Vec<u8>>> = fetched.into_iter().map(|f| f.map(|f| f.jpeg)).collect();
    let kept: Vec<bool> = fetched.iter().map(Option::is_some).collect();
    // Placeholder k (all candidates) -> photo n (kept ones, in order).
    let mut rank = Vec::with_capacity(kept.len());
    let mut n = 0;
    for &k in &kept {
        rank.push(n);
        n += usize::from(k);
    }
    let mut body = article::place_photos(&x.xhtml, &kept, |k| photo_token(rank[k]));
    body.push_str(&format!("<p class=\"source\">Source: {}</p>", esc(outlet)));
    (body, fetched.into_iter().flatten().collect(), extra)
}

/// Stories tried, in order, for the whole one in a `Lead` section.
pub(crate) const LEAD_TRIES: usize = 3;
/// A section's note when every story it found was already printed or too old.
pub const NO_NEW_STORIES: &str = "No new stories today.";
/// The marker a fallen-back story carries out of [`full_article`]. It never prints: a `Yes`
/// section strips it and runs the story as a brief, a `Lead` section skips the story.
pub(crate) const UNAVAILABLE: &str = "Full story unavailable offline.";
/// Article pages fetched at once.
const WORKERS: usize = 8;

/// `f` over `items` on up to [`WORKERS`] threads, results in input order.
pub(crate) fn par_map<T: Sync, R: Send>(items: &[T], f: impl Fn(&T) -> R + Sync) -> Vec<R> {
    let next = AtomicUsize::new(0);
    let results: Mutex<Vec<Option<R>>> = Mutex::new((0..items.len()).map(|_| None).collect());
    std::thread::scope(|scope| {
        for _ in 0..WORKERS.min(items.len()) {
            scope.spawn(|| {
                loop {
                    let i = next.fetch_add(1, Ordering::Relaxed);
                    let Some(item) = items.get(i) else { break };
                    let r = f(item);
                    results.lock().expect("no worker panics while holding it")[i] = Some(r);
                }
            });
        }
    });
    results
        .into_inner()
        .expect("workers joined")
        .into_iter()
        .map(|r| r.expect("every index ran"))
        .collect()
}

/// The first paragraph that reads like prose (not "Article URL: …" link lists), kept short.
fn teaser(paragraphs: &[String]) -> Option<String> {
    paragraphs
        .iter()
        .filter(|p| !p.contains("URL:"))
        .find_map(|p| article::teaser_of(p))
}

pub(crate) fn section_of(
    title: &str,
    kicker: Option<String>,
    items: &[Item],
    errors: &[String],
    cfg: &Config,
    net: &dyn Net,
    mode: FullText,
) -> Section {
    if items.is_empty() && !errors.is_empty() {
        return super::unavailable(title, errors.join("; "));
    }
    let articles = if mode == FullText::Lead {
        // Only the top story is printed whole: the first of the leading few that can be read.
        let mut whole = None;
        for (n, item) in items.iter().take(LEAD_TRIES).enumerate() {
            let a = full_article(item, cfg, net);
            if !a.body.contains(UNAVAILABLE) {
                whole = Some((n, a));
                break;
            }
        }
        let brief = |i: &Item| {
            let mut i = i.clone();
            i.paragraphs.clear();
            article(&i, None)
        };
        match whole {
            Some((n, a)) => std::iter::once(a)
                .chain(
                    items
                        .iter()
                        .enumerate()
                        .filter(|(k, _)| *k != n)
                        .map(|(_, i)| brief(i)),
                )
                .collect(),
            None => items.iter().map(brief).collect(),
        }
    } else if mode == FullText::Yes {
        // Whole stories lead; ones only the feed could give run after them as briefs, with no
        // note saying so (welcome design §3): a brief in a `lead` section carries none either.
        let (mut full, briefs): (Vec<_>, Vec<_>) = par_map(items, |i| full_article(i, cfg, net))
            .into_iter()
            .partition(|a| !a.body.contains(UNAVAILABLE));
        full.extend(briefs.into_iter().map(without_note));
        full
    } else {
        items.iter().map(|i| article(i, None)).collect()
    };
    let mut section = Section {
        weather: None,
        puzzle: None,
        brief: Vec::new(),
        key: String::new(),
        title: title.into(),
        kicker,
        articles,
        unavailable: None,
    };
    if items.is_empty() {
        section.unavailable = Some(NO_NEW_STORIES.into());
    }
    section
}

/// The fallen-back story as a plain brief: its [`UNAVAILABLE`] line taken out.
fn without_note(mut a: Article) -> Article {
    a.body = a
        .body
        .replace(&format!("<p class=\"note\">{UNAVAILABLE}</p>"), "");
    a
}

/// The Local section: the Google News search and local feeds, merged, deduplicated, newest
/// first.
pub fn local_section(
    cfg: &Config,
    now: DateTime<Utc>,
    net: &dyn Net,
    taken: &HashSet<String>,
) -> Section {
    let l = &cfg.local;
    if l.google_news.trim().is_empty() && l.feeds.is_empty() {
        return super::unavailable(
            "Local",
            "add a Google News search or local feeds to [local]",
        );
    }
    let kicker = (!cfg.place.is_empty()).then(|| format!("News from around {}", cfg.place));
    let (search, feeds, max) = (&l.google_news, &l.feeds, l.max_items);
    local_news(
        "Local",
        kicker,
        search,
        feeds,
        max,
        cfg,
        now,
        net,
        taken,
        l.hl.as_deref(),
        l.gl.as_deref(),
        l.ceid.as_deref(),
    )
}

/// Another place's local news (#102), built as Local is.
pub fn place_section(
    place: &crate::config::PlaceNews,
    cfg: &Config,
    now: DateTime<Utc>,
    net: &dyn Net,
) -> Section {
    // an empty search falls back to the name (local places design §1)
    if place.search().is_empty() && place.feeds.is_empty() {
        return super::unavailable(&place.name, "give this place a name or local feeds");
    }
    local_news(
        &place.name,
        Some(format!("News from around {}", place.name)),
        place.search(),
        &place.feeds,
        place.max_items.unwrap_or(cfg.local.max_items),
        cfg,
        now,
        net,
        // A place keeps its own stories even when Local printed them: two towns rarely overlap,
        // and a place is built exactly as Local is (local places design §2).
        &HashSet::new(),
        place.hl.as_deref(),
        place.gl.as_deref(),
        place.ceid.as_deref(),
    )
}

/// Google News searches (as briefs) and feeds, fetched, merged, deduplicated by title, newest
/// first. Errors read "Google News: …" or "<host>: …"; one source failing leaves the others.
#[allow(clippy::too_many_arguments)]
pub(crate) fn fetch_merged(
    searches: &[String],
    feeds: &[String],
    max_age_hours: u32,
    now: DateTime<Utc>,
    net: &dyn Net,
    hl: Option<&str>,
    gl: Option<&str>,
    ceid: Option<&str>,
) -> (Vec<Item>, Vec<String>) {
    let mut items = Vec::new();
    let mut errors = Vec::new();
    for search in searches {
        if search.trim().is_empty() {
            continue;
        }
        match net
            .get(&google_news_url(search, max_age_hours, hl, gl, ceid))
            .and_then(|b| parse(&b, now, max_age_hours))
        {
            Ok((_, found)) => items.extend(as_briefs(found)),
            Err(e) => errors.push(format!("Google News: {e}")),
        }
    }
    for url in feeds {
        match net.get(url).and_then(|b| parse(&b, now, max_age_hours)) {
            Ok((name, found)) => items.extend(found.into_iter().map(|mut i| {
                i.source = i.source.or(name.clone());
                i
            })),
            Err(e) => errors.push(format!("{}: {e}", host(url))),
        }
    }
    // Newest first, so the newest copy of a repeated story is the one kept.
    items.sort_by_key(|i| std::cmp::Reverse(i.published));
    let mut seen = HashSet::new();
    items.retain(|i| seen.insert(title_key(&i.title)));
    (items, errors)
}

/// A Google News search and local feeds, merged, deduplicated, newest first, under
/// `[local] max_age_hours`.
#[allow(clippy::too_many_arguments)]
fn local_news(
    title: &str,
    kicker: Option<String>,
    search: &str,
    feeds: &[String],
    max_items: usize,
    cfg: &Config,
    now: DateTime<Utc>,
    net: &dyn Net,
    taken: &HashSet<String>,
    hl: Option<&str>,
    gl: Option<&str>,
    ceid: Option<&str>,
) -> Section {
    let (mut items, errors) = fetch_merged(
        &[search.to_owned()],
        feeds,
        cfg.local.max_age_hours,
        now,
        net,
        hl,
        gl,
        ceid,
    );
    // A story an earlier section printed is dropped before the cut (whichever section comes
    // first in `sections` prints it), so Local refills from its next-newest items.
    items.retain(|i| !taken.contains(&title_key(&i.title)));
    items.truncate(max_items);
    let mode = if cfg.full_text {
        FullText::Yes
    } else {
        FullText::No
    };
    section_of(title, kicker, &items, &errors, cfg, net, mode)
}

/// One `[[feeds]]` entry as its own section: its topic and every feed merged, deduplicated,
/// newest first. `taken` holds the titles other sections already printed.
pub fn feed_section(
    feed: &FeedConfig,
    cfg: &Config,
    now: DateTime<Utc>,
    net: &dyn Net,
    taken: &mut HashSet<String>,
) -> Section {
    let opts = FeedOptions {
        kicker: None,
        max_age_hours: cfg.max_age_hours,
    };
    feed_section_with(feed, opts, cfg, now, net, taken)
}

/// What a feed section takes from its caller rather than the `[[feeds]]` entry.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FeedOptions {
    /// The line over the section's title (a subscribed group's name).
    pub kicker: Option<String>,
    /// Older items are left out.
    pub max_age_hours: u32,
}

/// [`feed_section`] with the kicker and age window given.
pub fn feed_section_with(
    feed: &FeedConfig,
    opts: FeedOptions,
    cfg: &Config,
    now: DateTime<Utc>,
    net: &dyn Net,
    taken: &mut HashSet<String>,
) -> Section {
    let sources = feed.sources();
    if sources.is_empty() {
        return super::unavailable(&feed.name, "give a url, urls or topic");
    }
    let topic_url = feed
        .topic
        .map(|t| t.url(feed.hl.as_deref(), feed.gl.as_deref(), feed.ceid.as_deref()));
    let mut items = Vec::new();
    let mut errors = Vec::new();
    for url in &sources {
        match net
            .get(url)
            .and_then(|b| parse(&b, now, opts.max_age_hours))
        {
            Ok((_, found)) if topic_url.as_ref() == Some(url) => items.extend(as_briefs(found)),
            Ok((name, found)) => items.extend(found.into_iter().map(|mut i| {
                // Several sources: name the channel so a reader can tell them apart.
                if sources.len() > 1 {
                    i.source = i.source.or(name.clone());
                }
                i
            })),
            Err(e) => {
                let what = if topic_url.as_ref() == Some(url) {
                    "Google News"
                } else {
                    host(url)
                };
                errors.push(format!("{what}: {e}"));
            }
        }
    }
    // Newest first, so the newest copy of a repeated story is the one kept.
    items.sort_by_key(|i| std::cmp::Reverse(i.published));
    let mut seen = HashSet::new();
    items.retain(|i| seen.insert(title_key(&i.title)));
    // Stories an earlier section printed are dropped before the cut, so this section refills
    // from its own next-newest items; what it keeps is recorded for the sections after it.
    items.retain(|i| !taken.contains(&title_key(&i.title)));
    items.truncate(feed.max_items);
    taken.extend(items.iter().map(|i| title_key(&i.title)));
    section_of(
        &feed.name,
        opts.kicker,
        &items,
        &errors,
        cfg,
        net,
        feed.full(cfg.full_text),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn html_becomes_paragraphs() {
        let p = html_paragraphs(
            "<p>One &amp; two</p><script>x()</script><p>Three<br/>Four</p><figure><img src=x></figure>",
        );
        assert_eq!(p, ["One & two", "Three", "Four"]);
    }

    #[test]
    fn fetch_merged_dedupes_and_names_feed_sources() {
        struct Two;
        impl Net for Two {
            fn get(&self, url: &str) -> Result<Vec<u8>, String> {
                if url.contains("news.google.com") {
                    Ok(b"<rss version=\"2.0\"><channel><title>G</title><item><title>Alpha - WXII</title>\
                         <link>https://x/a</link><pubDate>Tue, 20 Oct 2026 10:00:00 +0000</pubDate></item>\
                         </channel></rss>"
                        .to_vec())
                } else if url == "https://f.example/rss" {
                    Ok(b"<rss version=\"2.0\"><channel><title>The Feed</title><item><title>Alpha!</title>\
                         <link>https://x/a2</link><pubDate>Tue, 20 Oct 2026 09:00:00 +0000</pubDate></item>\
                         <item><title>Beta</title><link>https://x/b</link>\
                         <pubDate>Tue, 20 Oct 2026 11:00:00 +0000</pubDate></item></channel></rss>"
                        .to_vec())
                } else {
                    Err("offline".into())
                }
            }
        }
        let now = "2026-10-20T22:00:00Z".parse().unwrap();
        let (items, errors) = fetch_merged(
            &["Alpha".into()],
            &[
                "https://f.example/rss".into(),
                "https://bad.example/rss".into(),
            ],
            72,
            now,
            &Two,
            None,
            None,
            None,
        );
        assert_eq!(
            items.iter().map(|i| i.title.as_str()).collect::<Vec<_>>(),
            ["Beta", "Alpha"]
        );
        assert_eq!(items[1].source.as_deref(), Some("WXII"));
        assert_eq!(items[0].source.as_deref(), Some("The Feed"));
        assert_eq!(errors, ["bad.example: offline"]);
    }

    #[test]
    fn a_query_with_google_operators_encodes_and_gets_one_when() {
        let url = google_news_url(
            "\"Rust\" programming OR \"Rust language\" -game",
            72,
            None,
            None,
            None,
        );
        assert!(
            url.contains("q=%22Rust%22+programming+OR+%22Rust+language%22+-game+when%3A3d&"),
            "{url}"
        );
        assert_eq!(url.matches("when").count(), 1);
    }

    #[test]
    fn briefs_split_the_source() {
        let items = as_briefs(vec![Item {
            title: "Standoff in Candor - WXII".into(),
            source: None,
            published: None,
            paragraphs: vec!["noise".into()],
            link: None,
        }]);
        assert_eq!(items[0].title, "Standoff in Candor");
        assert_eq!(items[0].source.as_deref(), Some("WXII"));
        assert!(items[0].paragraphs.is_empty());
    }

    #[test]
    fn query_is_encoded() {
        assert_eq!(
            google_news_url("Moore County NC", 72, None, None, None),
            "https://news.google.com/rss/search?q=Moore+County+NC+when%3A3d&hl=en-US&gl=US&ceid=US:en"
        );
        assert!(google_news_url("x when:1d", 72, None, None, None).contains("x+when%3A1d&"));
    }

    #[test]
    fn google_news_url_i18n() {
        assert_eq!(
            google_news_url("Roma", 72, Some("it-IT"), Some("IT"), Some("IT:it")),
            "https://news.google.com/rss/search?q=Roma+when%3A3d&hl=it-IT&gl=IT&ceid=IT:it"
        );
    }

    /// A subscribed host serving `page` as a stranger would see it.
    struct WalledNet(String, crate::publisher::Walled);
    impl WalledNet {
        fn stranger(page: String) -> Self {
            WalledNet(page, crate::publisher::Walled::NotSignedIn)
        }
    }
    impl Net for WalledNet {
        fn get(&self, _: &str) -> Result<Vec<u8>, String> {
            Ok(self.0.clone().into_bytes())
        }
        fn get_page_as(&self, u: &str) -> Result<(Vec<u8>, crate::publisher::Access), String> {
            self.get(u).map(|b| {
                (
                    b,
                    crate::publisher::Access::Walled {
                        site: "gazette.example".into(),
                        why: self.1.clone(),
                    },
                )
            })
        }
    }

    fn item() -> Item {
        Item {
            title: "Council votes".into(),
            source: Some("The Gazette".into()),
            published: None,
            paragraphs: vec!["The feed's own summary.".into()],
            link: Some("https://www.gazette.example/story".into()),
        }
    }

    fn walled() -> crate::publisher::Access {
        crate::publisher::Access::Walled {
            site: "gazette.example".into(),
            why: crate::publisher::Walled::NotSignedIn,
        }
    }

    /// A site where some pages read and the rest are gone (offline, a wall, a video page).
    struct Patchy;
    impl Net for Patchy {
        fn get(&self, url: &str) -> Result<Vec<u8>, String> {
            if url.ends_with("/whole") {
                let words = "word ".repeat(300);
                Ok(format!(
                    "<html><body><article><h1>T</h1><p>{words}</p><p>{words}</p></article></body></html>"
                )
                .into_bytes())
            } else {
                Err("offline".into())
            }
        }
    }

    fn items(links: &[&str]) -> Vec<Item> {
        links
            .iter()
            .map(|l| Item {
                link: Some(format!("https://www.gazette.example{l}")),
                title: l.trim_start_matches('/').to_owned(),
                ..item()
            })
            .collect()
    }

    #[test]
    fn a_story_that_cannot_be_read_in_full_runs_as_a_brief_without_the_note() {
        let s = section_of(
            "News",
            None,
            &items(&["/gone", "/whole", "/walled"]),
            &[],
            &Config::default(),
            &Patchy,
            FullText::Yes,
        );
        // The whole story leads; the briefs follow in the feed's order.
        assert_eq!(
            s.articles
                .iter()
                .map(|a| a.title.as_str())
                .collect::<Vec<_>>(),
            ["whole", "gone", "walled"]
        );
        assert!(s.articles.iter().all(|a| !a.body.contains(UNAVAILABLE)));
        assert!(
            s.articles
                .iter()
                .all(|a| !a.body.contains("class=\"note\""))
        );
        // A brief keeps the feed's own text and its source line.
        assert!(s.articles[1].body.contains("The feed&#39;s own summary."));
        assert!(s.articles[1].body.contains("Source: gazette.example"));
        assert!(s.unavailable.is_none(), "{:?}", s.unavailable);
        // An empty feed still says what it said before.
        let empty = section_of(
            "News",
            None,
            &[],
            &[],
            &Config::default(),
            &Patchy,
            FullText::Yes,
        );
        assert_eq!(empty.unavailable.as_deref(), Some(NO_NEW_STORIES));
    }

    #[test]
    fn briefs_without_a_lead_still_print_as_briefs() {
        // Lead mode: the top story whole when one reads, the rest briefs by design; none noted.
        let s = section_of(
            "News",
            None,
            &items(&["/gone", "/walled", "/whole"]),
            &[],
            &Config::default(),
            &Patchy,
            FullText::Lead,
        );
        assert_eq!(s.articles.len(), 3);
        assert!(s.articles.iter().all(|a| !a.body.contains(UNAVAILABLE)));
        assert_eq!(s.articles[0].title, "whole");
    }

    #[test]
    fn a_withheld_story_prints_the_feeds_text_with_the_reason() {
        let net = WalledNet::stranger("<html>LOGIN</html>".into());
        let a = full_article(&item(), &Config::default(), &net);
        assert!(a.body.contains("own summary."));
        assert!(
            a.body
                .contains("Subscriber story (not signed in to gazette.example).")
        );
        assert!(!a.body.contains("LOGIN"));
        assert_eq!(a.extra.access, walled());
    }

    #[test]
    fn a_free_story_on_a_subscribed_host_reads_in_full_as_a_stranger() {
        let words = "word ".repeat(300);
        let net = WalledNet::stranger(format!(
            "<html><body><article><h1>T</h1><p>{words}</p><p>{words}</p></article></body></html>"
        ));
        let a = full_article(&item(), &Config::default(), &net);
        assert!(a.body.contains("word word"), "{}", a.body);
        assert!(!a.body.contains("Subscriber story"), "{}", a.body);
        assert!(!a.body.contains("own summary."), "{}", a.body);
        assert_eq!(a.extra.access, walled());
    }

    #[test]
    fn a_premium_page_outside_the_plan_prints_its_clear_text_with_the_reason() {
        use crate::publisher::Walled;
        let words = "word ".repeat(150);
        for why in [Walled::NotInPlan, Walled::PlanUnknown] {
            let net = WalledNet(
                format!("<html><body><article><h1>T</h1><p>{words}</p></article></body></html>"),
                why.clone(),
            );
            let a = full_article(&item(), &Config::default(), &net);
            assert!(a.body.contains("word word"), "{}", a.body);
            assert!(!a.body.contains("own summary."), "{}", a.body);
            let note = format!(
                "<p class=\"note\">{}</p>",
                esc(&why.note("gazette.example"))
            );
            assert!(a.body.contains(&note), "{}", a.body);
            // the note sits before the source line, as the headline-only fallback has it
            assert!(a.body.find(&note) < a.body.find("<p class=\"source\">"));
            assert_eq!(
                a.extra.access,
                crate::publisher::Access::Walled {
                    site: "gazette.example".into(),
                    why
                }
            );
        }
    }

    #[test]
    fn an_open_story_is_unchanged() {
        let words = "word ".repeat(300);
        let page = format!(
            "<html><body><article><h1>T</h1><p>{words}</p><p>{words}</p></article></body></html>"
        );
        let net = move |_: &str| Ok(page.clone().into_bytes());
        let a = full_article(&item(), &Config::default(), &net);
        assert_eq!(a.extra.access, crate::publisher::Access::Open);
        assert!(a.body.contains("word word"));
        assert!(!a.body.contains("Subscriber story"));
    }

    // --- Nesting depth (nesting-depth design, feeds) ---

    const NOW: &str = "2026-10-20T22:00:00Z";

    /// An RSS 2.0 feed of one item titled `title` holding `levels` nested `media:group`.
    fn rss(title: &str, levels: usize) -> String {
        format!(
            "<rss version=\"2.0\" xmlns:media=\"http://search.yahoo.com/mrss/\"><channel>\
             <title>Feed</title><item><title>{title}</title><link>https://x/a</link>{}{}</item>\
             </channel></rss>",
            "<media:group>".repeat(levels),
            "</media:group>".repeat(levels)
        )
    }

    /// `text` in the encoding `label` names.
    fn encode(text: &str, label: &str) -> Vec<u8> {
        let encoding = encoding_rs::Encoding::for_label(label.as_bytes()).unwrap();
        let (bytes, _, unmappable) = encoding.encode(text);
        assert!(!unmappable, "{label}");
        bytes.into_owned()
    }

    fn utf16(text: &str, little: bool) -> Vec<u8> {
        std::iter::once(0xFEFF)
            .chain(text.encode_utf16())
            .flat_map(|u| {
                if little {
                    u.to_le_bytes()
                } else {
                    u.to_be_bytes()
                }
            })
            .collect()
    }

    /// The item titles, parsed on a 2 MiB stack (the app's import threads): a check that came
    /// after `feed-rs` recursed would abort the process.
    fn titles(feed: Vec<u8>) -> Result<Vec<String>, String> {
        std::thread::Builder::new()
            .stack_size(2 << 20)
            .spawn(move || {
                parse(&feed, NOW.parse().unwrap(), 1_000_000)
                    .map(|(_, items)| items.into_iter().map(|i| i.title).collect())
            })
            .unwrap()
            .join()
            .unwrap()
    }

    /// The titles `feed-rs` itself reads, without Deckle's checks.
    fn feed_rs_titles(feed: &[u8]) -> Vec<String> {
        feed_rs::parser::parse(feed)
            .unwrap()
            .entries
            .into_iter()
            .map(|e| clean_line(&e.title.map(|t| t.content).unwrap_or_default()))
            .collect()
    }

    #[test]
    fn a_feed_nested_5_000_deep_is_refused_without_an_abort() {
        let e = titles(rss("Deep", 5_000).into_bytes()).unwrap_err();
        assert!(e.contains("nested more than 256 levels deep"), "{e}");
        assert_eq!(titles(rss("Café", 3).into_bytes()).unwrap(), ["Café"]);
    }

    /// QA's first bypass of a scan of the raw bytes: in these encodings `[` can be the second
    /// byte of a character. Read raw, the DOCTYPE opens an internal subset holding a comment
    /// that runs past the whole feed; decoded, there is no `[`, the DOCTYPE ends at the first
    /// `>`, and the feed's elements are read.
    #[test]
    fn a_trail_byte_that_looks_like_a_bracket_does_not_hide_nesting() {
        for (label, lead) in [
            ("Shift_JIS", 0x81),
            ("Big5", 0xA1),
            ("GBK", 0x81),
            ("gb18030", 0x81),
        ] {
            let mut feed = encode(
                &format!(
                    "<?xml version=\"1.0\" encoding=\"{label}\"?>\
                     <!DOCTYPE rss PUBLIC \"-//Deckle//Test//EN\" \"rss.dtd\" "
                ),
                label,
            );
            feed.extend([lead, b'[']);
            feed.extend(encode(
                &format!("<!-- <x> {} -->]>", rss("Deep", 5_000)),
                label,
            ));
            let e = titles(feed).unwrap_err();
            assert!(e.contains("nested more than 256"), "{label}: {e}");
        }
    }

    /// QA's second bypass: an `encoding=` inside the version's value hides the real one,
    /// ISO-2022-JP. Read raw, `<!--` opens a comment that runs past the whole feed; decoded,
    /// those four bytes between the escapes are two kanji, and the feed's elements are read.
    #[test]
    fn an_encoding_inside_another_value_does_not_hide_nesting() {
        let feed = format!(
            // The line breaks put the escapes past the first 64 bytes, which quick-xml decodes
            // before it reads the declaration.
            "<?xml version=\"1.0 encoding='utf-8'\" encoding=\"ISO-2022-JP\"?>\n\n\n\n\n\n\n\n\
             \x1b$B<!--\x1b(B{} -->",
            rss("Deep", 5_000)
        );
        let e = titles(feed.into_bytes()).unwrap_err();
        assert!(e.contains("nested more than 256"), "{e}");
    }

    /// A second declaration that changes the encoding made `quick-xml` panic, and the panic
    /// ended the edition. Now that feed is unreadable and the others still load.
    #[test]
    fn a_second_declaration_makes_the_feed_unreadable_not_the_edition() {
        struct Two;
        impl Net for Two {
            fn get(&self, url: &str) -> Result<Vec<u8>, String> {
                let body = rss("Fine", 0);
                Ok(if url.contains("bad") {
                    let mut feed = rss("Bad", 0);
                    // More than 64 bytes in, past what quick-xml reads before it decodes.
                    feed.insert_str(
                        feed.find("<item>").unwrap(),
                        "<?xml version=\"1.0\" encoding=\"windows-1252\"?>",
                    );
                    feed.into_bytes()
                } else {
                    body.into_bytes()
                })
            }
        }
        let (items, errors) = fetch_merged(
            &[],
            &[
                "https://bad.example/rss".into(),
                "https://good.example/rss".into(),
            ],
            72,
            NOW.parse().unwrap(),
            &Two,
            None,
            None,
            None,
        );
        assert_eq!(
            items.iter().map(|i| i.title.as_str()).collect::<Vec<_>>(),
            ["Fine"]
        );
        assert_eq!(errors.len(), 1, "{errors:?}");
        assert!(errors[0].starts_with("bad.example: "), "{errors:?}");
        assert!(errors[0].contains("could not be read"), "{errors:?}");
    }

    #[test]
    fn the_encoding_named_twice_in_the_declaration_is_unreadable() {
        let feed = format!(
            "<?xml version=\"1.0\" encoding=\"UTF-8\" encoding=\"UTF-8\"?>{}",
            rss("Twice", 0)
        );
        let e = titles(feed.into_bytes()).unwrap_err();
        assert!(e.contains("could not be read"), "{e}");
    }

    /// `quick-xml` panics on a UTF-16 big-endian feed declared `encoding="UTF-16"` (the label
    /// means little-endian, and the declaration ends past the bytes it reads first). The panic
    /// is caught: the feed is unreadable, nothing else is lost.
    #[test]
    fn a_parser_panic_is_an_unreadable_feed() {
        let feed = utf16(
            &format!(
                "<?xml version=\"1.0\" encoding=\"UTF-16\"?>{}",
                rss("Panics", 0)
            ),
            false,
        );
        let e = titles(feed).unwrap_err();
        // The panic's own message, so a parser bug can still be found; no file or path.
        assert_eq!(
            e,
            "this feed could not be read: its XML declaration names an encoding it cannot switch \
             to (the feed's parser failed: set_encoding() called after prefix buffer was drained; \
             encoding can only be changed while the prefix is still active)"
        );
        for path in [".cargo", "registry", "quick-xml-0.42", "encoding.rs"] {
            assert!(!e.contains(path), "{e}");
        }
        if let Some(home) = std::env::var_os("HOME") {
            assert!(!e.contains(&*home.to_string_lossy()), "{e}");
        }
    }

    /// A caught panic is described by its message, on one line and at most 200 characters; a
    /// payload that is not text says only that the parser failed.
    #[test]
    fn a_caught_panic_is_described_by_its_message() {
        assert_eq!(
            quietly(|| panic!("bad\n  byte {}", 7)).unwrap_err(),
            "the feed's parser failed: bad byte 7"
        );
        assert_eq!(
            quietly(|| panic!("plain")).unwrap_err(),
            "the feed's parser failed: plain"
        );
        assert_eq!(
            quietly(|| std::panic::panic_any(7u8)).unwrap_err(),
            "the feed's parser failed"
        );
        assert_eq!(quietly(|| 1), Ok(1));
        // A message that carries feed text is cut on a character boundary.
        let long = "é€".repeat(2_500);
        assert_eq!(long.chars().count(), 5_000);
        let why = quietly(move || panic!("{long}")).unwrap_err();
        let text = why.strip_prefix("the feed's parser failed: ").unwrap();
        assert_eq!(text.chars().count(), 200, "{text}");
        assert!(text.ends_with('…'), "{text}");
        assert!(text.starts_with("é€é€"), "{text}");
        let exact = "x".repeat(200);
        let why = quietly(move || panic!("{exact}")).unwrap_err();
        assert_eq!(
            why,
            format!("the feed's parser failed: {}", "x".repeat(200))
        );
        // Any other panic leads with the plain sentence.
        assert_eq!(
            unreadable(&quietly(|| panic!("plain")).unwrap_err()),
            "this feed could not be read (the feed's parser failed: plain)"
        );
    }

    /// The caught panic prints nothing (the app shows a Daily run's last stderr lines as its
    /// log), while a panic anywhere else still reaches the usual hook. Run in a child process
    /// of this test binary, so its stderr can be read.
    #[test]
    fn a_caught_parser_panic_prints_nothing() {
        const NAME: &str = "sources::feeds::tests::a_caught_parser_panic_prints_nothing";
        if std::env::var_os("DECKLE_QUIET_PANIC_CHILD").is_some() {
            let feed = utf16(
                &format!(
                    "<?xml version=\"1.0\" encoding=\"UTF-16\"?>{}",
                    rss("Panics", 0)
                ),
                false,
            );
            assert!(titles(feed).is_err());
            let elsewhere = std::panic::catch_unwind(|| panic!("a panic outside the feed parse"));
            assert!(elsewhere.is_err());
            return;
        }
        let out = std::process::Command::new(std::env::current_exe().unwrap())
            .args([NAME, "--exact", "--nocapture", "--test-threads=1"])
            .env("DECKLE_QUIET_PANIC_CHILD", "1")
            .output()
            .unwrap();
        let stderr = String::from_utf8_lossy(&out.stderr);
        assert!(out.status.success(), "{stderr}");
        assert!(
            stderr.contains("a panic outside the feed parse"),
            "{stderr}"
        );
        assert_eq!(stderr.matches("panicked").count(), 1, "{stderr}");
        assert!(!stderr.contains("quick-xml"), "{stderr}");
    }

    /// Shallow feeds in the encodings feeds use load with the titles `feed-rs` reads.
    #[test]
    fn shallow_feeds_in_every_encoding_load_as_before() {
        let mut feeds = Vec::new();
        for (label, title) in [
            ("Shift_JIS", "日本語のニュース"),
            ("EUC-JP", "日本語のニュース"),
            ("GBK", "中文新闻"),
            ("Big5", "中文新聞"),
            ("ISO-8859-1", "Café crème"),
            ("UTF-8", "Grüße ✓"),
        ] {
            let text = format!(
                "<?xml version=\"1.0\" encoding=\"{label}\"?>{}",
                rss(title, 3)
            );
            feeds.push((label.to_owned(), title, encode(&text, label)));
        }
        for little in [true, false] {
            let label = if little { "UTF-16LE" } else { "UTF-16BE" };
            let text = format!(
                "<?xml version=\"1.0\" encoding=\"{label}\"?>{}",
                rss("Grüße ✓", 3)
            );
            feeds.push((label.to_owned(), "Grüße ✓", utf16(&text, little)));
        }
        for (label, title, bytes) in feeds {
            assert_eq!(feed_rs_titles(&bytes), [title], "{label}");
            assert_eq!(titles(bytes), Ok(vec![title.to_owned()]), "{label}");
        }
    }

    /// Every feed fixture loads with the titles `feed-rs` reads.
    #[test]
    fn every_feed_fixture_loads_as_before() {
        let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures");
        let mut stack = vec![dir];
        let mut seen = 0;
        while let Some(d) = stack.pop() {
            for entry in std::fs::read_dir(&d).unwrap() {
                let path = entry.unwrap().path();
                if path.is_dir() {
                    stack.push(path);
                    continue;
                }
                let bytes = std::fs::read(&path).unwrap();
                if !matches!(
                    path.extension().and_then(|e| e.to_str()),
                    Some("xml" | "rss" | "atom")
                ) || feed_rs::parser::parse(&bytes[..]).is_err()
                {
                    continue;
                }
                seen += 1;
                let parsed = parse(&bytes, NOW.parse().unwrap(), 1_000_000).unwrap();
                let mut want = feed_rs_titles(&bytes);
                want.retain(|t| !t.is_empty());
                let mut got: Vec<String> = parsed.1.into_iter().map(|i| i.title).collect();
                want.sort();
                got.sort();
                assert_eq!(got, want, "{}", path.display());
            }
        }
        assert!(seen >= 6, "{seen} feed fixtures");
    }
}
