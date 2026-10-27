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
    ActionType, AnnotationType, CidFontType, FontFlags, NumberingStyle, SystemInfo, UnicodeCmap,
};
use pdf_writer::{Content, Filter, Finish, Name, Pdf, Rect, Ref, Str, TextStr};
use serde::{Deserialize, Serialize};

use crate::config::Photos;
use crate::print::{FRONT_LABEL, PrintPlan};
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

#[derive(Clone, Copy, Debug, Default, PartialEq)]
enum Align {
    #[default]
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

/// What a slot prints (print layout §4.2): a page label, the page count, or text.
#[derive(Clone, Debug)]
enum Slot {
    /// The label of the page an anchor lands on.
    Label(usize),
    Pages,
    Text(String),
}

/// A field's right-aligned ending, sized before layout for the widest text its slot could
/// print, so filling it in never moves a line or a page break.
#[derive(Clone, Debug)]
struct Tail {
    prefix: String,
    slot: Slot,
    suffix: String,
    face: Face,
    size: f32,
}

impl Tail {
    fn label(anchor: usize, face: Face, size: f32) -> Self {
        Tail {
            prefix: String::new(),
            slot: Slot::Label(anchor),
            suffix: String::new(),
            face,
            size,
        }
    }
}

/// One cell of a row of fields: wrapped runs, then its tail at the cell's right edge.
#[derive(Clone, Debug, Default)]
struct Field {
    runs: Vec<Run>,
    align: Align,
    tail: Option<Tail>,
    /// Dot leaders from the text to the tail.
    leader: bool,
    link: Option<usize>,
}

#[derive(Clone, Debug)]
enum Block {
    Para(Para),
    /// `cap`: the share of the page's height the picture may take.
    Image {
        image: usize,
        cap: f32,
    },
    /// Cells side by side, with hairlines between them when `divided`.
    Fields {
        fields: Vec<Field>,
        divided: bool,
        after: f32,
    },
    /// A run of story text set in columns (§4.5): paragraphs and spaces only.
    Columns {
        items: Vec<Block>,
    },
    Rule {
        thick: f32,
    },
    Table {
        rows: Vec<Vec<Cell>>,
        grid: bool,
    },
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
    /// The anchor just after the front page's jump line, when it has one: the fit checks it.
    jump_end: Option<usize>,
    /// The anchor at the end of the front page.
    front_end: usize,
    /// A large-print paper: story text in one column.
    large_print: bool,
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

/// Anchors the front page links to before the sections are laid out.
struct Anchors {
    section: Vec<usize>,
    story: Vec<Vec<usize>>,
    /// The front page's lead, where a refer to it lands.
    lead: usize,
    /// The jump line, where the jump head links back to.
    jump: usize,
    /// Just after the jump line.
    jump_end: usize,
    /// The end of the front page.
    front_end: usize,
}

/// How the front page sets its lead (§4.3). Tried in order until the jump line lands on the
/// front page: a smaller headline and photo, then no photo there, then fewer paragraphs of the
/// opening, which move to the head of the continuation.
#[derive(Clone, Copy, Debug, PartialEq)]
struct Fit {
    /// The headline's size, times the headline size.
    headline: f32,
    /// The lead photo's share of the page, or `None`: it stays with the continuation.
    photo: Option<f32>,
    /// Opening paragraphs the front page keeps.
    keep: usize,
}

const FITS: [Fit; 4] = [
    Fit {
        headline: 1.6,
        photo: Some(0.30),
        keep: usize::MAX,
    },
    Fit {
        headline: 1.35,
        photo: Some(0.24),
        keep: usize::MAX,
    },
    Fit {
        headline: 1.2,
        photo: Some(0.16),
        keep: usize::MAX,
    },
    Fit {
        headline: 1.2,
        photo: None,
        keep: usize::MAX,
    },
];

/// The jump's pieces under `fit`: the front page's photo and opening, the continuation, and
/// how many opening paragraphs moved to the continuation.
fn jump_parts(x: &crate::print::Jump, fit: Fit) -> (Option<String>, String, String, usize) {
    let (figure, rest) = crate::print::take_lead_figure(&x.rest);
    let ends = crate::print::paragraph_ends(&x.opening);
    let keep = fit.keep.min(ends.len()).max(1);
    let cut = ends.get(keep - 1).copied().unwrap_or(x.opening.len());
    let moved = x.opening[cut..].trim_start();
    let moved_count = ends.len().saturating_sub(keep);
    let (front_figure, rest) = match (fit.photo, figure) {
        (Some(_), figure) => (figure, format!("{moved}{rest}")),
        (None, Some(f)) => (None, format!("{f}\n{moved}{rest}")),
        (None, None) => (None, format!("{moved}{rest}")),
    };
    (front_figure, x.opening[..cut].to_owned(), rest, moved_count)
}

/// The blocks of the whole edition: the front page, then each section from a new page.
fn document(g: &Gathered, t: Type, photos: Photos, print: &PrintPlan, fit: Fit) -> Doc {
    let e = &g.edition;
    let mut d = Doc {
        blocks: Vec::new(),
        images: Vec::new(),
        outline: Vec::new(),
        anchors: 0,
        t,
        photos,
        jump_end: None,
        front_end: 0,
        large_print: g.edition.large_print,
    };
    // Every anchor the front page links to is known before the front page is built.
    let mut next = 0;
    let mut take = || {
        next += 1;
        next - 1
    };
    let section: Vec<usize> = e.sections.iter().map(|_| take()).collect();
    let story = e
        .sections
        .iter()
        .map(|s| s.articles.iter().map(|_| take()).collect())
        .collect();
    let ids = Anchors {
        section,
        story,
        lead: take(),
        jump: take(),
        jump_end: take(),
        front_end: take(),
    };
    d.anchors = next;
    let front = d.anchor();
    d.outline.push((0, "Front page".into(), front));
    front_page(&mut d, g, print, &ids, fit);
    d.blocks.push(Block::Anchor(ids.front_end));
    d.front_end = ids.front_end;
    for (i, s) in e.sections.iter().enumerate() {
        d.blocks.push(Block::PageBreak);
        d.blocks.push(Block::Anchor(ids.section[i]));
        d.outline.push((0, s.title.clone(), ids.section[i]));
        // The section flag: its title, with its kicker beside it when that fits (§4.1).
        let beside = s
            .kicker
            .as_ref()
            .filter(|k| k.chars().count() <= KICKER_BESIDE);
        d.blocks.push(Block::Fields {
            fields: vec![Field {
                runs: vec![d.run(Face::Bold, t.section(), &s.title)],
                tail: beside.map(|k| Tail {
                    prefix: String::new(),
                    slot: Slot::Text(k.clone()),
                    suffix: String::new(),
                    face: Face::Italic,
                    size: t.meta(),
                }),
                ..Field::default()
            }],
            divided: false,
            after: 0.0,
        });
        d.blocks.push(Block::Rule { thick: 1.2 });
        d.blocks.push(Block::Rule { thick: 0.4 });
        d.blocks.push(Block::Space(t.body * 0.4));
        if let Some(k) = s.kicker.as_ref().filter(|_| beside.is_none()) {
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
            let at = ids.story[i][j];
            d.blocks.push(Block::Anchor(at));
            d.outline.push((1, a.title.clone(), at));
            // The lead printed whole on the front page is a refer here (§5.4).
            if print.whole == Some((i, j)) {
                let mut p = Para::new(vec![d.run(
                    Face::Italic,
                    t.body,
                    &format!("{} — on {FRONT_LABEL}", a.title),
                )]);
                p.link = Some(ids.lead);
                d.para(p);
                continue;
            }
            let jump = print
                .jump
                .as_ref()
                .filter(|x| (x.section, x.article) == (i, j));
            let mut pull = print.pulls.get(&(i, j)).cloned();
            let text = match jump {
                // The jump head: the slug and where it came from, then the headline (§4.4).
                Some(x) => {
                    let mut head = Para::new(vec![
                        d.run(Face::Bold, t.headline(), &x.slug),
                        d.run(Face::Regular, t.body, " · "),
                        d.run(
                            Face::Italic,
                            t.body,
                            &format!("Continued from {FRONT_LABEL}"),
                        ),
                    ]);
                    head.keep = true;
                    head.link = Some(ids.jump);
                    d.para(head);
                    let mut title = Para::new(vec![d.run(Face::Italic, t.body, &a.title)]);
                    title.keep = true;
                    title.after = t.body * 0.25;
                    d.para(title);
                    // without what the front page printed; its pull quote follows the paragraphs
                    // that moved here
                    let (_, _, rest, moved) = jump_parts(x, fit);
                    if let Some(p) = &mut pull {
                        p.after += moved;
                    }
                    rest
                }
                None => {
                    let mut h = Para::new(vec![d.run(Face::Bold, t.headline(), &a.title)]);
                    h.keep = true;
                    h.after = t.body * 0.25;
                    d.para(h);
                    a.body.clone()
                }
            };
            if let Some(m) = &a.meta {
                let mut p = Para::new(vec![d.run(Face::Italic, t.meta(), m)]);
                p.keep = true;
                p.after = t.body * 0.4;
                d.para(p);
            }
            story_body(
                &mut d,
                &text,
                &a.photos,
                &e.assets,
                pull.as_ref(),
                columned(&s.key),
            );
        }
        if let Some(u) = &s.unavailable {
            let p = Para::new(vec![d.run(Face::Italic, t.body, u)]);
            d.para(p);
        }
    }
    d
}

/// A story's body, with its pull quote set between rules after its paragraph (§4.6).
/// In columns when `columns` (§4.5).
fn story_body(
    d: &mut Doc,
    xhtml: &str,
    photos: &[Vec<u8>],
    assets: &[(String, Vec<u8>)],
    pull: Option<&crate::print::Pull>,
    columns: bool,
) {
    let from = d.blocks.len();
    pulled_body(d, xhtml, photos, assets, pull);
    if columns {
        columnize(&mut d.blocks, from);
    }
}

fn pulled_body(
    d: &mut Doc,
    xhtml: &str,
    photos: &[Vec<u8>],
    assets: &[(String, Vec<u8>)],
    pull: Option<&crate::print::Pull>,
) {
    let at = pull.and_then(|p| crate::print::paragraph_ends(xhtml).get(p.after).copied());
    let (Some(p), Some(at)) = (pull, at) else {
        body(d, xhtml, photos, assets);
        return;
    };
    body(d, &xhtml[..at], photos, assets);
    let t = d.t;
    d.blocks.push(Block::Space(t.body * 0.3));
    d.blocks.push(Block::Rule { thick: 0.4 });
    let mut q = Para::new(vec![d.run(Face::Italic, t.body * 1.5, &p.text)]);
    q.align = Align::Center;
    d.para(q);
    d.blocks.push(Block::Rule { thick: 0.4 });
    d.blocks.push(Block::Space(t.body * 0.3));
    body(d, &xhtml[at..], photos, assets);
}

/// Whether a section's stories are set in columns (§4.5): news and the reading sections, not
/// the weather, the puzzle or the lottery, whose tables are the point.
fn columned(key: &str) -> bool {
    crate::print::is_news(key) || ["reading", "shelf", "horoscope", "onthisday"].contains(&key)
}

/// Gathers the left-aligned paragraphs and spaces from `from` on into column runs (§4.5).
/// Anything else (a figure, a table, a rule, a centered pull quote) spans the page and ends
/// a run.
fn columnize(blocks: &mut Vec<Block>, from: usize) {
    let tail: Vec<Block> = blocks.drain(from..).collect();
    let mut run: Vec<Block> = Vec::new();
    let flush = |run: &mut Vec<Block>, blocks: &mut Vec<Block>| {
        if run.iter().any(|b| matches!(b, Block::Para(_))) {
            blocks.push(Block::Columns {
                items: std::mem::take(run),
            });
        } else {
            blocks.append(run);
        }
    };
    for b in tail {
        match &b {
            Block::Para(p) if p.align == Align::Left && p.link.is_none() => run.push(b),
            Block::Space(_) if !run.is_empty() => run.push(b),
            _ => {
                flush(&mut run, blocks);
                blocks.push(b);
            }
        }
    }
    flush(&mut run, blocks);
}

fn front_page(d: &mut Doc, g: &Gathered, print: &PrintPlan, ids: &Anchors, fit: Fit) {
    let e = &g.edition;
    let t = d.t;
    // The skybox: teasers for what's inside, each with its page (§4.3).
    if !print.skybox.is_empty() {
        let fields = print
            .skybox
            .iter()
            .map(|x| Field {
                runs: vec![
                    d.run(Face::Bold, t.meta(), &x.title),
                    d.run(Face::Regular, t.meta(), &format!(" · {}", x.text)),
                ],
                tail: Some(Tail::label(ids.section[x.section], Face::Italic, t.meta())),
                link: Some(ids.section[x.section]),
                ..Field::default()
            })
            .collect();
        d.blocks.push(Block::Fields {
            fields,
            divided: true,
            after: t.body * 0.2,
        });
        d.blocks.push(Block::Rule { thick: 0.4 });
    }
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
    // The dateline band: the date, the edition, and how many pages it runs to.
    d.blocks.push(Block::Rule { thick: 1.6 });
    d.blocks.push(Block::Fields {
        fields: vec![
            Field {
                runs: vec![d.run(Face::Regular, t.meta(), &long_date(e.date))],
                ..Field::default()
            },
            Field {
                runs: vec![d.run(Face::Italic, t.meta(), "Final edition")],
                align: Align::Center,
                ..Field::default()
            },
            Field {
                tail: Some(Tail {
                    prefix: String::new(),
                    slot: Slot::Pages,
                    suffix: " pages".into(),
                    face: Face::Regular,
                    size: t.meta(),
                }),
                ..Field::default()
            },
        ],
        divided: false,
        after: 0.0,
    });
    d.blocks.push(Block::Rule { thick: 0.8 });
    d.blocks.push(Block::Rule { thick: 0.4 });
    if let Some(w) = &e.weather_line {
        let mut p = Para::new(vec![d.run(Face::Italic, t.body, w)]);
        p.align = Align::Center;
        p.after = t.body * 0.5;
        d.para(p);
    }
    let plan = crate::front::front_plan(g);
    if let Some((s, a)) = plan.lead {
        let art = &e.sections[s].articles[a];
        let story = ids.story[s][a];
        d.blocks.push(Block::Anchor(ids.lead));
        let mut k = Para::new(vec![d.run(
            Face::Bold,
            t.meta(),
            &e.sections[s].title.to_uppercase(),
        )]);
        k.before = t.body * 0.6;
        k.keep = true;
        d.para(k);
        let mut h = Para::new(vec![d.run(
            Face::Bold,
            t.headline() * fit.headline,
            &art.title,
        )]);
        h.link = Some(story);
        h.keep = true;
        d.para(h);
        if let Some(m) = &art.meta {
            let mut p = Para::new(vec![d.run(Face::Italic, t.meta(), m)]);
            p.after = t.body * 0.3;
            d.para(p);
        }
        match &print.jump {
            Some(x) if (x.section, x.article) == (s, a) => {
                let (figure, opening, _, _) = jump_parts(x, fit);
                if let (Some(figure), Some(share)) = (figure, fit.photo) {
                    let from = d.blocks.len();
                    body(d, &figure, &art.photos, &e.assets);
                    for b in &mut d.blocks[from..] {
                        if let Block::Image { cap, .. } = b {
                            *cap = share;
                        }
                    }
                }
                story_body(
                    d,
                    &opening,
                    &art.photos,
                    &e.assets,
                    None,
                    columned(&e.sections[s].key),
                );
                d.blocks.push(Block::Anchor(ids.jump));
                d.blocks.push(Block::Fields {
                    fields: vec![Field {
                        tail: Some(Tail {
                            prefix: format!("See {}, ", x.slug),
                            slot: Slot::Label(story),
                            suffix: " \u{203a}".into(),
                            face: Face::Bold,
                            size: t.body,
                        }),
                        link: Some(story),
                        ..Field::default()
                    }],
                    divided: false,
                    after: t.body * 0.4,
                });
                d.blocks.push(Block::Anchor(ids.jump_end));
                d.jump_end = Some(ids.jump_end);
            }
            _ if print.whole == Some((s, a)) => story_body(
                d,
                &art.body,
                &art.photos,
                &e.assets,
                None,
                columned(&e.sections[s].key),
            ),
            _ => {
                if let Some(teaser) = &art.teaser {
                    let mut p = Para::new(vec![d.run(Face::Regular, t.body, teaser)]);
                    p.before = t.body * 0.3;
                    d.para(p);
                }
            }
        }
    }
    // Refers: the other front-page stories, each with its page.
    if !plan.tiles.is_empty() {
        d.blocks.push(Block::Space(t.body * 0.6));
        d.blocks.push(Block::Rule { thick: 0.4 });
        d.blocks.push(Block::Space(t.body * 0.3));
        for &(s, a) in &plan.tiles {
            let story = ids.story[s][a];
            d.blocks.push(Block::Fields {
                fields: vec![Field {
                    runs: vec![
                        d.run(Face::Bold, t.body, &e.sections[s].articles[a].title),
                        d.run(
                            Face::Italic,
                            t.meta(),
                            &format!("  {}", e.sections[s].title),
                        ),
                    ],
                    tail: Some(Tail::label(story, Face::Italic, t.meta())),
                    link: Some(story),
                    ..Field::default()
                }],
                divided: false,
                after: t.body * 0.35,
            });
        }
    }
    // Inside today: every section, its page, and how much is in it.
    d.blocks.push(Block::Space(t.body * 0.6));
    d.blocks.push(Block::Rule { thick: 1.6 });
    let mut h = Para::new(vec![d.run(Face::Bold, t.headline(), "Inside today")]);
    h.keep = true;
    h.after = t.body * 0.2;
    d.para(h);
    for (i, s) in e.sections.iter().enumerate() {
        let count = match (s.articles.len(), &s.unavailable) {
            (0, Some(u)) if u == crate::sources::feeds::NO_NEW_STORIES => "no new stories".into(),
            (0, Some(_)) => "unavailable".to_owned(),
            (1, _) => "1 item".to_owned(),
            (n, _) => format!("{n} items"),
        };
        d.blocks.push(Block::Fields {
            fields: vec![Field {
                runs: vec![
                    d.run(Face::Regular, t.body, &s.title),
                    d.run(Face::Italic, t.meta(), &format!("  {count}")),
                ],
                tail: Some(Tail::label(ids.section[i], Face::Bold, t.body)),
                leader: true,
                link: Some(ids.section[i]),
                ..Field::default()
            }],
            divided: false,
            after: t.body * 0.15,
        });
    }
    d.blocks.push(Block::Rule { thick: 0.4 });
}

/// A marker size: the masthead is sized at layout to fill the column.
const MASTHEAD_FIT: f32 = -1.0;
/// Share of the page's height a photo may take; the front page's lead photo less (`FITS`).
const PHOTO_CAP: f32 = 0.45;
/// The folio's room at the top of a page, in meta lines (§4.1).
const FOLIO_LINES: f32 = 2.2;
/// A kicker longer than this goes under the section flag rather than beside its title.
const KICKER_BESIDE: usize = 40;

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
                d.blocks.push(Block::Image {
                    image: i,
                    cap: PHOTO_CAP,
                });
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
    /// Marked content whose text reads as `0` (`/ActualText`): a hyphenated word's halves.
    BeginActual(String),
    EndActual,
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
    margin: f32,
    /// What each anchor's page is labelled, once a first pass has found out (§4.2).
    labels: Vec<Option<String>>,
    pages_total: Option<usize>,
    /// The widest label the paper could print: every label slot is this wide.
    widest_label: String,
    /// Story text in one column whatever the width: a large-print paper.
    single_column: bool,
}

/// A laid-out line: its pieces and their widths.
type Line = Vec<(Face, f32, String, f32)>;

/// How a line ends (print layout §4.5).
#[derive(Clone, Debug, Default, PartialEq)]
struct End {
    /// The line ends with the head of this word and a hyphen.
    hyphen: Option<String>,
    /// The line starts with the rest of the word the line before broke.
    continues: bool,
    /// A paragraph's last line, or one ending at a forced break: never justified.
    last: bool,
}

/// A column line, with what packing needs to know about its paragraph.
#[derive(Clone, Debug)]
struct ColumnLine {
    line: Line,
    end: End,
    h: f32,
    /// Its place in its paragraph, and the paragraph's line count.
    idx: usize,
    count: usize,
    /// A subhead's line: never a column's last.
    keep: bool,
    indent: f32,
    bullet: f32,
}

#[derive(Clone, Debug)]
enum Unit {
    Line(ColumnLine),
    /// Space between paragraphs, dropped at a column's top.
    Gap(f32),
}

impl Unit {
    fn height(&self) -> f32 {
        match self {
            Unit::Line(l) => l.h,
            Unit::Gap(h) => *h,
        }
    }
}

/// `units` packed into at most `n` columns no taller than `h`: each column's range. A
/// paragraph leaves two lines at a column's foot and takes two to the next, and a subhead
/// never ends a column (§4.5). The ranges cover a prefix of `units`.
fn pack(units: &[Unit], h: f32, n: usize) -> Vec<std::ops::Range<usize>> {
    let mut cols = Vec::new();
    let mut i = 0;
    while cols.len() < n && i < units.len() {
        while matches!(units.get(i), Some(Unit::Gap(_))) {
            i += 1;
        }
        if i >= units.len() {
            break;
        }
        let start = i;
        let (mut used, mut j) = (0.0, i);
        while j < units.len() && used + units[j].height() <= h + 0.01 {
            used += units[j].height();
            j += 1;
        }
        if j < units.len() {
            j = settle(units, start, j);
        }
        if j == start {
            break;
        }
        cols.push(start..j);
        i = j;
    }
    // trailing spaces belong to no column
    if cols.last().is_some_and(|c| c.end < units.len())
        && units[cols.last().expect("a column").end..]
            .iter()
            .all(|u| matches!(u, Unit::Gap(_)))
    {
        cols.last_mut().expect("a column").end = units.len();
    }
    cols
}

/// Where a column that would end before `end` should end instead, so it leaves no orphan,
/// widow or subhead at its foot. Never at `start` or before.
fn settle(units: &[Unit], start: usize, end: usize) -> usize {
    let line_at = |k: usize| match &units[k] {
        Unit::Line(l) => Some(l),
        Unit::Gap(_) => None,
    };
    let mut k = end;
    for _ in 0..4 {
        let Some(last) = (start..k).rev().find(|&u| line_at(u).is_some()) else {
            break;
        };
        let l = line_at(last).expect("a line");
        let next = (k..units.len()).find_map(line_at);
        // the line after the column is its paragraph's last: a widow in the next column
        let widow = next.is_some_and(|n| n.idx + 1 == n.count && n.idx == l.idx + 1 && l.idx >= 1);
        let orphan = l.idx == 0 && l.count >= 2;
        if !(l.keep || orphan || widow) || last <= start {
            break;
        }
        k = last;
    }
    if k <= start { end } else { k }
}

/// The stretch each space between words takes for `line` to fill `width`, or `None` when the
/// line stays ragged: it has no spaces, or one would stretch past 2.5 times `space`.
fn justify(line: &Line, width: f32, space: f32) -> Option<f32> {
    let gaps = line.iter().skip(1).filter(|p| p.2.starts_with(' ')).count();
    if gaps == 0 {
        return None;
    }
    let extra = (width - line.iter().map(|p| p.3).sum::<f32>()).max(0.0);
    let per = extra / gaps as f32;
    (per <= space * 1.5).then_some(per)
}

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
            // the folio's room at the top of every page but the front, which starts at the
            // margin (§4.1)
            top: margin + FOLIO_LINES * t.meta(),
            bottom: page_h - margin,
            pages: vec![Vec::new()],
            y: margin,
            anchors: vec![None; anchors],
            links: Vec::new(),
            margin,
            labels: Vec::new(),
            pages_total: None,
            widest_label: "A88".into(),
            single_column: false,
        }
    }

    fn page(&self) -> usize {
        self.pages.len() - 1
    }

    fn at_top(&self) -> bool {
        self.y <= self.top + 0.01 && (self.page() > 0 || self.pages[0].is_empty())
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
        self.lines_with(runs, width, false)
            .into_iter()
            .map(|(line, _)| line)
            .collect()
    }

    /// [`Layout::lines`], with how each line ends, hyphenating words that don't fit when
    /// `hyphenate` (column text, print layout §4.5).
    fn lines_with(&self, runs: &[Run], width: f32, hyphenate: bool) -> Vec<(Line, End)> {
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
        let mut lines: Vec<(Line, End)> = vec![(Vec::new(), End::default())];
        let mut x = 0.0f32;
        // hyphenated lines in a row
        let mut hyphens = 0;
        for (face, size, word, spaced) in words {
            if word == "\n" {
                lines.last_mut().expect("a line").1.last = true;
                lines.push((Vec::new(), End::default()));
                x = 0.0;
                continue;
            }
            let sp = if spaced && !lines.last().is_some_and(|l| l.0.is_empty()) {
                self.fonts.width(face, size, " ")
            } else {
                0.0
            };
            let mut word = word;
            let w = self.fonts.width(face, size, &word);
            if x + sp + w > width && !lines.last().is_some_and(|l| l.0.is_empty()) {
                let split = (hyphenate && hyphens < 2)
                    .then(|| self.split_word(face, size, &word, width - x - sp))
                    .flatten();
                match split {
                    Some((head, tail)) => {
                        let hw = self.fonts.width(face, size, &head);
                        let (line, end) = lines.last_mut().expect("a line");
                        line.push((
                            face,
                            size,
                            format!("{}{head}", if sp > 0.0 { " " } else { "" }),
                            sp + hw,
                        ));
                        end.hyphen = Some(word.clone());
                        hyphens += 1;
                        lines.push((
                            Vec::new(),
                            End {
                                continues: true,
                                ..End::default()
                            },
                        ));
                        x = 0.0;
                        word = tail;
                    }
                    None => {
                        hyphens = 0;
                        lines.push((Vec::new(), End::default()));
                        x = 0.0;
                    }
                }
            }
            let w = self.fonts.width(face, size, &word);
            let (line, _) = lines.last_mut().expect("a line");
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
                    lines.last_mut().expect("a line").0.push((
                        face,
                        size,
                        std::mem::take(&mut piece),
                        pw,
                    ));
                    lines.push((Vec::new(), End::default()));
                    x = 0.0;
                    pw = 0.0;
                }
                piece.push(c);
                pw += cw;
            }
            lines
                .last_mut()
                .expect("a line")
                .0
                .push((face, size, piece, pw));
            x += pw;
        }
        if let Some((_, end)) = lines.last_mut() {
            end.last = true;
        }
        // An empty line (a forced break at the end) carries its `last` to the line before.
        let mut out: Vec<(Line, End)> = Vec::with_capacity(lines.len());
        for (line, end) in lines {
            if line.is_empty() {
                if let Some(prev) = out.last_mut() {
                    prev.1.last |= end.last;
                }
                continue;
            }
            out.push((line, end));
        }
        out
    }

    /// `word` broken to fit `room`: its longest head that fits, with a hyphen (or its own
    /// hyphen), and the rest. Only words of five letters or more with no digit, `@`, `/` or
    /// `.`, at least two letters before the break and three after (print layout §4.5).
    fn split_word(&self, face: Face, size: f32, word: &str, room: f32) -> Option<(String, String)> {
        let lead = word.len()
            - word
                .trim_start_matches(|c: char| !c.is_alphanumeric())
                .len();
        let core_end = word.trim_end_matches(|c: char| !c.is_alphanumeric()).len();
        if core_end <= lead {
            return None;
        }
        let (prefix, core, suffix) = (&word[..lead], &word[lead..core_end], &word[core_end..]);
        let letters = core.chars().filter(|c| c.is_alphabetic()).count();
        if letters < 5
            || core
                .chars()
                .any(|c| c.is_ascii_digit() || "@/.".contains(c))
        {
            return None;
        }
        // Where it may break: after its own hyphens, else between syllables.
        let breaks: Vec<(usize, bool)> = if core.contains('-') {
            core.match_indices('-')
                .map(|(i, _)| (i + 1, false))
                .filter(|(i, _)| *i < core.len())
                .collect()
        } else {
            let mut at = 0;
            hypher::hyphenate_bounded(core, hypher::Lang::English, 2, 3)
                .map(|syllable| {
                    at += syllable.len();
                    (at, true)
                })
                .filter(|(i, _)| *i < core.len())
                .collect()
        };
        breaks.into_iter().rev().find_map(|(i, add)| {
            let head = format!("{prefix}{}{}", &core[..i], if add { "-" } else { "" });
            (self.fonts.width(face, size, &head) <= room)
                .then(|| (head, format!("{}{suffix}", &core[i..])))
        })
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

    fn image(&mut self, img: &Image, cap: f32) {
        let (pw, ph) = img.size();
        let (pw, ph) = (pw.max(1) as f32, ph.max(1) as f32);
        // Small pictures (icons) stay small; photos fill the column, up to `cap` of the page.
        let natural = pw * 72.0 / 150.0;
        let mut w = natural.min(self.width);
        let mut h = w * ph / pw;
        let max_h = (self.bottom - self.top) * cap;
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

    /// The width `t` takes whatever its slot ends up saying.
    fn tail_width(&self, t: &Tail) -> f32 {
        let widest = match &t.slot {
            Slot::Label(_) => self.widest_label.clone(),
            Slot::Pages => "888".to_owned(),
            Slot::Text(s) => s.clone(),
        };
        self.fonts
            .width(t.face, t.size, &format!("{}{widest}{}", t.prefix, t.suffix))
    }

    fn tail_text(&self, t: &Tail) -> String {
        let slot = match &t.slot {
            Slot::Label(a) => self
                .labels
                .get(*a)
                .cloned()
                .flatten()
                .unwrap_or_else(|| self.widest_label.clone()),
            Slot::Pages => self.pages_total.map_or("888".into(), |n| n.to_string()),
            Slot::Text(s) => s.clone(),
        };
        format!("{}{slot}{}", t.prefix, t.suffix)
    }

    /// Cells side by side: each wraps its runs in what its tail leaves, then sets the tail at
    /// its right edge on its last line, with dot leaders when asked.
    fn fields(&mut self, fields: &[Field], divided: bool, after: f32) {
        if fields.is_empty() {
            return;
        }
        let gap = self.t.body;
        let n = fields.len() as f32;
        let cell_w = (self.width - gap * (n - 1.0)) / n;
        let laid: Vec<(Vec<Line>, f32, f32)> = fields
            .iter()
            .map(|f| {
                let tail_w = f
                    .tail
                    .as_ref()
                    .map_or(0.0, |t| self.tail_width(t) + gap * 0.5);
                let text_w = (cell_w - tail_w).max(cell_w * 0.2);
                let mut runs = f.runs.clone();
                if let Some(r) = runs.first_mut() {
                    r.text = r.text.trim_start().to_owned();
                }
                if let Some(r) = runs.last_mut() {
                    r.text = r.text.trim_end().to_owned();
                }
                let lines = self.lines(&runs, text_w);
                let tail_h = f.tail.as_ref().map_or(0.0, |t| self.t.leading(t.size));
                let h: f32 = lines.iter().map(|l| self.line_height(l)).sum();
                (lines, text_w, h.max(tail_h))
            })
            .collect();
        let h = laid.iter().map(|l| l.2).fold(0.0, f32::max);
        self.room(h);
        let (page, y0) = (self.page(), self.y);
        for (k, (f, (lines, text_w, _))) in fields.iter().zip(&laid).enumerate() {
            let x0 = self.left + k as f32 * (cell_w + gap);
            let mut y = y0;
            let (mut baseline, mut end_x) = (None, x0);
            for line in lines {
                let size = line.iter().map(|p| p.1).fold(0.0, f32::max);
                let base = y + size * 1.05;
                let line_w: f32 = line.iter().map(|p| p.3).sum();
                let mut x = x0
                    + match f.align {
                        Align::Left => 0.0,
                        Align::Center => ((text_w - line_w) / 2.0).max(0.0),
                    };
                for (face, size, text, w) in line {
                    self.op(Op::Text {
                        x,
                        y: base,
                        face: *face,
                        size: *size,
                        text: text.clone(),
                    });
                    x += w;
                }
                (baseline, end_x) = (Some(base), x);
                y += self.line_height(line);
            }
            if let Some(t) = &f.tail {
                let text = self.tail_text(t);
                let tx = x0 + cell_w - self.fonts.width(t.face, t.size, &text);
                let base = baseline.unwrap_or(y0 + t.size * 1.05);
                if f.leader {
                    let dot = self.fonts.width(Face::Regular, t.size, ". ");
                    let room = tx - end_x - gap * 0.8;
                    let count = (room / dot.max(0.01)).floor();
                    if count >= 2.0 {
                        self.op(Op::Text {
                            x: tx - gap * 0.4 - count * dot,
                            y: base,
                            face: Face::Regular,
                            size: t.size,
                            text: ". ".repeat(count as usize),
                        });
                    }
                }
                self.op(Op::Text {
                    x: tx,
                    y: base,
                    face: t.face,
                    size: t.size,
                    text,
                });
            }
            if divided && k > 0 {
                let x = x0 - gap / 2.0;
                self.op(Op::Line {
                    x1: x,
                    y1: y0,
                    x2: x,
                    y2: y0 + h,
                    width: 0.4,
                });
            }
            if let Some(anchor) = f.link {
                self.links.push(Link {
                    page,
                    x: x0,
                    y: y0,
                    w: cell_w,
                    h,
                    anchor,
                });
            }
        }
        self.y = y0 + h + after;
    }

    /// How many columns story text takes on this page (§4.5): as many as fit at 17 ems each,
    /// at most three; one for a large-print paper.
    fn column_count(&self) -> usize {
        if self.single_column {
            return 1;
        }
        let g = self.gutter();
        (((self.width + g) / (17.0 * self.t.body + g)).floor() as usize).clamp(1, 3)
    }

    fn gutter(&self) -> f32 {
        1.4 * self.t.body
    }

    /// A run of story text in columns: cut into lines, packed column by column and page by
    /// page, the last page balanced, justified and hyphenated, with hairlines between columns.
    /// One column lays the run out as ordinary text.
    fn columns(&mut self, items: &[Block], images: &[Image]) {
        let n = self.column_count();
        if n == 1 {
            self.run_blocks(items, images);
            return;
        }
        let g = self.gutter();
        let col_w = (self.width - g * (n - 1) as f32) / n as f32;
        let units = self.units(items, col_w);
        let tallest = units.iter().map(Unit::height).fold(0.0, f32::max);
        let mut i = 0;
        while i < units.len() {
            let avail = self.bottom - self.y;
            let rest = &units[i..];
            let all = pack(rest, avail, n);
            if all.last().is_some_and(|c| c.end == rest.len()) {
                // The rest fits on this page: the shortest columns that still hold it.
                let (mut lo, mut hi) = (tallest.min(avail), avail);
                for _ in 0..24 {
                    let mid = (lo + hi) / 2.0;
                    match pack(rest, mid, n)
                        .last()
                        .is_some_and(|c| c.end == rest.len())
                    {
                        true => hi = mid,
                        false => lo = mid,
                    }
                }
                let cols = pack(rest, hi, n);
                self.place(rest, &cols, col_w);
                break;
            }
            let consumed = all.last().map_or(0, |c| c.end);
            if consumed == 0 {
                if self.at_top() {
                    // a line taller than the page: set it anyway rather than loop
                    self.place(rest, std::slice::from_ref(&(0..1)), col_w);
                    i += 1;
                }
                self.new_page();
                continue;
            }
            self.place(rest, &all, col_w);
            i += consumed;
            self.new_page();
        }
    }

    /// A run's paragraphs as line units and the spaces between them.
    fn units(&self, items: &[Block], col_w: f32) -> Vec<Unit> {
        let mut units = Vec::new();
        for item in items {
            match item {
                Block::Para(p) => {
                    let mut runs = p.runs.clone();
                    if let Some(r) = runs.first_mut() {
                        r.text = r.text.trim_start().to_owned();
                    }
                    if let Some(r) = runs.last_mut() {
                        r.text = r.text.trim_end().to_owned();
                    }
                    let bullet = if p.bullet { self.t.body } else { 0.0 };
                    let lines = self.lines_with(&runs, col_w - p.indent - bullet, true);
                    if lines.is_empty() {
                        continue;
                    }
                    if p.before > 0.0 {
                        units.push(Unit::Gap(p.before));
                    }
                    let count = lines.len();
                    for (idx, (line, end)) in lines.into_iter().enumerate() {
                        units.push(Unit::Line(ColumnLine {
                            h: self.line_height(&line),
                            line,
                            end,
                            idx,
                            count,
                            keep: p.keep,
                            indent: p.indent,
                            bullet,
                        }));
                    }
                    if p.after > 0.0 {
                        units.push(Unit::Gap(p.after));
                    }
                }
                Block::Space(h) => units.push(Unit::Gap(*h)),
                _ => {}
            }
        }
        units
    }

    /// Draws `units`' columns side by side from the current height, then the rules between
    /// them, and moves below the tallest.
    fn place(&mut self, units: &[Unit], cols: &[std::ops::Range<usize>], col_w: f32) {
        let g = self.gutter();
        let space = self.fonts.width(Face::Regular, self.t.body, " ");
        let top = self.y;
        let mut tallest = 0.0f32;
        for (c, range) in cols.iter().enumerate() {
            let x0 = self.left + c as f32 * (col_w + g);
            let mut y = top;
            for (k, unit) in units[range.clone()].iter().enumerate() {
                let l = match unit {
                    Unit::Gap(h) => {
                        if k > 0 {
                            y += h;
                        }
                        continue;
                    }
                    Unit::Line(l) => l,
                };
                let size = l.line.iter().map(|p| p.1).fold(0.0, f32::max);
                let baseline = y + size * 1.05;
                if l.bullet > 0.0 && l.idx == 0 {
                    self.op(Op::Text {
                        x: x0 + l.indent,
                        y: baseline,
                        face: Face::Regular,
                        size,
                        text: "•".into(),
                    });
                }
                let room = col_w - l.indent - l.bullet;
                let stretch = match l.end.last {
                    true => None,
                    false => justify(&l.line, room, space),
                };
                let mut x = x0 + l.indent + l.bullet;
                let pieces = l.line.len();
                for (p, (face, size, text, w)) in l.line.iter().enumerate() {
                    if p > 0 && text.starts_with(' ') {
                        x += stretch.unwrap_or(0.0);
                    }
                    let tail = p == 0 && l.end.continues;
                    let head = p + 1 == pieces && l.end.hyphen.is_some();
                    if tail {
                        self.op(Op::BeginActual(String::new()));
                    }
                    if head {
                        let word = l.end.hyphen.clone().unwrap_or_default();
                        self.op(Op::BeginActual(word));
                    }
                    self.op(Op::Text {
                        x,
                        y: baseline,
                        face: *face,
                        size: *size,
                        text: text.clone(),
                    });
                    for _ in 0..(usize::from(tail) + usize::from(head)) {
                        self.op(Op::EndActual);
                    }
                    x += w;
                }
                y += l.h;
            }
            tallest = tallest.max(y - top);
        }
        for c in 1..cols.len() {
            let x = self.left + c as f32 * (col_w + g) - g / 2.0;
            self.op(Op::Line {
                x1: x,
                y1: top,
                x2: x,
                y2: top + tallest,
                width: 0.4,
            });
        }
        self.y = top + tallest + self.t.body * 0.3;
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
            Some(Block::Fields { .. } | Block::Columns { .. }) => self.t.leading(self.t.body),
            _ => 0.0,
        }
    }

    fn run(&mut self, doc: &Doc) {
        self.run_blocks(&doc.blocks, &doc.images);
    }

    fn run_blocks(&mut self, blocks: &[Block], images: &[Image]) {
        for (k, b) in blocks.iter().enumerate() {
            match b {
                Block::Para(p) => {
                    let next = blocks[k + 1..]
                        .iter()
                        .find(|b| !matches!(b, Block::Anchor(_) | Block::Space(_)));
                    let keep = self.first_height(next);
                    self.para(p, keep);
                }
                Block::Columns { items } => self.columns(items, images),
                Block::Image { image, cap } => {
                    self.image(&images[*image], *cap);
                    if let Some(Op::Image { image: slot, .. }) =
                        self.pages.last_mut().and_then(|p| p.last_mut())
                    {
                        *slot = *image;
                    }
                }
                Block::Rule { thick } => self.rule(*thick),
                Block::Fields {
                    fields,
                    divided,
                    after,
                } => self.fields(fields, *divided, *after),
                Block::Table { rows, grid } => self.table(rows, *grid, images),
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
    let print = crate::print::print_plan(g, &crate::front::front_plan(g));
    let doc = fitted(&fonts, g, t, photos, &print, page);
    let (_, lay, ranges, labels) = two_passes(&fonts, &doc, &print.book, page, t);
    let furniture = Furniture {
        paper: &g.edition.paper,
        date: long_date(g.edition.date),
        labels: &labels,
        ranges: &ranges,
    };
    write(&fonts, &doc, &lay, &furniture)
}

/// The document with the first fit that keeps the whole front page on page 1, else the first
/// that keeps its jump line there (§4.3), else the last one tried. Each try is one layout pass.
fn fitted(
    fonts: &Fonts,
    g: &Gathered,
    t: Type,
    photos: Photos,
    print: &PrintPlan,
    page: (f32, f32),
) -> Doc {
    let mut fits = FITS.to_vec();
    if let Some(x) = &print.jump {
        let n = crate::print::paragraph_ends(&x.opening).len();
        fits.extend((1..n).rev().map(|keep| Fit {
            keep,
            ..FITS[FITS.len() - 1]
        }));
    }
    let widest = widest_label(fonts, &print.book);
    let on_front = |lay: &Layout, a: usize| lay.anchors[a].is_some_and(|(p, _)| p == 0);
    let (mut jump_fits, mut last) = (None, None);
    for fit in fits {
        let d = document(g, t, photos, print, fit);
        let Some(jump_end) = d.jump_end else {
            return d;
        };
        let lay = lay_out(fonts, &d, page, t, &widest, Vec::new(), None);
        if on_front(&lay, d.front_end) {
            return d;
        }
        if on_front(&lay, jump_end) && jump_fits.is_none() {
            jump_fits = Some(fit);
        }
        last = Some(d);
    }
    match jump_fits {
        Some(fit) => document(g, t, photos, print, fit),
        None => last.expect("at least one fit"),
    }
}

/// Book ranges and every page's label.
type Labels = (Vec<(usize, char)>, Vec<String>);

/// Two passes (§4.2): the first finds the pages, the second prints their labels in slots
/// already sized for them, so nothing moves. Both layouts, the book ranges and each page's
/// label. Sections' anchors are the document's first.
fn two_passes<'a>(
    fonts: &'a Fonts,
    doc: &Doc,
    book: &[Option<char>],
    page: (f32, f32),
    t: Type,
) -> (Layout<'a>, Layout<'a>, Vec<(usize, char)>, Vec<String>) {
    let widest = widest_label(fonts, book);
    let first = lay_out(fonts, doc, page, t, &widest, Vec::new(), None);
    let section_pages: Vec<Option<usize>> = first.anchors[..book.len()]
        .iter()
        .map(|a| a.map(|(p, _)| p))
        .collect();
    let (ranges, labels): Labels = page_labels(book, &section_pages, first.pages.len());
    let by_anchor = first
        .anchors
        .iter()
        .map(|a| a.map(|(p, _)| labels[p].clone()))
        .collect();
    let second = lay_out(
        fonts,
        doc,
        page,
        t,
        &widest,
        by_anchor,
        Some(first.pages.len()),
    );
    debug_assert_eq!(second.pages.len(), first.pages.len(), "pass 2 moved a page");
    debug_assert_eq!(second.anchors, first.anchors, "pass 2 moved an anchor");
    (first, second, ranges, labels)
}

fn lay_out<'a>(
    fonts: &'a Fonts,
    doc: &Doc,
    page: (f32, f32),
    t: Type,
    widest: &str,
    labels: Vec<Option<String>>,
    pages_total: Option<usize>,
) -> Layout<'a> {
    let mut lay = Layout::new(fonts, page, t, doc.anchors);
    lay.widest_label = widest.to_owned();
    lay.labels = labels;
    lay.pages_total = pages_total;
    lay.single_column = doc.large_print;
    lay.run(doc);
    if lay.pages.last().is_some_and(Vec::is_empty) && lay.pages.len() > 1 {
        lay.pages.pop();
    }
    lay
}

/// The widest label the paper could print: its widest letter and two digits.
fn widest_label(fonts: &Fonts, book: &[Option<char>]) -> String {
    let letter = book
        .iter()
        .flatten()
        .copied()
        .chain(['A'])
        .max_by(|a, b| {
            let w = |c: &char| fonts.width(Face::Bold, 10.0, &c.to_string());
            w(a).total_cmp(&w(b))
        })
        .unwrap_or('A');
    format!("{letter}88")
}

/// Each book's first page, and every page's label (§3, §4.2). The front page opens book A;
/// a section that starts a new book starts its range at its first page.
fn page_labels(
    book: &[Option<char>],
    section_pages: &[Option<usize>],
    pages: usize,
) -> (Vec<(usize, char)>, Vec<String>) {
    let mut ranges = vec![(0, 'A')];
    for (b, p) in book.iter().zip(section_pages) {
        let (Some(b), Some(p)) = (b, p) else {
            continue;
        };
        let last = ranges.last_mut().expect("the front page's");
        if *b == last.1 {
            continue;
        }
        if *p > last.0 {
            ranges.push((*p, *b));
        } else {
            last.1 = *b;
        }
    }
    let labels = (0..pages)
        .map(|n| {
            let (start, c) = ranges
                .iter()
                .rev()
                .find(|(s, _)| *s <= n)
                .copied()
                .unwrap_or((0, 'A'));
            format!("{c}{}", n - start + 1)
        })
        .collect();
    (ranges, labels)
}

/// What every page but the front carries, and the label of each page.
struct Furniture<'a> {
    paper: &'a str,
    date: String,
    labels: &'a [String],
    ranges: &'a [(usize, char)],
}

fn deflate(data: &[u8]) -> Vec<u8> {
    let mut e = flate2::write::ZlibEncoder::new(Vec::new(), flate2::Compression::default());
    e.write_all(data).expect("in memory");
    e.finish().expect("in memory")
}

fn glyph_bytes(glyphs: &[(u16, char)]) -> Vec<u8> {
    glyphs.iter().flat_map(|(g, _)| g.to_be_bytes()).collect()
}

fn write(fonts: &Fonts, doc: &Doc, lay: &Layout, furniture: &Furniture) -> Vec<u8> {
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
        let folio = match n {
            0 => Vec::new(),
            _ => folio(fonts, lay, furniture, n),
        };
        // the folio first, so text extraction and read-back meet it first
        for op in folio.iter().chain(ops) {
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
                // a hyphenated word's halves read as the whole word (§4.5)
                Op::BeginActual(text) => {
                    c.begin_marked_content_with_properties(Name(b"Span"))
                        .properties()
                        .actual_text(TextStr(text));
                }
                Op::EndActual => {
                    c.end_marked_content();
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
        if fonts.masthead.is_blackletter() {
            "UnifrakturMaguntia-Book"
        } else {
            "Masthead"
        },
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
    // Page labels: each book's pages numbered from 1 under its letter (§4.2).
    let label_refs: Vec<Ref> = furniture.ranges.iter().map(|_| alloc()).collect();
    for (r, (_, letter)) in label_refs.iter().zip(furniture.ranges) {
        pdf.indirect(*r)
            .start::<pdf_writer::writers::PageLabel>()
            .style(NumberingStyle::Arabic)
            .prefix(TextStr(&letter.to_string()));
    }
    {
        let mut cat = pdf.catalog(catalog);
        cat.pages(tree)
            .outlines(outline_root)
            .page_mode(pdf_writer::types::PageMode::UseOutlines);
        let mut tree = cat.page_labels();
        let mut nums = tree.nums();
        for (r, (start, _)) in label_refs.iter().zip(furniture.ranges) {
            nums.insert(*start as i32, *r);
        }
    }
    pdf.finish()
}

/// Page `n`'s folio (§4.1): its label at the left, the paper in the middle, the date at the
/// right, over a hairline.
fn folio(fonts: &Fonts, lay: &Layout, f: &Furniture, n: usize) -> Vec<Op> {
    let size = lay.t.meta();
    let y = lay.margin + size * 1.05;
    let label = f.labels.get(n).cloned().unwrap_or_default();
    let paper_w = fonts.width(Face::Regular, size, f.paper);
    let date_w = fonts.width(Face::Italic, size, &f.date);
    vec![
        Op::Text {
            x: lay.left,
            y,
            face: Face::Bold,
            size,
            text: label,
        },
        Op::Text {
            x: lay.left + (lay.width - paper_w) / 2.0,
            y,
            face: Face::Regular,
            size,
            text: f.paper.to_owned(),
        },
        Op::Text {
            x: lay.left + lay.width - date_w,
            y,
            face: Face::Italic,
            size,
            text: f.date.clone(),
        },
        Op::Line {
            x1: lay.left,
            y1: y + size * 0.5,
            x2: lay.left + lay.width,
            y2: y + size * 0.5,
            width: 0.4,
        },
    ]
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

    fn run(text: &str) -> Vec<Run> {
        vec![Run {
            face: Face::Regular,
            size: 10.0,
            text: text.into(),
        }]
    }

    fn words_of(lines: &[(Line, End)]) -> Vec<String> {
        lines
            .iter()
            .map(|(l, _)| l.iter().map(|p| p.2.as_str()).collect::<String>())
            .collect()
    }

    #[test]
    fn a_long_word_breaks_at_a_syllable_inside_its_bounds() {
        let f = fonts();
        let l = layout(&f);
        let text = "The council voted unquestionably for the extraordinarily complicated plan";
        let width = 120.0;
        let lines = l.lines_with(&run(text), width, true);
        let got = words_of(&lines);
        let broken: Vec<&(Line, End)> = lines.iter().filter(|(_, e)| e.hyphen.is_some()).collect();
        assert!(!broken.is_empty(), "{got:?}");
        for (i, (line, end)) in lines.iter().enumerate() {
            let w: f32 = line.iter().map(|p| p.3).sum();
            assert!(w <= width + 0.5, "{w}: {got:?}");
            if let Some(word) = &end.hyphen {
                let head = line.last().unwrap().2.trim().trim_end_matches('-');
                let tail = lines[i + 1].0[0].2.trim();
                assert!(
                    head.chars().count() >= 2 && tail.chars().count() >= 3,
                    "{head}|{tail}"
                );
                assert_eq!(format!("{head}{tail}"), *word);
                assert!(lines[i + 1].1.continues);
            }
        }
        // every word comes back once, whole
        let joined = got.join("").replace("-", "");
        assert_eq!(
            joined.split_whitespace().collect::<Vec<_>>().join(" "),
            text
        );
        assert!(lines.last().unwrap().1.last);
    }

    #[test]
    fn a_hyphenated_compound_breaks_at_its_own_hyphen() {
        let f = fonts();
        let l = layout(&f);
        let width = f.width(Face::Regular, 10.0, "A well-") + 2.0;
        let lines = l.lines_with(&run("A well-known name"), width, true);
        assert_eq!(words_of(&lines)[0], "A well-");
        assert_eq!(lines[0].1.hyphen.as_deref(), Some("well-known"));
    }

    #[test]
    fn numbers_addresses_and_short_words_never_break() {
        let f = fonts();
        let l = layout(&f);
        for word in [
            "1234567890",
            "example.com",
            "a@b.org",
            "path/to/thing",
            "cats",
        ] {
            let text = format!("Seven words come first here and then {word}");
            let lines = l.lines_with(&run(&text), 140.0, true);
            assert!(lines.iter().all(|(_, e)| e.hyphen.is_none()), "{word}");
        }
    }

    #[test]
    fn never_three_hyphens_in_a_row() {
        let f = fonts();
        let l = layout(&f);
        let text = [
            "extraordinarily",
            "unquestionably",
            "notwithstanding",
            "incomprehensibility",
        ]
        .repeat(6)
        .join(" ");
        for width in [70.0, 85.0, 100.0, 120.0] {
            let lines = l.lines_with(&run(&text), width, true);
            let mut row = 0;
            for (_, e) in &lines {
                row = if e.hyphen.is_some() { row + 1 } else { 0 };
                assert!(row <= 2, "{width}: {:?}", words_of(&lines));
            }
        }
    }

    #[test]
    fn without_hyphenation_the_lines_are_todays() {
        let f = fonts();
        let l = layout(&f);
        let text = "The mill stood by the river for a hundred years and more, extraordinarily.";
        let plain: Vec<Line> = l
            .lines_with(&run(text), 90.0, false)
            .into_iter()
            .map(|(l, _)| l)
            .collect();
        assert_eq!(plain, l.lines(&run(text), 90.0));
    }

    #[test]
    fn justified_lines_fill_the_column_and_last_lines_dont() {
        let f = fonts();
        let l = layout(&f);
        let width = 150.0;
        let space = f.width(Face::Regular, 10.0, " ");
        let text = "The fence comes down in March and the first market is set for the second \
                    Saturday in April, rain or shine, and the wheel will be turning when it opens.";
        let lines = l.lines_with(&run(text), width, true);
        for (line, end) in &lines[..lines.len() - 1] {
            let stretch = justify(line, width, space).expect("a full line justifies");
            let w: f32 = line.iter().map(|p| p.3).sum();
            let gaps = line.iter().skip(1).filter(|p| p.2.starts_with(' ')).count() as f32;
            assert!((w + stretch * gaps - width).abs() < 0.5, "{w} {stretch}");
            assert!(end.hyphen.is_some() || !end.last);
        }
        // a line with too few gaps to fill stays ragged
        let lonely = l.lines_with(&run("Short words"), width, true);
        assert_eq!(justify(&lonely[0].0, width, space), None);
    }

    #[test]
    fn column_counts_follow_the_page_width() {
        let f = fonts();
        let count = |screen: Screen, single: bool| {
            let page = screen.page();
            let mut l = Layout::new(&f, page, Type::for_width(page.0), 0);
            l.single_column = single;
            l.column_count()
        };
        assert_eq!(count(Screen::Remarkable2, false), 2);
        assert_eq!(count(Screen::RemarkablePaperPro, false), 2);
        assert_eq!(count(Screen::RemarkablePaperProMove, false), 1);
        assert_eq!(count(Screen::Ipad, false), 2);
        assert_eq!(count(Screen::A5, false), 2);
        assert_eq!(count(Screen::Remarkable2, true), 1, "large print");
    }

    fn paras(n: usize, words: usize) -> Vec<Block> {
        (0..n)
            .map(|i| {
                let text = (0..words)
                    .map(|k| format!("p{i}w{k}"))
                    .collect::<Vec<_>>()
                    .join(" ");
                let mut p = Para::new(run(&text));
                p.after = 4.0;
                Block::Para(p)
            })
            .collect()
    }

    /// The x of every text op, the y of every text op, and the text in drawing order.
    fn column_ops(l: &Layout) -> (Vec<f32>, Vec<f32>, String) {
        let texts: Vec<(f32, f32)> = l
            .pages
            .iter()
            .flatten()
            .filter_map(|o| match o {
                Op::Text { x, y, .. } => Some((*x, *y)),
                _ => None,
            })
            .collect();
        let all: Vec<Op> = l.pages.iter().flatten().cloned().collect();
        (
            texts.iter().map(|t| t.0).collect(),
            texts.iter().map(|t| t.1).collect(),
            text_of(&all),
        )
    }

    #[test]
    fn a_run_reads_in_order_and_balances_its_last_page() {
        let f = fonts();
        let mut l = layout(&f);
        let items = paras(3, 40);
        l.columns(&items, &[]);
        let (xs, ys, text) = column_ops(&l);
        // reading order: every word once, in order
        let want: Vec<String> = (0..3)
            .flat_map(|i| (0..40).map(move |k| format!("p{i}w{k}")))
            .collect();
        assert_eq!(text.replace('-', ""), want.join(" "));
        // two columns, the second starting at the first one's top
        let mid = l.left + l.width / 2.0;
        assert!(xs.iter().any(|x| *x < mid) && xs.iter().any(|x| *x > mid));
        // balanced: the columns end within one line of each other
        let foot = |right: bool| {
            xs.iter()
                .zip(&ys)
                .filter(|(x, _)| (**x > mid) == right)
                .map(|(_, y)| *y)
                .fold(0.0f32, f32::max)
        };
        assert!(
            (foot(false) - foot(true)).abs() <= l.t.leading(10.0) + 0.1,
            "{} {}",
            foot(false),
            foot(true)
        );
        assert_eq!(l.pages.len(), 1);
        // a hairline between the columns
        assert!(
            l.pages[0]
                .iter()
                .any(|o| matches!(o, Op::Line { x1, x2, .. } if (x1 - x2).abs() < 0.01))
        );
    }

    #[test]
    fn a_long_run_snakes_onto_the_next_page() {
        let f = fonts();
        let mut l = layout(&f);
        let items = paras(30, 50);
        l.columns(&items, &[]);
        assert!(l.pages.len() > 1);
        let all: Vec<Op> = l.pages.iter().flatten().cloned().collect();
        let text = text_of(&all).replace('-', "");
        assert!(text.starts_with("p0w0 p0w1") && text.ends_with("p29w48 p29w49"));
        assert_eq!(text.split(' ').count(), 30 * 50, "every word once");
    }

    #[test]
    fn no_paragraph_leaves_one_line_alone_at_a_columns_edge() {
        let f = fonts();
        for words in [12, 25, 33, 47, 60] {
            let mut l = layout(&f);
            let items = paras(9, words);
            l.columns(&items, &[]);
            // per column: the paragraphs whose words it holds, in order
            let mid = l.left + l.width / 2.0;
            for page in &l.pages {
                for right in [false, true] {
                    let lines: Vec<(f32, usize)> = page
                        .iter()
                        .filter_map(|o| match o {
                            Op::Text { x, y, text, .. } if (*x > mid) == right => {
                                let para = text
                                    .trim()
                                    .trim_start_matches('p')
                                    .split('w')
                                    .next()?
                                    .parse()
                                    .ok()?;
                                Some((*y, para))
                            }
                            _ => None,
                        })
                        .collect();
                    let mut ys: Vec<(f32, usize)> = lines.clone();
                    ys.sort_by(|a, b| a.0.total_cmp(&b.0));
                    ys.dedup_by(|a, b| a.0 == b.0);
                    if ys.len() < 2 {
                        continue;
                    }
                    let first = ys[0].1;
                    let last = ys[ys.len() - 1].1;
                    let at_top = ys.iter().filter(|(_, p)| *p == first).count();
                    let at_foot = ys.iter().filter(|(_, p)| *p == last).count();
                    let total = |p: usize| {
                        l.lines_with(
                            &run(&(0..words)
                                .map(|k| format!("p{p}w{k}"))
                                .collect::<Vec<_>>()
                                .join(" ")),
                            (l.width - 14.0) / 2.0,
                            true,
                        )
                        .len()
                    };
                    if at_top < total(first) {
                        assert!(at_top >= 2, "{words}: a widow at a column's top");
                    }
                    if at_foot < total(last) {
                        assert!(at_foot >= 2, "{words}: an orphan at a column's foot");
                    }
                }
            }
        }
    }

    #[test]
    fn a_hyphenated_words_halves_read_as_the_whole_word() {
        let f = fonts();
        let mut l = layout(&f);
        let text = ["extraordinarily", "unquestionably", "notwithstanding"]
            .repeat(20)
            .join(" ");
        l.columns(&[Block::Para(Para::new(run(&text)))], &[]);
        let ops: Vec<&Op> = l.pages.iter().flatten().collect();
        let mut wrapped = 0;
        for (k, op) in ops.iter().enumerate() {
            if let Op::BeginActual(word) = op {
                let Op::Text { text, .. } = ops[k + 1] else {
                    panic!("text follows a BeginActual");
                };
                if word.is_empty() {
                    assert!(!text.ends_with('-') || text.trim().len() > 1);
                } else {
                    assert!(text.ends_with('-'), "{text}");
                    assert!(
                        word.starts_with(text.trim().trim_end_matches('-')),
                        "{word} {text}"
                    );
                    wrapped += 1;
                }
                assert!(matches!(ops[k + 2], Op::EndActual | Op::BeginActual(_)));
            }
        }
        assert!(wrapped > 0);
        let begins = ops
            .iter()
            .filter(|o| matches!(o, Op::BeginActual(_)))
            .count();
        let ends = ops.iter().filter(|o| matches!(o, Op::EndActual)).count();
        assert_eq!(begins, ends);
    }

    #[test]
    fn the_written_pdf_marks_hyphenated_words() {
        let mut g = paper();
        g.edition.sections[1].articles[0].body = format!(
            "<p>{}</p>",
            ["extraordinarily", "unquestionably"].repeat(40).join(" ")
        );
        let bytes = render(&g, Screen::Remarkable2, Photos::Gray);
        // page content is deflated: inflate every stream and look inside
        let mut found = false;
        let mut rest = &bytes[..];
        while let Some(at) = rest.windows(7).position(|w| w == b"stream\n") {
            let from = at + 7;
            let Some(len) = rest[from..].windows(9).position(|w| w == b"endstream") else {
                break;
            };
            let mut out = Vec::new();
            use std::io::Read;
            if flate2::read::ZlibDecoder::new(&rest[from..from + len])
                .read_to_end(&mut out)
                .is_ok()
                && out.windows(11).any(|w| w == b"/ActualText")
            {
                found = true;
                break;
            }
            rest = &rest[from + len + b"endstream".len()..];
        }
        assert!(found, "no /ActualText in any content stream");
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
    fn page_labels_number_each_book_from_its_first_page() {
        let book = [Some('A'), Some('A'), Some('B'), Some('C')];
        let pages = [Some(1), Some(2), Some(4), Some(7)];
        let (ranges, labels) = page_labels(&book, &pages, 9);
        assert_eq!(ranges, vec![(0, 'A'), (4, 'B'), (7, 'C')]);
        assert_eq!(
            labels,
            ["A1", "A2", "A3", "A4", "B1", "B2", "B3", "C1", "C2"]
        );
        // a section that was never laid out doesn't start a book
        let (ranges, _) = page_labels(&[Some('A'), Some('B')], &[Some(1), None], 2);
        assert_eq!(ranges, vec![(0, 'A')]);
    }

    fn text_of(ops: &[Op]) -> String {
        ops.iter()
            .filter_map(|o| match o {
                Op::Text { text, .. } => Some(text.as_str()),
                _ => None,
            })
            .collect::<Vec<_>>()
            .join(" ")
            .split_whitespace()
            .collect::<Vec<_>>()
            .join(" ")
    }

    fn row(text: &str, leader: bool) -> Field {
        Field {
            runs: vec![Run {
                face: Face::Regular,
                size: 10.0,
                text: text.into(),
            }],
            tail: Some(Tail::label(0, Face::Bold, 10.0)),
            leader,
            ..Field::default()
        }
    }

    #[test]
    fn a_slot_is_set_at_the_cells_edge_and_never_moves_a_line() {
        let f = fonts();
        let long = "Council weighs a night bus to the hospital and back, every forty minutes";
        let lay = |label: &str| {
            let mut l = layout(&f);
            l.labels = vec![Some(label.to_owned())];
            l.fields(&[row(long, false)], false, 0.0);
            l
        };
        let (a, b) = (lay("A2"), lay("B88"));
        assert_eq!(a.y, b.y);
        let words = |l: &Layout| -> Vec<(f32, f32)> {
            l.pages[0]
                .iter()
                .filter_map(|o| match o {
                    Op::Text { x, y, text, .. } if !text.contains('A') && !text.contains('B') => {
                        Some((*x, *y))
                    }
                    _ => None,
                })
                .collect()
        };
        assert_eq!(
            words(&a),
            words(&b),
            "the text sits the same with either label"
        );
        let Some(Op::Text {
            x,
            face,
            size,
            text,
            ..
        }) = a.pages[0].last()
        else {
            panic!("the tail is drawn last");
        };
        assert_eq!(text, "A2");
        let right = x + f.width(*face, *size, text);
        assert!((right - (a.left + a.width)).abs() < 0.01, "{right}");
    }

    #[test]
    fn leaders_only_when_asked() {
        let f = fonts();
        let dots = |leader: bool| {
            let mut l = layout(&f);
            l.labels = vec![Some("B1".into())];
            l.fields(&[row("Local", leader)], false, 0.0);
            text_of(&l.pages[0]).contains(". . .")
        };
        assert!(dots(true));
        assert!(!dots(false));
    }

    fn paper() -> Gathered {
        use crate::{Article, Edition, Section};
        let p = |n: usize, w: &str| format!("<p>{}</p>\n", vec![w; n].join(" "));
        let quote = "\u{201c}We have waited forty years for somebody to do something with that yard,\u{201d} she said.";
        let quoted: String = (0..7)
            .map(|i| match i {
                2 => format!("<p>{quote}</p>\n"),
                _ => p(12, &format!("para{i}")),
            })
            .collect();
        let section = |key: &str, title: &str, articles: Vec<Article>| Section {
            key: key.into(),
            title: title.into(),
            articles,
            ..Default::default()
        };
        let story = |title: &str, body: String| Article {
            title: title.into(),
            body,
            meta: Some("Courier".into()),
            ..Default::default()
        };
        Gathered {
            news: vec![0, 1],
            reading: None,
            long_reads: Vec::new(),
            reading_outcomes: Vec::new(),
            notes: Vec::new(),
            names: Vec::new(),
            edition: Edition {
                large_print: false,
                paper: "The Deckle Daily".into(),
                date: chrono::NaiveDate::from_ymd_opt(2026, 10, 26).unwrap(),
                place: "Aberdale".into(),
                weather_line: None,
                masthead: Masthead::default(),
                sections: vec![
                    section(
                        "local",
                        "Local",
                        vec![story(
                            "Mill Yard Will Be a Market Again",
                            format!("{}{}", p(120, "opening"), p(40, "later")),
                        )],
                    ),
                    section("feeds:A", "World", vec![story("Herons Stay Later", quoted)]),
                    section("puzzle", "Puzzle", vec![story("Crossword", p(5, "clue"))]),
                    section(
                        "reading",
                        "Your Reading",
                        vec![story("Saved", p(5, "saved"))],
                    ),
                ],
                assets: Vec::new(),
            },
        }
    }

    /// Both passes of `g` on `screen`, and every page's text.
    fn laid(g: &Gathered, screen: Screen) -> (usize, usize, Vec<String>) {
        let f = Box::leak(Box::new(Fonts::new(g.edition.masthead)));
        let page = screen.page();
        let t = Type::for_width(page.0);
        let print = crate::print::print_plan(g, &crate::front::front_plan(g));
        let doc = fitted(f, g, t, Photos::Gray, &print, page);
        let (first, second, _, _) = two_passes(f, &doc, &print.book, page, t);
        let texts = second.pages.iter().map(|ops| text_of(ops)).collect();
        (first.pages.len(), second.pages.len(), texts)
    }

    #[test]
    fn the_front_page_teases_jumps_and_indexes_with_page_labels() {
        let (_, _, pages) = laid(&paper(), Screen::Remarkable2);
        let front = &pages[0];
        // the skybox: what's inside, not on the front, with its page
        assert!(front.contains("PUZZLE · Crossword B1"), "{front}");
        assert!(front.contains("YOUR READING · Saved B2"), "{front}");
        assert!(front.contains("Final edition"));
        assert!(front.contains(&format!("{} pages", pages.len())), "{front}");
        // the lead's opening, then its jump
        assert!(front.contains("opening opening"));
        assert!(!front.contains("later"));
        assert!(front.contains("See MILL, A2 \u{203a}"), "{front}");
        // a refer and the index, with labels and leaders
        assert!(front.contains("Herons Stay Later World A"), "{front}");
        assert!(front.contains("Inside today"));
        assert!(front.contains(". . ."));
    }

    #[test]
    fn the_jump_lands_under_its_head_and_the_pull_quote_between_rules() {
        let (_, _, pages) = laid(&paper(), Screen::Remarkable2);
        let landing = pages
            .iter()
            .find(|t| t.contains("Continued from A1"))
            .expect("a jump head");
        assert!(landing.contains("MILL · Continued from A1 Mill Yard Will Be a Market Again"));
        assert!(landing.contains("later later"));
        assert!(
            !landing.contains("opening"),
            "the opening isn't printed twice"
        );
        let world = pages.iter().find(|t| t.contains("para0")).unwrap();
        let quote = "\u{201c}We have waited forty years for somebody to do something with that yard\u{201d}";
        let at = world
            .find(quote)
            .unwrap_or_else(|| panic!("the pull quote: {world}"));
        assert!(world.find("para4").unwrap() < at && at < world.find("para5").unwrap());
    }

    #[test]
    fn a_short_lead_prints_whole_and_its_section_refers_to_it() {
        let mut g = paper();
        g.edition.sections[0].articles[0].body = "<p>All of it.</p>".into();
        let (_, _, pages) = laid(&g, Screen::Remarkable2);
        assert!(pages[0].contains("All of it."));
        assert!(!pages[0].contains("See MILL"));
        assert!(
            pages
                .iter()
                .any(|t| t.contains("Mill Yard Will Be a Market Again — on A1"))
        );
    }

    #[test]
    fn the_jump_line_stays_on_the_front_page_and_nothing_is_lost() {
        // print layout §4.3: a front page too full for its lead is fitted, never overflowed
        let mut g = paper();
        let lead = &mut g.edition.sections[0].articles[0];
        lead.title = "Beyond the governor\u{2019}s race: New York\u{2019}s minor parties fight to \
                      maintain ballot access and political influence across the state"
            .into();
        let p = |w: &str| format!("<p>{}</p>\n", vec![w; 40].join(" "));
        lead.body = format!("{}{}{}{}", p("alpha"), p("beta"), p("gamma"), p("later"));
        for screen in [
            Screen::Remarkable2,
            Screen::RemarkablePaperProMove,
            Screen::A5,
        ] {
            let (_, _, pages) = laid(&g, screen);
            assert!(
                pages[0].contains("See BEYOND, A2 \u{203a}"),
                "{screen:?}: page 2 is [{}]",
                pages[1].chars().take(400).collect::<String>()
            );
            let all = pages.join(" ");
            for w in ["alpha", "beta", "gamma", "later"] {
                assert_eq!(all.matches(w).count(), 40, "{screen:?}: {w}");
            }
        }
    }

    #[test]
    fn both_passes_agree_on_every_screen() {
        for screen in [
            Screen::Remarkable2,
            Screen::RemarkablePaperPro,
            Screen::RemarkablePaperProMove,
            Screen::Ipad,
            Screen::A5,
        ] {
            let (first, second, pages) = laid(&paper(), screen);
            assert_eq!(first, second, "{screen:?}");
            assert!(pages[0].contains(&format!("{first} pages")), "{screen:?}");
        }
    }

    #[test]
    fn a_folio_carries_the_label_the_paper_and_the_date() {
        let f = fonts();
        let l = layout(&f);
        let labels = vec!["A1".to_owned(), "A2".to_owned()];
        let furniture = Furniture {
            paper: "The Deckle Daily",
            date: "Monday, October 26, 2026".into(),
            labels: &labels,
            ranges: &[(0, 'A')],
        };
        let ops = folio(&f, &l, &furniture, 1);
        assert_eq!(
            text_of(&ops),
            "A2 The Deckle Daily Monday, October 26, 2026"
        );
        assert!(ops.iter().any(|o| matches!(o, Op::Line { .. })));
        // it sits in the room the top margin keeps for it
        let Op::Text { y, .. } = &ops[0] else {
            panic!()
        };
        assert!(*y > l.margin && *y < l.top, "{y}");
    }

    #[test]
    fn the_pdf_carries_page_labels_per_book() {
        let bytes = render(&paper(), Screen::Remarkable2, Photos::Gray);
        let pdf = String::from_utf8_lossy(&bytes);
        assert!(pdf.contains("/PageLabels"), "no page labels");
        assert_eq!(pdf.matches("/Type /PageLabel").count(), 2, "A and B");
        assert!(pdf.contains("/P (A)") && pdf.contains("/P (B)"));
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
