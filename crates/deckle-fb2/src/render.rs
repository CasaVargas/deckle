// SPDX-License-Identifier: GPL-3.0-or-later
//! From a parsed FictionBook to XHTML pages: sections and the contents, notes, links and images
//! (FB2 spec §2).

use std::collections::{HashMap, HashSet};

use base64::Engine;
use roxmltree::Node;

/// An image larger than this is dropped with a warning.
const MAX_IMAGE: usize = 10 * 1024 * 1024;
/// An internal link's target is written as OPEN id CLOSE and resolved once every page exists.
const OPEN: char = '\u{E000}';
const CLOSE: char = '\u{E001}';
pub const NOTES_PAGE: &str = "notes.xhtml";

pub struct Page {
    pub name: String,
    /// The first heading on the page, for its `<title>`.
    pub title: Option<String>,
    pub html: String,
}

pub struct Image {
    pub name: String,
    pub mime: &'static str,
    pub bytes: Vec<u8>,
}

pub fn chapter_name(i: usize) -> String {
    format!("chapter-{:03}.xhtml", i + 1)
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

/// The element's local name; FB2 files disagree on namespace prefixes.
pub fn name<'a>(n: Node<'a, '_>) -> &'a str {
    n.tag_name().name()
}

pub fn elements<'a, 'i>(n: Node<'a, 'i>) -> impl Iterator<Item = Node<'a, 'i>> {
    n.children().filter(|c| c.is_element())
}

pub fn child<'a, 'i>(n: Node<'a, 'i>, tag: &str) -> Option<Node<'a, 'i>> {
    elements(n).find(|c| name(*c) == tag)
}

/// `l:href`, `xlink:href` or a bare `href`.
fn href<'a>(n: Node<'a, '_>) -> Option<&'a str> {
    n.attributes()
        .find(|a| a.name() == "href")
        .map(|a| a.value())
}

/// All the text under `n`, whitespace runs collapsed.
pub fn text_of(n: Node) -> String {
    let raw: String = n
        .descendants()
        .filter(|d| d.is_text())
        .filter_map(|d| d.text())
        .collect();
    raw.split_whitespace().collect::<Vec<_>>().join(" ")
}

pub struct Renderer<'a, 'i> {
    binaries: HashMap<&'a str, Node<'a, 'i>>,
    /// FB2 id → XML-safe, unique id.
    ids: HashMap<String, String>,
    used: HashSet<String>,
    /// FB2 ids inside the notes bodies.
    notes: HashSet<String>,
    /// Safe id → the page it ended up on.
    placed: HashMap<String, String>,
    /// Note FB2 id → (page, anchor id) of its first reference, for the back-link.
    first_ref: HashMap<String, (String, String)>,
    image_index: HashMap<String, Option<usize>>,
    generated: usize,
    pub images: Vec<Image>,
    pub pages: Vec<Page>,
    pub contents: Vec<(u8, String)>,
    pub warnings: Vec<String>,
}

impl<'a, 'i> Renderer<'a, 'i> {
    pub fn new(root: Node<'a, 'i>, bodies: &[Node<'a, 'i>], notes: &[Node<'a, 'i>]) -> Self {
        let mut r = Renderer {
            binaries: elements(root)
                .filter(|n| name(*n) == "binary")
                .filter_map(|n| n.attribute("id").map(|id| (id, n)))
                .collect(),
            ids: HashMap::new(),
            used: HashSet::new(),
            notes: HashSet::new(),
            placed: HashMap::new(),
            first_ref: HashMap::new(),
            image_index: HashMap::new(),
            generated: 0,
            images: Vec::new(),
            pages: Vec::new(),
            contents: Vec::new(),
            warnings: Vec::new(),
        };
        for body in bodies.iter().chain(notes) {
            for d in body.descendants().filter(|d| d.is_element()) {
                if let Some(raw) = d.attribute("id")
                    && !r.ids.contains_key(raw)
                {
                    let safe = r.unique(&sanitize(raw));
                    r.ids.insert(raw.to_owned(), safe);
                }
            }
        }
        for body in notes {
            for d in body.descendants() {
                if let Some(raw) = d.attribute("id") {
                    r.notes.insert(raw.to_owned());
                }
            }
        }
        r
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

    fn generate(&mut self, what: &str) -> String {
        self.generated += 1;
        let base = format!("fb2-{what}-{}", self.generated);
        self.unique(&base)
    }

    fn page(&self) -> String {
        self.pages
            .last()
            .map(|p| p.name.clone())
            .unwrap_or_default()
    }

    /// A new page, unless the current one is still empty.
    fn start_page(&mut self) {
        if self.pages.last().is_none_or(|p| !p.html.is_empty()) {
            let name = chapter_name(self.pages.len());
            self.pages.push(Page {
                name,
                title: None,
                html: String::new(),
            });
        }
    }

    fn push(&mut self, html: &str) {
        self.pages.last_mut().expect("a page").html.push_str(html);
    }

    /// The element's safe id, recorded on the current page.
    fn id_of(&mut self, n: Node) -> Option<String> {
        let safe = self.ids.get(n.attribute("id")?)?.clone();
        let page = self.page();
        self.placed.insert(safe.clone(), page);
        Some(safe)
    }

    fn id_attr(&mut self, n: Node) -> String {
        self.id_of(n)
            .map(|id| format!(" id=\"{id}\""))
            .unwrap_or_default()
    }

    /// The main body: its title page, then its sections.
    pub fn body(&mut self, body: Node<'a, 'i>) {
        self.start_page();
        for c in elements(body) {
            match name(c) {
                "title" => {
                    let id = self.id_attr(c);
                    let (html, label) = self.title(c);
                    self.push(&format!("<h1 class=\"book-title\"{id}>{html}</h1>"));
                    if !label.is_empty() {
                        self.pages
                            .last_mut()
                            .expect("a page")
                            .title
                            .get_or_insert(label);
                    }
                }
                "section" => self.section(c, 1, 0),
                _ => {
                    let html = self.block(c);
                    self.push(&html);
                }
            }
        }
    }

    /// A section: a new page at the top level, and at the second level when it has a title.
    fn section(&mut self, n: Node<'a, 'i>, depth: usize, level: usize) {
        let title = child(n, "title");
        if depth == 1 || (depth == 2 && title.is_some()) {
            self.start_page();
        }
        let id = self.id_of(n);
        if let Some(t) = title {
            let h = (level + 1).min(6);
            let (html, label) = self.title(t);
            let hid = match id {
                Some(id) => id,
                None => self.generate("h"),
            };
            let page = self.page();
            self.push(&format!("<h{h} id=\"{hid}\">{html}</h{h}>"));
            if !label.is_empty() {
                self.contents.push((
                    h as u8,
                    format!("<a href=\"{page}#{hid}\">{}</a>", escape(&label)),
                ));
                self.pages
                    .last_mut()
                    .expect("a page")
                    .title
                    .get_or_insert(label);
            }
        } else if let Some(id) = id {
            self.push(&format!("<div id=\"{id}\"></div>"));
        }
        let inner = level + usize::from(title.is_some());
        for c in elements(n) {
            match name(c) {
                "title" => {}
                "section" => self.section(c, depth + 1, inner),
                _ => {
                    let html = self.block(c);
                    self.push(&html);
                }
            }
        }
    }

    /// A `<title>` as heading html (lines joined by `<br/>`) and a contents label.
    fn title(&mut self, t: Node<'a, 'i>) -> (String, String) {
        let mut lines = Vec::new();
        let mut labels = Vec::new();
        for p in elements(t).filter(|p| name(*p) == "p") {
            let anchor = self
                .id_of(p)
                .map(|id| format!("<span id=\"{id}\"></span>"))
                .unwrap_or_default();
            let html = self.inline(p);
            let label = text_of(p);
            if html.trim().is_empty() && anchor.is_empty() {
                continue;
            }
            lines.push(format!("{anchor}{}", html.trim()));
            if !label.is_empty() {
                labels.push(label);
            }
        }
        (lines.join("<br/>"), labels.join(" — "))
    }

    fn blocks(&mut self, n: Node<'a, 'i>) -> String {
        elements(n).map(|c| self.block(c)).collect()
    }

    fn block(&mut self, n: Node<'a, 'i>) -> String {
        match name(n) {
            "p" => self.para(n, None),
            "subtitle" => self.para(n, Some("subtitle")),
            "text-author" => self.para(n, Some("text-author")),
            "date" => self.para(n, Some("date")),
            "empty-line" => "<p class=\"empty-line\">&#160;</p>".into(),
            "epigraph" => {
                let id = self.id_attr(n);
                format!(
                    "<blockquote class=\"epigraph\"{id}>{}</blockquote>",
                    self.blocks(n)
                )
            }
            "cite" => {
                let id = self.id_attr(n);
                format!("<blockquote{id}>{}</blockquote>", self.blocks(n))
            }
            "annotation" => {
                let id = self.id_attr(n);
                format!(
                    "<blockquote class=\"annotation\"{id}>{}</blockquote>",
                    self.blocks(n)
                )
            }
            "title" => {
                let id = self.id_attr(n);
                let (html, _) = self.title(n);
                format!("<p class=\"inner-title\"{id}>{html}</p>")
            }
            "poem" => self.poem(n),
            "stanza" => self.stanza(n),
            "table" => self.table(n),
            "image" => match self.img(n) {
                Some(img) => format!("<div class=\"image\">{img}</div>"),
                None => String::new(),
            },
            "section" => {
                let id = self.id_attr(n);
                format!("<div{id}>{}</div>", self.blocks(n))
            }
            other => {
                let other = other.to_owned();
                self.warn(format!("unsupported FB2 element <{other}>; kept its text"));
                if elements(n).next().is_some()
                    && n.children()
                        .all(|c| c.is_element() || c.text().is_none_or(|t| t.trim().is_empty()))
                {
                    self.blocks(n)
                } else {
                    self.para(n, None)
                }
            }
        }
    }

    fn para(&mut self, n: Node<'a, 'i>, class: Option<&str>) -> String {
        let id = self.id_attr(n);
        let class = class.map(|c| format!(" class=\"{c}\"")).unwrap_or_default();
        format!("<p{class}{id}>{}</p>", self.inline(n).trim())
    }

    fn poem(&mut self, n: Node<'a, 'i>) -> String {
        let id = self.id_attr(n);
        let mut out = format!("<div class=\"poem\"{id}>");
        for c in elements(n) {
            let html = match name(c) {
                "title" => {
                    let (html, _) = self.title(c);
                    format!("<p class=\"poem-title\">{html}</p>")
                }
                "stanza" => self.stanza(c),
                _ => self.block(c),
            };
            out.push_str(&html);
        }
        out.push_str("</div>");
        out
    }

    fn stanza(&mut self, n: Node<'a, 'i>) -> String {
        let id = self.id_attr(n);
        let mut before = String::new();
        let mut lines = Vec::new();
        for c in elements(n) {
            match name(c) {
                "title" => {
                    let (html, _) = self.title(c);
                    before.push_str(&format!("<p class=\"poem-title\">{html}</p>"));
                }
                "subtitle" => before.push_str(&self.para(c, Some("subtitle"))),
                _ => {
                    let anchor = self
                        .id_of(c)
                        .map(|id| format!("<span id=\"{id}\"></span>"))
                        .unwrap_or_default();
                    lines.push(format!("{anchor}{}", self.inline(c).trim()));
                }
            }
        }
        format!(
            "{before}<p class=\"stanza\"{id}>{}</p>",
            lines.join("<br/>")
        )
    }

    fn table(&mut self, n: Node<'a, 'i>) -> String {
        let id = self.id_attr(n);
        let mut out = format!("<table{id}>");
        for tr in elements(n).filter(|r| name(*r) == "tr") {
            out.push_str("<tr>");
            for cell in elements(tr) {
                let tag = if name(cell) == "th" { "th" } else { "td" };
                let mut attrs = self.id_attr(cell);
                for a in ["colspan", "rowspan", "align", "valign"] {
                    if let Some(v) = cell.attribute(a) {
                        attrs.push_str(&format!(" {a}=\"{}\"", escape(v)));
                    }
                }
                out.push_str(&format!(
                    "<{tag}{attrs}>{}</{tag}>",
                    self.inline(cell).trim()
                ));
            }
            out.push_str("</tr>");
        }
        out.push_str("</table>");
        out
    }

    fn inline(&mut self, n: Node<'a, 'i>) -> String {
        let mut out = String::new();
        for c in n.children() {
            if c.is_text() {
                out.push_str(&escape(c.text().unwrap_or("")));
                continue;
            }
            if !c.is_element() {
                continue;
            }
            let wrap = |tag: &str, inner: String| format!("<{tag}>{inner}</{tag}>");
            let html = match name(c) {
                "strong" => wrap("strong", self.inline(c)),
                "emphasis" => wrap("em", self.inline(c)),
                "strikethrough" => wrap("s", self.inline(c)),
                "sub" => wrap("sub", self.inline(c)),
                "sup" => wrap("sup", self.inline(c)),
                "code" => wrap("code", self.inline(c)),
                "style" => self.inline(c),
                "a" => self.link(c),
                "image" => self.img(c).unwrap_or_default(),
                other => {
                    let other = other.to_owned();
                    self.warn(format!("unsupported FB2 element <{other}>; kept its text"));
                    self.inline(c)
                }
            };
            out.push_str(&html);
        }
        out
    }

    fn link(&mut self, a: Node<'a, 'i>) -> String {
        let label = self.inline(a);
        let Some(target) = href(a) else {
            return label;
        };
        if let Some(raw) = target.strip_prefix('#') {
            let Some(safe) = self.ids.get(raw).cloned() else {
                self.warn(format!(
                    "link to #{raw}, which isn't in the book; kept its text"
                ));
                return label;
            };
            let note = a.attribute("type") == Some("note") || self.notes.contains(raw);
            if !note {
                return format!("<a href=\"{OPEN}{safe}{CLOSE}\">{label}</a>");
            }
            let mut anchor = String::new();
            if !self.first_ref.contains_key(raw) {
                let rid = self.unique(&format!("ref-{safe}"));
                anchor = format!(" id=\"{rid}\"");
                let page = self.page();
                self.first_ref.insert(raw.to_owned(), (page, rid));
            }
            return format!(
                "<a epub:type=\"noteref\" href=\"{OPEN}{safe}{CLOSE}\"{anchor}>{label}</a>"
            );
        }
        let lower = target.to_ascii_lowercase();
        if ["http://", "https://", "mailto:"]
            .iter()
            .any(|s| lower.starts_with(s))
        {
            return format!("<a href=\"{}\">{label}</a>", escape(target));
        }
        self.warn(format!("link to {target} isn't supported; kept its text"));
        label
    }

    /// An `<img>` for an FB2 `<image>`, or nothing when its binary can't be used.
    fn img(&mut self, n: Node<'a, 'i>) -> Option<String> {
        let target = href(n).unwrap_or("");
        let Some(bid) = target.strip_prefix('#') else {
            self.warn(format!("image {target} isn't in the book; left out"));
            return None;
        };
        let idx = self.image(bid)?;
        let id = self.id_attr(n);
        let alt = n.attribute("alt").or(n.attribute("title")).unwrap_or("");
        Some(format!(
            "<img{id} src=\"{}\" alt=\"{}\"/>",
            self.images[idx].name,
            escape(alt)
        ))
    }

    /// Packages binary `bid` once; `None` (with a warning) when it can't be used.
    pub fn image(&mut self, bid: &str) -> Option<usize> {
        if let Some(found) = self.image_index.get(bid) {
            return *found;
        }
        let found = self.package(bid);
        self.image_index.insert(bid.to_owned(), found);
        found
    }

    fn package(&mut self, bid: &str) -> Option<usize> {
        let Some(node) = self.binaries.get(bid).copied() else {
            self.warn(format!("image #{bid} is missing; left out"));
            return None;
        };
        let data: String = node
            .descendants()
            .filter(|d| d.is_text())
            .filter_map(|d| d.text())
            .flat_map(|t| t.chars())
            .filter(|c| !c.is_whitespace())
            .collect();
        let Ok(bytes) = base64::engine::general_purpose::STANDARD.decode(data.as_bytes()) else {
            self.warn(format!("image #{bid} isn't valid base64; left out"));
            return None;
        };
        if bytes.len() > MAX_IMAGE {
            self.warn(format!("image #{bid} is over 10 MB; left out"));
            return None;
        }
        // Signatures, not content-type: real files mislabel it.
        let (ext, mime) = if bytes.starts_with(b"\x89PNG\r\n\x1a\n") {
            ("png", "image/png")
        } else if bytes.starts_with(b"\xff\xd8\xff") {
            ("jpg", "image/jpeg")
        } else if bytes.starts_with(b"GIF87a") || bytes.starts_with(b"GIF89a") {
            ("gif", "image/gif")
        } else {
            self.warn(format!("image #{bid} isn't a PNG, JPEG or GIF; left out"));
            return None;
        };
        self.images.push(Image {
            name: format!("images/image-{:03}.{ext}", self.images.len() + 1),
            mime,
            bytes,
        });
        Some(self.images.len() - 1)
    }

    /// The notes page: one pop-up footnote per section with an id, each linking back to its
    /// first reference.
    pub fn notes(&mut self, bodies: &[Node<'a, 'i>]) {
        if bodies.is_empty() {
            return;
        }
        let label = bodies
            .iter()
            .find_map(|b| child(*b, "title").map(text_of))
            .filter(|t| !t.is_empty())
            .unwrap_or_else(|| "Notes".into());
        self.pages.push(Page {
            name: NOTES_PAGE.into(),
            title: Some(label.clone()),
            html: String::new(),
        });
        let hid = self.generate("notes");
        self.push(&format!("<h1 id=\"{hid}\">{}</h1>", escape(&label)));
        self.contents.push((
            1,
            format!("<a href=\"{NOTES_PAGE}#{hid}\">{}</a>", escape(&label)),
        ));
        for body in bodies {
            for c in elements(*body) {
                match name(c) {
                    "title" => {}
                    "section" => self.note(c),
                    _ => {
                        let html = self.block(c);
                        self.push(&html);
                    }
                }
            }
        }
    }

    fn note(&mut self, n: Node<'a, 'i>) {
        let Some(id) = self.id_of(n) else {
            // Nothing can point at it; plain text.
            let html = self.blocks(n);
            self.push(&html);
            return;
        };
        let mut html = format!("<aside epub:type=\"footnote\" id=\"{id}\">");
        for c in elements(n) {
            if name(c) == "title" {
                let (title, _) = self.title(c);
                html.push_str(&format!(
                    "<p class=\"note-title\"><strong>{title}</strong></p>"
                ));
            } else {
                html.push_str(&self.block(c));
            }
        }
        if let Some((page, rid)) = n.attribute("id").and_then(|raw| self.first_ref.get(raw)) {
            html.push_str(&format!(
                "<p class=\"backlink\"><a href=\"{page}#{rid}\">↩</a></p>"
            ));
        }
        html.push_str("</aside>");
        self.push(&html);
    }

    /// Replaces every internal-link placeholder with the page its target landed on.
    pub fn resolve_links(&mut self) {
        let first = self
            .pages
            .first()
            .map(|p| p.name.clone())
            .unwrap_or_default();
        let mut missing = Vec::new();
        for page in &mut self.pages {
            if !page.html.contains(OPEN) {
                continue;
            }
            let mut out = String::with_capacity(page.html.len());
            let mut rest = page.html.as_str();
            while let Some(start) = rest.find(OPEN) {
                out.push_str(&rest[..start]);
                let after = &rest[start + OPEN.len_utf8()..];
                let end = after.find(CLOSE).expect("placeholders are closed");
                let id = &after[..end];
                match self.placed.get(id) {
                    Some(target) => out.push_str(&format!("{target}#{id}")),
                    None => {
                        missing.push(id.to_owned());
                        out.push_str(&first);
                    }
                }
                rest = &after[end + CLOSE.len_utf8()..];
            }
            out.push_str(rest);
            page.html = out;
        }
        for id in missing {
            self.warn(format!(
                "link target #{id} wasn't kept; it points at the start of the book"
            ));
        }
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

#[cfg(test)]
mod tests {
    use super::sanitize;

    #[test]
    fn ids_become_xml_names() {
        assert_eq!(sanitize("n_1"), "n_1");
        assert_eq!(sanitize("1"), "id-1");
        assert_eq!(sanitize("a b#c"), "a_b_c");
        assert_eq!(sanitize(""), "id-");
    }
}
