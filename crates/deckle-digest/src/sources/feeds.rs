// SPDX-License-Identifier: GPL-3.0-or-later
//! RSS / Atom feeds: the Local section (a Google News search plus local papers) and one
//! section per `[[feeds]]` entry. Feed HTML is reduced to text paragraphs (v0).

use std::collections::HashSet;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{LazyLock, Mutex};

use chrono::{DateTime, Local, Utc};
use regex::Regex;

use crate::config::{Config, FeedConfig, FullText, Topic};
use crate::fetch::{Net, encode};
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
pub fn google_news_url(query: &str, max_age_hours: u32) -> String {
    let query = if query.contains("when:") {
        query.to_owned()
    } else {
        format!("{query} when:{}d", max_age_hours.div_ceil(24).max(1))
    };
    format!(
        "https://news.google.com/rss/search?q={}&hl=en-US&gl=US&ceid=US:en",
        encode(&query)
    )
}

/// Feed title and its items, newest first, without items older than `max_age_hours` (items
/// without a date are kept).
pub fn parse(
    bytes: &[u8],
    now: DateTime<Utc>,
    max_age_hours: u32,
) -> Result<(Option<String>, Vec<Item>), String> {
    let feed = feed_rs::parser::parse(bytes).map_err(|e| e.to_string())?;
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

pub(crate) fn title_key(title: &str) -> String {
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
        title: item.title.clone(),
        meta: (!meta.is_empty()).then_some(meta),
        body,
        teaser: teaser(&item.paragraphs),
        photos: Vec::new(),
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
    let extracted = net
        .get_page(&url)
        .ok()
        .and_then(|b| article::extract(&String::from_utf8_lossy(&b), &url));
    let Some(x) = extracted else {
        return article(item, Some(UNAVAILABLE));
    };
    let outlet = item.source.clone().unwrap_or_else(|| host(&url).to_owned());
    let (body, photos) = story_body(&x, &outlet, cfg, net);
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

/// An extracted story's body with its photos fetched and placed, and a source line.
pub(crate) fn story_body(
    x: &article::Extracted,
    outlet: &str,
    cfg: &Config,
    net: &dyn Net,
) -> (String, Vec<Vec<u8>>) {
    let fetched = if cfg.photos {
        article::fetch_photos(&x.photos, x.lead, cfg.max_photos, net)
    } else {
        vec![None; x.photos.len()]
    };
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
    (body, fetched.into_iter().flatten().collect())
}

/// Stories tried, in order, for the whole one in a `Lead` section.
const LEAD_TRIES: usize = 3;
/// A section's note when every story it found was already printed or too old.
pub const NO_NEW_STORIES: &str = "No new stories today.";
const UNAVAILABLE: &str = "Full story unavailable offline.";
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

/// The first paragraph that reads like prose (not "Article URL: …" link lists).
fn teaser(paragraphs: &[String]) -> Option<String> {
    paragraphs
        .iter()
        .find(|p| p.len() >= 80 && !p.contains("URL:"))
        .cloned()
}

fn section_of(
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
        // Whole stories lead; ones only the feed could give run after them, like briefs.
        let (mut full, briefs): (Vec<_>, Vec<_>) = par_map(items, |i| full_article(i, cfg, net))
            .into_iter()
            .partition(|a| !a.body.contains(UNAVAILABLE));
        full.extend(briefs);
        full
    } else {
        items.iter().map(|i| article(i, None)).collect()
    };
    let mut section = Section {
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

/// The Local section: the Google News search and local feeds, merged, deduplicated, newest
/// first.
pub fn local_section(cfg: &Config, now: DateTime<Utc>, net: &dyn Net) -> Section {
    let l = &cfg.local;
    if l.google_news.trim().is_empty() && l.feeds.is_empty() {
        return super::unavailable(
            "Local",
            "add a Google News search or local feeds to [local]",
        );
    }
    let kicker = (!cfg.place.is_empty()).then(|| format!("News from around {}", cfg.place));
    let (search, feeds, max) = (&l.google_news, &l.feeds, l.max_items);
    local_news("Local", kicker, search, feeds, max, cfg, now, net)
}

/// Another place's local news (#102), built as Local is.
pub fn place_section(
    place: &crate::config::PlaceNews,
    cfg: &Config,
    now: DateTime<Utc>,
    net: &dyn Net,
) -> Section {
    if place.google_news.trim().is_empty() && place.feeds.is_empty() {
        return super::unavailable(
            &place.name,
            format!("add a Google News search or local feeds for {}", place.name),
        );
    }
    local_news(
        &place.name,
        Some(format!("News from around {}", place.name)),
        &place.google_news,
        &place.feeds,
        place.max_items.unwrap_or(cfg.local.max_items),
        cfg,
        now,
        net,
    )
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
) -> Section {
    let mut items = Vec::new();
    let mut errors = Vec::new();
    if !search.trim().is_empty() {
        match net
            .get(&google_news_url(search, cfg.local.max_age_hours))
            .and_then(|b| parse(&b, now, cfg.local.max_age_hours))
        {
            Ok((_, found)) => items.extend(as_briefs(found)),
            Err(e) => errors.push(format!("Google News: {e}")),
        }
    }
    for url in feeds {
        match net
            .get(url)
            .and_then(|b| parse(&b, now, cfg.local.max_age_hours))
        {
            Ok((name, found)) => items.extend(found.into_iter().map(|mut i| {
                i.source = i.source.or(name.clone());
                i
            })),
            Err(e) => errors.push(format!("{url}: {e}")),
        }
    }
    let mut seen = HashSet::new();
    items.retain(|i| seen.insert(title_key(&i.title)));
    items.sort_by_key(|i| std::cmp::Reverse(i.published));
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
    let sources = feed.sources();
    if sources.is_empty() {
        return super::unavailable(&feed.name, "give a url, urls or topic");
    }
    let topic_url = feed.topic.map(Topic::url);
    let mut items = Vec::new();
    let mut errors = Vec::new();
    for url in &sources {
        match net.get(url).and_then(|b| parse(&b, now, cfg.max_age_hours)) {
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
        None,
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
            google_news_url("Moore County NC", 72),
            "https://news.google.com/rss/search?q=Moore+County+NC+when%3A3d&hl=en-US&gl=US&ceid=US:en"
        );
        assert!(google_news_url("x when:1d", 72).contains("x+when%3A1d&"));
    }
}
