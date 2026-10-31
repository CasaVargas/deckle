// SPDX-License-Identifier: GPL-3.0-or-later
//! Full text and photos for a story (design §5a): resolve the publisher URL, extract the
//! article Reader-View style, rewrite it to a small XHTML whitelist, and fetch its photos as
//! e-ink JPEGs. Anything that fails leaves the story as the feed gave it.

use std::collections::HashSet;
use std::sync::LazyLock;
use std::time::Duration;

use regex::Regex;

use crate::config::Photos;
use crate::esc;
use crate::fetch::{Net, encode};

/// Fewer words than this after extraction means a paywall, consent wall or video page.
pub const MIN_WORDS: usize = 120;
/// The paper's cap on one story.
pub const MAX_WORDS: usize = 6000;
/// Photos narrower or shorter than this are icons or trackers.
const MIN_PHOTO_SIDE: u32 = 200;
/// An inline photo's shortest side. Smaller ones (a site's default author avatar is typically
/// ~280 px square) are icons, not news; the lead photo only has to clear [`MIN_PHOTO_SIDE`].
const MIN_INLINE_PHOTO_SIDE: u32 = 300;
const PHOTO_WIDTH: u32 = 1000;
const PHOTO_QUALITY: u8 = 70;
/// A photo master's long side and quality: sharp on a 12.9-inch iPad (Books and Kindle design §9).
const MASTER_SIDE: u32 = 2400;
const MASTER_QUALITY: u8 = 85;

/// The placeholder `src` for photo `k` in [`Extracted::xhtml`]; the renderer swaps in a file
/// path, or drops the `<img>` when the photo was not kept.
pub fn placeholder(k: usize) -> String {
    format!("__DECKLE_IMG_{k}__")
}

/// What the page says about one photo: its caption, the credit split from it, its alt text.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct PhotoMeta {
    pub caption: Option<String>,
    pub credit: Option<String>,
    pub alt: Option<String>,
}

#[derive(Clone, Debug, Default)]
pub struct Extracted {
    /// The page's own headline.
    pub title: Option<String>,
    pub byline: Option<String>,
    /// Body XHTML; photos appear as `<img src="{placeholder(k)}"/>`.
    pub xhtml: String,
    /// Photo URLs in placeholder order (the lead image first, when there is one).
    pub photos: Vec<String>,
    /// One per photo, in the same order.
    pub photo_meta: Vec<PhotoMeta>,
    /// Photo 0 is the page's lead image, printed above the text.
    pub lead: bool,
    /// When the page says it was published, as written there (usually RFC 3339).
    pub published: Option<String>,
    pub words: usize,
    /// The first paragraph that reads like prose, for the cover.
    pub teaser: Option<String>,
}

static GN_ATTR: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r#"data-n-a-(id|ts|sg)="([^"]+)""#).unwrap());

/// The longest a teaser runs: a dek of a few lines, not the story.
pub const TEASER_MAX_CHARS: usize = 300;

/// A paragraph's teaser: its first line (a `<br/>` starts one) long enough to read as prose,
/// kept to the whole sentences that fit, or cut at a word when one sentence runs on.
pub(crate) fn teaser_of(text: &str) -> Option<String> {
    let line = text
        .lines()
        .map(|l| l.split_whitespace().collect::<Vec<_>>().join(" "))
        .find(|l| l.len() >= 80)?;
    if line.chars().count() <= TEASER_MAX_CHARS {
        return Some(line);
    }
    let head: String = line.chars().take(TEASER_MAX_CHARS + 1).collect();
    // The last sentence end that fits, so long as it leaves a teaser's worth. The next sentence
    // starts with a capital, so "p. 144" and "No. 3" don't end one.
    let end = head
        .char_indices()
        .filter(|&(i, c)| {
            c == ' '
                && i >= 80
                && head[..i]
                    .trim_end_matches(['\u{201d}', '\u{2019}', '"', '\'', ')'])
                    .ends_with(['.', '!', '?'])
                && line[i + 1..]
                    .trim_start_matches(['\u{201c}', '\u{2018}', '"', '\'', '('])
                    .starts_with(char::is_uppercase)
        })
        .map(|(i, _)| i)
        .next_back();
    Some(match end {
        Some(i) => head[..i].to_owned(),
        None => crate::print::shorten(&line, TEASER_MAX_CHARS),
    })
}

pub fn is_google_news(url: &str) -> bool {
    url.starts_with("https://news.google.com/rss/articles/")
        || url.starts_with("https://news.google.com/articles/")
}

/// The publisher URL behind a Google News link, via the interstitial page's signature and the
/// `batchexecute` decoder. Undocumented: `None` on any surprise.
pub fn resolve_google_news(url: &str, net: &dyn Net) -> Option<String> {
    let page = String::from_utf8_lossy(&retrying(|| net.get_page(url)).ok()?).into_owned();
    let (mut id, mut ts, mut sg) = (None, None, None);
    for c in GN_ATTR.captures_iter(&page) {
        let v = c[2].to_owned();
        match &c[1] {
            "id" => id = id.or(Some(v)),
            "ts" => ts = ts.or(Some(v)),
            _ => sg = sg.or(Some(v)),
        }
    }
    let (id, ts, sg) = (id?, ts?, sg?);
    let req = format!(
        r#"[[["Fbv4je","[\"garturlreq\",[[\"X\",\"X\",[\"X\",\"X\"],null,null,1,1,\"US:en\",null,1,null,null,null,null,null,0,1],\"X\",\"X\",1,[1,1,1],1,1,null,0,0,null,0],\"{id}\",{ts},\"{sg}\"]",null,"generic"]]]"#
    );
    let form = format!("f.req={}", encode(&req));
    let reply = retrying(|| {
        net.post_form(
            "https://news.google.com/_/DotsSplashUi/data/batchexecute",
            &form,
        )
    })
    .ok()?;
    garturl(&String::from_utf8_lossy(&reply))
}

/// The publisher URL in a `batchexecute` reply: after the `)]}'` guard, a JSON array whose
/// `Fbv4je` entry holds, as a JSON string, `["garturlres", url, 1]`. Parsed rather than matched,
/// so an escaped character (`=` comes as `\u003d` in legacy.com links) stays part of the URL.
fn garturl(reply: &str) -> Option<String> {
    let json = reply.trim_start().strip_prefix(")]}'")?;
    let entries: Vec<serde_json::Value> = serde_json::from_str(json.trim()).ok()?;
    entries.iter().find_map(|e| {
        let inner: serde_json::Value = serde_json::from_str(e.get(2)?.as_str()?).ok()?;
        (inner.get(0)?.as_str()? == "garturlres")
            .then(|| inner.get(1)?.as_str().map(str::to_owned))?
            .filter(|u| u.starts_with("https://") || u.starts_with("http://"))
    })
}

/// `fetch`, tried again when the connection dropped. Google's front end drops cold TLS
/// connections mid-handshake when several open at once (the Local section's parallel fetches
/// at 05:30 lost every Google News story to it); the next try almost always gets through.
fn retrying(fetch: impl Fn() -> Result<Vec<u8>, String>) -> Result<Vec<u8>, String> {
    let mut wait = Duration::from_millis(300);
    for _ in 0..2 {
        match fetch() {
            Err(e) if dropped(&e) => {
                std::thread::sleep(wait);
                wait *= 3;
            }
            got => return got,
        }
    }
    fetch()
}

/// A connection closed or reset under the request, as opposed to an answer or a timeout.
fn dropped(error: &str) -> bool {
    ["close_notify", "onnection reset", "roken pipe"]
        .iter()
        .any(|e| error.contains(e))
}

/// An absolute photo or page URL. `file:` only for a saved page (a `file:` base), and only
/// inside that page's own folder.
fn resolve(base: &str, href: &str) -> Option<String> {
    let href = href.trim();
    if href.is_empty() || href.starts_with("data:") {
        return None;
    }
    let base = url::Url::parse(base).ok()?;
    let url = base.join(href).ok()?;
    match url.scheme() {
        "http" | "https" => Some(url.to_string()),
        "file" if base.scheme() == "file" => {
            let dir = base.to_file_path().ok()?.parent()?.to_path_buf();
            let path = url.to_file_path().ok()?;
            (path.starts_with(&dir) && !href.contains("..")).then(|| url.to_string())
        }
        _ => None,
    }
}

/// The photo's identity for deduplication: scheme, host and path, without the query (CDNs
/// put sizes there).
fn photo_key(url: &str) -> String {
    url.split(['?', '#']).next().unwrap_or(url).to_owned()
}

struct Writer {
    base: String,
    out: String,
    photos: Vec<String>,
    /// Each photo's dedup key, alt text and caption, by index.
    keys: Vec<String>,
    alts: Vec<Option<String>>,
    captions: Vec<Option<String>>,
    /// The photos in each open `<figure>`, innermost last: a `<figcaption>` names them.
    figures: Vec<Vec<usize>>,
    seen: HashSet<String>,
    words: usize,
    max_words: Option<usize>,
    teaser: Option<String>,
}

const SKIP: &[&str] = &[
    "script", "style", "noscript", "iframe", "svg", "form", "button", "input", "select",
    "textarea", "video", "audio", "canvas", "table", "nav", "aside", "footer", "header",
];

impl Writer {
    /// A new photo's index; `None` for one already placed (its alt text and figure still count).
    fn photo(&mut self, url: String, alt: Option<String>) -> Option<usize> {
        let key = photo_key(&url);
        if !self.seen.insert(key.clone()) {
            if let Some(i) = self.keys.iter().position(|k| *k == key) {
                if self.alts[i].is_none() {
                    self.alts[i] = alt;
                }
                if let Some(f) = self.figures.last_mut() {
                    f.push(i);
                }
            }
            return None;
        }
        self.photos.push(url);
        self.keys.push(key);
        self.alts.push(alt);
        self.captions.push(None);
        let i = self.photos.len() - 1;
        if let Some(f) = self.figures.last_mut() {
            f.push(i);
        }
        Some(i)
    }

    fn img_src(&self, node: &dom_query::NodeRef) -> Option<String> {
        let attr = |n: &str| node.attr(n).map(|v| v.to_string());
        // The largest candidate in a srcset ("a.jpg 400w, b.jpg 800w" → b.jpg).
        let srcset_first = |n: &str| {
            attr(n).and_then(|s| {
                s.split(',')
                    .next_back()
                    .and_then(|c| c.split_whitespace().next())
                    .map(str::to_owned)
            })
        };
        // A lazy image's real source wins over a placeholder in `src`.
        let lazy = attr("data-src")
            .or_else(|| srcset_first("srcset"))
            .or_else(|| srcset_first("data-srcset"))
            .and_then(|u| resolve(&self.base, &u));
        if lazy.is_some() {
            return lazy;
        }
        let src = attr("src")?;
        // Single-file saved pages embed their photos (raster only).
        if src.starts_with("data:image/") {
            return (!src.starts_with("data:image/svg")).then_some(src);
        }
        resolve(&self.base, &src)
    }

    /// Writes `node`'s children; returns the plain text written.
    fn children(&mut self, node: &dom_query::NodeRef) -> String {
        let mut text = String::new();
        for child in node.children() {
            text.push_str(&self.node(&child));
        }
        text
    }

    /// Writes an element's children into a buffer and wraps them in `tag` when not empty.
    fn wrapped(&mut self, node: &dom_query::NodeRef, open: &str, close: &str) -> String {
        let saved = std::mem::take(&mut self.out);
        let text = self.children(node);
        let inner = std::mem::replace(&mut self.out, saved);
        if !text.trim().is_empty() || inner.contains("<img") {
            self.out.push_str(open);
            self.out.push_str(inner.trim());
            self.out.push_str(close);
        }
        text
    }

    /// Writes an inline element like [`Self::wrapped`], but moves a space at its edges outside
    /// the tags rather than dropping it: `<em>tenacity </em>and` stays two words.
    fn inline(&mut self, node: &dom_query::NodeRef, open: &str, close: &str) -> String {
        let saved = std::mem::take(&mut self.out);
        let text = self.children(node);
        let inner = std::mem::replace(&mut self.out, saved);
        if text.trim().is_empty() && !inner.contains("<img") {
            if !text.is_empty() {
                self.space();
            }
            return text;
        }
        if inner.starts_with(char::is_whitespace) {
            self.space();
        }
        self.out.push_str(open);
        self.out.push_str(inner.trim());
        self.out.push_str(close);
        if inner.ends_with(char::is_whitespace) {
            self.out.push(' ');
        }
        text
    }

    /// A space between words, unless one is already there.
    fn space(&mut self) {
        if !self.out.ends_with(char::is_whitespace) {
            self.out.push(' ');
        }
    }

    fn node(&mut self, node: &dom_query::NodeRef) -> String {
        if self.max_words.is_some_and(|m| self.words >= m) {
            return String::new();
        }
        if node.is_text() {
            let t = node.text().to_string();
            let mut words: Vec<&str> = t.split_whitespace().collect();
            let mut cut = false;
            if let Some(m) = self.max_words
                && self.words + words.len() > m
            {
                words.truncate(m - self.words);
                cut = true;
            }
            let mut collapsed = words.join(" ");
            if cut {
                collapsed.push('…');
            }
            if collapsed.is_empty() {
                if !t.is_empty() {
                    self.space();
                }
                return String::new();
            }
            let lead = if t.starts_with(char::is_whitespace) {
                " "
            } else {
                ""
            };
            let trail = if t.ends_with(char::is_whitespace) {
                " "
            } else {
                ""
            };
            self.words += collapsed.split(' ').count();
            if !lead.is_empty() {
                self.space();
            }
            self.out.push_str(&format!("{}{trail}", esc(&collapsed)));
            return format!("{lead}{collapsed}{trail}");
        }
        if !node.is_element() {
            return String::new();
        }
        let name = node
            .node_name()
            .map(|n| n.to_ascii_lowercase())
            .unwrap_or_default();
        match name.as_str() {
            n if SKIP.contains(&n) => String::new(),
            "p" => {
                let text = self.wrapped(node, "<p>", "</p>\n");
                if self.teaser.is_none() {
                    self.teaser = teaser_of(&text);
                }
                text
            }
            "h1" | "h2" | "h3" | "h4" | "h5" | "h6" => {
                self.wrapped(node, "<h3 class=\"subhead\">", "</h3>\n")
            }
            "blockquote" => self.wrapped(node, "<blockquote>", "</blockquote>\n"),
            "ul" => self.wrapped(node, "<ul>", "</ul>\n"),
            "ol" => self.wrapped(node, "<ol>", "</ol>\n"),
            "li" => self.wrapped(node, "<li>", "</li>"),
            "em" | "i" => self.inline(node, "<em>", "</em>"),
            "strong" | "b" => self.inline(node, "<strong>", "</strong>"),
            "figcaption" => {
                let text = self.wrapped(node, "<span class=\"caption\">", "</span>");
                let caption = text.split_whitespace().collect::<Vec<_>>().join(" ");
                if !caption.is_empty()
                    && let Some(f) = self.figures.last()
                {
                    for &i in f {
                        if self.captions[i].is_none() {
                            self.captions[i] = Some(caption.clone());
                        }
                    }
                }
                text
            }
            "figure" => {
                self.figures.push(Vec::new());
                let text = self.wrapped(node, "<div class=\"figure\">", "</div>\n");
                self.figures.pop();
                text
            }
            "br" => {
                self.out.push_str("<br/>");
                // A new line for the teaser: some sites set a whole story as one broken paragraph.
                "\n".to_owned()
            }
            "img" => {
                let alt = node
                    .attr("alt")
                    .map(|a| a.split_whitespace().collect::<Vec<_>>().join(" "))
                    .filter(|a| !a.is_empty());
                if let Some(k) = self.img_src(node).and_then(|u| self.photo(u, alt)) {
                    self.out
                        .push_str(&format!("<img src=\"{}\" alt=\"\"/>", placeholder(k)));
                }
                String::new()
            }
            _ => self.children(node),
        }
    }
}

/// Extracts a story for the paper: at most 6,000 words. `None` when it reads like a wall or a
/// stub (fewer than [`MIN_WORDS`] words).
pub fn extract(html: &str, url: &str) -> Option<Extracted> {
    extract_with(html, url, Some(MAX_WORDS))
}

/// [`extract`] with a chosen cap; `None` keeps every word (long reads, save-to-paper §5).
/// `url` may be `file://…` for a saved page: its photos then load from beside it.
pub fn extract_with(html: &str, url: &str, max_words: Option<usize>) -> Option<Extracted> {
    extract_linted(html, url, max_words).map(|(x, _)| x)
}

/// [`extract_with`], also returning the boilerplate paragraphs [`lint`] dropped
/// (`deckle digest lint`).
pub fn extract_linted(
    html: &str,
    url: &str,
    max_words: Option<usize>,
) -> Option<(Extracted, Vec<String>)> {
    let mut readability = dom_smoothie::Readability::new(html, Some(url), None).ok()?;
    let article = readability.parse().ok()?;
    let mut lead_kept = false;
    let mut w = Writer {
        base: url.to_owned(),
        out: String::new(),
        photos: Vec::new(),
        keys: Vec::new(),
        alts: Vec::new(),
        captions: Vec::new(),
        figures: Vec::new(),
        seen: HashSet::new(),
        words: 0,
        max_words,
        teaser: None,
    };
    // The lead image goes first, above the text.
    if let Some(lead) = article.image.as_deref().and_then(|i| resolve(url, i))
        && let Some(k) = w.photo(lead, None)
    {
        lead_kept = true;
        w.out.push_str(&format!(
            "<div class=\"figure lead\"><img src=\"{}\" alt=\"\"/></div>\n",
            placeholder(k)
        ));
    }
    let doc = dom_query::Document::from(article.content.to_string());
    for body in doc.select("body").nodes() {
        w.children(body);
    }
    if w.words < MIN_WORDS {
        return None;
    }
    let title = article.title.trim().to_owned();
    let photo_meta = w
        .captions
        .iter()
        .zip(&w.alts)
        .map(|(caption, alt)| {
            let (caption, credit) = caption.as_deref().map(split_credit).unwrap_or((None, None));
            PhotoMeta {
                caption,
                credit,
                alt: alt.clone(),
            }
        })
        .collect();
    let curl = deckle_core::typography::curls_quotes(article.lang.as_deref());
    let (linted, dropped) = lint(&w.out);
    let xhtml = tidy(&linted, curl);
    let x = Extracted {
        title: (!title.is_empty()).then_some(title),
        byline: article
            .byline
            .map(|b| b.to_string())
            .map(|b| b.trim().to_owned())
            .filter(|b| !b.is_empty() && b.len() < 120),
        xhtml,
        photos: w.photos,
        photo_meta,
        lead: lead_kept,
        published: article
            .published_time
            .map(|t| t.trim().to_owned())
            .filter(|t| !t.is_empty()),
        words: w.words,
        teaser: w.teaser,
    };
    Some((x, dropped))
}

/// A photo as an e-ink JPEG: grayscale, at most 1000 px wide. `None` for tiny images (icons,
/// trackers) and formats the decoder does not know.
pub fn eink_photo(bytes: &[u8]) -> Option<Vec<u8>> {
    photo(bytes, Photos::Gray)
}

/// A photo as a JPEG at most 1000 px wide, grayscale or in color. `None` as for [`eink_photo`].
pub fn photo(bytes: &[u8], photos: Photos) -> Option<Vec<u8>> {
    let img = image::load_from_memory(bytes).ok()?;
    if img.width().min(img.height()) < MIN_PHOTO_SIDE {
        return None;
    }
    let img = if img.width() > PHOTO_WIDTH {
        img.resize(PHOTO_WIDTH, u32::MAX, image::imageops::FilterType::Lanczos3)
    } else {
        img
    };
    let mut out = Vec::new();
    let encoder = image::codecs::jpeg::JpegEncoder::new_with_quality(&mut out, PHOTO_QUALITY);
    match photos {
        Photos::Gray => img.grayscale().to_luma8().write_with_encoder(encoder),
        Photos::Color => img.to_rgb8().write_with_encoder(encoder),
    }
    .ok()?;
    Some(out)
}

/// A photo master for a native reader (Books and Kindle design §9): in color, turned upright,
/// at most [`MASTER_SIDE`] on its long side and never enlarged, as a JPEG with no metadata.
/// `None` as for [`photo`].
pub fn master_photo(bytes: &[u8]) -> Option<Vec<u8>> {
    use image::ImageDecoder;
    let mut decoder = image::ImageReader::new(std::io::Cursor::new(bytes))
        .with_guessed_format()
        .ok()?
        .into_decoder()
        .ok()?;
    let orientation = decoder.orientation().ok();
    let mut img = image::DynamicImage::from_decoder(decoder).ok()?;
    if let Some(o) = orientation {
        img.apply_orientation(o);
    }
    if img.width().min(img.height()) < MIN_PHOTO_SIDE {
        return None;
    }
    if img.width().max(img.height()) > MASTER_SIDE {
        img = img.resize(
            MASTER_SIDE,
            MASTER_SIDE,
            image::imageops::FilterType::Lanczos3,
        );
    }
    let mut out = Vec::new();
    img.to_rgb8()
        .write_with_encoder(image::codecs::jpeg::JpegEncoder::new_with_quality(
            &mut out,
            MASTER_QUALITY,
        ))
        .ok()?;
    Some(out)
}

/// `jpeg` for an e-ink page: as it is when it's already grayscale (only its header is read),
/// else turned gray. A JPEG that won't decode is kept as it is.
pub fn gray_jpeg(jpeg: &[u8]) -> Vec<u8> {
    use image::ImageDecoder;
    let gray = image::codecs::jpeg::JpegDecoder::new(std::io::Cursor::new(jpeg))
        .is_ok_and(|d| d.color_type() == image::ColorType::L8);
    if gray {
        return jpeg.to_vec();
    }
    let Ok(img) = image::load_from_memory(jpeg) else {
        return jpeg.to_vec();
    };
    let mut out = Vec::new();
    match img.grayscale().to_luma8().write_with_encoder(
        image::codecs::jpeg::JpegEncoder::new_with_quality(&mut out, PHOTO_QUALITY),
    ) {
        Ok(()) => out,
        Err(_) => jpeg.to_vec(),
    }
}

/// A 64-bit difference hash: the picture's gradient at 9×8. Copies of one photo from a CDN
/// (other sizes, crops, recompression) land a few bits apart.
pub fn photo_hash(jpeg: &[u8]) -> Option<u64> {
    let small = image::load_from_memory(jpeg)
        .ok()?
        .resize_exact(9, 8, image::imageops::FilterType::Triangle)
        .to_luma8();
    let mut bits = 0u64;
    for y in 0..8 {
        for x in 0..8 {
            let brighter = small.get_pixel(x, y).0[0] > small.get_pixel(x + 1, y).0[0];
            bits = (bits << 1) | u64::from(brighter);
        }
    }
    Some(bits)
}

/// Hashes this close are the same photo. Measured on one live edition (2026-10-20): repeats
/// of a photo came 0–10 bits apart, different photos 20 or more.
pub const SAME_PHOTO_BITS: u32 = 14;

/// A photo's bytes: a `data:` URI, a file beside a saved page (already checked by
/// `resolve`), or a fetch.
pub fn load_photo(url: &str, net: &dyn Net) -> Option<Vec<u8>> {
    use base64::Engine;
    if let Some(rest) = url.strip_prefix("data:") {
        let (meta, data) = rest.split_once(',')?;
        return if meta.ends_with(";base64") {
            base64::engine::general_purpose::STANDARD
                .decode(data.trim())
                .ok()
        } else {
            None
        };
    }
    if url.starts_with("file:") {
        let path = url::Url::parse(url).ok()?.to_file_path().ok()?;
        return std::fs::read(path).ok();
    }
    net.get_page(url).ok()
}

/// Fetches up to `max` of `photos`, in order, skipping repeats of a photo already kept; each
/// slot is the JPEG or `None` (not kept). With `lead`, photo 0 is the lead image; every other
/// photo is inline and must clear [`MIN_INLINE_PHOTO_SIDE`].
pub fn fetch_photos(
    photos: &[String],
    lead: bool,
    max: usize,
    net: &dyn Net,
) -> Vec<Option<Vec<u8>>> {
    fetch_photos_as(photos, lead, max, net, Photos::Gray)
}

/// [`fetch_photos`], keeping the photos in color when `as_` says so.
pub fn fetch_photos_as(
    photos: &[String],
    lead: bool,
    max: usize,
    net: &dyn Net,
    as_: Photos,
) -> Vec<Option<Vec<u8>>> {
    fetch_photos_with(photos, lead, max, net, as_, false)
        .into_iter()
        .map(|f| f.map(|f| f.jpeg))
        .collect()
}

/// A fetched photo: the edition's JPEG, and its master when one was asked for.
#[derive(Clone, Debug)]
pub struct Fetched {
    pub jpeg: Vec<u8>,
    pub master: Option<Vec<u8>>,
}

/// [`fetch_photos_as`], also making each kept photo's master (for an edition bundle) from the
/// same download.
pub fn fetch_photos_with(
    photos: &[String],
    lead: bool,
    max: usize,
    net: &dyn Net,
    as_: Photos,
    masters: bool,
) -> Vec<Option<Fetched>> {
    let mut hashes: Vec<u64> = Vec::new();
    photos
        .iter()
        .enumerate()
        .map(|(k, url)| {
            if hashes.len() >= max {
                return None;
            }
            let bytes = load_photo(url, net)?;
            let jpeg = photo(&bytes, as_)?;
            if !(lead && k == 0) && !clears_inline_size(&jpeg) {
                return None;
            }
            let hash = photo_hash(&jpeg)?;
            if hashes
                .iter()
                .any(|h| (h ^ hash).count_ones() <= SAME_PHOTO_BITS)
            {
                return None;
            }
            hashes.push(hash);
            Some(Fetched {
                master: masters.then(|| master_photo(&bytes)).flatten(),
                jpeg,
            })
        })
        .collect()
}

/// Whether an e-ink JPEG is big enough to sit inside the text. (Resizing only ever shrinks
/// wide photos, so a small image's size here is its original size.)
fn clears_inline_size(jpeg: &[u8]) -> bool {
    image::load_from_memory(jpeg).is_ok_and(|i| i.width().min(i.height()) >= MIN_INLINE_PHOTO_SIDE)
}

/// Removes `<div class="figure">` wrappers left with nothing in them.
pub fn strip_empty_figures(xhtml: &str) -> String {
    static EMPTY_FIGURE: LazyLock<Regex> =
        LazyLock::new(|| Regex::new(r#"<div class="figure( lead)?">\s*</div>\n?"#).unwrap());
    EMPTY_FIGURE.replace_all(xhtml, "").into_owned()
}

/// Replaces each photo placeholder with `path(k)` for kept photos and removes the `<img>` of
/// the rest (and any figure left empty).
pub fn place_photos(xhtml: &str, kept: &[bool], path: impl Fn(usize) -> String) -> String {
    let mut out = xhtml.to_owned();
    for (k, keep) in kept.iter().enumerate() {
        let img = format!("<img src=\"{}\" alt=\"\"/>", placeholder(k));
        out = if *keep {
            out.replace(&placeholder(k), &path(k))
        } else {
            out.replace(&img, "")
        };
    }
    // Photos beyond the list (none expected) and figures emptied above.
    static LEFTOVER: LazyLock<Regex> =
        LazyLock::new(|| Regex::new(r#"<img src="__DECKLE_IMG_\d+__" alt=""/>"#).unwrap());
    strip_empty_figures(&LEFTOVER.replace_all(&out, ""))
}

/// Paragraphs that are a site's furniture, not the story (Books and Kindle design §9). Matched
/// against a paragraph's whole text, case-insensitively.
static BOILERPLATE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(concat!(
        r"(?i)^\s*(",
        r"advertisement|advertisements|sponsored( content)?|story continues below( advertisement)?",
        r"|(sign up|subscribe) (for|to) (our|the) [^.]{0,60}newsletters?[^.]{0,80}",
        r"|get (our|the) [^.]{0,40}newsletter[^.]{0,80}",
        r"|(read|see) (more|also|next)\s*:.{0,160}",
        r"|related( stories| articles| coverage)?\s*:.{0,160}",
        r"|(share|follow us) on (facebook|twitter|x|instagram|linkedin|threads|bluesky)[^.]{0,80}",
        r"|share this (article|story)[^.]{0,40}",
        r"|click here to [^.]{0,80}",
        r"|we use cookies[^.]{0,200}",
        r"|this (site|website) uses cookies[^.]{0,200}",
        r")\.?\s*$",
    ))
    .unwrap()
});

/// A newsletter sign-up's headline, as a paragraph or a subhead: short, and starting with the
/// ask ("Sign up for the California Morning Report newsletter"). A paragraph that only mentions
/// a newsletter doesn't start this way.
static SIGNUP: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(
        r"(?i)^(sign up|subscribe) (for|to) [^.!?,;:]{0,60}\bnewsletters?\b[^.!?,;:]{0,30}[.!]?$",
    )
    .unwrap()
});

/// A sign-up module's thank-you, which a page hides until the form is sent.
static SIGNED_UP: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"(?i)^thanks? (you )?for (signing up|subscribing)[.!]?$").unwrap()
});

/// The most words a sign-up's tagline has ("The biggest news, opinion and culture shaping
/// California right now.").
const TAGLINE_WORDS: usize = 20;

/// Drops boilerplate paragraphs from extracted XHTML, and newsletter sign-ups with the short
/// tagline paragraph right after them; returns the rest and the text of what went, for
/// `deckle digest lint`.
pub fn lint(xhtml: &str) -> (String, Vec<String>) {
    static BLOCK: LazyLock<Regex> = LazyLock::new(|| {
        Regex::new(r#"(?s)<p>(.*?)</p>\n?|<h3 class="subhead">(.*?)</h3>\n?"#).unwrap()
    });
    static TAG: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"<[^>]+>").unwrap());
    let mut dropped = Vec::new();
    // Where the last sign-up dropped ended, while its tagline may still follow.
    let mut signup_end = None;
    let out = BLOCK.replace_all(xhtml, |c: &regex::Captures| {
        let all = c.get(0).unwrap();
        let (inner, para) = match c.get(1) {
            Some(p) => (p.as_str(), true),
            None => (&c[2], false),
        };
        let text = html_escape::decode_html_entities(&TAG.replace_all(inner, "")).into_owned();
        let text = text.split_whitespace().collect::<Vec<_>>().join(" ");
        let tagline = signup_end.take().is_some_and(|end| {
            para && xhtml[end..all.start()].trim().is_empty()
                && text.split(' ').count() <= TAGLINE_WORDS
        });
        let signup = SIGNUP.is_match(&text);
        let boilerplate = SIGNED_UP.is_match(&text) || para && BOILERPLATE.is_match(&text);
        if (signup || boilerplate || tagline) && !inner.contains("<img") {
            if signup {
                signup_end = Some(all.end());
            }
            dropped.push(text);
            String::new()
        } else {
            all.as_str().to_owned()
        }
    });
    (out.into_owned(), dropped)
}

/// The book typography pass (curly quotes, dashes, ellipses) over an XHTML fragment.
fn tidy(fragment: &str, curl_quotes: bool) -> String {
    let mut report = deckle_core::typography::TypographyReport::default();
    let doc = format!("<body>{fragment}</body>");
    let out = deckle_core::typography::tidy_xhtml(&doc, curl_quotes, &mut report);
    out.strip_prefix("<body>")
        .and_then(|o| o.strip_suffix("</body>"))
        .unwrap_or(fragment)
        .to_owned()
}

/// A caption split from the credit it ends with: "Stalls at the shed. (Jane Doe / The Gazette)",
/// "… Photo: Jane Doe", "… Jane Doe/AP".
pub fn split_credit(caption: &str) -> (Option<String>, Option<String>) {
    static MARKED: LazyLock<Regex> = LazyLock::new(|| {
        Regex::new(r"(?i)^(.*?)[\s;,–—-]*\(?\b(?:photo(?:graph)?s?|image|picture|credit|illustration)(?:\s+by|\s*:)\s*([^()]{2,80}?)\)?\s*$").unwrap()
    });
    static AGENCY: LazyLock<Regex> = LazyLock::new(|| {
        Regex::new(r"(?i)^(.*?[.!?”\)])\s*\(?([^.()]{2,60}?\s*/\s*(?:AP|AFP|Reuters|Getty Images|EPA|Shutterstock|Bloomberg|[A-Z][\w .&]{1,40}))\)?\s*$").unwrap()
    });
    let clean = |s: &str| {
        let t = s
            .trim()
            .trim_end_matches([' ', ',', ';', '–', '—', '-'])
            .trim();
        (!t.is_empty()).then(|| t.to_owned())
    };
    if let Some(c) = MARKED.captures(caption) {
        return (clean(&c[1]), clean(&c[2]));
    }
    if let Some(c) = AGENCY.captures(caption) {
        return (clean(&c[1]), clean(&c[2]));
    }
    (clean(caption), None)
}

/// Whether a story's text reads as cut off by a paywall: it ends asking you to subscribe or
/// sign in.
pub fn looks_cut_off(text: &str) -> bool {
    static WALL: LazyLock<Regex> = LazyLock::new(|| {
        Regex::new(r"(?i)(subscribe (now |today )?to (continue|read|keep)|for subscribers only|this (article|story) is (only )?(available )?(to|for) (subscribers|members)|sign in to (continue|read)|log in to (continue|read)|already a subscriber|to continue reading)").unwrap()
    });
    let tail: String = {
        let chars: Vec<char> = text.trim_end().chars().collect();
        chars[chars.len().saturating_sub(300)..].iter().collect()
    };
    WALL.is_match(&tail)
}

/// A sentence to pull out of a long story (60 to 200 characters): a quotation if the story has
/// one, else the one that best stands alone. `None` for stories under 600 words. `text` is the
/// story's paragraphs, one a line: a sentence never runs across two.
pub fn pull_quote(text: &str, words: usize) -> Option<String> {
    if words < 600 {
        return None;
    }
    static SENTENCE: LazyLock<Regex> =
        LazyLock::new(|| Regex::new(r#"[^.!?\n]+[.!?]+["”’]?"#).unwrap());
    let sentences: Vec<String> = SENTENCE
        .find_iter(text)
        .map(|m| m.as_str().split_whitespace().collect::<Vec<_>>().join(" "))
        .collect();
    let fits = |s: &str| (60..=200).contains(&s.chars().count());
    // The first sentence is the lede the reader just read; skip it.
    let candidates: Vec<&String> = sentences.iter().skip(1).filter(|s| fits(s)).collect();
    let quoted = |s: &str| {
        let t = s.trim();
        (t.starts_with('“') || t.starts_with('"'))
            && (t.contains('”') || t.matches('"').count() >= 2)
    };
    if let Some(q) = candidates.iter().find(|s| quoted(s)) {
        return Some((*q).clone());
    }
    let score = |s: &str| {
        let len = s.chars().count() as i64;
        let digits = s.chars().filter(char::is_ascii_digit).count() as i64;
        let starts_weak = [
            "But ", "And ", "So ", "It ", "This ", "That ", "He ", "She ", "They ",
        ]
        .iter()
        .any(|w| s.starts_with(w));
        -(len - 120).abs() - digits * 8 - if starts_weak { 40 } else { 0 }
    };
    candidates.into_iter().max_by_key(|s| score(s)).cloned()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn page(body: &str) -> String {
        let words = "The council met on Tuesday night to talk about the old rail yard and what it could become for the town. ".repeat(20);
        format!(
            "<html lang=\"en\"><head><title>Market</title></head><body><article><h1>Market</h1><p>{words}</p>{body}<p>{words}</p></article></body></html>"
        )
    }

    #[test]
    fn captions_credits_and_alt_text_ride_with_their_photos() {
        let html = page(concat!(
            "<figure><img src=\"https://cdn.example/a.jpg\" alt=\"A crowd under a steel roof\"/>",
            "<figcaption>Stall holders at the freight shed on Saturday. (Photo: Jane Doe / The Gazette)</figcaption></figure>",
            "<p>More words about the shed and the market, the stalls and the people who will run them.</p>",
            "<figure><img src=\"https://cdn.example/b.jpg\"/><figcaption>The yard in 1952.</figcaption></figure>",
        ));
        let x = extract(&html, "https://news.example/market").unwrap();
        let a = x.photos.iter().position(|p| p.ends_with("a.jpg")).unwrap();
        let b = x.photos.iter().position(|p| p.ends_with("b.jpg")).unwrap();
        assert_eq!(
            x.photo_meta[a],
            PhotoMeta {
                caption: Some("Stall holders at the freight shed on Saturday.".into()),
                credit: Some("Jane Doe / The Gazette".into()),
                alt: Some("A crowd under a steel roof".into()),
            }
        );
        assert_eq!(
            x.photo_meta[b].caption.as_deref(),
            Some("The yard in 1952.")
        );
        assert_eq!(x.photo_meta[b].credit, None);
        assert_eq!(x.photo_meta.len(), x.photos.len());
    }

    #[test]
    fn credits_split_off_in_their_usual_forms() {
        assert_eq!(
            split_credit("Crowds on Main Street. Photo by Sam Lee"),
            (
                Some("Crowds on Main Street.".into()),
                Some("Sam Lee".into())
            )
        );
        assert_eq!(
            split_credit("The mayor speaks on Friday. Ana Ruiz/AP"),
            (
                Some("The mayor speaks on Friday.".into()),
                Some("Ana Ruiz/AP".into())
            )
        );
        assert_eq!(
            split_credit("Credit: Getty Images"),
            (None, Some("Getty Images".into()))
        );
        assert_eq!(
            split_credit("Just a caption."),
            (Some("Just a caption.".into()), None)
        );
    }

    #[test]
    fn lint_drops_furniture_and_keeps_the_story() {
        let x = "<p>The vote was close.</p>\n<p>Advertisement</p>\n<p>Sign up for our daily newsletter to get the news first.</p>\n<p>Read more: Ten things about the yard</p>\n<p>She said the newsletter was late again.</p>\n";
        let (out, dropped) = lint(x);
        assert_eq!(
            out,
            "<p>The vote was close.</p>\n<p>She said the newsletter was late again.</p>\n"
        );
        assert_eq!(dropped.len(), 3);
        assert_eq!(dropped[0], "Advertisement");
    }

    #[test]
    fn lint_drops_a_newsletter_sign_up_and_its_tagline() {
        // The New York Post's inline module, as the writer leaves it (2026-10-26).
        let x = concat!(
            "<p>Calderón pleaded guilty on Monday.</p>\n",
            "<div class=\"figure\"><img src=\"__DECKLE_IMG_1__\" alt=\"\"/></div>\n",
            "   <h3 class=\"subhead\">Sign up for the California Morning Report newsletter</h3>\n",
            " <p>The biggest news, opinion and culture shaping California right now.</p>\n",
            "  <h3 class=\"subhead\">Thanks for signing up!</h3>\n",
            "<p>She faces up to five years.</p>\n",
        );
        let (out, dropped) = lint(x);
        assert_eq!(
            out,
            concat!(
                "<p>Calderón pleaded guilty on Monday.</p>\n",
                "<div class=\"figure\"><img src=\"__DECKLE_IMG_1__\" alt=\"\"/></div>\n",
                "      <p>She faces up to five years.</p>\n",
            )
        );
        assert_eq!(
            dropped,
            [
                "Sign up for the California Morning Report newsletter",
                "The biggest news, opinion and culture shaping California right now.",
                "Thanks for signing up!",
            ]
        );
        // As a bold paragraph, too.
        let (out, _) = lint(concat!(
            "<p><strong>Sign up for California Morning Report newsletter</strong></p>\n",
            "<p>The biggest news shaping California right now.</p>\n",
        ));
        assert_eq!(out, "");
    }

    #[test]
    fn lint_keeps_a_story_that_mentions_a_newsletter() {
        let x = concat!(
            "<p>Residents can sign up for the city's newsletter, the mayor said, but few have.</p>\n",
            "<p>\"Subscribe to the newsletter,\" she told the council, \"and you will know more than we do.\"</p>\n",
            "<h3 class=\"subhead\">The newsletter that broke the story</h3>\n",
            "<p>It was short.</p>\n",
        );
        let (out, dropped) = lint(x);
        assert_eq!(out, x);
        assert!(dropped.is_empty());
    }

    #[test]
    fn extracted_stories_get_curly_quotes_and_lose_boilerplate() {
        let html = page("<p>\"It is time,\" she said -- and it was.</p><p>Advertisement</p>");
        let x = extract(&html, "https://news.example/market").unwrap();
        assert!(
            x.xhtml
                .contains("\u{201c}It is time,\u{201d} she said \u{2014} and it was."),
            "{}",
            x.xhtml
        );
        assert!(!x.xhtml.contains("Advertisement"));
    }

    #[test]
    fn paywalls_and_pull_quotes() {
        assert!(looks_cut_off(
            "The council voted. Subscribe now to continue reading."
        ));
        assert!(looks_cut_off(
            "Part of it. This article is only available to subscribers."
        ));
        assert!(!looks_cut_off(
            "The council voted and the market opens in spring."
        ));
        let body = "The council met on Tuesday. ".repeat(10)
            + "\u{201c}We have waited forty years for somebody to do something with that yard,\u{201d} said one stall holder. "
            + &"More of the story follows here. ".repeat(150);
        let words = body.split_whitespace().count();
        assert_eq!(
            pull_quote(&body, words).as_deref(),
            Some(
                "\u{201c}We have waited forty years for somebody to do something with that yard,\u{201d} said one stall holder."
            )
        );
        assert_eq!(pull_quote(&body, 300), None, "short stories get none");
    }

    #[test]
    fn a_master_keeps_color_and_size_up_to_2400() {
        let mut png = Vec::new();
        image::DynamicImage::ImageRgb8(image::RgbImage::from_pixel(
            3000,
            1500,
            image::Rgb([200, 30, 30]),
        ))
        .write_to(&mut std::io::Cursor::new(&mut png), image::ImageFormat::Png)
        .unwrap();
        let m = image::load_from_memory(&master_photo(&png).unwrap()).unwrap();
        assert_eq!((m.width(), m.height()), (2400, 1200));
        assert_eq!(m.color(), image::ColorType::Rgb8);
        let mut small = Vec::new();
        image::DynamicImage::new_rgb8(800, 600)
            .write_to(
                &mut std::io::Cursor::new(&mut small),
                image::ImageFormat::Png,
            )
            .unwrap();
        let m = image::load_from_memory(&master_photo(&small).unwrap()).unwrap();
        assert_eq!((m.width(), m.height()), (800, 600), "never enlarged");
    }

    #[test]
    fn placing_drops_photos_not_kept_and_empty_figures() {
        let x = format!(
            "<div class=\"figure lead\"><img src=\"{}\" alt=\"\"/></div>\n<p>a<img src=\"{}\" alt=\"\"/></p>",
            placeholder(0),
            placeholder(1)
        );
        let out = place_photos(&x, &[false, true], |k| format!("images/x-{k}.jpg"));
        assert_eq!(out, "<p>a<img src=\"images/x-1.jpg\" alt=\"\"/></p>");
    }

    fn scene(w: u32, h: u32, shift: u32) -> Vec<u8> {
        let img = image::RgbImage::from_fn(w, h, |x, y| {
            let v = (((x + shift) * 7 / w.max(1) * 37 + y * 5 / h.max(1) * 53) % 256) as u8;
            image::Rgb([v, v / 2, 255 - v])
        });
        let mut png = Vec::new();
        image::DynamicImage::ImageRgb8(img)
            .write_to(&mut std::io::Cursor::new(&mut png), image::ImageFormat::Png)
            .unwrap();
        eink_photo(&png).unwrap()
    }

    #[test]
    fn the_same_photo_at_another_size_hashes_close_and_another_photo_does_not() {
        let a = photo_hash(&scene(1600, 900, 0)).unwrap();
        let resized = photo_hash(&scene(800, 450, 0)).unwrap();
        let other = photo_hash(&scene(1600, 900, 700)).unwrap();
        assert!((a ^ resized).count_ones() <= SAME_PHOTO_BITS);
        assert!((a ^ other).count_ones() > SAME_PHOTO_BITS);
    }

    #[test]
    fn tiny_images_are_dropped_and_photos_become_gray_jpegs() {
        let mut png = Vec::new();
        image::DynamicImage::new_rgb8(1600, 900)
            .write_to(&mut std::io::Cursor::new(&mut png), image::ImageFormat::Png)
            .unwrap();
        let jpeg = eink_photo(&png).unwrap();
        let back = image::load_from_memory(&jpeg).unwrap();
        assert_eq!((back.width(), back.height()), (1000, 563));
        assert_eq!(back.color(), image::ColorType::L8);
        let mut icon = Vec::new();
        image::DynamicImage::new_rgb8(64, 64)
            .write_to(
                &mut std::io::Cursor::new(&mut icon),
                image::ImageFormat::Png,
            )
            .unwrap();
        assert!(eink_photo(&icon).is_none());
    }

    #[test]
    fn a_teaser_is_the_opening_sentences_not_the_whole_story() {
        // A journal sets its whole article as one paragraph broken by <br/>.
        let sentence = "Historically, animated films have been used as propaganda in war, and some convey a pacifist view. ";
        let html = format!(
            "<html lang=\"en\"><head><title>Porco</title></head><body><article><h1>Porco</h1><p><strong>Introduction</strong><br/> {}</p></article></body></html>",
            sentence.repeat(60)
        );
        let x = extract(&html, "https://journal.example/article/66/").unwrap();
        let t = x.teaser.unwrap();
        assert!(
            t.chars().count() <= TEASER_MAX_CHARS,
            "{} chars: {t}",
            t.chars().count()
        );
        assert!(t.starts_with("Historically,"), "{t}");
        assert!(t.ends_with("view."), "{t}");
    }

    #[test]
    fn a_teaser_without_a_sentence_end_is_cut_at_a_word() {
        let run_on = "and the stall holders kept talking about the rail yard ".repeat(12);
        let t = teaser_of(&run_on).unwrap();
        assert!(t.chars().count() <= TEASER_MAX_CHARS + 1, "{t}");
        assert!(t.ends_with("…") && !t.ends_with(" …"), "{t}");
        let short =
            "The council met on Tuesday night to talk about the old rail yard and the market.";
        assert_eq!(teaser_of(short).as_deref(), Some(short));
        assert_eq!(teaser_of("Too short."), None);
        // An abbreviation before a number doesn't end a sentence.
        let cited = "Some animated films, however, have conveyed an anti-war pacifist point of view (Takai 2011). "
            .to_owned()
            + &"Then the film can be called anti-war propaganda (Okuda 2003, p. 144), "
                .repeat(4);
        let t = teaser_of(&cited).unwrap();
        assert!(t.ends_with("(Takai 2011)."), "{t}");
    }

    #[test]
    fn spaces_at_the_edge_of_emphasis_are_kept() {
        let html = page(concat!(
            "<p>John Collison uses <em>tenacity </em>and<em> passion</em> to describe something similar.</p>",
            "<p><strong>UNN </strong>reports this, citing <strong>MacRumors</strong>, the site.</p>",
        ));
        let x = extract(&html, "https://news.example/market").unwrap();
        assert!(
            x.xhtml
                .contains("uses <em>tenacity</em> and <em>passion</em> to describe"),
            "{}",
            x.xhtml
        );
        assert!(
            x.xhtml.contains(
                "<strong>UNN</strong> reports this, citing <strong>MacRumors</strong>, the site."
            ),
            "{}",
            x.xhtml
        );
        assert!(!x.xhtml.contains("  "), "no doubled spaces");
    }
}
