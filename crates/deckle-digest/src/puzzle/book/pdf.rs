// SPDX-License-Identifier: GPL-3.0-or-later
//! The puzzle book as a PDF (puzzle books §4): a title page, one puzzle a page, the answers at
//! the back. Exact grids, so a fixed-geometry writer, not the Daily's flowing layout. Black on
//! white, no links: a pen stroke on a link would jump away mid-solve.

use std::collections::BTreeMap;

use ab_glyph::{Font, FontRef, GlyphId};
use pdf_writer::{Content, Name, Pdf, Rect, Ref, Str, TextStr};

use super::{Book, BookError, SEARCH_SIZES, words_wanted};
use crate::pdf::{BOLD, EmbedFace, REGULAR, Screen, embed_font, glyph_bytes, shape};
use crate::puzzle::search::Search;

/// The page a book is laid out for: the Kindle Scribe's screen, or one of the Daily's.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum PageSize {
    #[default]
    KindleScribe,
    Screen(Screen),
}

impl PageSize {
    /// Every name `--screen` takes, the Scribe's first.
    pub const NAMES: [&'static str; 6] = [
        "kindle-scribe",
        "remarkable-2",
        "remarkable-paper-pro",
        "remarkable-paper-pro-move",
        "ipad",
        "a5",
    ];

    /// The page, in points: the screen's pixels at its dpi.
    pub fn page(self) -> (f32, f32) {
        match self {
            // 1860×2480 px at 300 ppi
            PageSize::KindleScribe => (1860.0 / 300.0 * 72.0, 2480.0 / 300.0 * 72.0),
            PageSize::Screen(s) => s.page(),
        }
    }

    /// A `--screen` name.
    pub fn parse(name: &str) -> Option<PageSize> {
        match name {
            "kindle-scribe" => Some(PageSize::KindleScribe),
            other => serde_json::from_value(serde_json::Value::String(other.to_owned()))
                .ok()
                .map(PageSize::Screen),
        }
    }
}

/// Clear of the edges, where the Scribe's page-turn taps and toolbar sit (§4.1).
const MARGIN: f32 = 28.0;
/// A word-search square, at least: room to circle a letter (§4.2).
const MIN_CELL: f32 = 18.0;
const HEAD: f32 = 16.0;
const GAP: f32 = 12.0;
const LIST: f32 = 11.0;
const LEAD: f32 = 14.5;
const FOOT: f32 = 9.0;
/// Space between word-list columns.
const GUTTER: f32 = 12.0;
const CELL_LINE: f32 = 0.75;
const BORDER: f32 = 2.0;
/// Answers a page, two by two.
const ANSWERS_PER_PAGE: usize = 4;

#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
enum Face {
    Regular = 0,
    Bold = 1,
}

struct Faces {
    fonts: [FontRef<'static>; 2],
}

impl Faces {
    fn new() -> Faces {
        let f = |b: &'static [u8]| FontRef::try_from_slice(b).expect("checked font");
        Faces {
            fonts: [f(REGULAR), f(BOLD)],
        }
    }

    fn font(&self, face: Face) -> &FontRef<'static> {
        &self.fonts[face as usize]
    }

    fn width(&self, face: Face, size: f32, text: &str) -> f32 {
        let font = self.font(face);
        let units: f32 = shape(font, text)
            .iter()
            .map(|(g, _)| font.h_advance_unscaled(GlyphId(*g)))
            .sum();
        units / font.units_per_em().unwrap_or(1000.0) * size
    }
}

/// The title's size and lines: 28 pt on one line if it fits, else smaller and wrapped at
/// spaces onto up to three lines, down to 14 pt; a word too long even then is cut.
fn title_lines(faces: &Faces, title: &str, width: f32) -> (f32, Vec<String>) {
    let fits = |size: f32, s: &str| faces.width(Face::Bold, size, s) <= width;
    let wrap = |size: f32| {
        let mut lines: Vec<String> = Vec::new();
        for word in title.split_whitespace() {
            match lines.last_mut() {
                Some(l) if fits(size, &format!("{l} {word}")) => {
                    l.push(' ');
                    l.push_str(word);
                }
                _ => lines.push(word.to_owned()),
            }
        }
        lines
    };
    let mut size = 28.0;
    loop {
        let lines = wrap(size);
        let last = size <= 14.0;
        if (lines.len() <= 3 && lines.iter().all(|l| fits(size, l))) || last {
            let lines = lines
                .into_iter()
                .map(|l| {
                    let mut l = l;
                    while !fits(size, &l) && l.chars().count() > 1 {
                        l.pop();
                    }
                    l
                })
                .collect();
            return (size, lines);
        }
        size -= 1.0;
    }
}

/// Where a puzzle page's parts go.
struct PuzzleLayout {
    grid_x: f32,
    grid_y: f32,
    cell: f32,
    cols: usize,
    col_w: f32,
    list_y: f32,
}

/// The largest grid that leaves room for `n` words as wide as `widest` at [`LIST`] size, with
/// squares of [`MIN_CELL`] or more; `None` when even that doesn't fit. More list columns are
/// tried before giving up, so the grid shrinks first and the list widens after (§4.2).
fn lay_out(page: (f32, f32), size: usize, n: usize, widest: f32) -> Option<PuzzleLayout> {
    let (w, h) = page;
    let content = w - 2.0 * MARGIN;
    let grid_y = MARGIN + HEAD + GAP;
    let bottom = h - MARGIN - FOOT - GAP;
    let col_w = widest + GUTTER;
    let most = (((content + GUTTER) / col_w).floor() as usize).max(1);
    for cols in most.min(3)..=most {
        let rows = n.div_ceil(cols);
        let list_h = rows as f32 * LEAD;
        let side = content.min(bottom - grid_y - GAP - list_h);
        let cell = side / size as f32;
        if cell >= MIN_CELL {
            let side = cell * size as f32;
            return Some(PuzzleLayout {
                grid_x: (w - side) / 2.0,
                grid_y,
                cell,
                cols,
                col_w: (content + GUTTER) / cols as f32,
                list_y: grid_y + side + GAP + LIST,
            });
        }
    }
    None
}

/// A page's drawing: PDF content, with the glyphs it used noted for embedding.
struct Painter<'a> {
    c: Content,
    ph: f32,
    faces: &'a Faces,
    used: &'a mut [BTreeMap<u16, char>; 2],
}

impl Painter<'_> {
    /// `text` with its baseline `y` from the page's top.
    fn text(&mut self, face: Face, size: f32, x: f32, y: f32, text: &str) {
        let glyphs = shape(self.faces.font(face), text);
        if glyphs.is_empty() {
            return;
        }
        for (g, ch) in &glyphs {
            self.used[face as usize].insert(*g, *ch);
        }
        self.c.begin_text();
        self.c
            .set_font(Name(format!("F{}", face as usize).as_bytes()), size);
        self.c.set_text_matrix([1.0, 0.0, 0.0, 1.0, x, self.ph - y]);
        self.c.show(Str(&glyph_bytes(&glyphs)));
        self.c.end_text();
    }

    fn centred(&mut self, face: Face, size: f32, cx: f32, y: f32, text: &str) {
        let w = self.faces.width(face, size, text);
        self.text(face, size, cx - w / 2.0, y, text);
    }

    fn line(&mut self, width: f32, (x1, y1): (f32, f32), (x2, y2): (f32, f32)) {
        self.c.set_line_width(width);
        self.c.move_to(x1, self.ph - y1);
        self.c.line_to(x2, self.ph - y2);
        self.c.stroke();
    }

    /// A grid's lines: hairlines between squares, a heavy border round them.
    fn grid(&mut self, x: f32, y: f32, cell: f32, size: usize) {
        let side = cell * size as f32;
        for k in 1..size {
            let d = k as f32 * cell;
            self.line(CELL_LINE, (x + d, y), (x + d, y + side));
            self.line(CELL_LINE, (x, y + d), (x + side, y + d));
        }
        self.c.set_line_width(BORDER);
        self.c.rect(x, self.ph - y - side, side, side);
        self.c.stroke();
    }

    /// A grid's letters, centred in their squares; `size_of` gives each square's letter size.
    fn letters(
        &mut self,
        s: &Search,
        x: f32,
        y: f32,
        cell: f32,
        size_of: impl Fn(usize, usize) -> f32,
    ) {
        for (r, row) in s.rows.iter().enumerate() {
            for (c, ch) in row.chars().enumerate() {
                let size = size_of(r, c);
                let cx = x + (c as f32 + 0.5) * cell;
                // capitals stand about 0.7 em tall: centre them on the square
                let base = y + (r as f32 + 0.5) * cell + 0.35 * size;
                self.centred(Face::Regular, size, cx, base, &ch.to_string());
            }
        }
    }

    /// A rounded outline round a hidden word's squares, from its first to its last.
    fn capsule(&mut self, from: (f32, f32), to: (f32, f32), r: f32) {
        let flip = |(x, y): (f32, f32)| (x, self.ph - y);
        let (p0, p1) = (flip(from), flip(to));
        let len = ((p1.0 - p0.0).powi(2) + (p1.1 - p0.1).powi(2)).sqrt();
        let u = ((p1.0 - p0.0) / len, (p1.1 - p0.1) / len);
        let n = (-u.1, u.0);
        let k = 0.5523 * r;
        let at = |p: (f32, f32), a: f32, b: f32| (p.0 + u.0 * a + n.0 * b, p.1 + u.1 * a + n.1 * b);
        let c = &mut self.c;
        c.set_line_width(CELL_LINE);
        let s = at(p0, 0.0, r);
        c.move_to(s.0, s.1);
        let e = at(p1, 0.0, r);
        c.line_to(e.0, e.1);
        for (a, b, d) in [
            (at(p1, k, r), at(p1, r, k), at(p1, r, 0.0)),
            (at(p1, r, -k), at(p1, k, -r), at(p1, 0.0, -r)),
        ] {
            c.cubic_to(a.0, a.1, b.0, b.1, d.0, d.1);
        }
        let e = at(p0, 0.0, -r);
        c.line_to(e.0, e.1);
        for (a, b, d) in [
            (at(p0, -k, -r), at(p0, -r, -k), at(p0, -r, 0.0)),
            (at(p0, -r, k), at(p0, -k, r), at(p0, 0.0, r)),
        ] {
            c.cubic_to(a.0, a.1, b.0, b.1, d.0, d.1);
        }
        c.close_path();
        c.stroke();
    }
}

/// Pages in the book's PDF: the title, a page a puzzle, and the answers.
pub fn pages(b: &Book) -> usize {
    1 + b.puzzles.len() + b.puzzles.len().div_ceil(ANSWERS_PER_PAGE)
}

/// The book as a PDF for `page`, or [`BookError::DoesNotFit`] when its grids would be too fine
/// to write in there. The same book gives the same bytes.
pub fn render(b: &Book, page: PageSize) -> Result<Vec<u8>, BookError> {
    let faces = Faces::new();
    let (pw, ph) = page.page();
    let widest = b
        .puzzles
        .iter()
        .flat_map(|p| &p.words)
        .map(|h| faces.width(Face::Regular, LIST, &h.word.to_uppercase()))
        .fold(0.0, f32::max);
    let size = b.puzzles.first().map_or(0, Search::size);
    let layouts: Vec<PuzzleLayout> = b
        .puzzles
        .iter()
        .map(|p| lay_out((pw, ph), p.size(), p.words.len(), widest))
        .collect::<Option<_>>()
        .ok_or_else(|| BookError::DoesNotFit {
            size,
            largest: SEARCH_SIZES
                .rev()
                .find(|&s| s < size && lay_out((pw, ph), s, words_wanted(s), widest).is_some())
                .unwrap_or(0),
        })?;

    let mut used: [BTreeMap<u16, char>; 2] = Default::default();
    let mut pages: Vec<Content> = Vec::new();
    let count = b.puzzles.len();
    let first_answers = 1 + count;

    // The title page.
    {
        let mut p = Painter {
            c: Content::new(),
            ph,
            faces: &faces,
            used: &mut used,
        };
        let (size, lines) = title_lines(&faces, &b.title, pw - 2.0 * MARGIN);
        let lead = size * 1.2;
        // the last line sits where a one-line title would
        let y = ph * 0.38 - lead * (lines.len() - 1) as f32;
        for (k, line) in lines.iter().enumerate() {
            p.centred(Face::Bold, size, pw / 2.0, y + lead * k as f32, line);
        }
        let y = ph * 0.38;
        p.centred(Face::Regular, 14.0, pw / 2.0, y + 34.0, &b.subtitle);
        let date = b.date.format("%-d %B %Y").to_string();
        p.centred(Face::Regular, 12.0, pw / 2.0, y + 58.0, &date);
        p.centred(
            Face::Regular,
            10.0,
            pw / 2.0,
            ph - MARGIN,
            "Made with Deckle",
        );
        pages.push(p.c);
    }

    // A puzzle a page.
    for (i, (s, l)) in b.puzzles.iter().zip(&layouts).enumerate() {
        let mut p = Painter {
            c: Content::new(),
            ph,
            faces: &faces,
            used: &mut used,
        };
        p.text(
            Face::Bold,
            HEAD,
            MARGIN,
            MARGIN + HEAD,
            &format!("No. {}", i + 1),
        );
        p.grid(l.grid_x, l.grid_y, l.cell, s.size());
        p.letters(s, l.grid_x, l.grid_y, l.cell, |_, _| 0.55 * l.cell);
        // alphabetical down each column, then across
        let rows = s.words.len().div_ceil(l.cols);
        for (k, h) in s.words.iter().enumerate() {
            let (col, row) = (k / rows, k % rows);
            let x = MARGIN + col as f32 * l.col_w;
            let y = l.list_y + row as f32 * LEAD;
            p.text(Face::Regular, LIST, x, y, &h.word.to_uppercase());
        }
        let answer_page = first_answers + i / ANSWERS_PER_PAGE + 1;
        p.text(
            Face::Regular,
            FOOT,
            MARGIN,
            ph - MARGIN,
            &format!("Answers on page {answer_page}"),
        );
        pages.push(p.c);
    }

    // The answers, two by two.
    let top = MARGIN + HEAD + GAP;
    let slot_w = (pw - 2.0 * MARGIN - GAP) / 2.0;
    let slot_h = (ph - MARGIN - top - GAP) / 2.0;
    for (n, chunk) in b.puzzles.chunks(ANSWERS_PER_PAGE).enumerate() {
        let mut p = Painter {
            c: Content::new(),
            ph,
            faces: &faces,
            used: &mut used,
        };
        p.text(Face::Bold, HEAD, MARGIN, MARGIN + HEAD, "Answers");
        for (k, s) in chunk.iter().enumerate() {
            let number = n * ANSWERS_PER_PAGE + k + 1;
            let x = MARGIN + (k % 2) as f32 * (slot_w + GAP);
            let y = top + (k / 2) as f32 * (slot_h + GAP);
            p.text(Face::Bold, LIST, x, y + LIST, &format!("No. {number}"));
            let side = slot_w.min(slot_h - LIST - 6.0);
            let cell = side / s.size() as f32;
            let gy = y + LIST + 6.0;
            let mut answer = vec![false; s.size() * s.size()];
            for h in &s.words {
                for (r, c) in s.cells(h) {
                    answer[r * s.size() + c] = true;
                }
            }
            p.grid(x, gy, cell, s.size());
            let size = s.size();
            p.letters(s, x, gy, cell, |r, c| {
                // filler letters smaller, so the answers stand out
                if answer[r * size + c] {
                    0.55 * cell
                } else {
                    0.33 * cell
                }
            });
            for h in &s.words {
                let cells = s.cells(h);
                let centre = |(r, c): (usize, usize)| {
                    (x + (c as f32 + 0.5) * cell, gy + (r as f32 + 0.5) * cell)
                };
                p.capsule(
                    centre(cells[0]),
                    centre(*cells.last().expect("words have letters")),
                    0.42 * cell,
                );
            }
        }
        pages.push(p.c);
    }

    Ok(write(b, &faces, &used, pages, (pw, ph), first_answers))
}

/// The PDF: pages, both faces embedded, the outline (title, each puzzle, the answers) and the
/// book's title. No dates and no annotations, so the same pages give the same bytes.
fn write(
    b: &Book,
    faces: &Faces,
    used: &[BTreeMap<u16, char>; 2],
    pages: Vec<Content>,
    (pw, ph): (f32, f32),
    first_answers: usize,
) -> Vec<u8> {
    let mut pdf = Pdf::new();
    let mut next = 1;
    let mut alloc = || {
        next += 1;
        Ref::new(next - 1)
    };
    let catalog = alloc();
    let tree = alloc();
    let outline_root = alloc();
    let info = alloc();
    let font_refs = [alloc(), alloc()];
    let page_refs: Vec<Ref> = pages.iter().map(|_| alloc()).collect();
    let content_refs: Vec<Ref> = pages.iter().map(|_| alloc()).collect();

    for ((page, id), content) in pages.into_iter().zip(&page_refs).zip(&content_refs) {
        let data = page.finish();
        pdf.stream(*content, &data);
        let mut p = pdf.page(*id);
        p.parent(tree)
            .media_box(Rect::new(0.0, 0.0, pw, ph))
            .contents(*content);
        let mut resources = p.resources();
        let mut fonts = resources.fonts();
        fonts.pair(Name(b"F0"), font_refs[0]);
        fonts.pair(Name(b"F1"), font_refs[1]);
    }
    pdf.pages(tree)
        .kids(page_refs.iter().copied())
        .count(page_refs.len() as i32);

    for (face, name, file) in [
        (Face::Regular, "OldStandard-Regular", REGULAR),
        (Face::Bold, "OldStandard-Bold", BOLD),
    ] {
        let refs = [font_refs[face as usize], alloc(), alloc(), alloc(), alloc()];
        embed_font(
            &mut pdf,
            refs,
            &EmbedFace {
                name,
                file,
                font: faces.font(face),
                italic: false,
                bold: face == Face::Bold,
                used: &used[face as usize],
            },
        );
    }

    let mut marks: Vec<(String, usize)> = vec![("Title".to_owned(), 0)];
    marks.extend((1..first_answers).map(|n| (format!("No. {n}"), n)));
    marks.push(("Answers".to_owned(), first_answers));
    let items: Vec<Ref> = marks.iter().map(|_| alloc()).collect();
    for (k, ((title, page), id)) in marks.iter().zip(&items).enumerate() {
        let mut it = pdf.outline_item(*id);
        it.title(TextStr(title)).parent(outline_root);
        if k > 0 {
            it.prev(items[k - 1]);
        }
        if let Some(n) = items.get(k + 1) {
            it.next(*n);
        }
        it.dest().page(page_refs[*page]).xyz(0.0, ph, None);
    }
    pdf.outline(outline_root)
        .first(items[0])
        .last(*items.last().expect("a title and answers"))
        .count(items.len() as i32);
    pdf.document_info(info)
        .title(TextStr(&b.title))
        .creator(TextStr("Deckle"));
    pdf.catalog(catalog)
        .pages(tree)
        .outlines(outline_root)
        .page_mode(pdf_writer::types::PageMode::UseOutlines);
    pdf.finish()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A long title wraps onto lines that each fit between the margins, never off the page.
    #[test]
    fn a_long_title_wraps_inside_the_margins() {
        let faces = Faces::new();
        let width = 446.4 - 2.0 * MARGIN;
        let title =
            "The Extremely Long and Winding Technology Word Search Collection for Rainy Weekends";
        let (size, lines) = title_lines(&faces, title, width);
        assert!(size >= 14.0);
        assert!(lines.len() > 1 && lines.len() <= 3, "{lines:?}");
        assert_eq!(lines.join(" "), title);
        for l in &lines {
            assert!(faces.width(Face::Bold, size, l) <= width, "{l}");
        }
        let (short, one) = title_lines(&faces, "Word Search", width);
        assert_eq!((short, one), (28.0, vec!["Word Search".to_owned()]));
    }
}
