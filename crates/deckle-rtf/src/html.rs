// SPDX-License-Identifier: GPL-3.0-or-later
//! XHTML building blocks shared with the other importers' output: formatted runs, blocks, and
//! nested lists.

/// Character formatting that becomes XHTML elements.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Fmt {
    pub bold: bool,
    pub italic: bool,
    pub underline: bool,
    pub strike: bool,
    pub small_caps: bool,
    pub sup: bool,
    pub sub: bool,
}

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
    /// Notes referenced by the blocks just before, by index into the notes list.
    Notes(Vec<usize>),
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

#[derive(Clone, Debug)]
enum Seg {
    Text(Fmt, String),
    Raw(String),
}

/// One paragraph's content under construction. Formatting is applied when it's rendered, so a
/// heading can leave out its own bold.
#[derive(Clone, Debug, Default)]
pub struct Inline {
    segs: Vec<Seg>,
    pub plain: String,
    pub has_image: bool,
}

impl Inline {
    pub fn text(&mut self, f: Fmt, s: &str) {
        if s.is_empty() {
            return;
        }
        self.plain.push_str(s);
        match self.segs.last_mut() {
            Some(Seg::Text(pf, t)) if *pf == f => t.push_str(s),
            _ => self.segs.push(Seg::Text(f, s.to_owned())),
        }
    }

    pub fn raw(&mut self, s: &str) {
        self.segs.push(Seg::Raw(s.to_owned()));
    }

    pub fn is_empty(&self) -> bool {
        self.segs
            .iter()
            .all(|s| matches!(s, Seg::Raw(r) if r.is_empty()))
    }

    /// A position to wrap from later (see [`Inline::wrap_from`]). Text after it starts a new
    /// run even in the same formatting.
    pub fn mark(&mut self) -> usize {
        self.segs.push(Seg::Raw(String::new()));
        self.segs.len()
    }

    /// Renders everything after `mark` inside `open`…`close`.
    pub fn wrap_from(&mut self, mark: usize, open: &str, close: &str) {
        if mark > self.segs.len() {
            return;
        }
        let inner: Vec<Seg> = self.segs.drain(mark..).collect();
        let html = render(&inner, false);
        self.segs.push(Seg::Raw(format!("{open}{html}{close}")));
    }

    /// The paragraph's HTML, trimmed; `drop_bold` leaves out bold (headings, titles).
    pub fn html(&self, drop_bold: bool) -> String {
        render(&self.segs, drop_bold).trim().to_owned()
    }

    pub fn plain_text(&self) -> String {
        self.plain.split_whitespace().collect::<Vec<_>>().join(" ")
    }
}

fn render(segs: &[Seg], drop_bold: bool) -> String {
    let mut out = String::new();
    for seg in segs {
        match seg {
            Seg::Raw(r) => out.push_str(r),
            Seg::Text(f, t) => {
                let tags: Vec<(&str, &str)> = [
                    (f.bold && !drop_bold, ("<b>", "</b>")),
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
                    out.push_str(open);
                }
                out.push_str(&escape(t).replace('\u{a0}', "&#160;"));
                for (_, close) in tags.iter().rev() {
                    out.push_str(close);
                }
            }
        }
    }
    out
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
    use super::{Fmt, Inline};

    #[test]
    fn runs_merge_and_headings_drop_bold() {
        let bold = Fmt {
            bold: true,
            ..Fmt::default()
        };
        let mut p = Inline::default();
        p.text(bold, "Chap");
        p.text(bold, "ter <1>");
        assert_eq!(p.html(false), "<b>Chapter &lt;1&gt;</b>");
        assert_eq!(p.html(true), "Chapter &lt;1&gt;");
        let m = p.mark();
        p.text(bold, " link");
        p.wrap_from(m, "<a href=\"x\">", "</a>");
        assert_eq!(
            p.html(false),
            "<b>Chapter &lt;1&gt;</b><a href=\"x\"><b> link</b></a>"
        );
    }
}
