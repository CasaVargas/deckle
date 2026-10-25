// SPDX-License-Identifier: GPL-3.0-or-later
//! `office:text` to XHTML blocks: headings, paragraphs, list items, tables, images and notes
//! (ODT spec §2).

use std::collections::{HashMap, HashSet};

use base64::Engine;
use roxmltree::Node;

use crate::styles::{Fmt, Styles, attr, child, local};

/// An image larger than this is skipped with a warning.
const MAX_IMAGE: usize = 10 * 1024 * 1024;
/// A bookmark link's target is written as OPEN id CLOSE and resolved once the chapters exist.
pub const OPEN: char = '\u{E000}';
pub const CLOSE: char = '\u{E001}';

#[derive(Clone, Debug)]
pub enum Block {
    Heading {
        level: u8,
        id: String,
        html: String,
        text: String,
    },
    /// A finished element (`<p>`, `<blockquote>`, `<table>`).
    Html(String),
    Item {
        ordered: bool,
        start: u32,
        level: u8,
        html: String,
    },
    /// Notes referenced by the blocks just before, by index into [`Body::notes`].
    Notes(Vec<usize>),
}

pub struct Image {
    /// Path in the EPUB relative to the content documents, e.g. `images/image-001.png`.
    pub href: String,
    pub media_type: &'static str,
    pub bytes: Vec<u8>,
}

pub fn escape(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for c in s.chars() {
        match c {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            c => out.push(c),
        }
    }
    out
}

/// Inline HTML under construction; adjacent text with the same formatting shares its tags.
#[derive(Default)]
struct Inline {
    html: String,
    pending: Option<(Fmt, String)>,
    text: String,
    has_image: bool,
}

impl Inline {
    fn text(&mut self, f: Fmt, s: &str) {
        if s.is_empty() {
            return;
        }
        self.text.push_str(s);
        match &mut self.pending {
            Some((pf, t)) if *pf == f => t.push_str(&escape(s)),
            _ => {
                self.flush();
                self.pending = Some((f, escape(s)));
            }
        }
    }

    /// Already-escaped text, e.g. `&#160;`, in formatting `f`.
    fn entity(&mut self, f: Fmt, html: &str, plain: &str) {
        self.text.push_str(plain);
        match &mut self.pending {
            Some((pf, t)) if *pf == f => t.push_str(html),
            _ => {
                self.flush();
                self.pending = Some((f, html.to_owned()));
            }
        }
    }

    fn raw(&mut self, s: &str) {
        self.flush();
        self.html.push_str(s);
    }

    fn flush(&mut self) {
        let Some((f, t)) = self.pending.take() else {
            return;
        };
        let tags: Vec<(&str, &str)> = [
            (f.bold, ("<b>", "</b>")),
            (f.italic, ("<i>", "</i>")),
            (f.underline, ("<u>", "</u>")),
            (f.strike, ("<s>", "</s>")),
            (f.small_caps, ("<span class=\"sc\">", "</span>")),
            (f.sup, ("<sup>", "</sup>")),
            (f.sub && !f.sup, ("<sub>", "</sub>")),
        ]
        .into_iter()
        .filter_map(|(on, t)| on.then_some(t))
        .collect();
        for (open, _) in &tags {
            self.html.push_str(open);
        }
        self.html.push_str(&t);
        for (_, close) in tags.iter().rev() {
            self.html.push_str(close);
        }
    }

    fn finish(mut self) -> (String, String, bool) {
        self.flush();
        (
            self.html.trim().to_owned(),
            self.text.split_whitespace().collect::<Vec<_>>().join(" "),
            self.has_image,
        )
    }
}

/// Field and metadata elements whose displayed text is kept without a warning.
const FIELDS: &[&str] = &[
    "page-number",
    "page-count",
    "page-continuation",
    "date",
    "time",
    "title",
    "subject",
    "description",
    "keywords",
    "author-name",
    "author-initials",
    "initial-creator",
    "creator",
    "creation-date",
    "creation-time",
    "modification-date",
    "modification-time",
    "print-date",
    "editing-cycles",
    "editing-duration",
    "chapter",
    "sequence",
    "sequence-ref",
    "bookmark-ref",
    "reference-ref",
    "note-ref",
    "file-name",
    "template-name",
    "sender-firstname",
    "sender-lastname",
    "user-defined",
    "variable-get",
    "variable-set",
    "variable-input",
    "user-field-get",
    "user-field-input",
    "expression",
    "text-input",
    "placeholder",
    "conditional-text",
    "word-count",
    "character-count",
    "paragraph-count",
    "image-count",
    "table-count",
    "object-count",
    "measure",
    "meta",
    "meta-field",
    "database-display",
    "database-name",
    "drop-down",
];

/// Inline markers with nothing to show.
const SILENT_INLINE: &[&str] = &[
    "soft-page-break",
    "bookmark-end",
    "reference-mark-end",
    "annotation",
    "annotation-end",
    "change",
    "change-start",
    "change-end",
    "hidden-text",
    "toc-mark",
    "toc-mark-start",
    "toc-mark-end",
    "alphabetical-index-mark",
    "alphabetical-index-mark-start",
    "alphabetical-index-mark-end",
    "user-index-mark",
    "user-index-mark-start",
    "user-index-mark-end",
];

/// Block-level elements dropped on purpose: generated indexes, deleted text, declarations.
const SILENT_BLOCK: &[&str] = &[
    "table-of-content",
    "alphabetical-index",
    "illustration-index",
    "table-index",
    "object-index",
    "user-index",
    "bibliography",
    "tracked-changes",
    "sequence-decls",
    "variable-decls",
    "user-field-decls",
    "dde-connection-decls",
    "forms",
    "soft-page-break",
    "annotation",
    "annotation-end",
    "hidden-paragraph",
    "change",
    "change-start",
    "change-end",
    "bookmark-end",
    "table-columns",
    "table-column",
    "table-column-group",
];

pub struct Body<'s> {
    pub styles: &'s Styles,
    /// Package files by path (`Pictures/1.png`); empty for flat ODF.
    pictures: &'s HashMap<String, Vec<u8>>,
    /// Bookmark name → XML-safe, unique id.
    bookmarks: HashMap<String, String>,
    used: HashSet<String>,
    headings: usize,
    image_index: HashMap<String, Option<usize>>,
    pub images: Vec<Image>,
    pub notes: Vec<String>,
    footnotes: usize,
    endnotes: usize,
    pending_notes: Vec<usize>,
    pub title: Option<String>,
    pub warnings: Vec<String>,
}

impl<'s> Body<'s> {
    pub fn new(styles: &'s Styles, pictures: &'s HashMap<String, Vec<u8>>, text: Node) -> Self {
        let mut b = Body {
            styles,
            pictures,
            bookmarks: HashMap::new(),
            used: HashSet::new(),
            headings: 0,
            image_index: HashMap::new(),
            images: Vec::new(),
            notes: Vec::new(),
            footnotes: 0,
            endnotes: 0,
            pending_notes: Vec::new(),
            title: None,
            warnings: Vec::new(),
        };
        for d in text.descendants().filter(|d| d.is_element()) {
            if matches!(
                local(d),
                "bookmark" | "bookmark-start" | "reference-mark" | "reference-mark-start"
            ) && let Some(name) = attr(d, "name")
                && !b.bookmarks.contains_key(name)
            {
                let id = b.unique(&sanitize(name));
                b.bookmarks.insert(name.to_owned(), id);
            }
        }
        b
    }

    fn warn(&mut self, w: String) {
        if !self.warnings.contains(&w) {
            self.warnings.push(w);
        }
    }

    fn unique(&mut self, base: &str) -> String {
        let mut id = base.to_owned();
        let mut n = 1;
        while self.used.contains(&id) {
            n += 1;
            id = format!("{base}-{n}");
        }
        self.used.insert(id.clone());
        id
    }

    /// The document body as blocks, with note markers after the block that references them.
    pub fn body(&mut self, text: Node) -> Vec<Block> {
        let mut out = Vec::new();
        self.blocks(text, 0, None, &mut out, true);
        out
    }

    fn flush_notes(&mut self, out: &mut Vec<Block>) {
        if !self.pending_notes.is_empty() {
            out.push(Block::Notes(std::mem::take(&mut self.pending_notes)));
        }
    }

    /// Block children of `n`. `top` places notes after each block (the document body), not
    /// inside a table cell or note.
    fn blocks(
        &mut self,
        n: Node,
        list_level: u8,
        list_style: Option<&str>,
        out: &mut Vec<Block>,
        top: bool,
    ) {
        for c in n.children().filter(|c| c.is_element()) {
            self.block(c, list_level, list_style, out);
            if top {
                self.flush_notes(out);
            }
        }
    }

    fn block(&mut self, c: Node, list_level: u8, list_style: Option<&str>, out: &mut Vec<Block>) {
        match local(c) {
            "h" => {
                let level = attr(c, "outline-level")
                    .and_then(|v| v.parse::<u8>().ok())
                    .or_else(|| self.styles.paragraph(attr(c, "style-name")).outline)
                    .unwrap_or(1)
                    .clamp(1, 6);
                self.heading(c, level, out);
            }
            "p" => {
                let info = self.styles.paragraph(attr(c, "style-name"));
                match info.outline {
                    Some(level) if level > 0 => self.heading(c, level.min(6), out),
                    _ => self.paragraph(c, out),
                }
            }
            "list" => {
                let style = attr(c, "style-name")
                    .map(str::to_owned)
                    .or_else(|| list_style.map(str::to_owned));
                self.list(c, list_level, style.as_deref(), out);
            }
            "table" => {
                let html = self.table(c);
                out.push(Block::Html(html));
            }
            "section" | "index-body" | "numbered-paragraph" => {
                self.blocks(c, list_level, list_style, out, false);
            }
            "frame" | "a" => {
                let mut inline = Inline::default();
                self.inline_node(c, Fmt::default(), &mut inline);
                let (html, _, _) = inline.finish();
                if !html.is_empty() {
                    out.push(Block::Html(format!("<p class=\"figure\">{html}</p>")));
                }
            }
            name if SILENT_BLOCK.contains(&name) => {}
            other => {
                self.warn(format!(
                    "unsupported OpenDocument element <{other}>; kept its text"
                ));
                if c.children().any(|g| g.is_element()) {
                    self.blocks(c, list_level, list_style, out, false);
                } else {
                    self.paragraph(c, out);
                }
            }
        }
    }

    fn heading(&mut self, c: Node, level: u8, out: &mut Vec<Block>) {
        let info = self.styles.paragraph(attr(c, "style-name"));
        // A heading's own bold isn't repeated inside it.
        let base = Fmt {
            bold: false,
            ..info.fmt
        };
        let mut inline = Inline::default();
        self.inlines(c, base, &mut inline);
        let (html, text, _) = inline.finish();
        if text.is_empty() && html.is_empty() {
            return;
        }
        self.headings += 1;
        let id = self.unique(&format!("h{}", self.headings));
        out.push(Block::Heading {
            level,
            html: format!("<h{level} id=\"{id}\">{html}</h{level}>"),
            id,
            text,
        });
    }

    fn paragraph(&mut self, c: Node, out: &mut Vec<Block>) {
        if let Some(html) = self.paragraph_html(c) {
            out.push(Block::Html(html));
        }
    }

    /// A `<p>` with its style classes, `<blockquote>` for quotation styles; `None` when empty.
    fn paragraph_html(&mut self, c: Node) -> Option<String> {
        let info = self.styles.paragraph(attr(c, "style-name"));
        let named = |want: &[&str]| info.names.iter().any(|n| want.contains(&n.as_str()));
        // The title classes carry their own weight, so a bold style chain isn't repeated.
        let base = if named(&["title", "subtitle"]) {
            Fmt {
                bold: false,
                ..info.fmt
            }
        } else {
            info.fmt
        };
        let mut inline = Inline::default();
        self.inlines(c, base, &mut inline);
        let (html, text, has_image) = inline.finish();
        if html.is_empty() {
            return None;
        }
        let mut classes = Vec::new();
        if named(&["title"]) {
            classes.push("title");
            if self.title.is_none() && !text.is_empty() {
                self.title = Some(text.clone());
            }
        } else if named(&["subtitle"]) {
            classes.push("subtitle");
        }
        match info.align.as_deref() {
            Some("center") => classes.push("center"),
            Some("end" | "right") => classes.push("right"),
            _ => {}
        }
        if has_image && text.is_empty() {
            classes.push("figure");
        }
        let class = if classes.is_empty() {
            String::new()
        } else {
            format!(" class=\"{}\"", classes.join(" "))
        };
        let para = format!("<p{class}>{html}</p>");
        Some(if named(&["quotations", "quote", "quotation"]) {
            format!("<blockquote>{para}</blockquote>")
        } else {
            para
        })
    }

    fn list(&mut self, list: Node, level: u8, style: Option<&str>, out: &mut Vec<Block>) {
        let mut first = true;
        for item in list.children().filter(|i| i.is_element()) {
            if !matches!(local(item), "list-item" | "list-header") {
                continue;
            }
            let start = attr(item, "start-value")
                .and_then(|v| v.parse().ok())
                .filter(|_| first)
                .unwrap_or(1);
            first = false;
            let mut parts = Vec::new();
            for c in item.children().filter(|c| c.is_element()) {
                match local(c) {
                    "p" => {
                        let info = self.styles.paragraph(attr(c, "style-name"));
                        let mut inline = Inline::default();
                        self.inlines(c, info.fmt, &mut inline);
                        let (html, _, _) = inline.finish();
                        if !html.is_empty() {
                            parts.push(html);
                        }
                    }
                    "list" => {
                        self.push_item(&mut parts, level, style, start, out);
                        let inner = attr(c, "style-name").or(style);
                        self.list(c, level + 1, inner, out);
                    }
                    "h" => {
                        self.push_item(&mut parts, level, style, start, out);
                        self.block(c, level, style, out);
                    }
                    _ => {
                        self.push_item(&mut parts, level, style, start, out);
                        self.block(c, level + 1, style, out);
                    }
                }
            }
            self.push_item(&mut parts, level, style, start, out);
        }
    }

    fn push_item(
        &mut self,
        parts: &mut Vec<String>,
        level: u8,
        style: Option<&str>,
        start: u32,
        out: &mut Vec<Block>,
    ) {
        if parts.is_empty() {
            return;
        }
        out.push(Block::Item {
            ordered: self.styles.ordered(style, usize::from(level)),
            start,
            level,
            html: std::mem::take(parts).join("<br/>"),
        });
    }

    fn table(&mut self, t: Node) -> String {
        let mut html = String::from("<table>");
        let mut rows: Vec<(Node, bool)> = Vec::new();
        fn collect<'a, 'i>(n: Node<'a, 'i>, header: bool, rows: &mut Vec<(Node<'a, 'i>, bool)>) {
            for c in n.children().filter(|c| c.is_element()) {
                match local(c) {
                    "table-row" => rows.push((c, header)),
                    "table-header-rows" => collect(c, true, rows),
                    "table-rows" | "table-row-group" => collect(c, header, rows),
                    _ => {}
                }
            }
        }
        collect(t, false, &mut rows);
        for (row, header) in rows {
            html.push_str("<tr>");
            for cell in row.children().filter(|c| c.is_element()) {
                if local(cell) != "table-cell" {
                    continue;
                }
                let tag = if header { "th" } else { "td" };
                let mut attrs = String::new();
                for (odf, html_attr) in [
                    ("number-columns-spanned", "colspan"),
                    ("number-rows-spanned", "rowspan"),
                ] {
                    if let Some(v) = attr(cell, odf).filter(|v| *v != "1")
                        && v.parse::<u32>().is_ok()
                    {
                        attrs.push_str(&format!(" {html_attr}=\"{v}\""));
                    }
                }
                let mut blocks = Vec::new();
                self.blocks(cell, 0, None, &mut blocks, false);
                html.push_str(&format!("<{tag}{attrs}>{}</{tag}>", flatten(&blocks)));
            }
            html.push_str("</tr>");
        }
        html.push_str("</table>");
        html
    }

    fn inlines(&mut self, n: Node, f: Fmt, out: &mut Inline) {
        for c in n.children() {
            self.inline_node(c, f, out);
        }
    }

    fn inline_node(&mut self, c: Node, f: Fmt, out: &mut Inline) {
        if c.is_text() {
            // ODF collapses white space; `text:s` carries the extra spaces.
            let t = c.text().unwrap_or("");
            let mut collapsed = String::with_capacity(t.len());
            let mut space = false;
            for ch in t.chars() {
                if matches!(ch, ' ' | '\t' | '\n' | '\r') {
                    if !space {
                        collapsed.push(' ');
                    }
                    space = true;
                } else {
                    collapsed.push(ch);
                    space = false;
                }
            }
            out.text(f, &collapsed);
            return;
        }
        if !c.is_element() {
            return;
        }
        match local(c) {
            "span" => {
                let inner = self.styles.text(attr(c, "style-name"), f);
                self.inlines(c, inner, out);
            }
            "s" => {
                let n: usize = attr(c, "c").and_then(|v| v.parse().ok()).unwrap_or(1);
                let n = n.clamp(1, 100);
                out.entity(f, &format!(" {}", "&#160;".repeat(n - 1)), &" ".repeat(n));
            }
            "tab" => out.text(f, " "),
            "line-break" => out.raw("<br/>"),
            "a" => self.link(c, f, out),
            "bookmark" | "bookmark-start" | "reference-mark" | "reference-mark-start" => {
                if let Some(id) = attr(c, "name").and_then(|n| self.bookmarks.get(n)) {
                    out.raw(&format!("<a id=\"{id}\"></a>"));
                }
            }
            "note" => self.noteref(c, out),
            "frame" => self.frame(c, f, out),
            "ruby" => {
                if let Some(base) = child(c, "ruby-base") {
                    self.inlines(base, f, out);
                }
            }
            name if SILENT_INLINE.contains(&name) => {}
            name if FIELDS.contains(&name) => self.inlines(c, f, out),
            other => {
                self.warn(format!(
                    "unsupported OpenDocument element <{other}>; kept its text"
                ));
                self.inlines(c, f, out);
            }
        }
    }

    fn link(&mut self, a: Node, f: Fmt, out: &mut Inline) {
        let href = attr(a, "href").unwrap_or("");
        let mut inner = Inline::default();
        self.inlines(a, f, &mut inner);
        let (html, text, has_image) = inner.finish();
        out.text.push_str(&text);
        out.has_image |= has_image;
        let lower = href.to_ascii_lowercase();
        if ["http://", "https://", "mailto:"]
            .iter()
            .any(|s| lower.starts_with(s))
        {
            out.raw(&format!("<a href=\"{}\">{html}</a>", escape(href)));
        } else if let Some(id) = href
            .strip_prefix('#')
            .and_then(|name| self.bookmarks.get(name))
        {
            out.raw(&format!("<a href=\"{OPEN}{id}{CLOSE}\">{html}</a>"));
        } else {
            if !href.is_empty() {
                self.warn(format!("link to {href} isn't supported; kept its text"));
            }
            out.raw(&html);
        }
    }

    fn noteref(&mut self, n: Node, out: &mut Inline) {
        let endnote = attr(n, "note-class") == Some("endnote");
        let (prefix, number) = if endnote {
            self.endnotes += 1;
            ("en", self.endnotes)
        } else {
            self.footnotes += 1;
            ("fn", self.footnotes)
        };
        let id = self.unique(&format!("{prefix}{number}"));
        let label = child(n, "note-citation")
            .map(|c| {
                c.descendants()
                    .filter(|d| d.is_text())
                    .filter_map(|d| d.text())
                    .collect::<String>()
                    .trim()
                    .to_owned()
            })
            .filter(|l| !l.is_empty())
            .unwrap_or_else(|| number.to_string());
        let mut blocks = Vec::new();
        if let Some(body) = child(n, "note-body") {
            self.blocks(body, 0, None, &mut blocks, false);
        }
        let body = flatten(&blocks);
        let back = format!("<a href=\"#{id}-ref\">{}.</a> ", escape(&label));
        // The number goes inside the note's first paragraph, as in a printed book.
        let body = match body.strip_prefix("<p>") {
            Some(rest) => format!("<p>{back}{}", rest.trim_start()),
            None => format!("<p>{back}</p>{body}"),
        };
        let kind = if endnote { "endnote" } else { "footnote" };
        self.notes.push(format!(
            "<aside epub:type=\"{kind}\" class=\"{kind}\" id=\"{id}\">{body}</aside>"
        ));
        self.pending_notes.push(self.notes.len() - 1);
        out.raw(&format!(
            "<sup><a epub:type=\"noteref\" class=\"noteref\" href=\"#{id}\" id=\"{id}-ref\">{}</a></sup>",
            escape(&label)
        ));
    }

    /// A frame: its picture, or a text box's paragraphs inline (where LibreOffice puts
    /// captioned images).
    fn frame(&mut self, frame: Node, f: Fmt, out: &mut Inline) {
        if let Some(image) = frame
            .children()
            .filter(|c| c.is_element() && local(*c) == "image")
            .find_map(|img| self.image(img))
        {
            let alt = child(frame, "title")
                .or_else(|| child(frame, "desc"))
                .map(|t| {
                    t.descendants()
                        .filter(|d| d.is_text())
                        .filter_map(|d| d.text())
                        .collect::<String>()
                })
                .or_else(|| attr(frame, "name").map(str::to_owned))
                .unwrap_or_default();
            out.has_image = true;
            out.raw(&format!(
                "<img src=\"{}\" alt=\"{}\"/>",
                self.images[image].href,
                escape(alt.trim())
            ));
            return;
        }
        if let Some(text_box) = child(frame, "text-box") {
            let mut first = true;
            for c in text_box.children().filter(|c| c.is_element()) {
                if !first {
                    out.raw("<br/>");
                }
                first = false;
                match local(c) {
                    "p" | "h" => self.inlines(c, f, out),
                    _ => {
                        let mut blocks = Vec::new();
                        self.block(c, 0, None, &mut blocks);
                        out.raw(&flatten(&blocks));
                    }
                }
            }
            return;
        }
        if frame
            .children()
            .any(|c| c.is_element() && local(c) == "object")
        {
            self.warn("embedded objects (charts, formulas) aren't supported; left out".into());
        }
    }

    /// Packages a `draw:image` once; `None` (with a warning) when it can't be used.
    fn image(&mut self, img: Node) -> Option<usize> {
        let (key, bytes) = if let Some(data) = child(img, "binary-data") {
            let b64: String = data
                .descendants()
                .filter(|d| d.is_text())
                .filter_map(|d| d.text())
                .flat_map(str::chars)
                .filter(|c| !c.is_whitespace())
                .collect();
            if let Some(found) = self.image_index.get(&b64) {
                return *found;
            }
            let decoded = base64::engine::general_purpose::STANDARD
                .decode(b64.as_bytes())
                .ok();
            if decoded.is_none() {
                self.warn("an embedded image isn't valid base64; left out".into());
            }
            (b64, decoded)
        } else {
            let href = attr(img, "href").unwrap_or("").trim_start_matches("./");
            if let Some(found) = self.image_index.get(href) {
                return *found;
            }
            let lower = href.to_ascii_lowercase();
            if lower.starts_with("http://") || lower.starts_with("https://") {
                self.warn(format!("image {href} is on the web; left out"));
                return None;
            }
            let bytes = self.pictures.get(href).cloned();
            if bytes.is_none() {
                self.warn(format!(
                    "image {href} is missing from the document; left out"
                ));
            }
            (href.to_owned(), bytes)
        };
        let found = bytes.and_then(|bytes| self.package(&key, bytes));
        self.image_index.insert(key, found);
        found
    }

    fn package(&mut self, key: &str, bytes: Vec<u8>) -> Option<usize> {
        let label = if key.len() > 60 {
            "an embedded image"
        } else {
            key
        };
        if bytes.len() > MAX_IMAGE {
            self.warn(format!("image {label} is over 10 MB; left out"));
            return None;
        }
        // Signatures, not extensions: SVG, EMF and WMF can't be passed through.
        let (ext, media_type) = if bytes.starts_with(b"\x89PNG\r\n\x1a\n") {
            ("png", "image/png")
        } else if bytes.starts_with(b"\xff\xd8\xff") {
            ("jpg", "image/jpeg")
        } else if bytes.starts_with(b"GIF87a") || bytes.starts_with(b"GIF89a") {
            ("gif", "image/gif")
        } else {
            self.warn(format!("image {label} isn't a PNG, JPEG or GIF; left out"));
            return None;
        };
        self.images.push(Image {
            href: format!("images/image-{:03}.{ext}", self.images.len() + 1),
            media_type,
            bytes,
        });
        Some(self.images.len() - 1)
    }
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

/// Blocks as one HTML string: runs of list items become nested `<ul>`/`<ol>`; note markers are
/// dropped (the caller places the notes).
pub fn flatten(blocks: &[Block]) -> String {
    let mut out = String::new();
    let mut items: Vec<(u8, bool, u32, &str)> = Vec::new();
    for b in blocks {
        if matches!(b, Block::Notes(_)) {
            continue;
        }
        if let Block::Item {
            ordered,
            start,
            level,
            html,
        } = b
        {
            items.push((*level, *ordered, *start, html));
            continue;
        }
        if !items.is_empty() {
            out.push_str(&nest(&items));
            items.clear();
        }
        match b {
            Block::Heading { html, .. } | Block::Html(html) => out.push_str(html),
            _ => {}
        }
    }
    if !items.is_empty() {
        out.push_str(&nest(&items));
    }
    out
}

/// Nested lists from (level, ordered, start, item html), in order.
pub fn nest(items: &[(u8, bool, u32, &str)]) -> String {
    let tag = |ordered: bool| if ordered { "ol" } else { "ul" };
    let open = |ordered: bool, start: u32| {
        if ordered && start != 1 {
            format!("<ol start=\"{start}\">")
        } else {
            format!("<{}>", tag(ordered))
        }
    };
    let mut out = String::new();
    let mut stack: Vec<(u8, bool)> = Vec::new();
    for &(level, ordered, start, html) in items {
        while let Some(&(l, o)) = stack.last()
            && l > level
        {
            out.push_str(&format!("</li></{}>", tag(o)));
            stack.pop();
        }
        match stack.last().copied() {
            Some((l, o)) if l == level => {
                out.push_str("</li>");
                if o != ordered {
                    out.push_str(&format!("</{}>", tag(o)));
                    stack.pop();
                    out.push_str(&open(ordered, start));
                    stack.push((level, ordered));
                }
            }
            _ => {
                out.push_str(&open(ordered, start));
                stack.push((level, ordered));
            }
        }
        out.push_str("<li>");
        out.push_str(html);
    }
    while let Some((_, o)) = stack.pop() {
        out.push_str(&format!("</li></{}>", tag(o)));
    }
    out
}

#[cfg(test)]
mod tests {
    use super::{nest, sanitize};

    #[test]
    fn list_items_nest_by_level() {
        let html = nest(&[
            (0, true, 3, "one"),
            (1, false, 1, "a"),
            (1, false, 1, "b"),
            (0, true, 1, "two"),
        ]);
        assert_eq!(
            html,
            "<ol start=\"3\"><li>one<ul><li>a</li><li>b</li></ul></li><li>two</li></ol>"
        );
    }

    #[test]
    fn bookmark_names_become_xml_ids() {
        assert_eq!(sanitize("Figure 1"), "Figure_1");
        assert_eq!(sanitize("1st"), "id-1st");
    }
}
