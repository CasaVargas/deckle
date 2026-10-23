// SPDX-License-Identifier: GPL-3.0-or-later
//! Full text and photos for a story (design §5a): resolve the publisher URL, extract the
//! article Reader-View style, rewrite it to a small XHTML whitelist, and fetch its photos as
//! e-ink JPEGs. Anything that fails leaves the story as the feed gave it.

use std::collections::HashSet;
use std::sync::LazyLock;

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

/// The placeholder `src` for photo `k` in [`Extracted::xhtml`]; the renderer swaps in a file
/// path, or drops the `<img>` when the photo was not kept.
pub fn placeholder(k: usize) -> String {
    format!("__DECKLE_IMG_{k}__")
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
    /// Photo 0 is the page's lead image, printed above the text.
    pub lead: bool,
    pub words: usize,
    /// The first paragraph that reads like prose, for the cover.
    pub teaser: Option<String>,
}

static GN_ATTR: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r#"data-n-a-(id|ts|sg)="([^"]+)""#).unwrap());
static GN_URL: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r#"garturlres\\",\\"(https?://[^\\"]+)\\""#).unwrap());

pub fn is_google_news(url: &str) -> bool {
    url.starts_with("https://news.google.com/rss/articles/")
        || url.starts_with("https://news.google.com/articles/")
}

/// The publisher URL behind a Google News link, via the interstitial page's signature and the
/// `batchexecute` decoder. Undocumented: `None` on any surprise.
pub fn resolve_google_news(url: &str, net: &dyn Net) -> Option<String> {
    let page = String::from_utf8_lossy(&net.get_page(url).ok()?).into_owned();
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
    let reply = net
        .post_form(
            "https://news.google.com/_/DotsSplashUi/data/batchexecute",
            &format!("f.req={}", encode(&req)),
        )
        .ok()?;
    let reply = String::from_utf8_lossy(&reply);
    GN_URL.captures(&reply).map(|c| c[1].to_owned())
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
    fn photo(&mut self, url: String) -> Option<usize> {
        if !self.seen.insert(photo_key(&url)) {
            return None;
        }
        self.photos.push(url);
        Some(self.photos.len() - 1)
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
                    self.out.push(' ');
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
            self.out
                .push_str(&format!("{lead}{}{trail}", esc(&collapsed)));
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
                let prose = text.trim();
                if self.teaser.is_none() && prose.len() >= 80 {
                    self.teaser = Some(prose.to_owned());
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
            "em" | "i" => self.wrapped(node, "<em>", "</em>"),
            "strong" | "b" => self.wrapped(node, "<strong>", "</strong>"),
            "figcaption" => self.wrapped(node, "<span class=\"caption\">", "</span>"),
            "figure" => self.wrapped(node, "<div class=\"figure\">", "</div>\n"),
            "br" => {
                self.out.push_str("<br/>");
                String::new()
            }
            "img" => {
                if let Some(k) = self.img_src(node).and_then(|u| self.photo(u)) {
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
    let mut readability = dom_smoothie::Readability::new(html, Some(url), None).ok()?;
    let article = readability.parse().ok()?;
    let mut lead_kept = false;
    let mut w = Writer {
        base: url.to_owned(),
        out: String::new(),
        photos: Vec::new(),
        seen: HashSet::new(),
        words: 0,
        max_words,
        teaser: None,
    };
    // The lead image goes first, above the text.
    if let Some(lead) = article.image.as_deref().and_then(|i| resolve(url, i))
        && let Some(k) = w.photo(lead)
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
    Some(Extracted {
        title: (!title.is_empty()).then_some(title),
        byline: article
            .byline
            .map(|b| b.to_string())
            .map(|b| b.trim().to_owned())
            .filter(|b| !b.is_empty() && b.len() < 120),
        xhtml: w.out,
        photos: w.photos,
        lead: lead_kept,
        words: w.words,
        teaser: w.teaser,
    })
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
    let mut hashes: Vec<u64> = Vec::new();
    photos
        .iter()
        .enumerate()
        .map(|(k, url)| {
            if hashes.len() >= max {
                return None;
            }
            let jpeg = load_photo(url, net).and_then(|b| photo(&b, as_))?;
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
            Some(jpeg)
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

#[cfg(test)]
mod tests {
    use super::*;

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
}
