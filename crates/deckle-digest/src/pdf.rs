// SPDX-License-Identifier: GPL-3.0-or-later
//! The PDF edition (docs/superpowers/specs/2026-10-23-pdf-edition-design.md): the paper laid
//! out for one tablet's page, for a reMarkable and `format = "pdf"` export folders.
//!
//! Deckle lays the pages out itself on `pdf-writer`: each story's XHTML becomes blocks
//! (paragraphs of styled runs, images, rules, tables), the blocks become pages of positioned
//! text, images and lines, and the pages become a PDF with the paper's own fonts embedded and a
//! ToUnicode map, an outline, and links from the front page to the sections.

use std::collections::{BTreeMap, BTreeSet};
use std::io::Write;

use crate::masthead::Masthead;
use ab_glyph::{Font, FontRef, GlyphId};
use pdf_writer::types::{
    ActionType, AnnotationType, CidFontType, FontFlags, SystemInfo, UnicodeCmap,
};
use pdf_writer::{Content, Filter, Finish, Name, Pdf, Rect, Ref, Str, TextStr};
use serde::{Deserialize, Serialize};

use crate::config::Photos;
use crate::sources::feeds::photo_token;
use crate::{Gathered, long_date};

const REGULAR: &[u8] = include_bytes!("../fonts/OldStandard-Regular.ttf");
const BOLD: &[u8] = include_bytes!("../fonts/OldStandard-Bold.ttf");
const ITALIC: &[u8] = include_bytes!("../fonts/OldStandard-Italic.ttf");

/// A screen the PDF edition is laid out for: its page is the screen's size.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Screen {
    #[default]
    #[serde(rename = "remarkable-2")]
    Remarkable2,
    RemarkablePaperPro,
    RemarkablePaperProMove,
    Ipad,
    A5,
}

impl Screen {
    /// The page, in points: the screen's pixels at its dpi (spec §5).
    pub fn page(self) -> (f32, f32) {
        let px = |w: f32, h: f32, dpi: f32| (w / dpi * 72.0, h / dpi * 72.0);
        match self {
            Screen::Remarkable2 => px(1404.0, 1872.0, 226.0),
            Screen::RemarkablePaperPro => px(1620.0, 2160.0, 229.0),
            Screen::RemarkablePaperProMove => px(954.0, 1696.0, 264.0),
            Screen::Ipad => px(1640.0, 2360.0, 264.0),
            Screen::A5 => (420.0, 595.0),
        }
    }

    /// The photos it shows best: in color on a color screen.
    pub fn photos(self) -> Photos {
        match self {
            Screen::RemarkablePaperPro | Screen::Ipad => Photos::Color,
            _ => Photos::Gray,
        }
    }
}

// ---- fonts ------------------------------------------------------------------------------------

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
enum Face {
    Regular,
    Bold,
    Italic,
    Masthead,
}

const FACES: [Face; 4] = [Face::Regular, Face::Bold, Face::Italic, Face::Masthead];

impl Face {
    fn index(self) -> usize {
        self as usize
    }
}

struct Fonts {
    faces: [FontRef<'static>; 4],
    /// The masthead face's file, as embedded.
    masthead: Masthead,
}

impl Fonts {
    fn new(masthead: Masthead) -> Self {
        let f = |b: &'static [u8]| FontRef::try_from_slice(b).expect("checked font");
        Fonts {
            faces: [f(REGULAR), f(BOLD), f(ITALIC), f(masthead.bytes())],
            masthead,
        }
    }

    fn font(&self, face: Face) -> &FontRef<'static> {
        &self.faces[face.index()]
    }

    fn upm(&self, face: Face) -> f32 {
        self.font(face).units_per_em().unwrap_or(1000.0)
    }

    /// `text` as glyphs of `face`, each with the character it stands for. A character the font
    /// lacks is transliterated first ("ł" → "l"); what still has no glyph is left out.
    fn shape(&self, face: Face, text: &str) -> Vec<(u16, char)> {
        let font = self.font(face);
        let mut out = Vec::with_capacity(text.len());
        for c in text.chars() {
            let g = font.glyph_id(c);
            if g.0 != 0 {
                out.push((g.0, c));
                continue;
            }
            for t in deunicode::deunicode_char(c).unwrap_or("").chars() {
                let g = font.glyph_id(t);
                if g.0 != 0 {
                    out.push((g.0, t));
                }
            }
        }
        out
    }

    fn width(&self, face: Face, size: f32, text: &str) -> f32 {
        let font = self.font(face);
        let units: f32 = self
            .shape(face, text)
            .iter()
            .map(|(g, _)| font.h_advance_unscaled(GlyphId(*g)))
            .sum();
        units / self.upm(face) * size
    }
}

// ---- blocks -----------------------------------------------------------------------------------

#[derive(Clone, Debug, PartialEq)]
struct Run {
    face: Face,
    size: f32,
    text: String,
}

#[derive(Clone, Copy, Debug, PartialEq)]
enum Align {
    Left,
    Center,
}

#[derive(Clone, Debug)]
struct Para {
    runs: Vec<Run>,
    align: Align,
    indent: f32,
    before: f32,
    after: f32,
    /// A headline: never the last thing on a page.
    keep: bool,
    /// A link to an anchor: the whole paragraph is the link.
    link: Option<usize>,
    bullet: bool,
}

impl Para {
    fn new(runs: Vec<Run>) -> Self {
        Para {
            runs,
            align: Align::Left,
            indent: 0.0,
            before: 0.0,
            after: 0.0,
            keep: false,
            link: None,
            bullet: false,
        }
    }
}

#[derive(Clone, Debug, Default)]
struct Cell {
    runs: Vec<Run>,
    image: Option<usize>,
    black: bool,
    number: Option<String>,
}

#[derive(Clone, Debug)]
enum Block {
    Para(Para),
    Image { image: usize },
    Rule { thick: f32 },
    Table { rows: Vec<Vec<Cell>>, grid: bool },
    Space(f32),
    PageBreak,
    Anchor(usize),
}

/// An image to embed: a JPEG as it is, or pixels deflated.
#[derive(Clone, Debug)]
enum Image {
    Jpeg {
        bytes: Vec<u8>,
        w: u32,
        h: u32,
        gray: bool,
    },
    Rgb {
        pixels: Vec<u8>,
        w: u32,
        h: u32,
    },
}

impl Image {
    fn size(&self) -> (u32, u32) {
        match self {
            Image::Jpeg { w, h, .. } | Image::Rgb { w, h, .. } => (*w, *h),
        }
    }

    fn jpeg(bytes: Vec<u8>) -> Option<Image> {
        use image::ImageDecoder;
        let d = image::codecs::jpeg::JpegDecoder::new(std::io::Cursor::new(&bytes)).ok()?;
        let (w, h) = d.dimensions();
        let gray = match d.color_type() {
            image::ColorType::L8 => true,
            image::ColorType::Rgb8 => false,
            _ => return None,
        };
        Some(Image::Jpeg { bytes, w, h, gray })
    }

    /// Any picture the `image` crate reads, flattened onto white.
    fn decoded(bytes: &[u8]) -> Option<Image> {
        let img = image::load_from_memory(bytes).ok()?.to_rgba8();
        let (w, h) = img.dimensions();
        let mut pixels = Vec::with_capacity((w * h * 3) as usize);
        for p in img.pixels() {
            let a = u16::from(p.0[3]);
            for c in &p.0[..3] {
                pixels.push(((u16::from(*c) * a + 255 * (255 - a)) / 255) as u8);
            }
        }
        Some(Image::Rgb { pixels, w, h })
    }
}

/// Sizes, from the body size: the page's width sets it (spec §5).
#[derive(Clone, Copy, Debug)]
struct Type {
    body: f32,
}

impl Type {
    fn for_width(w: f32) -> Self {
        Type {
            body: (w / 44.7).clamp(7.5, 11.0),
        }
    }
    fn leading(self, size: f32) -> f32 {
        size * 1.35
    }
    fn meta(self) -> f32 {
        self.body * 0.85
    }
    fn headline(self) -> f32 {
        self.body * 1.5
    }
    fn section(self) -> f32 {
        self.body * 2.0
    }
}

/// Turns the edition into blocks, with the outline it carries.
struct Doc {
    blocks: Vec<Block>,
    images: Vec<Image>,
    /// Outline entries: depth (0 or 1), title, anchor.
    outline: Vec<(u8, String, usize)>,
    anchors: usize,
    t: Type,
    photos: Photos,
}

impl Doc {
    fn anchor(&mut self) -> usize {
        self.anchors += 1;
        self.blocks.push(Block::Anchor(self.anchors - 1));
        self.anchors - 1
    }

    fn run(&self, face: Face, size: f32, text: &str) -> Run {
        Run {
            face,
            size,
            text: text.to_owned(),
        }
    }

    fn para(&mut self, p: Para) {
        if p.runs.iter().any(|r| !r.text.trim().is_empty()) {
            self.blocks.push(Block::Para(p));
        }
    }

    fn photo(&mut self, jpeg: &[u8]) -> Option<usize> {
        let bytes = crate::render::page_photo(jpeg, self.photos);
        let img = Image::jpeg(bytes.clone()).or_else(|| Image::decoded(&bytes))?;
        self.images.push(img);
        Some(self.images.len() - 1)
    }

    fn asset(&mut self, png: &[u8]) -> Option<usize> {
        let img = Image::decoded(png)?;
        self.images.push(img);
        Some(self.images.len() - 1)
    }
}

/// The blocks of the whole edition: the front page, then each section from a new page.
fn document(g: &Gathered, t: Type, photos: Photos) -> Doc {
    let e = &g.edition;
    let mut d = Doc {
        blocks: Vec::new(),
        images: Vec::new(),
        outline: Vec::new(),
        anchors: 0,
        t,
        photos,
    };
    // Each section's anchor is known before the front page links to it.
    let section_anchor: Vec<usize> = (0..e.sections.len()).collect();
    d.anchors = e.sections.len();
    let front = d.anchor();
    d.outline.push((0, "Front page".into(), front));
    front_page(&mut d, g, &section_anchor);
    for (i, s) in e.sections.iter().enumerate() {
        d.blocks.push(Block::PageBreak);
        d.blocks.push(Block::Anchor(section_anchor[i]));
        d.outline.push((0, s.title.clone(), section_anchor[i]));
        let mut head = Para::new(vec![d.run(Face::Bold, t.section(), &s.title)]);
        head.keep = true;
        d.para(head);
        d.blocks.push(Block::Rule { thick: 1.2 });
        d.blocks.push(Block::Space(t.body * 0.4));
        if let Some(k) = &s.kicker {
            let mut p = Para::new(vec![d.run(Face::Italic, t.body, k)]);
            p.after = t.body * 0.5;
            d.para(p);
        }
        if !s.brief.is_empty() {
            d.para(Para::new(vec![d.run(
                Face::Italic,
                t.body,
                "AI-written section brief",
            )]));
            for line in &s.brief {
                d.para(Para::new(vec![d.run(Face::Regular, t.body, line)]));
            }
        }
        for (j, a) in s.articles.iter().enumerate() {
            if j > 0 {
                d.blocks.push(Block::Space(t.body * 0.6));
                d.blocks.push(Block::Rule { thick: 0.4 });
                d.blocks.push(Block::Space(t.body * 0.6));
            }
            let at = d.anchor();
            d.outline.push((1, a.title.clone(), at));
            let mut h = Para::new(vec![d.run(Face::Bold, t.headline(), &a.title)]);
            h.keep = true;
            h.after = t.body * 0.25;
            d.para(h);
            if let Some(m) = &a.meta {
                let mut p = Para::new(vec![d.run(Face::Italic, t.meta(), m)]);
                p.keep = true;
                p.after = t.body * 0.4;
                d.para(p);
            }
            body(&mut d, &a.body, &a.photos, &e.assets);
        }
        if let Some(u) = &s.unavailable {
            let p = Para::new(vec![d.run(Face::Italic, t.body, u)]);
            d.para(p);
        }
    }
    d
}

fn front_page(d: &mut Doc, g: &Gathered, sections: &[usize]) {
    let e = &g.edition;
    let t = d.t;
    let mut ears = format!("No. {}", e.number());
    if !e.place.is_empty() {
        ears.push_str(&format!(" · {}", e.place));
    }
    let mut p = Para::new(vec![d.run(Face::Italic, t.meta(), &ears)]);
    p.align = Align::Center;
    d.para(p);
    // The masthead's size is set at layout, to fit the width (`Size::Fit`).
    let mut p = Para::new(vec![d.run(Face::Masthead, MASTHEAD_FIT, &e.paper)]);
    p.align = Align::Center;
    d.para(p);
    d.blocks.push(Block::Rule { thick: 0.8 });
    let mut p = Para::new(vec![d.run(Face::Regular, t.meta(), &long_date(e.date))]);
    p.align = Align::Center;
    d.para(p);
    d.blocks.push(Block::Rule { thick: 0.8 });
    if let Some(w) = &e.weather_line {
        let mut p = Para::new(vec![d.run(Face::Italic, t.body, w)]);
        p.align = Align::Center;
        p.after = t.body * 0.5;
        d.para(p);
    }
    let plan = crate::front::front_plan(g);
    if let Some((s, a)) = plan.lead {
        let art = &e.sections[s].articles[a];
        let mut h = Para::new(vec![d.run(Face::Bold, t.headline() * 1.2, &art.title)]);
        h.before = t.body * 0.6;
        h.link = Some(sections[s]);
        d.para(h);
        let meta = match &art.meta {
            Some(m) => format!("{} · {m}", e.sections[s].title),
            None => e.sections[s].title.clone(),
        };
        let p = Para::new(vec![d.run(Face::Italic, t.meta(), &meta)]);
        d.para(p);
        if let Some(teaser) = &art.teaser {
            let mut p = Para::new(vec![d.run(Face::Regular, t.body, teaser)]);
            p.before = t.body * 0.3;
            d.para(p);
        }
    }
    if !plan.tiles.is_empty() {
        d.blocks.push(Block::Space(t.body * 0.6));
        d.blocks.push(Block::Rule { thick: 0.4 });
        for &(s, a) in &plan.tiles {
            let mut p = Para::new(vec![
                d.run(Face::Bold, t.meta(), &format!("{} — ", e.sections[s].title)),
                d.run(Face::Regular, t.body, &e.sections[s].articles[a].title),
            ]);
            p.before = t.body * 0.35;
            p.link = Some(sections[s]);
            d.para(p);
        }
    }
    let mut h = Para::new(vec![d.run(Face::Bold, t.headline(), "Inside today")]);
    h.before = t.body;
    h.keep = true;
    d.para(h);
    for (i, s) in e.sections.iter().enumerate() {
        let count = match (s.articles.len(), &s.unavailable) {
            (0, Some(u)) if u == crate::sources::feeds::NO_NEW_STORIES => "no new stories".into(),
            (0, Some(_)) => "unavailable".to_owned(),
            (1, _) => "1 item".to_owned(),
            (n, _) => format!("{n} items"),
        };
        let mut p = Para::new(vec![
            d.run(Face::Regular, t.body, &s.title),
            d.run(Face::Italic, t.meta(), &format!("  {count}")),
        ]);
        p.bullet = true;
        p.link = Some(sections[i]);
        d.para(p);
    }
}

/// A marker size: the masthead is sized at layout to fill the column.
const MASTHEAD_FIT: f32 = -1.0;

/// HTML's named entities, but XML's own five, as characters; roxmltree knows only XML's.
fn xml_entities(s: &str) -> String {
    static NAMED: std::sync::LazyLock<regex::Regex> =
        std::sync::LazyLock::new(|| regex::Regex::new(r"&([A-Za-z][A-Za-z0-9]*);").unwrap());
    NAMED
        .replace_all(s, |c: &regex::Captures| {
            let name = &c[1];
            if matches!(name, "amp" | "lt" | "gt" | "quot" | "apos") {
                return c[0].to_owned();
            }
            let decoded = html_escape::decode_html_entities(&c[0]).into_owned();
            if decoded == c[0] {
                String::new()
            } else {
                crate::esc(&decoded)
            }
        })
        .into_owned()
}

/// Style while walking a story's XHTML.
#[derive(Clone, Copy)]
struct Style {
    face: Face,
    size: f32,
}

/// One story's XHTML as blocks. A body that isn't XML falls back to its text.
fn body(d: &mut Doc, xhtml: &str, photos: &[Vec<u8>], assets: &[(String, Vec<u8>)]) {
    let wrapped = format!("<root>{}</root>", xml_entities(xhtml));
    let style = Style {
        face: Face::Regular,
        size: d.t.body,
    };
    match roxmltree::Document::parse(&wrapped) {
        Ok(doc) => {
            let mut runs = Vec::new();
            let mut cx = Walk {
                photos,
                assets,
                indent: 0.0,
            };
            for n in doc.root_element().children() {
                walk(d, &mut cx, n, style, &mut runs);
            }
            flush(d, &mut runs, Para::new(Vec::new()));
        }
        Err(_) => {
            static TAG: std::sync::LazyLock<regex::Regex> =
                std::sync::LazyLock::new(|| regex::Regex::new(r"<[^>]*>").unwrap());
            for chunk in xhtml.split("</p>") {
                let text = html_escape::decode_html_entities(&TAG.replace_all(chunk, " "))
                    .split_whitespace()
                    .collect::<Vec<_>>()
                    .join(" ");
                let mut p = Para::new(vec![d.run(Face::Regular, d.t.body, &text)]);
                p.after = d.t.body * 0.45;
                d.para(p);
            }
        }
    }
}

struct Walk<'a> {
    photos: &'a [Vec<u8>],
    assets: &'a [(String, Vec<u8>)],
    indent: f32,
}

fn has_class(n: roxmltree::Node, class: &str) -> bool {
    n.attribute("class")
        .is_some_and(|c| c.split_whitespace().any(|x| x == class))
}

/// Writes the runs gathered so far as a paragraph shaped like `like`.
fn flush(d: &mut Doc, runs: &mut Vec<Run>, like: Para) {
    if runs.is_empty() {
        return;
    }
    // Whitespace runs at either end go; the paragraph reads from its first word.
    let mut p = like;
    p.runs = std::mem::take(runs);
    d.para(p);
}

fn img_index(d: &mut Doc, cx: &Walk, src: &str) -> Option<usize> {
    if let Some(n) = (0..cx.photos.len()).find(|n| src == photo_token(*n)) {
        return d.photo(&cx.photos[n]);
    }
    let png = cx.assets.iter().find(|(href, _)| href == src)?;
    d.asset(&png.1.clone())
}

fn walk(d: &mut Doc, cx: &mut Walk, n: roxmltree::Node, style: Style, runs: &mut Vec<Run>) {
    if n.is_text() {
        if let Some(text) = n.text() {
            runs.push(Run {
                face: style.face,
                size: style.size,
                text: text.to_owned(),
            });
        }
        return;
    }
    if !n.is_element() {
        return;
    }
    let tag = n.tag_name().name().to_ascii_lowercase();
    let t = d.t;
    let small = [
        "meta",
        "credit",
        "note",
        "small",
        "subhead",
        "kicker",
        "unavailable",
    ]
    .iter()
    .any(|c| has_class(n, c));
    match tag.as_str() {
        "script" | "style" | "head" | "title" => {}
        "br" => {
            let mut p = Para::new(Vec::new());
            p.indent = cx.indent;
            flush(d, runs, p);
        }
        "hr" => {
            flush(d, runs, Para::new(Vec::new()));
            d.blocks.push(Block::Rule { thick: 0.4 });
        }
        "img" => {
            flush(d, runs, Para::new(Vec::new()));
            if let Some(i) = n.attribute("src").and_then(|s| img_index(d, cx, s)) {
                d.blocks.push(Block::Image { image: i });
            }
        }
        "table" => {
            flush(d, runs, Para::new(Vec::new()));
            table(d, cx, n);
        }
        "p" | "h1" | "h2" | "h3" | "h4" | "h5" | "h6" | "li" | "dt" | "dd" | "figcaption"
        | "blockquote" | "div" | "section" | "aside" | "figure" | "ul" | "ol" | "nav" => {
            let mut before = Para::new(Vec::new());
            before.indent = cx.indent;
            flush(d, runs, before);
            let heading = tag.len() == 2 && tag.starts_with('h');
            let mut inner = match tag.as_str() {
                "h1" | "h2" => Style {
                    face: Face::Bold,
                    size: t.body * 1.25,
                },
                "h3" | "h4" | "h5" | "h6" => Style {
                    face: Face::Bold,
                    size: t.body * 1.05,
                },
                "figcaption" => Style {
                    face: Face::Italic,
                    size: t.meta(),
                },
                _ => style,
            };
            if small {
                inner = Style {
                    face: Face::Italic,
                    size: t.meta(),
                };
            }
            let indent = cx.indent;
            if matches!(tag.as_str(), "blockquote" | "ul" | "ol") {
                cx.indent += t.body * 1.2;
            }
            let mut own = Vec::new();
            for c in n.children() {
                walk(d, cx, c, inner, &mut own);
            }
            let mut p = Para::new(Vec::new());
            p.indent = cx.indent;
            p.bullet = tag == "li";
            p.keep = heading;
            p.before = if heading { t.body * 0.5 } else { 0.0 };
            p.after = match tag.as_str() {
                "p" | "blockquote" | "figcaption" | "dd" => t.body * 0.45,
                _ if heading => t.body * 0.2,
                "li" => t.body * 0.15,
                _ => 0.0,
            };
            flush(d, &mut own, p);
            cx.indent = indent;
        }
        "em" | "i" | "cite" | "var" => {
            let s = Style {
                face: Face::Italic,
                ..style
            };
            for c in n.children() {
                walk(d, cx, c, s, runs);
            }
        }
        "strong" | "b" | "th" => {
            let s = Style {
                face: Face::Bold,
                ..style
            };
            for c in n.children() {
                walk(d, cx, c, s, runs);
            }
        }
        "small" | "sup" | "sub" => {
            let s = Style {
                size: style.size * 0.8,
                ..style
            };
            for c in n.children() {
                walk(d, cx, c, s, runs);
            }
        }
        _ => {
            let s = if small {
                Style {
                    face: Face::Italic,
                    size: t.meta(),
                }
            } else {
                style
            };
            for c in n.children() {
                walk(d, cx, c, s, runs);
            }
        }
    }
}

/// A table: its rows of cells, each cell's text, picture, black fill and corner number.
fn table(d: &mut Doc, cx: &mut Walk, n: roxmltree::Node) {
    let grid = has_class(n, "grid");
    let size = if has_class(n, "small") {
        d.t.meta()
    } else {
        d.t.body
    };
    let mut rows = Vec::new();
    for tr in n.descendants().filter(|x| x.has_tag_name("tr")) {
        let mut row = Vec::new();
        for td in tr
            .children()
            .filter(|x| x.has_tag_name("td") || x.has_tag_name("th"))
        {
            let mut cell = Cell {
                black: has_class(td, "b"),
                ..Default::default()
            };
            let face = if td.has_tag_name("th") {
                Face::Bold
            } else {
                Face::Regular
            };
            collect_cell(d, cx, td, face, size, &mut cell);
            row.push(cell);
        }
        if !row.is_empty() {
            rows.push(row);
        }
    }
    if !rows.is_empty() {
        d.blocks.push(Block::Table { rows, grid });
    }
}

fn collect_cell(
    d: &mut Doc,
    cx: &mut Walk,
    n: roxmltree::Node,
    face: Face,
    size: f32,
    cell: &mut Cell,
) {
    for c in n.children() {
        if c.is_text() {
            let text = c.text().unwrap_or_default();
            if !text.trim().is_empty() || !cell.runs.is_empty() {
                cell.runs.push(Run {
                    face,
                    size,
                    text: text.to_owned(),
                });
            }
        } else if c.has_tag_name("img") {
            if cell.image.is_none() {
                cell.image = c.attribute("src").and_then(|s| img_index(d, cx, s));
            }
        } else if c.has_tag_name("span") && has_class(c, "n") {
            cell.number = Some(c.text().unwrap_or_default().trim().to_owned());
        } else if c.has_tag_name("br") {
            cell.runs.push(Run {
                face,
                size,
                text: "\n".into(),
            });
        } else if c.is_element() {
            let f = match c.tag_name().name() {
                "b" | "strong" => Face::Bold,
                "i" | "em" => Face::Italic,
                _ => face,
            };
            collect_cell(d, cx, c, f, size, cell);
        }
    }
}

// ---- layout -----------------------------------------------------------------------------------

/// What a page draws, in points from its top-left corner.
#[derive(Clone, Debug, PartialEq)]
enum Op {
    /// Text whose baseline starts at (`x`, `y`).
    Text {
        x: f32,
        y: f32,
        face: Face,
        size: f32,
        text: String,
    },
    Image {
        image: usize,
        x: f32,
        y: f32,
        w: f32,
        h: f32,
    },
    Line {
        x1: f32,
        y1: f32,
        x2: f32,
        y2: f32,
        width: f32,
    },
    /// A rectangle, filled black or stroked.
    Rect {
        x: f32,
        y: f32,
        w: f32,
        h: f32,
        fill: bool,
    },
}

/// A link on a page to an anchor, its box from the top-left corner.
#[derive(Clone, Debug, PartialEq)]
struct Link {
    page: usize,
    x: f32,
    y: f32,
    w: f32,
    h: f32,
    anchor: usize,
}

struct Layout<'a> {
    fonts: &'a Fonts,
    t: Type,
    page_w: f32,
    page_h: f32,
    left: f32,
    width: f32,
    top: f32,
    bottom: f32,
    pages: Vec<Vec<Op>>,
    y: f32,
    /// Each anchor's page and height.
    anchors: Vec<Option<(usize, f32)>>,
    links: Vec<Link>,
}

/// A laid-out line: its pieces and their widths.
type Line = Vec<(Face, f32, String, f32)>;

impl<'a> Layout<'a> {
    fn new(fonts: &'a Fonts, page: (f32, f32), t: Type, anchors: usize) -> Self {
        let (page_w, page_h) = page;
        let margin = page_w * 0.06;
        Layout {
            fonts,
            t,
            page_w,
            page_h,
            left: margin,
            width: page_w - 2.0 * margin,
            top: margin,
            bottom: page_h - margin - t.meta() * 2.0,
            pages: vec![Vec::new()],
            y: margin,
            anchors: vec![None; anchors],
            links: Vec::new(),
        }
    }

    fn page(&self) -> usize {
        self.pages.len() - 1
    }

    fn at_top(&self) -> bool {
        self.y <= self.top + 0.01
    }

    fn new_page(&mut self) {
        if !self.pages.last().is_some_and(Vec::is_empty) {
            self.pages.push(Vec::new());
        }
        self.y = self.top;
    }

    /// Makes room for `h` more points, on a new page if this one is too full.
    fn room(&mut self, h: f32) {
        self.room_with(h, 0.0);
    }

    /// [`Layout::room`], letting the line run `slack` points into the space above the footer.
    fn room_with(&mut self, h: f32, slack: f32) {
        if self.y + h > self.bottom + slack && !self.at_top() {
            self.new_page();
        }
    }

    fn op(&mut self, op: Op) {
        self.pages.last_mut().expect("a page").push(op);
    }

    /// Breaks `runs` into lines no wider than `width`.
    fn lines(&self, runs: &[Run], width: f32) -> Vec<Line> {
        // Words with the spaces before them, each in its run's style.
        let mut words: Vec<(Face, f32, String, bool)> = Vec::new();
        let mut space = false;
        for r in runs {
            for (k, part) in r.text.split('\n').enumerate() {
                if k > 0 {
                    words.push((r.face, r.size, "\n".into(), false));
                }
                let mut chars = part.chars().peekable();
                while chars.peek().is_some() {
                    if chars.peek().is_some_and(|c| c.is_whitespace()) {
                        space = true;
                        chars.next();
                        continue;
                    }
                    let mut w = String::new();
                    while let Some(&c) = chars.peek() {
                        if c.is_whitespace() {
                            break;
                        }
                        w.push(c);
                        chars.next();
                    }
                    words.push((r.face, r.size, w, space));
                    space = false;
                }
            }
        }
        let mut lines: Vec<Line> = vec![Vec::new()];
        let mut x = 0.0f32;
        for (face, size, word, spaced) in words {
            if word == "\n" {
                lines.push(Vec::new());
                x = 0.0;
                continue;
            }
            let sp = if spaced && !lines.last().is_some_and(Vec::is_empty) {
                self.fonts.width(face, size, " ")
            } else {
                0.0
            };
            let w = self.fonts.width(face, size, &word);
            if x + sp + w > width && !lines.last().is_some_and(Vec::is_empty) {
                lines.push(Vec::new());
                x = 0.0;
            }
            let line = lines.last_mut().expect("a line");
            let sp = if line.is_empty() { 0.0 } else { sp };
            if w <= width {
                line.push((
                    face,
                    size,
                    format!("{}{word}", if sp > 0.0 { " " } else { "" }),
                    sp + w,
                ));
                x += sp + w;
                continue;
            }
            // Wider than the column: broken by character.
            let mut piece = String::new();
            let mut pw = 0.0;
            for c in word.chars() {
                let cw = self.fonts.width(face, size, &c.to_string());
                if pw + cw > width - x && !piece.is_empty() {
                    lines.last_mut().expect("a line").push((
                        face,
                        size,
                        std::mem::take(&mut piece),
                        pw,
                    ));
                    lines.push(Vec::new());
                    x = 0.0;
                    pw = 0.0;
                }
                piece.push(c);
                pw += cw;
            }
            lines
                .last_mut()
                .expect("a line")
                .push((face, size, piece, pw));
            x += pw;
        }
        lines.retain(|l| !l.is_empty());
        lines
    }

    fn line_height(&self, line: &Line) -> f32 {
        let size = line.iter().map(|p| p.1).fold(0.0, f32::max);
        self.t.leading(size.max(1.0))
    }

    fn para(&mut self, p: &Para, next_keep: f32) {
        let mut runs = p.runs.clone();
        // Leading and trailing whitespace of the paragraph doesn't count.
        if let Some(r) = runs.first_mut() {
            r.text = r.text.trim_start().to_owned();
        }
        if let Some(r) = runs.last_mut() {
            r.text = r.text.trim_end().to_owned();
        }
        for r in &mut runs {
            if r.size == MASTHEAD_FIT {
                let at_one = self.fonts.width(r.face, 1.0, r.text.trim());
                r.size = (self.width / at_one.max(0.01)).min(self.t.body * 4.0);
            }
        }
        let bullet_w = if p.bullet { self.t.body } else { 0.0 };
        let width = self.width - p.indent - bullet_w;
        let lines = self.lines(&runs, width);
        if lines.is_empty() {
            return;
        }
        if !self.at_top() {
            self.y += p.before;
        }
        let first = self.line_height(&lines[0]);
        // A headline brings the line after it onto the same page.
        let keep = if p.keep {
            first + next_keep
        } else {
            first * lines.len().min(2) as f32
        };
        // A lone small line (a credit, a note) takes the room above the footer rather than a
        // page of its own.
        let small = lines.len() == 1 && lines[0].iter().all(|p| p.1 <= self.t.meta());
        let slack = if small { self.t.meta() * 1.5 } else { 0.0 };
        self.room_with(keep, slack);
        let start = (self.page(), self.y);
        for (k, line) in lines.iter().enumerate() {
            let h = self.line_height(line);
            self.room_with(h, slack);
            let size = line.iter().map(|p| p.1).fold(0.0, f32::max);
            let baseline = self.y + size * 1.05;
            let line_w: f32 = line.iter().map(|p| p.3).sum();
            let mut x = self.left
                + p.indent
                + bullet_w
                + match p.align {
                    Align::Left => 0.0,
                    Align::Center => ((width - line_w) / 2.0).max(0.0),
                };
            if p.bullet && k == 0 {
                self.op(Op::Text {
                    x: self.left + p.indent,
                    y: baseline,
                    face: Face::Regular,
                    size,
                    text: "•".into(),
                });
            }
            for (face, size, text, w) in line {
                self.op(Op::Text {
                    x,
                    y: baseline,
                    face: *face,
                    size: *size,
                    text: text.clone(),
                });
                x += w;
            }
            self.y += h;
        }
        if let Some(anchor) = p.link {
            // On one page only: a paragraph that broke across pages links from its first.
            let (page, y0) = start;
            let y1 = if page == self.page() {
                self.y
            } else {
                self.bottom
            };
            self.links.push(Link {
                page,
                x: self.left + p.indent,
                y: y0,
                w: width + bullet_w,
                h: y1 - y0,
                anchor,
            });
        }
        self.y += p.after;
    }

    fn image(&mut self, img: &Image) {
        let (pw, ph) = img.size();
        let (pw, ph) = (pw.max(1) as f32, ph.max(1) as f32);
        // Small pictures (icons) stay small; photos fill the column, up to 45% of the page.
        let natural = pw * 72.0 / 150.0;
        let mut w = natural.min(self.width);
        let mut h = w * ph / pw;
        let max_h = (self.bottom - self.top) * 0.45;
        if h > max_h {
            h = max_h;
            w = h * pw / ph;
        }
        self.room(h + self.t.body * 0.4);
        let x = self.left + (self.width - w) / 2.0;
        let y = self.y;
        self.op(Op::Image {
            image: usize::MAX,
            x,
            y,
            w,
            h,
        });
        self.y += h + self.t.body * 0.5;
    }

    fn rule(&mut self, thick: f32) {
        self.room(thick + 4.0);
        let y = self.y + 2.0;
        self.op(Op::Line {
            x1: self.left,
            y1: y,
            x2: self.left + self.width,
            y2: y,
            width: thick,
        });
        self.y += thick + 4.0;
    }

    fn table(&mut self, rows: &[Vec<Cell>], grid: bool, images: &[Image]) {
        let cols = rows.iter().map(Vec::len).max().unwrap_or(1).max(1);
        let col_w = self.width / cols as f32;
        if grid {
            let side = col_w.min(self.t.body * 2.4);
            let x0 = self.left + (self.width - side * cols as f32) / 2.0;
            self.y += self.t.body * 0.3;
            for row in rows {
                self.room(side);
                let y = self.y;
                for (c, cell) in row.iter().enumerate() {
                    let x = x0 + c as f32 * side;
                    self.op(Op::Rect {
                        x,
                        y,
                        w: side,
                        h: side,
                        fill: cell.black,
                    });
                    if let Some(n) = &cell.number {
                        let s = side * 0.28;
                        self.op(Op::Text {
                            x: x + side * 0.08,
                            y: y + s * 1.05,
                            face: Face::Regular,
                            size: s,
                            text: n.clone(),
                        });
                    }
                    let text: String = cell.runs.iter().map(|r| r.text.as_str()).collect();
                    let text = text.trim();
                    if !text.is_empty() && !cell.black {
                        let s = side * 0.5;
                        let w = self.fonts.width(Face::Regular, s, text);
                        self.op(Op::Text {
                            x: x + (side - w) / 2.0,
                            y: y + side * 0.72,
                            face: Face::Regular,
                            size: s,
                            text: text.to_owned(),
                        });
                    }
                }
                self.y += side;
            }
            self.y += self.t.body * 0.6;
            return;
        }
        let pad = 2.0;
        for row in rows {
            let laid: Vec<(Vec<Line>, Option<usize>)> = row
                .iter()
                .map(|cell| (self.lines(&cell.runs, col_w - 2.0 * pad), cell.image))
                .collect();
            let icon = self.t.body * 1.8;
            let h = laid
                .iter()
                .map(|(lines, img)| {
                    let text: f32 = lines.iter().map(|l| self.line_height(l)).sum();
                    text + if img.is_some() { icon + 2.0 } else { 0.0 }
                })
                .fold(0.0, f32::max)
                + 2.0 * pad;
            self.room(h);
            let y = self.y;
            self.op(Op::Line {
                x1: self.left,
                y1: y,
                x2: self.left + self.width,
                y2: y,
                width: 0.4,
            });
            for (c, (lines, img)) in laid.iter().enumerate() {
                let x = self.left + c as f32 * col_w + pad;
                let mut yy = y + pad;
                if let Some(i) = img {
                    let (pw, ph) = images[*i].size();
                    let w = icon * pw.max(1) as f32 / ph.max(1) as f32;
                    let w = w.min(col_w - 2.0 * pad);
                    self.op(Op::Image {
                        image: *i,
                        x: x + (col_w - 2.0 * pad - w) / 2.0,
                        y: yy,
                        w,
                        h: icon,
                    });
                    yy += icon + 2.0;
                }
                for line in lines {
                    let size = line.iter().map(|p| p.1).fold(0.0, f32::max);
                    let line_w: f32 = line.iter().map(|p| p.3).sum();
                    let mut xx = x + ((col_w - 2.0 * pad - line_w) / 2.0).max(0.0);
                    for (face, size_p, text, w) in line {
                        self.op(Op::Text {
                            x: xx,
                            y: yy + size * 1.05,
                            face: *face,
                            size: *size_p,
                            text: text.clone(),
                        });
                        xx += w;
                    }
                    yy += self.line_height(line);
                }
            }
            self.y += h;
        }
        self.op(Op::Line {
            x1: self.left,
            y1: self.y,
            x2: self.left + self.width,
            y2: self.y,
            width: 0.4,
        });
        self.y += self.t.body * 0.6;
    }

    /// The first line's height of `b`, for a headline before it to keep with.
    fn first_height(&self, b: Option<&Block>) -> f32 {
        match b {
            Some(Block::Para(p)) => p
                .runs
                .iter()
                .map(|r| self.t.leading(r.size.max(self.t.body)))
                .fold(0.0, f32::max),
            Some(Block::Image { .. }) => (self.bottom - self.top) * 0.2,
            Some(Block::Table { .. }) => self.t.body * 2.5,
            _ => 0.0,
        }
    }

    fn run(&mut self, doc: &Doc) {
        for (k, b) in doc.blocks.iter().enumerate() {
            match b {
                Block::Para(p) => {
                    let next = doc.blocks[k + 1..]
                        .iter()
                        .find(|b| !matches!(b, Block::Anchor(_) | Block::Space(_)));
                    let keep = self.first_height(next);
                    self.para(p, keep);
                }
                Block::Image { image } => {
                    self.image(&doc.images[*image]);
                    if let Some(Op::Image { image: slot, .. }) =
                        self.pages.last_mut().and_then(|p| p.last_mut())
                    {
                        *slot = *image;
                    }
                }
                Block::Rule { thick } => self.rule(*thick),
                Block::Table { rows, grid } => self.table(rows, *grid, &doc.images),
                Block::Space(h) => {
                    if !self.at_top() {
                        self.y += h;
                    }
                }
                Block::PageBreak => self.new_page(),
                Block::Anchor(a) => self.anchors[*a] = Some((self.page(), self.y)),
            }
        }
    }
}

// ---- writing ----------------------------------------------------------------------------------

/// The edition `g` as a PDF for `screen`, its photos as `photos` asks.
pub fn render(g: &Gathered, screen: Screen, photos: Photos) -> Vec<u8> {
    let page = screen.page();
    let t = Type::for_width(page.0);
    let fonts = Fonts::new(g.edition.masthead);
    let doc = document(g, t, photos);
    let mut lay = Layout::new(&fonts, page, t, doc.anchors);
    lay.run(&doc);
    if lay.pages.last().is_some_and(Vec::is_empty) && lay.pages.len() > 1 {
        lay.pages.pop();
    }
    let footer = format!("{} · {}", g.edition.paper, long_date(g.edition.date));
    write(&fonts, &doc, &lay, &footer)
}

fn deflate(data: &[u8]) -> Vec<u8> {
    let mut e = flate2::write::ZlibEncoder::new(Vec::new(), flate2::Compression::default());
    e.write_all(data).expect("in memory");
    e.finish().expect("in memory")
}

fn glyph_bytes(glyphs: &[(u16, char)]) -> Vec<u8> {
    glyphs.iter().flat_map(|(g, _)| g.to_be_bytes()).collect()
}

fn write(fonts: &Fonts, doc: &Doc, lay: &Layout, footer: &str) -> Vec<u8> {
    let mut pdf = Pdf::new();
    let mut next = 1;
    let mut alloc = || {
        next += 1;
        Ref::new(next - 1)
    };
    let catalog = alloc();
    let tree = alloc();
    let outline_root = alloc();
    let font_refs: Vec<Ref> = FACES.iter().map(|_| alloc()).collect();
    let image_refs: Vec<Ref> = doc.images.iter().map(|_| alloc()).collect();
    let page_refs: Vec<Ref> = lay.pages.iter().map(|_| alloc()).collect();
    let (pw, ph) = (lay.page_w, lay.page_h);
    let mut used: [BTreeMap<u16, char>; 4] = Default::default();

    // Pages.
    let mut page_links: Vec<Vec<Ref>> = vec![Vec::new(); lay.pages.len()];
    let mut link_objs: Vec<(Ref, &Link)> = Vec::new();
    for link in &lay.links {
        if lay.anchors.get(link.anchor).copied().flatten().is_some() {
            let id = alloc();
            page_links[link.page].push(id);
            link_objs.push((id, link));
        }
    }
    let total = lay.pages.len();
    for (n, ops) in lay.pages.iter().enumerate() {
        let mut c = Content::new();
        let mut faces_on: BTreeSet<Face> = BTreeSet::new();
        let mut images_on: BTreeSet<usize> = BTreeSet::new();
        let foot = format!("{footer} · {}", n + 1);
        let foot_size = lay.t.meta() * 0.85;
        let foot_op = Op::Text {
            x: lay.left,
            y: lay.page_h - lay.top * 0.6,
            face: Face::Italic,
            size: foot_size,
            text: if total > 1 { foot } else { footer.to_owned() },
        };
        for op in ops.iter().chain(std::iter::once(&foot_op)) {
            match op {
                Op::Text {
                    x,
                    y,
                    face,
                    size,
                    text,
                } => {
                    let glyphs = fonts.shape(*face, text);
                    if glyphs.is_empty() {
                        continue;
                    }
                    for (g, ch) in &glyphs {
                        used[face.index()].insert(*g, *ch);
                    }
                    faces_on.insert(*face);
                    c.begin_text();
                    c.set_font(Name(format!("F{}", face.index()).as_bytes()), *size);
                    c.set_text_matrix([1.0, 0.0, 0.0, 1.0, *x, ph - *y]);
                    c.show(Str(&glyph_bytes(&glyphs)));
                    c.end_text();
                }
                Op::Image { image, x, y, w, h } => {
                    if *image == usize::MAX {
                        continue;
                    }
                    images_on.insert(*image);
                    c.save_state();
                    c.transform([*w, 0.0, 0.0, *h, *x, ph - *y - *h]);
                    c.x_object(Name(format!("Im{image}").as_bytes()));
                    c.restore_state();
                }
                Op::Line {
                    x1,
                    y1,
                    x2,
                    y2,
                    width,
                } => {
                    c.set_line_width(*width);
                    c.move_to(*x1, ph - *y1);
                    c.line_to(*x2, ph - *y2);
                    c.stroke();
                }
                Op::Rect { x, y, w, h, fill } => {
                    c.set_line_width(0.6);
                    c.rect(*x, ph - *y - *h, *w, *h);
                    if *fill {
                        c.fill_nonzero_and_stroke();
                    } else {
                        c.stroke();
                    }
                }
            }
        }
        let content_id = alloc();
        let data = deflate(&c.finish());
        pdf.stream(content_id, &data).filter(Filter::FlateDecode);
        let mut page = pdf.page(page_refs[n]);
        page.media_box(Rect::new(0.0, 0.0, pw, ph))
            .parent(tree)
            .contents(content_id);
        if !page_links[n].is_empty() {
            page.annotations(page_links[n].iter().copied());
        }
        let mut res = page.resources();
        {
            let mut f = res.fonts();
            for face in &faces_on {
                f.pair(
                    Name(format!("F{}", face.index()).as_bytes()),
                    font_refs[face.index()],
                );
            }
        }
        if !images_on.is_empty() {
            let mut x = res.x_objects();
            for i in &images_on {
                x.pair(Name(format!("Im{i}").as_bytes()), image_refs[*i]);
            }
        }
    }
    pdf.pages(tree)
        .kids(page_refs.iter().copied())
        .count(total as i32);

    // Links from the front page.
    for (id, link) in &link_objs {
        let (page, y) = lay.anchors[link.anchor].expect("checked");
        let mut a = pdf.annotation(*id);
        a.subtype(AnnotationType::Link)
            .rect(Rect::new(
                link.x,
                ph - link.y - link.h,
                link.x + link.w,
                ph - link.y,
            ))
            .border(0.0, 0.0, 0.0, None);
        a.action()
            .action_type(ActionType::GoTo)
            .destination()
            .page(page_refs[page])
            .xyz(0.0, ph - y, None);
    }

    // Images.
    for (i, img) in doc.images.iter().enumerate() {
        match img {
            Image::Jpeg { bytes, w, h, gray } => {
                let mut x = pdf.image_xobject(image_refs[i], bytes);
                x.width(*w as i32).height(*h as i32).bits_per_component(8);
                x.filter(Filter::DctDecode);
                if *gray {
                    x.color_space().device_gray();
                } else {
                    x.color_space().device_rgb();
                }
            }
            Image::Rgb { pixels, w, h } => {
                let data = deflate(pixels);
                let mut x = pdf.image_xobject(image_refs[i], &data);
                x.width(*w as i32).height(*h as i32).bits_per_component(8);
                x.filter(Filter::FlateDecode);
                x.color_space().device_rgb();
            }
        }
    }

    // Fonts: Type0 over CIDFontType2, Identity-H, the glyphs used and a ToUnicode map.
    let names = [
        "OldStandard-Regular",
        "OldStandard-Bold",
        "OldStandard-Italic",
        fonts.masthead.name(),
    ];
    let files = [REGULAR, BOLD, ITALIC, fonts.masthead.bytes()];
    let info = SystemInfo {
        registry: Str(b"Adobe"),
        ordering: Str(b"Identity"),
        supplement: 0,
    };
    for face in FACES {
        let i = face.index();
        if used[i].is_empty() {
            continue;
        }
        let (cid, descriptor, file, cmap) = (alloc(), alloc(), alloc(), alloc());
        let name = Name(names[i].as_bytes());
        pdf.type0_font(font_refs[i])
            .base_font(name)
            .encoding_predefined(Name(b"Identity-H"))
            .descendant_font(cid)
            .to_unicode(cmap);
        let font = fonts.font(face);
        let scale = 1000.0 / fonts.upm(face);
        let mut c = pdf.cid_font(cid);
        c.subtype(CidFontType::Type2)
            .base_font(name)
            .system_info(info)
            .font_descriptor(descriptor)
            .default_width(0.0)
            .cid_to_gid_map_predefined(Name(b"Identity"));
        {
            let mut w = c.widths();
            for g in used[i].keys() {
                w.consecutive(*g, [font.h_advance_unscaled(GlyphId(*g)) * scale]);
            }
        }
        c.finish();
        let mut flags = FontFlags::SERIF | FontFlags::NON_SYMBOLIC;
        if face == Face::Italic {
            flags |= FontFlags::ITALIC;
        }
        let ascent = font.ascent_unscaled() * scale;
        let descent = font.descent_unscaled() * scale;
        pdf.font_descriptor(descriptor)
            .name(name)
            .flags(flags)
            .bbox(Rect::new(-200.0, descent, 1200.0, ascent))
            .italic_angle(if face == Face::Italic { -12.0 } else { 0.0 })
            .ascent(ascent)
            .descent(descent)
            .cap_height(ascent * 0.9)
            .stem_v(if face == Face::Bold { 120.0 } else { 80.0 })
            .font_file2(file);
        let data = deflate(files[i]);
        pdf.stream(file, &data)
            .filter(Filter::FlateDecode)
            .pair(Name(b"Length1"), files[i].len() as i32);
        let mut map = UnicodeCmap::new(Name(b"Deckle-UCS"), info);
        for (g, ch) in &used[i] {
            map.pair(*g, *ch);
        }
        let map = map.finish();
        pdf.cmap(cmap, &map);
    }

    // Outline: the front page and each section, with its stories under it.
    // Each top-level item: its object, its anchor, and its children's objects and anchors.
    type Item = (Ref, usize, Vec<(Ref, usize)>);
    let mut items: Vec<Item> = Vec::new();
    for (depth, _, anchor) in &doc.outline {
        if lay.anchors[*anchor].is_none() {
            continue;
        }
        match depth {
            0 => items.push((alloc(), *anchor, Vec::new())),
            _ => {
                let id = alloc();
                if let Some(last) = items.last_mut() {
                    last.2.push((id, *anchor));
                }
            }
        }
    }
    let title_of = |anchor: usize| {
        doc.outline
            .iter()
            .find(|(_, _, a)| *a == anchor)
            .map(|(_, t, _)| t.clone())
            .unwrap_or_default()
    };
    let dest = |pdf: &mut Pdf,
                id: Ref,
                parent: Ref,
                prev: Option<Ref>,
                next: Option<Ref>,
                anchor: usize,
                kids: &[(Ref, usize)]| {
        let (page, y) = lay.anchors[anchor].expect("laid out");
        let title = title_of(anchor);
        let mut it = pdf.outline_item(id);
        it.title(TextStr(&title)).parent(parent);
        if let Some(p) = prev {
            it.prev(p);
        }
        if let Some(n) = next {
            it.next(n);
        }
        if let (Some(f), Some(l)) = (kids.first(), kids.last()) {
            it.first(f.0).last(l.0).count(-(kids.len() as i32));
        }
        it.dest().page(page_refs[page]).xyz(0.0, ph - y, None);
    };
    for (k, (id, anchor, kids)) in items.iter().enumerate() {
        let prev = k.checked_sub(1).map(|p| items[p].0);
        let next = items.get(k + 1).map(|n| n.0);
        dest(&mut pdf, *id, outline_root, prev, next, *anchor, kids);
        for (j, (kid, a)) in kids.iter().enumerate() {
            let prev = j.checked_sub(1).map(|p| kids[p].0);
            let next = kids.get(j + 1).map(|n| n.0);
            dest(&mut pdf, *kid, *id, prev, next, *a, &[]);
        }
    }
    {
        let mut o = pdf.outline(outline_root);
        if let (Some(f), Some(l)) = (items.first(), items.last()) {
            o.first(f.0).last(l.0);
        }
        o.count(items.len() as i32);
    }
    pdf.catalog(catalog)
        .pages(tree)
        .outlines(outline_root)
        .page_mode(pdf_writer::types::PageMode::UseOutlines);
    pdf.finish()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fonts() -> Fonts {
        Fonts::new(Masthead::default())
    }

    #[test]
    fn screens_have_their_page_sizes() {
        let (w, h) = Screen::Remarkable2.page();
        assert!(
            (w - 447.3).abs() < 0.5 && (h - 596.4).abs() < 0.5,
            "{w} {h}"
        );
        let (w, h) = Screen::RemarkablePaperPro.page();
        assert!(
            (w - 509.3).abs() < 0.5 && (h - 679.1).abs() < 0.5,
            "{w} {h}"
        );
        assert_eq!(Screen::A5.page(), (420.0, 595.0));
        assert_eq!(Screen::RemarkablePaperPro.photos(), Photos::Color);
        assert_eq!(Screen::Remarkable2.photos(), Photos::Gray);
        let parsed: Screen = serde_json::from_str("\"remarkable-2\"").unwrap();
        assert_eq!(parsed, Screen::Remarkable2);
        let parsed: Screen = serde_json::from_str("\"remarkable-paper-pro-move\"").unwrap();
        assert_eq!(parsed, Screen::RemarkablePaperProMove);
        assert!(serde_json::from_str::<Screen>("\"kindle\"").is_err());
    }

    #[test]
    fn a_character_outside_the_font_is_transliterated() {
        let f = fonts();
        let glyphs = f.shape(Face::Regular, "€");
        assert!(!glyphs.is_empty());
        let none = f.shape(Face::Regular, "\u{1F600}");
        assert!(none.iter().all(|(g, _)| *g != 0));
    }

    fn layout(f: &Fonts) -> Layout<'_> {
        Layout::new(f, Screen::Remarkable2.page(), Type { body: 10.0 }, 4)
    }

    #[test]
    fn lines_fit_the_width_and_a_long_word_is_broken() {
        let f = fonts();
        let l = layout(&f);
        let runs = vec![Run {
            face: Face::Regular,
            size: 10.0,
            text: "The mill stood by the river for a hundred years and more, and nobody \
                   remembered who had built it."
                .into(),
        }];
        let lines = l.lines(&runs, 120.0);
        assert!(lines.len() > 2);
        for line in &lines {
            let w: f32 = line.iter().map(|p| p.3).sum();
            assert!(w <= 120.5, "{w}");
        }
        let long = vec![Run {
            face: Face::Bold,
            size: 10.0,
            text: "x".repeat(200),
        }];
        let lines = l.lines(&long, 100.0);
        assert!(lines.len() > 1);
        assert!(
            lines
                .iter()
                .all(|line| line.iter().map(|p| p.3).sum::<f32>() <= 100.5)
        );
    }

    #[test]
    fn a_headline_is_never_the_last_thing_on_a_page() {
        let f = fonts();
        let mut l = layout(&f);
        l.op(Op::Line {
            x1: 0.0,
            y1: 0.0,
            x2: 1.0,
            y2: 0.0,
            width: 1.0,
        });
        l.y = l.bottom - 12.0;
        let mut h = Para::new(vec![Run {
            face: Face::Bold,
            size: 15.0,
            text: "Headline".into(),
        }]);
        h.keep = true;
        l.para(&h, 13.5);
        assert_eq!(l.pages.len(), 2, "the headline moved to a new page");
        assert_eq!(l.pages[0].len(), 1);
    }

    #[test]
    fn a_grid_draws_black_squares_and_numbers() {
        let f = fonts();
        let mut l = layout(&f);
        let rows = vec![vec![
            Cell {
                number: Some("1".into()),
                ..Default::default()
            },
            Cell {
                black: true,
                ..Default::default()
            },
        ]];
        l.table(&rows, true, &[]);
        let ops = &l.pages[0];
        assert!(ops.iter().any(|o| matches!(o, Op::Rect { fill: true, .. })));
        assert!(
            ops.iter()
                .any(|o| matches!(o, Op::Rect { fill: false, .. }))
        );
        assert!(
            ops.iter()
                .any(|o| matches!(o, Op::Text { text, .. } if text == "1"))
        );
    }

    #[test]
    fn named_entities_become_characters() {
        assert_eq!(
            xml_entities("a&nbsp;b &amp; c&mdash;d"),
            "a\u{a0}b &amp; c—d"
        );
        assert_eq!(xml_entities("&bogus;"), "");
    }
}
