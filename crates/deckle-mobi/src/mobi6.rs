// SPDX-License-Identifier: GPL-3.0-or-later
//! An older DRM-free MOBI (version 6 and earlier, no KF8 part) or a plain PalmDOC file back into
//! an EPUB (MOBI import design §2). The text is one stream of Mobipocket HTML: `filepos` links
//! become anchors and links between parts, `recindex` pictures become files, page breaks split
//! the parts, and each part is parsed like a browser would (html5ever) and written out as a small
//! whitelist of XHTML. The NCX index, else the headings, gives the contents.

use std::collections::{BTreeMap, BTreeSet, HashSet};
use std::sync::LazyLock;

use regex::bytes::Regex;

use crate::MobiError;
use crate::epub::{Book, TocEntry};
use crate::kf8::{NONE, Rebuilt, exth, fnv, index_at, resources, text_stream};
use crate::palmdb::{PalmDb, be32};

/// A part with no page breaks to split it is cut at about this size.
const SPLIT_BYTES: usize = 200_000;
/// A link to another anchor is written as OPEN id CLOSE until the parts exist.
const OPEN: char = '\u{E000}';
const CLOSE: char = '\u{E001}';

fn esc(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}

/// An XML ID: letters, digits, `-`, `_` and `.`, starting with a letter or `_`.
fn sanitize(raw: &str) -> String {
    let mut id: String = raw
        .chars()
        .map(|c| {
            if c.is_alphanumeric() || matches!(c, '-' | '_' | '.') {
                c
            } else {
                '_'
            }
        })
        .collect();
    if !id.starts_with(|c: char| c.is_alphabetic() || c == '_') {
        id = format!("id-{id}");
    }
    id
}

/// Inserts an empty anchor `filepos{N}` at each byte position N, moving one that falls inside a
/// tag to the tag's start.
fn insert_anchors(raw: &[u8], targets: &BTreeSet<usize>) -> Vec<u8> {
    let mut out = raw.to_vec();
    for &pos in targets.iter().rev() {
        if pos > raw.len() {
            continue;
        }
        let mut at = pos;
        let lt = raw[..pos].iter().rposition(|&b| b == b'<');
        let gt = raw[..pos].iter().rposition(|&b| b == b'>');
        if let Some(l) = lt
            && gt.is_none_or(|g| g < l)
        {
            at = l;
        }
        let anchor = format!("<a id=\"filepos{pos}\"></a>");
        out.splice(at..at, anchor.bytes());
    }
    out
}

/// What the whitelist writer needs and collects across parts.
struct Writer<'a> {
    images: &'a BTreeMap<u64, (String, &'static str)>,
    used_ids: HashSet<String>,
    headings: Vec<(usize, String, String)>,
    generated: usize,
    warnings: Vec<String>,
    out: String,
    text: String,
    has_image: bool,
}

/// Elements written as themselves (or as the given XHTML element).
fn mapped(name: &str) -> Option<&'static str> {
    Some(match name {
        "p" => "p",
        "div" => "div",
        "blockquote" => "blockquote",
        "h1" => "h1",
        "h2" => "h2",
        "h3" => "h3",
        "h4" => "h4",
        "h5" => "h5",
        "h6" => "h6",
        "ul" => "ul",
        "ol" => "ol",
        "li" => "li",
        "dl" => "dl",
        "dt" => "dt",
        "dd" => "dd",
        "table" => "table",
        "tbody" => "tbody",
        "thead" => "thead",
        "tr" => "tr",
        "td" => "td",
        "th" => "th",
        "b" | "strong" => "b",
        "i" | "em" | "cite" | "var" => "i",
        "u" | "ins" => "u",
        "s" | "strike" | "del" => "s",
        "sub" => "sub",
        "sup" => "sup",
        "small" => "small",
        "big" => "big",
        "code" | "tt" | "kbd" | "samp" => "code",
        "pre" => "pre",
        _ => return None,
    })
}

const DROP: &[&str] = &[
    "script",
    "style",
    "head",
    "guide",
    "title",
    "meta",
    "link",
    "mbp:frameset",
    "object",
    "embed",
    "audio",
    "video",
];

impl Writer<'_> {
    fn warn(&mut self, w: String) {
        if !self.warnings.contains(&w) {
            self.warnings.push(w);
        }
    }

    fn unique(&mut self, base: &str) -> String {
        let mut id = base.to_owned();
        let mut n = 1;
        while self.used_ids.contains(&id) {
            n += 1;
            id = format!("{base}-{n}");
        }
        self.used_ids.insert(id.clone());
        id
    }

    /// The element's `id` (or an `<a name>`), sanitised and unique.
    fn id_of(&mut self, node: &dom_query::NodeRef, name: &str) -> Option<String> {
        let raw = node
            .attr("id")
            .or_else(|| (name == "a").then(|| node.attr("name")).flatten())?;
        let raw = raw.trim().to_owned();
        if raw.is_empty() {
            return None;
        }
        // Inserted anchors are already unique and safe.
        if raw.starts_with("filepos") && !self.used_ids.contains(&raw) {
            self.used_ids.insert(raw.clone());
            return Some(raw);
        }
        Some(self.unique(&sanitize(&raw)))
    }

    fn children(&mut self, node: &dom_query::NodeRef) {
        for c in node.children() {
            self.node(&c);
        }
    }

    fn node(&mut self, node: &dom_query::NodeRef) {
        if node.is_text() {
            let t = node.text().to_string();
            let collapsed = t.split_whitespace().collect::<Vec<_>>().join(" ");
            if collapsed.is_empty() {
                if !t.is_empty() && !self.out.ends_with(' ') && !self.out.ends_with('>') {
                    self.out.push(' ');
                }
                return;
            }
            let lead = if t.starts_with(char::is_whitespace) && !self.out.ends_with(' ') {
                " "
            } else {
                ""
            };
            let trail = if t.ends_with(char::is_whitespace) {
                " "
            } else {
                ""
            };
            self.out
                .push_str(&format!("{lead}{}{trail}", esc(&collapsed)));
            self.text.push_str(&format!("{lead}{collapsed}{trail}"));
            return;
        }
        if !node.is_element() {
            return;
        }
        let name = node
            .node_name()
            .map(|n| n.to_ascii_lowercase())
            .unwrap_or_default();
        match name.as_str() {
            n if DROP.contains(&n) => {}
            "br" => self.out.push_str("<br/>"),
            "hr" => self.out.push_str("<hr/>"),
            "img" => self.img(node),
            "a" => self.link(node),
            "center" => self.element(node, "div", Some("center")),
            "html" | "body" | "font" | "span" | "mbp:nu" | "mbp:section" | "section"
            | "article" | "nobr" | "abbr" | "acronym" | "address" | "q" | "label" | "caption"
            | "figure" | "figcaption" => self.children(node),
            n => match mapped(n) {
                Some(tag) => self.element(node, tag, None),
                None => {
                    if !n.starts_with("mbp:") {
                        self.warn(format!("unsupported MOBI element <{n}>; kept its text"));
                    }
                    self.children(node);
                }
            },
        }
    }

    fn element(&mut self, node: &dom_query::NodeRef, tag: &str, class: Option<&str>) {
        let mut attrs = String::new();
        let mut id = self.id_of(node, tag);
        let class = class.or_else(|| {
            match node
                .attr("align")
                .map(|a| a.to_ascii_lowercase())
                .as_deref()
            {
                Some("center") => Some("center"),
                Some("right") => Some("right"),
                _ => None,
            }
        });
        if let Some(c) = class {
            attrs.push_str(&format!(" class=\"{c}\""));
        }
        for a in ["colspan", "rowspan", "start"] {
            if matches!(tag, "td" | "th" | "ol")
                && let Some(v) = node.attr(a)
                && v.trim().parse::<u32>().is_ok()
            {
                attrs.push_str(&format!(" {a}=\"{}\"", v.trim()));
            }
        }
        let level = tag
            .strip_prefix('h')
            .and_then(|n| n.parse::<usize>().ok())
            .filter(|n| (1..=6).contains(n));
        if level.is_some_and(|l| l <= 3) && id.is_none() {
            self.generated += 1;
            let g = format!("h{}", self.generated);
            id = Some(self.unique(&g));
        }
        if let Some(id) = &id {
            attrs.push_str(&format!(" id=\"{id}\""));
        }
        let text_at = self.text.len();
        self.out.push_str(&format!("<{tag}{attrs}>"));
        self.children(node);
        self.out.push_str(&format!("</{tag}>"));
        if let (Some(l), Some(id)) = (level, id) {
            let label = self.text[text_at..]
                .split_whitespace()
                .collect::<Vec<_>>()
                .join(" ");
            if l <= 3 && !label.is_empty() {
                self.headings.push((l, label, id));
            }
        }
    }

    fn link(&mut self, node: &dom_query::NodeRef) {
        let id = self.id_of(node, "a");
        let href = node.attr("href").map(|h| h.trim().to_owned());
        let mut attrs = String::new();
        match href.as_deref() {
            Some(h) if h.starts_with('#') && h.len() > 1 => {
                attrs.push_str(&format!(" href=\"{OPEN}{}{CLOSE}\"", &h[1..]));
            }
            Some(h)
                if ["http://", "https://", "mailto:"]
                    .iter()
                    .any(|s| h.to_ascii_lowercase().starts_with(s)) =>
            {
                attrs.push_str(&format!(" href=\"{}\"", esc(h)));
            }
            _ => {}
        }
        if let Some(id) = &id {
            attrs.push_str(&format!(" id=\"{id}\""));
        }
        if attrs.is_empty() {
            self.children(node);
            return;
        }
        self.out.push_str(&format!("<a{attrs}>"));
        self.children(node);
        self.out.push_str("</a>");
    }

    fn img(&mut self, node: &dom_query::NodeRef) {
        let src = node.attr("src").map(|s| s.to_string()).unwrap_or_default();
        let Some(n) = src
            .strip_prefix("deckle-recindex:")
            .and_then(|n| n.parse::<u64>().ok())
        else {
            self.warn(format!("picture {src} isn't in the book; left out"));
            return;
        };
        let Some((path, _)) = self.images.get(&n) else {
            self.warn(format!("picture record {n} is missing; left out"));
            return;
        };
        let alt = node
            .attr("alt")
            .map(|a| a.split_whitespace().collect::<Vec<_>>().join(" "))
            .unwrap_or_default();
        self.has_image = true;
        self.out
            .push_str(&format!("<img src=\"{path}\" alt=\"{}\"/>", esc(&alt)));
    }

    /// One piece of the text as XHTML body content; `None` when it holds nothing to read.
    fn piece(&mut self, html: &str) -> (String, bool) {
        self.out.clear();
        self.text.clear();
        self.has_image = false;
        let doc = dom_query::Document::from(format!("<html><body>{html}</body></html>"));
        let body = doc.select("body");
        if let Some(b) = body.nodes().first() {
            self.children(b);
        }
        let readable = !self.text.trim().is_empty() || self.has_image;
        (std::mem::take(&mut self.out), readable)
    }
}

/// Cuts `html` at about [`SPLIT_BYTES`], after a closing `</p>` or `</div>`.
fn split_large(html: &str) -> Vec<String> {
    if html.len() <= SPLIT_BYTES {
        return vec![html.to_owned()];
    }
    let mut out = Vec::new();
    let mut rest = html;
    while rest.len() > SPLIT_BYTES {
        let window = &rest[SPLIT_BYTES.min(rest.len())..];
        let lower = window.to_ascii_lowercase();
        let cut = ["</p>", "</div>"]
            .iter()
            .filter_map(|t| lower.find(t).map(|i| i + t.len()))
            .min();
        let Some(cut) = cut else { break };
        let at = SPLIT_BYTES + cut;
        out.push(rest[..at].to_owned());
        rest = &rest[at..];
    }
    out.push(rest.to_owned());
    out
}

fn document(title: &str, lang: &str, body: &str) -> String {
    format!(
        "<?xml version=\"1.0\" encoding=\"utf-8\"?>\n<html xmlns=\"http://www.w3.org/1999/xhtml\" \
         xml:lang=\"{lang}\" lang=\"{lang}\"><head><title>{}</title></head><body>{body}</body></html>\n",
        esc(title),
        lang = esc(lang)
    )
}

/// A plain PalmDOC text as paragraphs: blank lines separate them, single breaks are `<br/>`.
fn plain_paragraphs(text: &str) -> String {
    let text = text.replace("\r\n", "\n").replace('\r', "\n");
    let mut out = String::new();
    for block in text.split("\n\n") {
        let lines: Vec<String> = block
            .lines()
            .map(|l| esc(l.trim()))
            .filter(|l| !l.is_empty())
            .collect();
        if !lines.is_empty() {
            out.push_str(&format!("<p>{}</p>", lines.join("<br/>")));
        }
    }
    out
}

/// Rebuilds an older MOBI or a PalmDOC file in `bytes` as an EPUB at `out`. `fallback_title`
/// names a book that has no title of its own.
pub fn rebuild(
    bytes: &[u8],
    out: &std::path::Path,
    fallback_title: &str,
) -> Result<Rebuilt, MobiError> {
    let db = PalmDb::parse(bytes)?;
    let meta = crate::read_meta(bytes)?;
    if meta.drm {
        return Err(MobiError::Drm);
    }
    let rec0 = db.record(0);
    let mobi = rec0.get(16..20) == Some(b"MOBI");
    let raw = text_stream(&db, 0, rec0)?;
    let utf8 = mobi && be32(rec0, 0x1C) == Some(65001);
    let decode = |b: &[u8]| -> String {
        if utf8 {
            String::from_utf8_lossy(b).into_owned()
        } else {
            encoding_rs::WINDOWS_1252.decode(b).0.into_owned()
        }
    };
    let ex = if mobi { exth(rec0) } else { Vec::new() };
    let get = |k: u32| {
        ex.iter()
            .filter(move |(kind, _)| *kind == k)
            .map(|(_, d)| String::from_utf8_lossy(d).trim().to_owned())
            .filter(|s| !s.is_empty())
    };
    let palm_name = {
        let n = &bytes[..32.min(bytes.len())];
        let end = n.iter().position(|&b| b == 0).unwrap_or(n.len());
        decode(&n[..end]).trim().to_owned()
    };
    let title = get(503)
        .next()
        .or(meta.title.clone())
        .or_else(|| (!palm_name.is_empty() && !mobi).then(|| palm_name.clone()))
        .unwrap_or_else(|| fallback_title.to_owned());
    let language = get(524).next().unwrap_or_else(|| "en".into());
    let res = if mobi {
        resources(&db, 0, rec0)
    } else {
        resources(&db, 0, &[])
    };
    let mut warnings = Vec::new();

    // The NCX index, when there is one: (position, label, level).
    let mut ncx = Vec::new();
    if mobi && let Some(index) = index_at(&db, 0, rec0, 0xF4)? {
        for e in &index.entries {
            let Some(pos) = e.tag(1, 0) else { continue };
            let label = e
                .tag(3, 0)
                .and_then(|o| index.ctoc.get(&o))
                .map(|t| decode(t).trim().to_owned())
                .unwrap_or_default();
            ncx.push((pos as usize, label, e.tag(4, 0).unwrap_or(0) as usize));
        }
    }

    let pieces: Vec<String> = if mobi {
        static FILEPOS: LazyLock<Regex> =
            LazyLock::new(|| Regex::new(r#"(?i)filepos\s*=\s*["']?0*([0-9]+)["']?"#).unwrap());
        static RECINDEX: LazyLock<Regex> =
            LazyLock::new(|| Regex::new(r#"(?i)recindex\s*=\s*["']?0*([0-9]+)["']?"#).unwrap());
        static BREAK: LazyLock<regex::Regex> =
            LazyLock::new(|| regex::Regex::new(r"(?i)<mbp:pagebreak[^>]*>").unwrap());
        static BODY: LazyLock<regex::Regex> =
            LazyLock::new(|| regex::Regex::new(r"(?i)<body[^>]*>").unwrap());
        let mut targets: BTreeSet<usize> = FILEPOS
            .captures_iter(&raw)
            .filter_map(|c| std::str::from_utf8(&c[1]).ok()?.parse().ok())
            .collect();
        targets.extend(ncx.iter().map(|(p, _, _)| *p));
        let anchored = insert_anchors(&raw, &targets);
        let linked = FILEPOS.replace_all(&anchored, &b"href=\"#filepos$1\""[..]);
        let pictured = RECINDEX.replace_all(&linked, &b"src=\"deckle-recindex:$1\""[..]);
        let html = decode(&pictured);
        let body = match BODY.find(&html) {
            Some(m) => &html[m.end()..],
            None => html.as_str(),
        };
        let body = match body.to_ascii_lowercase().rfind("</body>") {
            Some(e) => &body[..e],
            None => body,
        };
        let split: Vec<&str> = BREAK.split(body).collect();
        if split.len() == 1 {
            split_large(split[0])
        } else {
            split.into_iter().map(str::to_owned).collect()
        }
    } else {
        split_large(&plain_paragraphs(&decode(&raw)))
    };

    let mut w = Writer {
        images: &res.files,
        used_ids: HashSet::new(),
        headings: Vec::new(),
        generated: 0,
        warnings: Vec::new(),
        out: String::new(),
        text: String::new(),
        has_image: false,
    };
    // Parts: (file name, first heading or None, body). A piece with nothing to read carries its
    // markup (its anchors) into the next one.
    let mut bodies: Vec<String> = Vec::new();
    let mut carry = String::new();
    for piece in &pieces {
        let (xhtml, readable) = w.piece(piece);
        carry.push_str(&xhtml);
        if readable {
            bodies.push(std::mem::take(&mut carry));
        }
    }
    if !carry.is_empty() {
        match bodies.last_mut() {
            Some(last) => last.push_str(&carry),
            None => bodies.push(carry),
        }
    }
    warnings.append(&mut w.warnings);
    if bodies.is_empty() {
        return Err(MobiError::Corrupt("the book has no text"));
    }
    let names: Vec<String> = (0..bodies.len())
        .map(|i| format!("part{i:04}.xhtml"))
        .collect();

    // Where each id landed, then links resolved to it.
    let mut placed: BTreeMap<String, usize> = BTreeMap::new();
    for (i, b) in bodies.iter().enumerate() {
        let mut rest = b.as_str();
        while let Some(at) = rest.find(" id=\"") {
            rest = &rest[at + 5..];
            if let Some(end) = rest.find('"') {
                placed.entry(rest[..end].to_owned()).or_insert(i);
            }
        }
    }
    let mut missing = BTreeSet::new();
    for b in &mut bodies {
        if !b.contains(OPEN) {
            continue;
        }
        let mut out = String::with_capacity(b.len());
        let mut rest = b.as_str();
        while let Some(start) = rest.find(OPEN) {
            out.push_str(&rest[..start]);
            let after = &rest[start + OPEN.len_utf8()..];
            let end = after.find(CLOSE).expect("placeholders are closed");
            let id = &after[..end];
            match placed.get(id) {
                Some(&p) => out.push_str(&format!("{}#{id}", names[p])),
                None => {
                    missing.insert(id.to_owned());
                    out.push('#');
                }
            }
            rest = &after[end + CLOSE.len_utf8()..];
        }
        out.push_str(rest);
        *b = out;
    }
    for id in missing {
        warnings.push(format!("a link to {id} goes nowhere in this book"));
    }

    // Contents: the NCX, else the headings, else the first part.
    let href = |id: &str| placed.get(id).map(|&p| format!("Text/{}#{id}", names[p]));
    let mut toc: Vec<TocEntry> = ncx
        .iter()
        .filter_map(|(pos, label, level)| {
            Some(TocEntry {
                level: *level,
                title: label.clone(),
                href: href(&format!("filepos{pos}"))?,
            })
        })
        .filter(|e| !e.title.is_empty())
        .collect();
    if toc.is_empty() {
        toc = w
            .headings
            .iter()
            .filter_map(|(level, label, id)| {
                Some(TocEntry {
                    level: level - 1,
                    title: label.clone(),
                    href: href(id)?,
                })
            })
            .collect();
    }
    if toc.is_empty() {
        toc.push(TocEntry {
            level: 0,
            title: title.clone(),
            href: format!("Text/{}", names[0]),
        });
    }

    // Every part must be well-formed XML.
    let mut parts = Vec::new();
    for (name, body) in names.iter().zip(&bodies) {
        let doc = document(&title, &language, body);
        if let Err(e) = roxmltree::Document::parse(&doc) {
            return Err(MobiError::BadPart(name.clone(), e.to_string()));
        }
        parts.push((format!("Text/{name}"), doc));
    }

    let cover = ex
        .iter()
        .find(|(k, _)| *k == 201)
        .and_then(|(_, d)| be32(d, 0))
        .filter(|o| *o != NONE)
        .and_then(|o| res.files.get(&(u64::from(o) + 1)))
        .map(|(p, _)| p.trim_start_matches("../").to_owned());
    let images = res
        .list
        .iter()
        .filter(|r| r.media_type.starts_with("image/"))
        .count();
    let book = Book {
        title: title.clone(),
        authors: get(100).collect(),
        language,
        identifier: meta
            .asin
            .clone()
            .map(|a| format!("urn:asin:{a}"))
            .unwrap_or_else(|| format!("urn:deckle:mobi:{:016x}", fnv(bytes))),
        publisher: get(101).next(),
        description: get(103).next(),
        isbn: get(104).next(),
        subjects: get(105).collect(),
        date: get(106).next(),
        cover,
        parts,
        resources: res.list,
        toc,
    };
    crate::epub::write(out, &book)?;
    Ok(Rebuilt {
        title,
        parts: book.parts.len(),
        images,
        warnings,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn anchors_go_at_their_byte_or_before_the_tag_they_fall_in() {
        let raw = b"<p>ab</p><p class=x>cd</p>";
        let targets = BTreeSet::from([3, 12]);
        let out = insert_anchors(raw, &targets);
        assert_eq!(
            String::from_utf8(out).unwrap(),
            "<p><a id=\"filepos3\"></a>ab</p><a id=\"filepos12\"></a><p class=x>cd</p>"
        );
    }

    #[test]
    fn plain_text_becomes_paragraphs() {
        assert_eq!(
            plain_paragraphs("One\nline\n\nTwo & <three>\r\n"),
            "<p>One<br/>line</p><p>Two &amp; &lt;three&gt;</p>"
        );
    }

    #[test]
    fn large_pieces_split_after_a_paragraph() {
        let p = "<p>xxxxxxxxxx</p>".repeat(30_000);
        let parts = split_large(&p);
        assert!(parts.len() >= 2);
        assert!(parts.iter().all(|s| s.ends_with("</p>")));
        assert_eq!(parts.concat(), p);
    }
}
