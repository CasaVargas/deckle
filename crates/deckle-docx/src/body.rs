// SPDX-License-Identifier: GPL-3.0-or-later
//! `word/document.xml` to XHTML blocks: paragraphs, headings, list items, tables, images and
//! footnote references.

use std::collections::HashMap;

use roxmltree::Node;

use crate::package::{Rel, attr};
use crate::styles::{Fmt, FmtPatch, Numbering, Styles, child, num_pr, outline_level};

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

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Part {
    Doc,
    Footnotes,
    Endnotes,
}

#[derive(Clone, Debug)]
pub struct Note {
    /// `fn3` or `en3`
    pub id: String,
    pub html: String,
}

#[derive(Clone, Debug)]
pub struct Image {
    /// Zip path, e.g. `word/media/image1.png`.
    pub source: String,
    /// Path in the EPUB relative to the content documents, e.g. `images/image1.png`.
    pub href: String,
    pub media_type: &'static str,
}

pub fn media_type(path: &str) -> Option<&'static str> {
    let ext = path.rsplit('.').next()?.to_ascii_lowercase();
    Some(match ext.as_str() {
        "png" => "image/png",
        "jpg" | "jpeg" => "image/jpeg",
        "gif" => "image/gif",
        "svg" => "image/svg+xml",
        _ => return None,
    })
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
        self.text.push_str(s);
        match &mut self.pending {
            Some((pf, t)) if *pf == f => t.push_str(&escape(s)),
            _ => {
                self.flush();
                self.pending = Some((f, escape(s)));
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
        (self.html, self.text, self.has_image)
    }
}

pub struct Body<'d, 'i> {
    pub styles: Styles,
    pub numbering: Numbering,
    pub rels: HashMap<Part, HashMap<String, Rel>>,
    pub footnotes: HashMap<String, Node<'d, 'i>>,
    pub endnotes: HashMap<String, Node<'d, 'i>>,
    pub images: Vec<Image>,
    pub notes: Vec<Note>,
    pub warnings: Vec<String>,
    /// Text of the first `Title`-style paragraph.
    pub title: Option<String>,
    note_index: HashMap<(Part, String), usize>,
    pending_notes: Vec<usize>,
    headings: usize,
    footnote_count: usize,
    endnote_count: usize,
}

impl<'d, 'i> Body<'d, 'i> {
    pub fn new(styles: Styles, numbering: Numbering) -> Self {
        Body {
            styles,
            numbering,
            rels: HashMap::new(),
            footnotes: HashMap::new(),
            endnotes: HashMap::new(),
            images: Vec::new(),
            notes: Vec::new(),
            warnings: Vec::new(),
            title: None,
            note_index: HashMap::new(),
            pending_notes: Vec::new(),
            headings: 0,
            footnote_count: 0,
            endnote_count: 0,
        }
    }

    /// The blocks of `w:body`, each followed by the notes it references.
    pub fn body(&mut self, body: Node) -> Vec<Block> {
        let mut out = Vec::new();
        for c in body.children().filter(Node::is_element) {
            self.block(c, Part::Doc, &mut out);
            if !self.pending_notes.is_empty() {
                out.push(Block::Notes(std::mem::take(&mut self.pending_notes)));
            }
        }
        out
    }

    fn blocks(&mut self, parent: Node, part: Part, out: &mut Vec<Block>) {
        for c in parent.children().filter(Node::is_element) {
            self.block(c, part, out);
        }
    }

    fn block(&mut self, n: Node, part: Part, out: &mut Vec<Block>) {
        match n.tag_name().name() {
            "p" => self.paragraph(n, part, out),
            "tbl" => out.push(Block::Html(self.table(n, part))),
            "sdt" => {
                if let Some(c) = child(n, "sdtContent") {
                    self.blocks(c, part, out)
                }
            }
            "customXml" | "ins" | "moveTo" | "smartTag" => self.blocks(n, part, out),
            _ => {}
        }
    }

    fn paragraph(&mut self, p: Node, part: Part, out: &mut Vec<Block>) {
        let ppr = child(p, "pPr");
        let style = ppr
            .and_then(|pp| child(pp, "pStyle"))
            .and_then(|s| attr(s, "val"))
            .map(str::to_owned)
            .or_else(|| self.styles.default_paragraph.clone())
            .unwrap_or_default();
        let name = self.styles.name(&style);
        let level = match ppr.and_then(outline_level) {
            Some(l) if part == Part::Doc => (l < 9).then_some(l + 1),
            _ if part == Part::Doc => self.styles.heading_level(&style),
            _ => None,
        };
        // A heading's own bold or size is the heading's look, not emphasis.
        let base = if level.is_some() {
            Fmt::default()
        } else {
            self.styles.fmt(&style)
        };
        let mut inline = Inline::default();
        self.inline(p, part, base, &mut inline);
        let (html, text, has_image) = inline.finish();
        if text.trim().is_empty() && !has_image && !html.contains("<a ") {
            return;
        }
        if let Some(level) = level {
            self.headings += 1;
            let level = level.min(6);
            let id = format!("h{}", self.headings);
            out.push(Block::Heading {
                level,
                html: format!("<h{level} id=\"{id}\">{html}</h{level}>"),
                id,
                text: text.trim().to_owned(),
            });
            return;
        }
        let num = ppr.and_then(num_pr).or_else(|| self.styles.num(&style));
        if let Some((num_id, ilvl)) = num
            && let Some(ordered) = self.numbering.ordered(&num_id, ilvl)
        {
            out.push(Block::Item {
                ordered,
                start: self.numbering.start(&num_id, ilvl),
                level: ilvl,
                html,
            });
            return;
        }
        let mut classes = Vec::new();
        match name.as_str() {
            "title" => {
                classes.push("title");
                if self.title.is_none() && part == Part::Doc {
                    self.title = Some(text.trim().to_owned());
                }
            }
            "subtitle" => classes.push("subtitle"),
            _ => {}
        }
        let jc = ppr
            .and_then(|pp| child(pp, "jc"))
            .and_then(|j| attr(j, "val"))
            .map(str::to_owned)
            .or_else(|| self.styles.jc(&style));
        match jc.as_deref() {
            Some("center") => classes.push("center"),
            Some("right" | "end") => classes.push("right"),
            _ => {}
        }
        if has_image && text.trim().is_empty() {
            classes.push("figure");
        }
        let class = if classes.is_empty() {
            String::new()
        } else {
            format!(" class=\"{}\"", classes.join(" "))
        };
        let para = format!("<p{class}>{html}</p>");
        let quote = matches!(name.as_str(), "quote" | "intense quote" | "block text");
        out.push(Block::Html(if quote {
            format!("<blockquote>{para}</blockquote>")
        } else {
            para
        }));
    }

    fn inline(&mut self, n: Node, part: Part, base: Fmt, out: &mut Inline) {
        for c in n.children().filter(Node::is_element) {
            match c.tag_name().name() {
                "r" => self.run(c, part, base, out),
                "hyperlink" => {
                    let href = attr(c, "id")
                        .and_then(|id| self.rels.get(&part)?.get(id))
                        .filter(|r| r.external)
                        .map(|r| r.target.clone());
                    match href {
                        Some(h) => {
                            out.raw(&format!("<a href=\"{}\">", escape(&h)));
                            self.inline(c, part, base, out);
                            out.raw("</a>");
                        }
                        None => self.inline(c, part, base, out),
                    }
                }
                "sdt" => {
                    if let Some(sc) = child(c, "sdtContent") {
                        self.inline(sc, part, base, out)
                    }
                }
                "ins" | "smartTag" | "fldSimple" | "customXml" | "moveTo" | "dir" | "bdo" => {
                    self.inline(c, part, base, out)
                }
                _ => {}
            }
        }
    }

    fn run(&mut self, r: Node, part: Part, base: Fmt, out: &mut Inline) {
        let mut f = base;
        if let Some(rpr) = child(r, "rPr") {
            if let Some(rs) = child(rpr, "rStyle").and_then(|s| attr(s, "val")) {
                let sf = self.styles.fmt(rs);
                // Character styles add to the paragraph's formatting.
                f.bold |= sf.bold;
                f.italic |= sf.italic;
                f.underline |= sf.underline;
                f.strike |= sf.strike;
                f.small_caps |= sf.small_caps;
                f.sup |= sf.sup;
                f.sub |= sf.sub;
                f.hidden |= sf.hidden;
            }
            FmtPatch::parse(rpr).apply(&mut f);
        }
        if f.hidden {
            return;
        }
        self.run_content(r, part, f, out);
    }

    fn run_content(&mut self, r: Node, part: Part, f: Fmt, out: &mut Inline) {
        for c in r.children().filter(Node::is_element) {
            match c.tag_name().name() {
                "t" => out.text(f, c.text().unwrap_or("")),
                "tab" | "ptab" => out.text(f, " "),
                "br" | "cr" => {
                    if !matches!(attr(c, "type"), Some("page" | "column")) {
                        out.raw("<br/>")
                    }
                }
                "noBreakHyphen" => out.text(f, "\u{2011}"),
                "softHyphen" => out.text(f, "\u{AD}"),
                "footnoteReference" => self.noteref(Part::Footnotes, c, out),
                "endnoteReference" => self.noteref(Part::Endnotes, c, out),
                "drawing" => {
                    let rid = c
                        .descendants()
                        .find(|d| d.tag_name().name() == "blip")
                        .and_then(|b| attr(b, "embed"));
                    let alt = c
                        .descendants()
                        .find(|d| d.tag_name().name() == "docPr")
                        .and_then(|d| attr(d, "descr").or_else(|| attr(d, "title")))
                        .unwrap_or("");
                    if let Some(rid) = rid {
                        self.image(part, rid, alt, out);
                    }
                }
                "pict" | "object" => {
                    if let Some(d) = c.descendants().find(|d| d.tag_name().name() == "imagedata")
                        && let Some(rid) = attr(d, "id")
                    {
                        let alt = attr(d, "title").unwrap_or("");
                        self.image(part, rid, alt, out);
                    }
                }
                // Office's "this or, for older readers, that": take the first choice.
                "AlternateContent" => {
                    if let Some(choice) = c
                        .children()
                        .find(|x| x.tag_name().name() == "Choice")
                        .or_else(|| c.children().find(|x| x.tag_name().name() == "Fallback"))
                    {
                        self.run_content(choice, part, f, out)
                    }
                }
                _ => {}
            }
        }
    }

    fn image(&mut self, part: Part, rid: &str, alt: &str, out: &mut Inline) {
        let Some(rel) = self.rels.get(&part).and_then(|m| m.get(rid)).cloned() else {
            self.warnings
                .push(format!("an image refers to a missing relationship {rid}"));
            return;
        };
        if rel.external {
            self.warnings.push(format!(
                "linked image {} skipped: only images inside the document are kept",
                rel.target
            ));
            return;
        }
        let Some(media_type) = media_type(&rel.target) else {
            let msg = format!(
                "image {} skipped: Kindle shows PNG, JPEG, GIF and SVG only",
                rel.target
            );
            if !self.warnings.contains(&msg) {
                self.warnings.push(msg);
            }
            return;
        };
        let href = match self.images.iter().find(|i| i.source == rel.target) {
            Some(i) => i.href.clone(),
            None => {
                let file = rel.target.rsplit('/').next().unwrap_or("image");
                let mut href = format!("images/{file}");
                if self.images.iter().any(|i| i.href == href) {
                    href = format!("images/{}-{file}", self.images.len());
                }
                self.images.push(Image {
                    source: rel.target.clone(),
                    href: href.clone(),
                    media_type,
                });
                href
            }
        };
        out.has_image = true;
        out.raw(&format!(
            "<img src=\"{}\" alt=\"{}\"/>",
            escape(&href),
            escape(alt)
        ));
    }

    fn noteref(&mut self, kind: Part, r: Node, out: &mut Inline) {
        let Some(id) = attr(r, "id") else { return };
        let key = (kind, id.to_owned());
        let (index, first) = match self.note_index.get(&key) {
            Some(i) => (*i, false),
            None => {
                let notes = if kind == Part::Footnotes {
                    &self.footnotes
                } else {
                    &self.endnotes
                };
                let Some(node) = notes.get(id).copied() else {
                    self.warnings
                        .push(format!("a note reference points to missing note {id}"));
                    return;
                };
                let (prefix, number) = if kind == Part::Footnotes {
                    self.footnote_count += 1;
                    ("fn", self.footnote_count)
                } else {
                    self.endnote_count += 1;
                    ("en", self.endnote_count)
                };
                let note_id = format!("{prefix}{number}");
                let mut blocks = Vec::new();
                self.blocks(node, kind, &mut blocks);
                let body = flatten(&blocks);
                let back = format!("<a href=\"#{note_id}-ref\">{number}.</a> ");
                // The number goes inside the note's first paragraph, as in a printed book.
                let body = match body.strip_prefix("<p>") {
                    Some(rest) => format!("<p>{back}{}", rest.trim_start()),
                    None => format!("<p>{back}</p>{body}"),
                };
                let epub_type = if kind == Part::Footnotes {
                    "footnote"
                } else {
                    "endnote"
                };
                self.notes.push(Note {
                    html: format!(
                        "<aside epub:type=\"{epub_type}\" class=\"{epub_type}\" id=\"{note_id}\">{body}</aside>"
                    ),
                    id: note_id,
                });
                let index = self.notes.len() - 1;
                self.note_index.insert(key, index);
                (index, true)
            }
        };
        let note = &self.notes[index];
        let label = note
            .id
            .trim_start_matches(|c: char| c.is_ascii_alphabetic());
        let back_id = if first {
            format!(" id=\"{}-ref\"", note.id)
        } else {
            String::new()
        };
        out.raw(&format!(
            "<sup><a epub:type=\"noteref\" class=\"noteref\" href=\"#{}\"{back_id}>{label}</a></sup>",
            note.id
        ));
        if first {
            self.pending_notes.push(index);
        }
    }

    fn table(&mut self, tbl: Node, part: Part) -> String {
        let mut html = String::from("<table>");
        for tr in tbl.children().filter(|c| c.tag_name().name() == "tr") {
            let header = child(tr, "trPr")
                .and_then(|p| child(p, "tblHeader"))
                .is_some_and(|h| !matches!(attr(h, "val"), Some("0" | "false" | "off")));
            let tag = if header { "th" } else { "td" };
            html.push_str("<tr>");
            for tc in tr.children().filter(|c| c.tag_name().name() == "tc") {
                let tcpr = child(tc, "tcPr");
                let span = tcpr
                    .and_then(|p| child(p, "gridSpan"))
                    .and_then(|g| attr(g, "val"))
                    .and_then(|v| v.parse::<u32>().ok())
                    .filter(|n| *n > 1);
                let mut blocks = Vec::new();
                self.blocks(tc, part, &mut blocks);
                let colspan = span
                    .map(|n| format!(" colspan=\"{n}\""))
                    .unwrap_or_default();
                html.push_str(&format!("<{tag}{colspan}>{}</{tag}>", flatten(&blocks)));
            }
            html.push_str("</tr>");
        }
        html.push_str("</table>");
        html
    }
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
    use super::nest;

    #[test]
    fn list_items_nest_by_level() {
        let html = nest(&[
            (0, false, 1, "a"),
            (1, false, 1, "a1"),
            (1, false, 1, "a2"),
            (0, false, 1, "b"),
            (2, true, 3, "deep"),
            (0, true, 1, "one"),
        ]);
        assert_eq!(
            html,
            "<ul><li>a<ul><li>a1</li><li>a2</li></ul></li><li>b<ol start=\"3\"><li>deep</li></ol>\
             </li></ul><ol><li>one</li></ol>"
        );
    }
}
