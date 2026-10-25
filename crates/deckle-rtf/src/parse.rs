// SPDX-License-Identifier: GPL-3.0-or-later
//! The RTF tree to blocks: the document header (fonts, styles, lists, info), then a walk of the
//! body with group-scoped formatting (RTF spec §2).

use std::collections::{HashMap, HashSet};

use sha2::{Digest, Sha256};

use crate::html::{Block, Fmt, Inline, escape, flatten};
use crate::lex::Node;

/// An image larger than this is skipped with a warning.
const MAX_IMAGE: usize = 10 * 1024 * 1024;
/// A bookmark link's target is written as OPEN id CLOSE and resolved once the chapters exist.
pub const OPEN: char = '\u{E000}';
pub const CLOSE: char = '\u{E001}';

/// Header destinations (and others) the body walk never shows.
const SKIP: &[&str] = &[
    "fonttbl",
    "colortbl",
    "stylesheet",
    "info",
    "listtable",
    "listoverridetable",
    "revtbl",
    "rsidtbl",
    "generator",
    "header",
    "headerl",
    "headerr",
    "headerf",
    "footer",
    "footerl",
    "footerr",
    "footerf",
    "pntext",
    "listtext",
    "nonshppict",
    "annotation",
    "atnid",
    "atnauthor",
    "atndate",
    "atnref",
    "atrfstart",
    "atrfend",
    "xe",
    "tc",
    "tcn",
    "txe",
    "filetbl",
    "themedata",
    "datastore",
    "latentstyles",
    "pgdsctbl",
    "userprops",
    "xmlnstbl",
    "mmathPr",
    "fldtype",
    "bkmkend",
    "objdata",
    "template",
    "pnseclvl",
    "ftnsep",
    "ftnsepc",
    "ftncn",
    "aftnsep",
    "aftnsepc",
    "aftncn",
    "panose",
    "falt",
    "fname",
    "listpicture",
    "background",
    "docvar",
    "wgrffmtfilter",
    "ud",
];

pub struct Image {
    pub href: String,
    pub media_type: &'static str,
    pub bytes: Vec<u8>,
}

#[derive(Default)]
pub struct Info {
    pub title: Option<String>,
    pub author: Option<String>,
    pub subject: Option<String>,
    pub doccomm: Option<String>,
    pub keywords: Option<String>,
    pub revtim: Option<String>,
}

/// What the document header says.
#[derive(Default)]
pub struct Header {
    pub ansicpg: u16,
    pub deflang: Option<i32>,
    deff: Option<i32>,
    /// Font number → code page.
    fonts: HashMap<i32, u16>,
    /// Paragraph style number → lowercased name.
    styles: HashMap<i32, String>,
    /// `\ls` number → (ordered, start) per level.
    lists: HashMap<i32, Vec<(bool, u32)>>,
    pub info: Info,
}

/// `\fcharsetN` to a Windows code page.
fn charset_page(charset: i32) -> Option<u16> {
    Some(match charset {
        0 => 1252,
        77 => 10000,
        128 => 932,
        129 => 949,
        134 => 936,
        136 => 950,
        161 => 1253,
        162 => 1254,
        163 => 1258,
        177 => 1255,
        178 => 1256,
        186 => 1257,
        204 => 1251,
        222 => 874,
        238 => 1250,
        _ => return None,
    })
}

/// A code page as an encoding; unknown ones read as Windows-1252.
pub fn encoding(page: u16) -> &'static encoding_rs::Encoding {
    let label = match page {
        874 => "windows-874",
        932 => "shift_jis",
        936 => "gbk",
        949 => "euc-kr",
        950 => "big5",
        1250..=1258 => {
            return encoding_rs::Encoding::for_label(format!("windows-{page}").as_bytes())
                .unwrap_or(encoding_rs::WINDOWS_1252);
        }
        10000 => "macintosh",
        20866 => "koi8-r",
        65001 => "utf-8",
        _ => "windows-1252",
    };
    encoding_rs::Encoding::for_label(label.as_bytes()).unwrap_or(encoding_rs::WINDOWS_1252)
}

fn decode(bytes: &[u8], page: u16) -> String {
    encoding(page)
        .decode_without_bom_handling(bytes)
        .0
        .into_owned()
}

/// The text of `nodes` (and their groups, apart from `\*` destinations), decoded with `page`.
pub fn plain_text(nodes: &[Node], page: u16) -> String {
    fn go(nodes: &[Node], page: u16, out: &mut String, bytes: &mut Vec<u8>, skip: &mut usize) {
        for n in nodes {
            let flush = |out: &mut String, bytes: &mut Vec<u8>| {
                if !bytes.is_empty() {
                    out.push_str(&decode(bytes, page));
                    bytes.clear();
                }
            };
            match n {
                Node::Text(t) => {
                    let start = (*skip).min(t.len());
                    *skip -= start;
                    bytes.extend_from_slice(&t[start..]);
                }
                Node::Hex(b) => {
                    if *skip > 0 {
                        *skip -= 1;
                    } else {
                        bytes.push(*b);
                    }
                }
                Node::Word(w, p) if w == "u" => {
                    flush(out, bytes);
                    if let Some(c) = unicode(*p) {
                        out.push(c);
                    }
                    *skip = 1;
                }
                Node::Symbol(c) => {
                    flush(out, bytes);
                    if let Some(s) = symbol(*c) {
                        out.push_str(s);
                    }
                }
                Node::Group(g) => {
                    flush(out, bytes);
                    if !matches!(g.first(), Some(Node::Symbol('*'))) {
                        go(g, page, out, bytes, skip);
                        flush(out, bytes);
                    }
                }
                _ => flush(out, bytes),
            }
        }
        if !bytes.is_empty() {
            out.push_str(&decode(bytes, page));
            bytes.clear();
        }
    }
    let mut out = String::new();
    go(nodes, page, &mut out, &mut Vec::new(), &mut 0);
    out
}

fn unicode(p: Option<i32>) -> Option<char> {
    let n = p?;
    let n = if n < 0 { n + 65536 } else { n };
    char::from_u32(n as u32)
}

fn symbol(c: char) -> Option<&'static str> {
    Some(match c {
        '~' => "\u{a0}",
        '_' => "\u{2011}",
        '{' => "{",
        '}' => "}",
        '\\' => "\\",
        _ => return None,
    })
}

/// A destination group's content after its `\*\name` or `\name`.
fn after_dest(g: &[Node]) -> &[Node] {
    match g {
        [Node::Symbol('*'), Node::Word(..), rest @ ..] => rest,
        [Node::Word(..), rest @ ..] => rest,
        rest => rest,
    }
}

fn word_of(nodes: &[Node], name: &str) -> Option<Option<i32>> {
    nodes.iter().find_map(|n| match n {
        Node::Word(w, p) if w == name => Some(*p),
        _ => None,
    })
}

fn groups<'a>(nodes: &'a [Node], dest: &'a str) -> impl Iterator<Item = &'a [Node]> + 'a {
    nodes.iter().filter_map(move |n| match n {
        Node::Group(g) if Node::destination(g).is_some_and(|(d, _)| d == dest) => {
            Some(g.as_slice())
        }
        _ => None,
    })
}

impl Header {
    pub fn parse(rtf: &[Node]) -> Header {
        let mut h = Header {
            ansicpg: 1252,
            ..Header::default()
        };
        for n in rtf {
            match n {
                Node::Word(w, p) => match w.as_str() {
                    "ansicpg" => h.ansicpg = p.and_then(|p| u16::try_from(p).ok()).unwrap_or(1252),
                    "mac" => h.ansicpg = 10000,
                    "pc" | "pca" => h.ansicpg = 437,
                    "deflang" => h.deflang = *p,
                    "deff" => h.deff = *p,
                    _ => {}
                },
                Node::Group(g) => match Node::destination(g).map(|d| d.0) {
                    Some("fonttbl") => h.fonts(g),
                    Some("stylesheet") => h.styles(g),
                    Some("listtable") => {}
                    Some("info") => h.info(g),
                    _ => {}
                },
                _ => {}
            }
        }
        h.lists(rtf);
        h
    }

    fn fonts(&mut self, g: &[Node]) {
        fn go(nodes: &[Node], current: &mut Option<i32>, fonts: &mut HashMap<i32, u16>) {
            for n in nodes {
                match n {
                    Node::Word(w, p) if w == "f" => *current = *p,
                    Node::Word(w, Some(p)) if w == "fcharset" => {
                        if let (Some(f), Some(page)) = (*current, charset_page(*p)) {
                            fonts.entry(f).or_insert(page);
                        }
                    }
                    Node::Word(w, Some(p)) if w == "cpg" => {
                        if let (Some(f), Ok(page)) = (*current, u16::try_from(*p)) {
                            fonts.insert(f, page);
                        }
                    }
                    Node::Group(g) if !matches!(g.first(), Some(Node::Symbol('*'))) => {
                        go(g, current, fonts)
                    }
                    _ => {}
                }
            }
        }
        go(g, &mut None, &mut self.fonts);
    }

    fn styles(&mut self, g: &[Node]) {
        for s in g.iter().filter_map(|n| match n {
            Node::Group(s) => Some(s),
            _ => None,
        }) {
            if matches!(s.first(), Some(Node::Symbol('*'))) {
                continue; // character, section and table styles
            }
            let number = word_of(s, "s").flatten().unwrap_or(0);
            let direct: Vec<Node> = s
                .iter()
                .filter(|n| !matches!(n, Node::Group(_)))
                .cloned()
                .collect();
            let name = plain_text(&direct, self.ansicpg);
            let name = name.trim().trim_end_matches(';').trim().to_lowercase();
            if !name.is_empty() {
                self.styles.insert(number, name);
            }
        }
    }

    fn lists(&mut self, rtf: &[Node]) {
        let mut by_id: HashMap<i32, Vec<(bool, u32)>> = HashMap::new();
        let mut overrides: Vec<(i32, i32)> = Vec::new();
        for g in rtf.iter().filter_map(|n| match n {
            Node::Group(g) => Some(g.as_slice()),
            _ => None,
        }) {
            match Node::destination(g).map(|d| d.0) {
                Some("listtable") => {
                    for list in groups(g, "list") {
                        let Some(Some(id)) = word_of(list, "listid") else {
                            continue;
                        };
                        let levels = groups(list, "listlevel")
                            .map(|l| {
                                let nfc = word_of(l, "levelnfc")
                                    .or_else(|| word_of(l, "levelnfcn"))
                                    .flatten()
                                    .unwrap_or(0);
                                let start = word_of(l, "levelstartat")
                                    .flatten()
                                    .and_then(|s| u32::try_from(s).ok())
                                    .unwrap_or(1);
                                (!matches!(nfc, 23 | 255), start)
                            })
                            .collect();
                        by_id.insert(id, levels);
                    }
                }
                Some("listoverridetable") => {
                    for o in groups(g, "listoverride") {
                        if let (Some(Some(id)), Some(Some(ls))) =
                            (word_of(o, "listid"), word_of(o, "ls"))
                        {
                            overrides.push((ls, id));
                        }
                    }
                }
                _ => {}
            }
        }
        for (ls, id) in overrides {
            if let Some(levels) = by_id.get(&id) {
                self.lists.insert(ls, levels.clone());
            }
        }
    }

    fn info(&mut self, g: &[Node]) {
        let page = self.ansicpg;
        let field = |name: &str| {
            groups(g, name)
                .next()
                .map(|f| plain_text(after_dest(f), page).trim().to_owned())
                .filter(|t| !t.is_empty())
        };
        self.info.title = field("title");
        self.info.author = field("author");
        self.info.subject = field("subject");
        self.info.doccomm = field("doccomm");
        self.info.keywords = field("keywords");
        self.info.revtim = groups(g, "revtim").next().and_then(|r| {
            let n = |w: &str| word_of(r, w).flatten();
            let (y, mo, d) = (n("yr")?, n("mo")?, n("dy")?);
            Some(format!(
                "{y:04}-{mo:02}-{d:02}T{:02}:{:02}:00Z",
                n("hr").unwrap_or(0),
                n("min").unwrap_or(0)
            ))
        });
    }

    fn page_of(&self, font: Option<i32>) -> u16 {
        font.or(self.deff)
            .and_then(|f| self.fonts.get(&f).copied())
            .unwrap_or(self.ansicpg)
    }
}

#[derive(Clone, Debug, Default)]
struct Para {
    style: Option<i32>,
    align: Option<&'static str>,
    outline: Option<u8>,
    ls: Option<i32>,
    ilvl: u8,
    intbl: bool,
    /// Old-style `\pn` numbering: (ordered, start).
    pn: Option<(bool, u32)>,
}

#[derive(Clone, Debug)]
struct State {
    fmt: Fmt,
    uc: usize,
    font: Option<i32>,
    hidden: bool,
    deleted: bool,
    para: Para,
}

#[derive(Clone, Copy, Debug, Default)]
struct CellDef {
    hmerge: bool,
    vfirst: bool,
    vmerge: bool,
}

#[derive(Default)]
struct Table {
    /// (header, cells as (definition, html))
    rows: Vec<(bool, Vec<(CellDef, String)>)>,
    def: Vec<CellDef>,
    pending: CellDef,
    header: bool,
    cells: Vec<String>,
    cell_blocks: Vec<Block>,
}

pub struct Walker<'h> {
    h: &'h Header,
    st: State,
    bytes: Vec<u8>,
    bytes_page: u16,
    skip: usize,
    inline: Inline,
    out: Vec<Block>,
    table: Option<Table>,
    pub notes: Vec<String>,
    pending_notes: Vec<usize>,
    footnotes: usize,
    endnotes: usize,
    headings: usize,
    paragraphs: usize,
    in_note: bool,
    pub images: Vec<Image>,
    image_index: HashMap<Vec<u8>, usize>,
    bookmarks: HashMap<String, String>,
    used: HashSet<String>,
    pub title: Option<String>,
    pub warnings: Vec<String>,
}

fn on(p: Option<i32>) -> bool {
    p != Some(0)
}

impl<'h> Walker<'h> {
    pub fn new(h: &'h Header) -> Self {
        Walker {
            h,
            st: State {
                fmt: Fmt::default(),
                uc: 1,
                font: None,
                hidden: false,
                deleted: false,
                para: Para::default(),
            },
            bytes: Vec::new(),
            bytes_page: h.ansicpg,
            skip: 0,
            inline: Inline::default(),
            out: Vec::new(),
            table: None,
            notes: Vec::new(),
            pending_notes: Vec::new(),
            footnotes: 0,
            endnotes: 0,
            headings: 0,
            paragraphs: 0,
            in_note: false,
            images: Vec::new(),
            image_index: HashMap::new(),
            bookmarks: HashMap::new(),
            used: HashSet::new(),
            title: None,
            warnings: Vec::new(),
        }
    }

    pub fn warn(&mut self, w: String) {
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

    fn bookmark_id(&mut self, name: &str) -> String {
        if let Some(id) = self.bookmarks.get(name) {
            return id.clone();
        }
        let id = self.unique(&sanitize(name));
        self.bookmarks.insert(name.to_owned(), id.clone());
        id
    }

    /// The whole document: the body of the `\rtf` group, then whatever is still open.
    pub fn run(&mut self, rtf: &[Node]) -> Vec<Block> {
        self.walk(rtf);
        self.flush_bytes();
        self.end_paragraph();
        self.finish_table();
        self.flush_notes();
        std::mem::take(&mut self.out)
    }

    fn page(&self) -> u16 {
        self.h.page_of(self.st.font)
    }

    fn flush_bytes(&mut self) {
        if self.bytes.is_empty() {
            return;
        }
        let s = decode(&self.bytes, self.bytes_page);
        self.bytes.clear();
        self.emit(&s);
    }

    fn push_bytes(&mut self, b: &[u8]) {
        let page = self.page();
        if page != self.bytes_page {
            self.flush_bytes();
            self.bytes_page = page;
        }
        self.bytes.extend_from_slice(b);
    }

    fn emit(&mut self, s: &str) {
        if self.st.hidden || self.st.deleted {
            return;
        }
        self.inline.text(self.st.fmt, s);
    }

    fn walk(&mut self, nodes: &[Node]) {
        for n in nodes {
            match n {
                Node::Group(g) => {
                    self.flush_bytes();
                    self.skip = 0;
                    self.group(g);
                }
                Node::Text(t) => {
                    let start = self.skip.min(t.len());
                    self.skip -= start;
                    self.push_bytes(&t[start..]);
                }
                Node::Hex(b) => {
                    if self.skip > 0 {
                        self.skip -= 1;
                    } else {
                        self.push_bytes(&[*b]);
                    }
                }
                Node::Bin(_) => {}
                Node::Symbol(c) => {
                    self.flush_bytes();
                    if self.skip > 0 {
                        self.skip -= 1;
                        continue;
                    }
                    match c {
                        '\n' => self.end_paragraph(),
                        c => {
                            if let Some(s) = symbol(*c) {
                                self.emit(s);
                            }
                        }
                    }
                }
                Node::Word(w, p) => {
                    self.flush_bytes();
                    self.word(w, *p);
                }
            }
        }
        self.flush_bytes();
    }

    fn group(&mut self, g: &[Node]) {
        if let Some((dest, star)) = Node::destination(g) {
            match dest {
                d if SKIP.contains(&d) => return,
                "pict" => return self.pict(g),
                "footnote" => return self.footnote(g),
                "field" => return self.field(g),
                "bkmkstart" => return self.bookmark(g),
                "pn" => return self.pn(g),
                "object" => return self.object(g),
                "shp" => return self.shape(g),
                "shppict" | "fldrslt" | "result" | "shprslt" | "shptxt" => {}
                _ if star => return,
                _ => {}
            }
        }
        let saved = self.st.clone();
        self.walk(g);
        self.st = saved;
    }

    fn word(&mut self, w: &str, p: Option<i32>) {
        let f = &mut self.st.fmt;
        match w {
            "par" => self.end_paragraph(),
            "pard" => self.st.para = Para::default(),
            "plain" => {
                self.st.fmt = Fmt::default();
                self.st.hidden = false;
                self.st.deleted = false;
            }
            "f" => self.st.font = p,
            "b" => f.bold = on(p),
            "i" => f.italic = on(p),
            "strike" | "striked" => f.strike = on(p),
            "scaps" => f.small_caps = on(p),
            "ulnone" => f.underline = false,
            "ulc" => {}
            w if w.starts_with("ul") => f.underline = on(p),
            "super" => {
                f.sup = on(p);
                f.sub = false;
            }
            "sub" => {
                f.sub = on(p);
                f.sup = false;
            }
            "nosupersub" => {
                f.sup = false;
                f.sub = false;
            }
            "up" => f.sup = p.unwrap_or(6) > 0,
            "dn" => f.sub = p.unwrap_or(6) > 0,
            "v" => self.st.hidden = on(p),
            "deleted" => self.st.deleted = on(p),
            "s" => self.st.para.style = p,
            "outlinelevel" => {
                self.st.para.outline = p.filter(|v| (0..=8).contains(v)).map(|v| v as u8 + 1)
            }
            "qc" => self.st.para.align = Some("center"),
            "qr" => self.st.para.align = Some("right"),
            "ql" | "qj" => self.st.para.align = None,
            "ls" => self.st.para.ls = p,
            "ilvl" => self.st.para.ilvl = p.unwrap_or(0).clamp(0, 8) as u8,
            "intbl" => self.st.para.intbl = true,
            "itap" => {
                let depth = p.unwrap_or(1);
                if depth > 1 {
                    self.warn("nested tables are flattened into their outer cell".into());
                }
                self.st.para.intbl = depth >= 1;
            }
            "line" => {
                if !(self.st.hidden || self.st.deleted) {
                    self.inline.raw("<br/>");
                }
            }
            "tab" | "emspace" | "enspace" | "qmspace" => self.emit(" "),
            "emdash" => self.emit("—"),
            "endash" => self.emit("–"),
            "lquote" => self.emit("‘"),
            "rquote" => self.emit("’"),
            "ldblquote" => self.emit("“"),
            "rdblquote" => self.emit("”"),
            "bullet" => self.emit("•"),
            "page" | "sect" => self.end_paragraph(),
            "uc" => self.st.uc = p.unwrap_or(1).clamp(0, 10) as usize,
            "u" => {
                if let Some(c) = unicode(p) {
                    self.emit(&c.to_string());
                }
                self.skip = self.st.uc;
            }
            "trowd" => {
                let t = self.table.get_or_insert_with(Table::default);
                t.def.clear();
                t.pending = CellDef::default();
                t.header = false;
            }
            "trhdr" => self.table.get_or_insert_with(Table::default).header = true,
            "clmgf" => {}
            "clmrg" => self.table.get_or_insert_with(Table::default).pending.hmerge = true,
            "clvmgf" => self.table.get_or_insert_with(Table::default).pending.vfirst = true,
            "clvmrg" => self.table.get_or_insert_with(Table::default).pending.vmerge = true,
            "cellx" => {
                let t = self.table.get_or_insert_with(Table::default);
                let d = std::mem::take(&mut t.pending);
                t.def.push(d);
            }
            "cell" => self.end_cell(),
            "nestcell" => {
                self.warn("nested tables are flattened into their outer cell".into());
                self.end_paragraph();
            }
            "row" => self.end_row(),
            _ => {}
        }
    }

    /// Ends the current paragraph: a heading, list item or `<p>`, into the open table cell or
    /// the document.
    fn end_paragraph(&mut self) {
        self.flush_bytes();
        let inline = std::mem::take(&mut self.inline);
        if inline.is_empty() {
            return;
        }
        self.paragraphs += 1;
        let para = self.st.para.clone();
        let style = para
            .style
            .and_then(|s| self.h.styles.get(&s))
            .cloned()
            .unwrap_or_default();
        let heading = para.outline.or_else(|| {
            style
                .strip_prefix("heading ")
                .and_then(|n| n.trim().parse::<u8>().ok())
                .filter(|n| (1..=9).contains(n))
        });
        let text = inline.plain_text();
        let block = if let Some(level) = heading.filter(|_| !self.in_note) {
            let level = level.min(6);
            self.headings += 1;
            let id = self.unique(&format!("h{}", self.headings));
            Block::Heading {
                level,
                html: format!("<h{level} id=\"{id}\">{}</h{level}>", inline.html(true)),
                id,
                text,
            }
        } else if para.ls.is_some() || para.pn.is_some() {
            let (ordered, start) = match (para.ls, para.pn) {
                (Some(ls), _) => self
                    .h
                    .lists
                    .get(&ls)
                    .and_then(|l| l.get(usize::from(para.ilvl)).copied())
                    .unwrap_or((false, 1)),
                (None, Some(pn)) => pn,
                _ => (false, 1),
            };
            Block::Item {
                ordered,
                start,
                level: para.ilvl,
                html: inline.html(false),
            }
        } else {
            let mut classes = Vec::new();
            let is_title = style == "title";
            let is_subtitle = style == "subtitle";
            if is_title {
                classes.push("title");
                if self.title.is_none() && !self.in_note && !text.is_empty() {
                    self.title = Some(text.clone());
                }
            } else if is_subtitle {
                classes.push("subtitle");
            }
            if let Some(a) = para.align {
                classes.push(a);
            }
            if inline.has_image && text.is_empty() {
                classes.push("figure");
            }
            let class = if classes.is_empty() {
                String::new()
            } else {
                format!(" class=\"{}\"", classes.join(" "))
            };
            let p = format!("<p{class}>{}</p>", inline.html(is_title || is_subtitle));
            Block::Html(
                if matches!(style.as_str(), "quote" | "quotations" | "block text") {
                    format!("<blockquote>{p}</blockquote>")
                } else {
                    p
                },
            )
        };
        if para.intbl {
            self.table
                .get_or_insert_with(Table::default)
                .cell_blocks
                .push(block);
        } else {
            self.finish_table();
            self.out.push(block);
            self.flush_notes();
        }
    }

    fn flush_notes(&mut self) {
        if !self.in_note && !self.pending_notes.is_empty() {
            self.out
                .push(Block::Notes(std::mem::take(&mut self.pending_notes)));
        }
    }

    fn end_cell(&mut self) {
        let intbl = self.st.para.intbl;
        self.st.para.intbl = true;
        self.end_paragraph();
        self.st.para.intbl = intbl;
        let t = self.table.get_or_insert_with(Table::default);
        let blocks = std::mem::take(&mut t.cell_blocks);
        t.cells.push(flatten(&blocks));
    }

    fn end_row(&mut self) {
        let t = self.table.get_or_insert_with(Table::default);
        let cells = std::mem::take(&mut t.cells);
        let row = cells
            .into_iter()
            .enumerate()
            .map(|(i, html)| (t.def.get(i).copied().unwrap_or_default(), html))
            .collect();
        t.rows.push((t.header, row));
    }

    fn finish_table(&mut self) {
        let Some(mut t) = self.table.take() else {
            return;
        };
        if !t.cells.is_empty() || !t.cell_blocks.is_empty() {
            let blocks = std::mem::take(&mut t.cell_blocks);
            t.cells.push(flatten(&blocks));
            let cells = std::mem::take(&mut t.cells);
            let row = cells
                .into_iter()
                .enumerate()
                .map(|(i, html)| (t.def.get(i).copied().unwrap_or_default(), html))
                .collect();
            t.rows.push((t.header, row));
        }
        if t.rows.is_empty() {
            return;
        }
        // (tag, html, colspan, rowspan) per row; a vertical merge start is remembered by column.
        let mut grid: Vec<Vec<(&str, String, u32, u32)>> = Vec::new();
        let mut vstart: HashMap<usize, (usize, usize)> = HashMap::new();
        for (header, row) in t.rows {
            let tag = if header { "th" } else { "td" };
            let r = grid.len();
            let mut out: Vec<(&str, String, u32, u32)> = Vec::new();
            for (col, (def, html)) in row.into_iter().enumerate() {
                if def.hmerge
                    && let Some(last) = out.last_mut()
                {
                    last.2 += 1;
                    continue;
                }
                if def.vmerge
                    && let Some(&(sr, sc)) = vstart.get(&col)
                {
                    grid[sr][sc].3 += 1;
                    continue;
                }
                if def.vfirst {
                    vstart.insert(col, (r, out.len()));
                } else {
                    vstart.remove(&col);
                }
                out.push((tag, html, 1, 1));
            }
            grid.push(out);
        }
        let mut html = String::from("<table>");
        for row in grid {
            html.push_str("<tr>");
            for (tag, cell, colspan, rowspan) in row {
                let mut attrs = String::new();
                if colspan > 1 {
                    attrs.push_str(&format!(" colspan=\"{colspan}\""));
                }
                if rowspan > 1 {
                    attrs.push_str(&format!(" rowspan=\"{rowspan}\""));
                }
                html.push_str(&format!("<{tag}{attrs}>{cell}</{tag}>"));
            }
            html.push_str("</tr>");
        }
        html.push_str("</table>");
        self.out.push(Block::Html(html));
        self.flush_notes();
    }

    fn footnote(&mut self, g: &[Node]) {
        self.flush_bytes();
        if self.st.hidden || self.st.deleted {
            return;
        }
        let endnote = word_of(g, "ftnalt").is_some();
        let saved_inline = std::mem::take(&mut self.inline);
        let saved_out = std::mem::take(&mut self.out);
        let saved_table = self.table.take();
        let saved_state = self.st.clone();
        let was_in_note = self.in_note;
        self.in_note = true;
        self.st.para = Para::default();
        self.walk(&g[1..]);
        self.end_paragraph();
        self.finish_table();
        let blocks = std::mem::replace(&mut self.out, saved_out);
        self.inline = saved_inline;
        self.table = saved_table;
        self.st = saved_state;
        self.in_note = was_in_note;

        let (prefix, number) = if endnote {
            self.endnotes += 1;
            ("en", self.endnotes)
        } else {
            self.footnotes += 1;
            ("fn", self.footnotes)
        };
        let id = self.unique(&format!("{prefix}{number}"));
        let body = flatten(&blocks);
        let back = format!("<a href=\"#{id}-ref\">{number}.</a> ");
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
        self.inline.raw(&format!(
            "<sup><a epub:type=\"noteref\" class=\"noteref\" href=\"#{id}\" id=\"{id}-ref\">{number}</a></sup>"
        ));
    }

    fn field(&mut self, g: &[Node]) {
        let inst = g
            .iter()
            .find_map(|n| match n {
                Node::Group(i) if Node::destination(i).is_some_and(|(d, _)| d == "fldinst") => {
                    Some(plain_text(after_dest(i), self.page()))
                }
                _ => None,
            })
            .unwrap_or_default();
        let inst = inst.trim();
        let kind = inst
            .split_whitespace()
            .next()
            .unwrap_or("")
            .to_ascii_uppercase();
        if kind == "TOC" {
            return; // the stored contents; Deckle builds its own
        }
        let target = if kind == "HYPERLINK" {
            let rest = inst[kind.len()..].trim();
            let (url, bookmark) = hyperlink_parts(rest);
            match (bookmark, url) {
                (Some(b), _) => {
                    let id = self.bookmark_id(&b);
                    Some(format!("{OPEN}{id}{CLOSE}"))
                }
                (None, Some(u)) => {
                    let lower = u.to_ascii_lowercase();
                    if ["http://", "https://", "mailto:"]
                        .iter()
                        .any(|s| lower.starts_with(s))
                    {
                        Some(escape(&u))
                    } else {
                        self.warn(format!("link to {u} isn't supported; kept its text"));
                        None
                    }
                }
                _ => None,
            }
        } else {
            None
        };
        let Some(result) = groups(g, "fldrslt").next() else {
            return;
        };
        self.flush_bytes();
        let mark = self.inline.mark();
        let paragraphs = self.paragraphs;
        self.group(result);
        self.flush_bytes();
        if let Some(href) = target
            && paragraphs == self.paragraphs
        {
            self.inline
                .wrap_from(mark, &format!("<a href=\"{href}\">"), "</a>");
        }
    }

    fn bookmark(&mut self, g: &[Node]) {
        let name = plain_text(after_dest(g), self.page()).trim().to_owned();
        if name.is_empty() {
            return;
        }
        let id = self.bookmark_id(&name);
        self.inline.raw(&format!("<a id=\"{id}\"></a>"));
    }

    /// Old-style paragraph numbering: sets this paragraph's list kind.
    fn pn(&mut self, g: &[Node]) {
        if word_of(g, "pnlvlcont").is_some() {
            return;
        }
        let ordered = word_of(g, "pnlvlblt").is_none();
        let start = word_of(g, "pnstart")
            .flatten()
            .and_then(|s| u32::try_from(s).ok())
            .unwrap_or(1);
        self.st.para.pn = Some((ordered, start));
    }

    fn object(&mut self, g: &[Node]) {
        match groups(g, "result").next() {
            Some(r) => self.group(r),
            None => {
                self.warn("embedded objects (charts, equations) aren't supported; left out".into())
            }
        }
    }

    /// A drawing shape: its text box's text and its picture, if any.
    fn shape(&mut self, g: &[Node]) {
        fn find<'a>(nodes: &'a [Node], dest: &str) -> Option<&'a [Node]> {
            for n in nodes {
                if let Node::Group(c) = n {
                    if Node::destination(c).is_some_and(|(d, _)| d == dest) {
                        return Some(c);
                    }
                    if let Some(found) = find(c, dest) {
                        return Some(found);
                    }
                }
            }
            None
        }
        let mut shown = false;
        if let Some(txt) = find(g, "shptxt") {
            self.group(txt);
            shown = true;
        }
        if let Some(pict) = find(g, "pict") {
            self.pict(pict);
            shown = true;
        }
        if !shown {
            self.warn("a drawing shape isn't supported; left out".into());
        }
    }

    fn pict(&mut self, g: &[Node]) {
        self.flush_bytes();
        if self.st.hidden || self.st.deleted {
            return;
        }
        let kind = g.iter().find_map(|n| match n {
            Node::Word(w, _)
                if matches!(
                    w.as_str(),
                    "pngblip"
                        | "jpegblip"
                        | "emfblip"
                        | "wmetafile"
                        | "macpict"
                        | "pmmetafile"
                        | "dibitmap"
                        | "wbitmap"
                ) =>
            {
                Some(w.as_str())
            }
            _ => None,
        });
        if !matches!(kind, Some("pngblip" | "jpegblip")) {
            let what = match kind {
                Some("emfblip") => "EMF",
                Some("wmetafile" | "pmmetafile") => "WMF",
                Some("macpict") => "PICT",
                Some(_) => "bitmap",
                None => "unknown",
            };
            self.warn(format!(
                "a {what} picture isn't supported (PNG and JPEG are); left out"
            ));
            return;
        }
        let mut bytes = Vec::new();
        let mut nibble: Option<u8> = None;
        for n in g {
            match n {
                Node::Bin(b) => bytes.extend_from_slice(b),
                Node::Text(t) => {
                    for &c in t {
                        let Some(v) = (c as char).to_digit(16) else {
                            continue;
                        };
                        match nibble.take() {
                            Some(hi) => bytes.push(hi << 4 | v as u8),
                            None => nibble = Some(v as u8),
                        }
                    }
                }
                _ => {}
            }
        }
        let alt = groups(g, "picprop")
            .next()
            .and_then(|pp| {
                groups(pp, "sp").find_map(|sp| {
                    let sn = groups(sp, "sn").next()?;
                    let name = plain_text(after_dest(sn), self.h.ansicpg);
                    if name.trim() != "wzDescription" {
                        return None;
                    }
                    let sv = groups(sp, "sv").next()?;
                    Some(plain_text(after_dest(sv), self.h.ansicpg).trim().to_owned())
                })
            })
            .unwrap_or_default();
        let Some(idx) = self.package(bytes) else {
            return;
        };
        self.inline.has_image = true;
        self.inline.raw(&format!(
            "<img src=\"{}\" alt=\"{}\"/>",
            self.images[idx].href,
            escape(&alt)
        ));
    }

    fn package(&mut self, bytes: Vec<u8>) -> Option<usize> {
        let key = Sha256::digest(&bytes).to_vec();
        if let Some(&i) = self.image_index.get(&key) {
            return Some(i);
        }
        if bytes.len() > MAX_IMAGE {
            self.warn("a picture is over 10 MB; left out".into());
            return None;
        }
        // Signatures, not the blip type: a mislabelled picture can't be passed through.
        let (ext, media_type) = if bytes.starts_with(b"\x89PNG\r\n\x1a\n") {
            ("png", "image/png")
        } else if bytes.starts_with(b"\xff\xd8\xff") {
            ("jpg", "image/jpeg")
        } else if bytes.starts_with(b"GIF87a") || bytes.starts_with(b"GIF89a") {
            ("gif", "image/gif")
        } else {
            self.warn("a picture's data isn't a PNG, JPEG or GIF; left out".into());
            return None;
        };
        self.images.push(Image {
            href: format!("images/image-{:03}.{ext}", self.images.len() + 1),
            media_type,
            bytes,
        });
        let i = self.images.len() - 1;
        self.image_index.insert(key, i);
        Some(i)
    }
}

/// `"https://x" \o "tip"` → (url, None); `\l "bookmark"` → (None, Some(bookmark)).
fn hyperlink_parts(rest: &str) -> (Option<String>, Option<String>) {
    let mut url = None;
    let mut bookmark = None;
    let mut chars = rest.chars().peekable();
    let mut switch: Option<char> = None;
    while let Some(c) = chars.next() {
        match c {
            '\\' => switch = chars.next(),
            '"' => {
                let value: String = chars.by_ref().take_while(|c| *c != '"').collect();
                match switch.take() {
                    Some('l') => bookmark = Some(value),
                    Some(_) => {}
                    None if url.is_none() => url = Some(value),
                    None => {}
                }
            }
            c if c.is_whitespace() => {}
            _ => {
                let mut value = String::from(c);
                while let Some(&n) = chars.peek() {
                    if n.is_whitespace() {
                        break;
                    }
                    value.push(n);
                    chars.next();
                }
                match switch.take() {
                    Some('l') => bookmark = Some(value),
                    Some(_) => {}
                    None if url.is_none() => url = Some(value),
                    None => {}
                }
            }
        }
    }
    (url, bookmark)
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
    use super::hyperlink_parts;

    #[test]
    fn hyperlink_instructions() {
        assert_eq!(
            hyperlink_parts("\"https://example.com/a b\" \\o \"Tip\""),
            (Some("https://example.com/a b".into()), None)
        );
        assert_eq!(
            hyperlink_parts("\\l \"Plate One\""),
            (None, Some("Plate One".into()))
        );
        assert_eq!(
            hyperlink_parts("https://x.org"),
            (Some("https://x.org".into()), None)
        );
    }
}
