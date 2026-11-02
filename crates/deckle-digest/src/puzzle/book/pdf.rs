// SPDX-License-Identifier: GPL-3.0-or-later
//! The puzzle book as a PDF (puzzle books §4): a title page, one puzzle a page, the answers at
//! the back. Exact grids, so a fixed-geometry writer, not the Daily's flowing layout. Black on
//! white, no links: a pen stroke on a link would jump away mid-solve.

use std::collections::BTreeMap;

use ab_glyph::{Font, FontRef, GlyphId};
use pdf_writer::{Content, Name, Pdf, Rect, Ref, Str, TextStr};
use rand::SeedableRng;

use super::{Book, BookError, CROSS_SIZES, Kind, LadderPage, Puzzles, SEARCH_SIZES, words_wanted};
use crate::pdf::{BOLD, EmbedFace, REGULAR, Screen, embed_font, glyph_bytes, shape};
use crate::puzzle::cross::{self, Crossword};
use crate::puzzle::cryptogram::Cryptogram;
use crate::puzzle::grid::Dir;
use crate::puzzle::maze::{self, Maze};
use crate::puzzle::nonogram::{self, Nonogram};
use crate::puzzle::search::Search;
use crate::puzzle::sudoku::Grid;

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
/// A sudoku square, at least: a digit and pencil marks (§4.2).
const MIN_SUDOKU_CELL: f32 = 40.0;

/// Answers a page and their columns: two by two, or two by three for sudoku's small grids;
/// ladders and cryptograms are lines of text, more to a page.
fn answer_grid(kind: Kind) -> (usize, usize) {
    match kind {
        Kind::Sudoku | Kind::Nonogram => (2, 3),
        Kind::Ladder => (2, 5),
        Kind::Cryptogram => (1, 6),
        _ => (2, 2),
    }
}

/// A nonogram square, at least: room to shade it with a pen (§4.2).
const MIN_NONO_CELL: f32 = 14.0;
/// A maze corridor, at least: a pen's line with room either side.
const MIN_MAZE_CELL: f32 = 9.0;
/// A cryptogram's letter square and its row (the blank, the cipher letter under it).
const CRYPT_CELL: f32 = 13.0;
const CRYPT_ROW: f32 = 34.0;
const CRYPT_WORD_GAP: f32 = 9.0;
/// A ladder's letter square, at most.
const MAX_LADDER_CELL: f32 = 26.0;

fn answers_per_page(kind: Kind) -> usize {
    let (cols, rows) = answer_grid(kind);
    cols * rows
}
/// A crossword square, at least: room to write a letter (§4.2).
const MIN_CROSS_CELL: f32 = 22.0;
/// A crossword square, at most: a 5×5 needn't fill the page.
const MAX_CROSS_CELL: f32 = 40.0;
/// The share of the page a crossword's grid may take; the clues get the rest.
const CROSS_GRID: f32 = 0.62;
const CLUE: f32 = 10.0;
const CLUE_LEAD: f32 = 12.5;

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
        rows: &[String],
        x: f32,
        y: f32,
        cell: f32,
        size_of: impl Fn(usize, usize) -> f32,
    ) {
        for (r, row) in rows.iter().enumerate() {
            for (c, ch) in row.chars().enumerate() {
                if ch == '#' {
                    continue;
                }
                let size = size_of(r, c);
                let cx = x + (c as f32 + 0.5) * cell;
                // capitals stand about 0.7 em tall: centre them on the square
                let base = y + (r as f32 + 0.5) * cell + 0.35 * size;
                self.centred(Face::Regular, size, cx, base, &ch.to_string());
            }
        }
    }

    /// One row of `n` squares: a ladder's rung.
    fn boxes(&mut self, x: f32, y: f32, cell: f32, n: usize) {
        for k in 1..n {
            let d = k as f32 * cell;
            self.line(CELL_LINE, (x + d, y), (x + d, y + cell));
        }
        self.c.set_line_width(BORDER);
        self.c.rect(x, self.ph - y - cell, cell * n as f32, cell);
        self.c.stroke();
    }

    /// A crossword's black squares, filled.
    fn blacks(&mut self, rows: &[String], x: f32, y: f32, cell: f32) {
        for (r, row) in rows.iter().enumerate() {
            for (c, ch) in row.chars().enumerate() {
                if ch == '#' {
                    let top = y + r as f32 * cell;
                    self.c
                        .rect(x + c as f32 * cell, self.ph - top - cell, cell, cell);
                    self.c.fill_nonzero();
                }
            }
        }
    }

    /// A crossword's square numbers, small at each entry's first square's top left.
    fn numbers(&mut self, rows: &[String], x: f32, y: f32, cell: f32) {
        let size = (0.25 * cell).max(5.0);
        let mut done = std::collections::BTreeSet::new();
        for s in cross::slots(rows) {
            if done.insert(s.number) {
                let tx = x + s.col as f32 * cell + 1.5;
                let ty = y + s.row as f32 * cell + size + 0.5;
                self.text(Face::Regular, size, tx, ty, &s.number.to_string());
            }
        }
    }

    /// A sudoku: its squares, heavier lines round each 3×3 box, the givens in bold and, for an
    /// answer, the rest of `solution` in the regular face.
    fn sudoku(&mut self, givens: &Grid, solution: Option<&Grid>, x: f32, y: f32, cell: f32) {
        let side = cell * 9.0;
        for k in 1..9 {
            let d = k as f32 * cell;
            let w = if k % 3 == 0 { BORDER } else { CELL_LINE };
            self.line(w, (x + d, y), (x + d, y + side));
            self.line(w, (x, y + d), (x + side, y + d));
        }
        self.c.set_line_width(BORDER);
        self.c.rect(x, self.ph - y - side, side, side);
        self.c.stroke();
        for i in 0..81 {
            let (face, digit, size) = match (givens[i], solution) {
                (0, Some(sol)) => (Face::Regular, sol[i], 0.55 * cell),
                (0, None) => continue,
                (g, _) => (Face::Bold, g, 0.6 * cell),
            };
            let cx = x + ((i % 9) as f32 + 0.5) * cell;
            let base = y + ((i / 9) as f32 + 0.5) * cell + 0.35 * size;
            self.centred(face, size, cx, base, &digit.to_string());
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

/// A piece of text a crossword's clues print, placed: on its puzzle's first page or a page
/// after it.
struct Line {
    page: usize,
    x: f32,
    y: f32,
    face: Face,
    size: f32,
    text: String,
}

/// Where a crossword page's parts go: its grid, and its clues over as many pages as they take.
struct CrossLayout {
    grid_x: f32,
    grid_y: f32,
    cell: f32,
    lines: Vec<Line>,
    pages: usize,
}

/// The largest square a `size` crossword's grid can have on `page`.
fn cross_cell(page: (f32, f32), size: usize) -> f32 {
    let (w, h) = page;
    let side = (w - 2.0 * MARGIN).min(CROSS_GRID * (h - 2.0 * MARGIN));
    (side / size as f32).min(MAX_CROSS_CELL)
}

/// `text` broken at spaces into lines no wider than `width`.
fn wrap(faces: &Faces, face: Face, size: f32, text: &str, width: f32) -> Vec<String> {
    let mut lines: Vec<String> = Vec::new();
    for word in text.split_whitespace() {
        match lines.last_mut() {
            Some(l) if faces.width(face, size, &format!("{l} {word}")) <= width => {
                l.push(' ');
                l.push_str(word);
            }
            _ => lines.push(word.to_owned()),
        }
    }
    lines
}

/// One baseline's pieces: (x from the column's left, face, size, text).
type Row = Vec<(f32, Face, f32, String)>;

/// A crossword's page: "No. N", the grid, then Across and Down in two columns, carried on to
/// the next page when they run out of room. A clue is never split between columns, nor a
/// heading from its first clue. `None` when its squares would be under [`MIN_CROSS_CELL`].
fn lay_out_cross(faces: &Faces, page: (f32, f32), x: &Crossword) -> Option<CrossLayout> {
    let (w, h) = page;
    let size = x.rows.len();
    let cell = cross_cell(page, size);
    if cell < MIN_CROSS_CELL {
        return None;
    }
    let side = cell * size as f32;
    let grid_y = MARGIN + HEAD + GAP;
    let col_w = (w - 2.0 * MARGIN - GAP) / 2.0;
    let num_w = faces.width(Face::Bold, CLUE, "88") + 4.0;
    let bottom = h - MARGIN - FOOT - GAP;
    // the first page's clues start under the grid, a later page's under its heading
    let top = |pg: usize| match pg {
        0 => grid_y + side + GAP + LIST,
        _ => MARGIN + HEAD + GAP + LIST,
    };
    let mut blocks: Vec<Vec<Row>> = Vec::new();
    for (dir, head) in [(Dir::Across, "Across"), (Dir::Down, "Down")] {
        for (k, c) in x.clues.iter().filter(|c| c.dir == dir).enumerate() {
            let mut rows: Vec<Row> = Vec::new();
            if k == 0 {
                rows.push(vec![(0.0, Face::Bold, LIST, head.to_owned())]);
            }
            let num = c.number.to_string();
            let nx = num_w - 4.0 - faces.width(Face::Bold, CLUE, &num);
            for (j, line) in wrap(faces, Face::Regular, CLUE, &c.clue, col_w - num_w)
                .into_iter()
                .enumerate()
            {
                let mut row = vec![(num_w, Face::Regular, CLUE, line)];
                if j == 0 {
                    row.insert(0, (nx, Face::Bold, CLUE, num.clone()));
                }
                rows.push(row);
            }
            blocks.push(rows);
        }
    }
    let mut lines = Vec::new();
    let (mut pg, mut col) = (0, 0);
    let mut y = top(0);
    for (b, rows) in blocks.into_iter().enumerate() {
        // a little air above "Down", unless it tops a column
        let heading = rows[0].len() == 1 && rows[0][0].1 == Face::Bold;
        if heading && b > 0 && y > top(pg) {
            y += CLUE_LEAD * 0.5;
        }
        let height = (rows.len() - 1) as f32 * CLUE_LEAD;
        while y + height > bottom {
            (pg, col) = if col == 0 { (pg, 1) } else { (pg + 1, 0) };
            y = top(pg);
        }
        let x0 = MARGIN + col as f32 * (col_w + GAP);
        for row in rows {
            for (dx, face, size, text) in row {
                lines.push(Line {
                    page: pg,
                    x: x0 + dx,
                    y,
                    face,
                    size,
                    text,
                });
            }
            y += CLUE_LEAD;
        }
    }
    Some(CrossLayout {
        grid_x: (w - side) / 2.0,
        grid_y,
        cell,
        lines,
        pages: pg + 1,
    })
}

/// How the book's puzzles lay out on `page`: a page or more each.
enum Layouts {
    Search(Vec<PuzzleLayout>),
    Crossword(Vec<CrossLayout>),
    /// Every sudoku's grid: its top left and square.
    Sudoku {
        count: usize,
        x: f32,
        y: f32,
        cell: f32,
    },
    /// A page a puzzle, laid out as it's drawn: ladders, cryptograms, mazes.
    Plain {
        count: usize,
    },
    Nonogram(Vec<NonoLayout>),
}

impl Layouts {
    /// Each puzzle's page count.
    fn pages(&self) -> Vec<usize> {
        match self {
            Layouts::Search(l) => vec![1; l.len()],
            Layouts::Crossword(l) => l.iter().map(|x| x.pages).collect(),
            Layouts::Sudoku { count, .. } => vec![1; *count],
            Layouts::Plain { count } => vec![1; *count],
            Layouts::Nonogram(l) => vec![1; l.len()],
        }
    }
}

/// Where a nonogram's grid sits once its clues have their room.
struct NonoLayout {
    x: f32,
    y: f32,
    cell: f32,
    /// Row clues' width, left of the grid; column clues' height, above it.
    left: f32,
    top: f32,
}

/// The clue text size for a nonogram's squares.
fn nono_clue_size(cell: f32) -> f32 {
    (0.55 * cell).clamp(6.0, 11.0)
}

fn lay_out_nonogram(page: (f32, f32), n: &Nonogram) -> Option<NonoLayout> {
    let (pw, ph) = page;
    let size = n.size() as f32;
    let rows = n.row_clues().iter().map(Vec::len).max().unwrap_or(1) as f32;
    let cols = n.col_clues().iter().map(Vec::len).max().unwrap_or(1) as f32;
    // a clue takes about 0.7 of a square in its line
    let fit_w = (pw - 2.0 * MARGIN) / (size + 0.7 * rows + 0.3);
    let top_y = MARGIN + HEAD + GAP;
    let fit_h = (ph - top_y - MARGIN - FOOT - GAP) / (size + 0.7 * cols + 0.3);
    let cell = fit_w.min(fit_h).min(30.0);
    if cell < MIN_NONO_CELL {
        return None;
    }
    let left = 0.7 * rows * cell + 0.3 * cell;
    let top = 0.7 * cols * cell + 0.3 * cell;
    let x = (pw - left - size * cell) / 2.0 + left;
    Some(NonoLayout {
        x,
        y: top_y + top,
        cell,
        left,
        top,
    })
}

/// Lays the book's puzzles out, or [`BookError::DoesNotFit`] with the largest size that does.
fn lay_out_book(faces: &Faces, b: &Book, page: (f32, f32)) -> Result<Layouts, BookError> {
    let size = b.puzzles.size();
    let kind = b.puzzles.kind();
    match &b.puzzles {
        Puzzles::Ladder(ps) => Ok(Layouts::Plain { count: ps.len() }),
        Puzzles::Cryptogram(ps) => Ok(Layouts::Plain { count: ps.len() }),
        Puzzles::Maze(ps) => {
            let (w, h) = page;
            let y = MARGIN + HEAD + GAP;
            let side = (w - 2.0 * MARGIN).min(h - y - MARGIN - FOOT - GAP);
            if side / (size as f32) < MIN_MAZE_CELL {
                return Err(BookError::DoesNotFit {
                    kind,
                    size,
                    largest: maze::SIZES
                        .rev()
                        .find(|&s| s < size && side / s as f32 >= MIN_MAZE_CELL)
                        .unwrap_or(0),
                });
            }
            Ok(Layouts::Plain { count: ps.len() })
        }
        Puzzles::Nonogram(ps) => ps
            .iter()
            .map(|n| lay_out_nonogram(page, n))
            .collect::<Option<Vec<_>>>()
            .map(Layouts::Nonogram)
            .ok_or_else(|| BookError::DoesNotFit {
                kind,
                size,
                largest: nonogram::SIZES
                    .rev()
                    .find(|&s| {
                        s < size
                            && lay_out_nonogram(
                                page,
                                &nonogram::random(s, &mut rand::rngs::StdRng::seed_from_u64(1)),
                            )
                            .is_some()
                    })
                    .unwrap_or(0),
            }),
        Puzzles::Search(ps) => {
            let widest = ps
                .iter()
                .flat_map(|p| &p.words)
                .map(|h| faces.width(Face::Regular, LIST, &h.word.to_uppercase()))
                .fold(0.0, f32::max);
            ps.iter()
                .map(|p| lay_out(page, p.size(), p.words.len(), widest))
                .collect::<Option<Vec<_>>>()
                .map(Layouts::Search)
                .ok_or_else(|| BookError::DoesNotFit {
                    kind,
                    size,
                    largest: SEARCH_SIZES
                        .rev()
                        .find(|&s| s < size && lay_out(page, s, words_wanted(s), widest).is_some())
                        .unwrap_or(0),
                })
        }
        Puzzles::Sudoku(ps) => {
            let (w, h) = page;
            let y = MARGIN + HEAD + GAP;
            let side = (w - 2.0 * MARGIN).min(h - y - MARGIN - FOOT - GAP);
            let cell = side / 9.0;
            if cell < MIN_SUDOKU_CELL {
                return Err(BookError::DoesNotFit {
                    kind,
                    size,
                    largest: 0,
                });
            }
            Ok(Layouts::Sudoku {
                count: ps.len(),
                x: (w - side) / 2.0,
                y,
                cell,
            })
        }
        Puzzles::Crossword(xs) => xs
            .iter()
            .map(|x| lay_out_cross(faces, page, x))
            .collect::<Option<Vec<_>>>()
            .map(Layouts::Crossword)
            .ok_or_else(|| BookError::DoesNotFit {
                kind: Kind::Crossword,
                size,
                largest: CROSS_SIZES
                    .rev()
                    .find(|&s| s < size && cross_cell(page, s) >= MIN_CROSS_CELL)
                    .unwrap_or(0),
            }),
    }
}

/// Pages in the book's PDF on `page`: the title, each puzzle's pages, and the answers.
pub fn pages(b: &Book, page: PageSize) -> usize {
    let faces = Faces::new();
    let puzzles: usize = lay_out_book(&faces, b, page.page())
        .map(|l| l.pages().iter().sum())
        .unwrap_or(b.puzzles.len());
    1 + puzzles + b.puzzles.len().div_ceil(answers_per_page(b.puzzles.kind()))
}

/// The book as a PDF for `page`, or [`BookError::DoesNotFit`] when its grids would be too fine
/// to write in there. The same book gives the same bytes.
pub fn render(b: &Book, page: PageSize) -> Result<Vec<u8>, BookError> {
    let faces = Faces::new();
    let (pw, ph) = page.page();
    let layouts = lay_out_book(&faces, b, (pw, ph))?;
    let counts = layouts.pages();
    // each puzzle's first page, after the title page
    let starts: Vec<usize> = counts
        .iter()
        .scan(1, |at, n| {
            let s = *at;
            *at += n;
            Some(s)
        })
        .collect();
    let first_answers = 1 + counts.iter().sum::<usize>();

    let mut used: [BTreeMap<u16, char>; 2] = Default::default();
    let mut pages: Vec<Content> = Vec::new();
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

    // The puzzles: a page each, or more for a crossword's clues.
    for i in 0..b.puzzles.len() {
        let answer_page = first_answers + i / answers_per_page(b.puzzles.kind()) + 1;
        let foot = format!("Answers on page {answer_page}");
        for k in 0..counts[i] {
            let mut p = Painter {
                c: Content::new(),
                ph,
                faces: &faces,
                used: &mut used,
            };
            let head = match k {
                0 => format!("No. {}", i + 1),
                _ => format!("No. {} (continued)", i + 1),
            };
            p.text(Face::Bold, HEAD, MARGIN, MARGIN + HEAD, &head);
            match (&b.puzzles, &layouts) {
                (Puzzles::Search(ps), Layouts::Search(ls)) => {
                    let (s, l) = (&ps[i], &ls[i]);
                    p.grid(l.grid_x, l.grid_y, l.cell, s.size());
                    p.letters(&s.rows, l.grid_x, l.grid_y, l.cell, |_, _| 0.55 * l.cell);
                    // alphabetical down each column, then across
                    let rows = s.words.len().div_ceil(l.cols);
                    for (k, h) in s.words.iter().enumerate() {
                        let (col, row) = (k / rows, k % rows);
                        let x = MARGIN + col as f32 * l.col_w;
                        let y = l.list_y + row as f32 * LEAD;
                        p.text(Face::Regular, LIST, x, y, &h.word.to_uppercase());
                    }
                }
                (Puzzles::Crossword(xs), Layouts::Crossword(ls)) => {
                    let (x, l) = (&xs[i], &ls[i]);
                    if k == 0 {
                        p.blacks(&x.rows, l.grid_x, l.grid_y, l.cell);
                        p.grid(l.grid_x, l.grid_y, l.cell, x.rows.len());
                        p.numbers(&x.rows, l.grid_x, l.grid_y, l.cell);
                    }
                    for line in l.lines.iter().filter(|line| line.page == k) {
                        p.text(line.face, line.size, line.x, line.y, &line.text);
                    }
                }
                (Puzzles::Sudoku(ps), Layouts::Sudoku { x, y, cell, .. }) => {
                    p.sudoku(&ps[i].givens, None, *x, *y, *cell);
                }
                (Puzzles::Ladder(ps), Layouts::Plain { .. }) => {
                    p.ladder_page(&ps[i], (pw, ph));
                }
                (Puzzles::Cryptogram(ps), Layouts::Plain { .. }) => {
                    p.cryptogram(&ps[i], (pw, ph));
                }
                (Puzzles::Maze(ps), Layouts::Plain { .. }) => {
                    let y = MARGIN + HEAD + GAP;
                    let side = (pw - 2.0 * MARGIN).min(ph - y - MARGIN - FOOT - GAP);
                    p.maze(
                        &ps[i],
                        (pw - side) / 2.0,
                        y,
                        side / ps[i].size as f32,
                        false,
                    );
                }
                (Puzzles::Nonogram(ps), Layouts::Nonogram(ls)) => {
                    let (n, l) = (&ps[i], &ls[i]);
                    p.nonogram(n, l.x, l.y, l.cell, l.left, l.top, false);
                }
                _ => unreachable!("a book's layouts are of its own kind"),
            }
            p.text(Face::Regular, FOOT, MARGIN, ph - MARGIN, &foot);
            pages.push(p.c);
        }
    }

    // The answers, two by two.
    let top = MARGIN + HEAD + GAP;
    let (cols, rows) = answer_grid(b.puzzles.kind());
    let per = cols * rows;
    let slot_w = (pw - 2.0 * MARGIN - (cols - 1) as f32 * GAP) / cols as f32;
    let slot_h = (ph - MARGIN - top - (rows - 1) as f32 * GAP) / rows as f32;
    for n in 0..b.puzzles.len().div_ceil(per) {
        let mut p = Painter {
            c: Content::new(),
            ph,
            faces: &faces,
            used: &mut used,
        };
        p.text(Face::Bold, HEAD, MARGIN, MARGIN + HEAD, "Answers");
        let first = n * per;
        for k in 0..per.min(b.puzzles.len() - first) {
            let number = first + k + 1;
            let x = MARGIN + (k % cols) as f32 * (slot_w + GAP);
            let y = top + (k / cols) as f32 * (slot_h + GAP);
            p.text(Face::Bold, LIST, x, y + LIST, &format!("No. {number}"));
            let side = slot_w.min(slot_h - LIST - 6.0);
            let gy = y + LIST + 6.0;
            match &b.puzzles {
                Puzzles::Search(ps) => answer_search(&mut p, &ps[first + k], x, gy, side),
                Puzzles::Crossword(xs) => {
                    let rows = &xs[first + k].rows;
                    let cell = side / rows.len() as f32;
                    p.blacks(rows, x, gy, cell);
                    p.grid(x, gy, cell, rows.len());
                    p.letters(rows, x, gy, cell, |_, _| 0.55 * cell);
                }
                Puzzles::Sudoku(ps) => {
                    let s = &ps[first + k];
                    p.sudoku(&s.givens, Some(&s.solution), x, gy, side / 9.0);
                }
                Puzzles::Ladder(ps) => {
                    for (j, l) in ps[first + k].ladders.iter().enumerate() {
                        let text = format!(
                            "{}. {}",
                            (b'a' + j as u8) as char,
                            l.path
                                .iter()
                                .map(|w| w.to_uppercase())
                                .collect::<Vec<_>>()
                                .join(" → ")
                        );
                        p.text(
                            Face::Regular,
                            LIST - 1.0,
                            x,
                            gy + 2.0 + LEAD * j as f32,
                            &text,
                        );
                    }
                }
                Puzzles::Cryptogram(ps) => {
                    let c = &ps[first + k];
                    let lines = wrap(&faces, Face::Regular, LIST - 1.0, &c.plain, slot_w);
                    for (j, line) in lines.iter().take(3).enumerate() {
                        p.text(
                            Face::Regular,
                            LIST - 1.0,
                            x,
                            gy + 2.0 + LEAD * j as f32,
                            line,
                        );
                    }
                    if !c.source.is_empty() {
                        let j = lines.len().min(3) as f32;
                        p.text(
                            Face::Bold,
                            LIST - 2.0,
                            x,
                            gy + 2.0 + LEAD * j,
                            &format!("— {}", c.source),
                        );
                    }
                }
                Puzzles::Nonogram(ps) => {
                    let n = &ps[first + k];
                    let cell = side / n.size() as f32;
                    p.nonogram(n, x, gy, cell, 0.0, 0.0, true);
                    p.text(Face::Regular, FOOT, x, gy + side + FOOT + 3.0, &n.title);
                }
                Puzzles::Maze(ps) => {
                    let m = &ps[first + k];
                    p.maze(m, x, gy, side / m.size as f32, true);
                }
            }
        }
        pages.push(p.c);
    }

    Ok(write(
        b,
        &faces,
        &used,
        pages,
        (pw, ph),
        &starts,
        first_answers,
    ))
}

impl Painter<'_> {
    /// A nonogram: its grid with the row clues to the left and the column clues above (the
    /// clue room `left` and `top`, none for an answer), heavier lines every fifth square, and
    /// the squares shaded when `filled`.
    #[allow(clippy::too_many_arguments)]
    fn nonogram(
        &mut self,
        n: &Nonogram,
        x: f32,
        y: f32,
        cell: f32,
        left: f32,
        top: f32,
        filled: bool,
    ) {
        let size = n.size();
        let side = cell * size as f32;
        if filled {
            for (r, row) in n.cells.iter().enumerate() {
                for (c, &on) in row.iter().enumerate() {
                    if on {
                        let ty = y + r as f32 * cell;
                        self.c
                            .rect(x + c as f32 * cell, self.ph - ty - cell, cell, cell);
                        self.c.fill_nonzero();
                    }
                }
            }
        }
        for k in 1..size {
            let d = k as f32 * cell;
            let w = if k % 5 == 0 { BORDER * 0.75 } else { CELL_LINE };
            self.line(w, (x + d, y), (x + d, y + side));
            self.line(w, (x, y + d), (x + side, y + d));
        }
        self.c.set_line_width(BORDER);
        self.c.rect(x, self.ph - y - side, side, side);
        self.c.stroke();
        if left <= 0.0 && top <= 0.0 {
            return;
        }
        let fs = nono_clue_size(cell);
        let step = 0.7 * cell;
        // row clues, right-aligned up to the grid's edge
        for (r, clues) in n.row_clues().iter().enumerate() {
            let base = y + (r as f32 + 0.5) * cell + 0.35 * fs;
            for (k, clue) in clues.iter().rev().enumerate() {
                let cx = x - 0.3 * cell - (k as f32 + 0.5) * step;
                self.centred(Face::Regular, fs, cx, base, &clue.to_string());
            }
        }
        // column clues, bottom-aligned down to the grid's edge
        for (c, clues) in n.col_clues().iter().enumerate() {
            let cx = x + (c as f32 + 0.5) * cell;
            for (k, clue) in clues.iter().rev().enumerate() {
                let base = y - 0.3 * cell - (k as f32 + 0.5) * step + 0.35 * fs;
                self.centred(Face::Regular, fs, cx, base, &clue.to_string());
            }
        }
    }

    /// A maze: every standing wall as a line, the outer edge heavier, an arrow in at the top
    /// and out at the bottom, and the solution drawn through the squares when `path`.
    fn maze(&mut self, m: &Maze, x: f32, y: f32, cell: f32, path: bool) {
        let size = m.size;
        let side = cell * size as f32;
        for r in 0..size {
            for c in 0..size {
                let w = m.at(r, c);
                let (x0, y0) = (x + c as f32 * cell, y + r as f32 * cell);
                let (x1, y1) = (x0 + cell, y0 + cell);
                let edge = |k: usize| {
                    if k == 0 || k == size - 1 {
                        BORDER
                    } else {
                        CELL_LINE * 1.6
                    }
                };
                if w & maze::N != 0 {
                    self.line(if r == 0 { BORDER } else { edge(size) }, (x0, y0), (x1, y0));
                }
                if w & maze::S != 0 {
                    self.line(
                        if r == size - 1 { BORDER } else { edge(size) },
                        (x0, y1),
                        (x1, y1),
                    );
                }
                if w & maze::W != 0 {
                    self.line(if c == 0 { BORDER } else { edge(size) }, (x0, y0), (x0, y1));
                }
                if w & maze::E != 0 {
                    self.line(
                        if c == size - 1 { BORDER } else { edge(size) },
                        (x1, y0),
                        (x1, y1),
                    );
                }
            }
        }
        // in and out: a short arrow above the entrance, another below the exit
        let arrow = |p: &mut Painter, cx: f32, top: f32, len: f32| {
            p.line(CELL_LINE * 1.6, (cx, top), (cx, top + len));
            p.line(
                CELL_LINE * 1.6,
                (cx - 0.2 * len, top + 0.7 * len),
                (cx, top + len),
            );
            p.line(
                CELL_LINE * 1.6,
                (cx + 0.2 * len, top + 0.7 * len),
                (cx, top + len),
            );
        };
        let len = (0.9 * cell).max(8.0);
        arrow(self, x + 0.5 * cell, y - len - 1.0, len);
        arrow(self, x + side - 0.5 * cell, y + side + 1.0, len);
        if path {
            self.c.set_line_width((0.3 * cell).max(1.5));
            self.c.set_stroke_gray(0.45);
            let centre = |(r, c): (usize, usize)| {
                (
                    x + (c as f32 + 0.5) * cell,
                    self.ph - (y + (r as f32 + 0.5) * cell),
                )
            };
            let mut it = m.solution.iter().copied();
            if let Some(first) = it.next() {
                let (fx, fy) = centre(first);
                self.c.move_to(fx, fy);
                for p in it {
                    let (px, py) = centre(p);
                    self.c.line_to(px, py);
                }
                self.c.stroke();
            }
            self.c.set_stroke_gray(0.0);
        }
    }

    /// A page of ladders side by side: each ladder's words in squares, the top and bottom
    /// words given, the rungs between them blank.
    fn ladder_page(&mut self, page: &LadderPage, (pw, ph): (f32, f32)) {
        let n = page.ladders.len().max(1);
        let col_w = (pw - 2.0 * MARGIN - (n - 1) as f32 * GAP) / n as f32;
        let tallest = page.ladders.iter().map(|l| l.path.len()).max().unwrap_or(1) as f32;
        let top = MARGIN + HEAD + GAP + LIST + 6.0;
        let cell = ((col_w - 8.0) / 4.0)
            .min((ph - top - MARGIN - FOOT - GAP - 2.0 * LEAD) / (tallest * 1.25))
            .min(MAX_LADDER_CELL);
        self.text(
            Face::Regular,
            FOOT + 1.0,
            MARGIN,
            MARGIN + HEAD + GAP + 2.0,
            "Change one letter a rung, each rung a word, to climb from the top word to the bottom one.",
        );
        for (j, l) in page.ladders.iter().enumerate() {
            let x0 = MARGIN + j as f32 * (col_w + GAP);
            let letter = (b'a' + j as u8) as char;
            self.text(
                Face::Bold,
                LIST,
                x0,
                top,
                &format!(
                    "{letter}. {} to {}",
                    l.from.to_uppercase(),
                    l.to.to_uppercase()
                ),
            );
            let gx = x0 + (col_w - 4.0 * cell) / 2.0;
            let mut gy = top + 8.0;
            let steps = l.path.len();
            for (k, word) in l.path.iter().enumerate() {
                let given = k == 0 || k == steps - 1;
                self.boxes(gx, gy, cell, 4);
                if given {
                    let row = [word.to_uppercase()];
                    self.letters(&row, gx, gy, cell, |_, _| 0.6 * cell);
                }
                gy += cell * 1.25;
            }
        }
    }

    /// A cryptogram page: the cipher's words as rows of squares with a blank above each
    /// cipher letter (the given letters filled in), then the key, A to Z, to work in.
    fn cryptogram(&mut self, c: &Cryptogram, (pw, ph): (f32, f32)) {
        let left = MARGIN;
        let width = pw - 2.0 * MARGIN;
        let mut x = left;
        let mut y = MARGIN + HEAD + GAP + CRYPT_ROW;
        self.text(
            Face::Regular,
            FOOT + 1.0,
            MARGIN,
            MARGIN + HEAD + GAP + 2.0,
            "Each letter stands for another, the same way throughout. Write the real letter above its stand-in.",
        );
        y += 4.0;
        for word in c.words() {
            let w = word.chars().count() as f32 * CRYPT_CELL;
            if x > left && x + w > left + width {
                x = left;
                y += CRYPT_ROW;
            }
            for ch in word.chars() {
                if ch.is_ascii_alphabetic() {
                    // the blank, the cipher letter under it
                    self.line(
                        CELL_LINE,
                        (x + 1.0, y - 12.0),
                        (x + CRYPT_CELL - 1.0, y - 12.0),
                    );
                    self.centred(
                        Face::Regular,
                        8.0,
                        x + CRYPT_CELL / 2.0,
                        y - 3.0,
                        &ch.to_string(),
                    );
                    if c.given.contains(&ch)
                        && let Some(plain) = c.decode(ch)
                    {
                        self.centred(
                            Face::Bold,
                            10.0,
                            x + CRYPT_CELL / 2.0,
                            y - 14.5,
                            &plain.to_string(),
                        );
                    }
                } else {
                    self.centred(
                        Face::Regular,
                        9.0,
                        x + CRYPT_CELL / 2.0,
                        y - 3.0,
                        &ch.to_string(),
                    );
                }
                x += CRYPT_CELL;
            }
            x += CRYPT_WORD_GAP;
        }
        // the key: two rows of thirteen, A to Z, a blank beside each
        let ky = (y + CRYPT_ROW + GAP).min(ph - MARGIN - FOOT - GAP - 2.0 * CRYPT_ROW);
        self.text(Face::Bold, LIST, left, ky, "Key");
        let kw = width / 13.0;
        for (i, letter) in (b'A'..=b'Z').enumerate() {
            let kx = left + (i % 13) as f32 * kw;
            let row_y = ky + 6.0 + (i / 13) as f32 * CRYPT_ROW + 14.0;
            self.text(Face::Regular, 9.0, kx, row_y, &(letter as char).to_string());
            self.line(
                CELL_LINE,
                (kx + 10.0, row_y + 1.0),
                (kx + kw - 6.0, row_y + 1.0),
            );
            if c.given.contains(&(letter as char))
                && let Some(plain) = c.decode(letter as char)
            {
                self.centred(
                    Face::Bold,
                    10.0,
                    kx + 10.0 + (kw - 16.0) / 2.0,
                    row_y - 1.0,
                    &plain.to_string(),
                );
            }
        }
    }
}

/// A word search's answer: the grid, each hidden word outlined, the filler letters smaller so
/// the answers stand out.
fn answer_search(p: &mut Painter, s: &Search, x: f32, y: f32, side: f32) {
    let size = s.size();
    let cell = side / size as f32;
    let mut answer = vec![false; size * size];
    for h in &s.words {
        for (r, c) in s.cells(h) {
            answer[r * size + c] = true;
        }
    }
    p.grid(x, y, cell, size);
    p.letters(&s.rows, x, y, cell, |r, c| {
        if answer[r * size + c] {
            0.55 * cell
        } else {
            0.33 * cell
        }
    });
    for h in &s.words {
        let cells = s.cells(h);
        let centre =
            |(r, c): (usize, usize)| (x + (c as f32 + 0.5) * cell, y + (r as f32 + 0.5) * cell);
        p.capsule(
            centre(cells[0]),
            centre(*cells.last().expect("words have letters")),
            0.42 * cell,
        );
    }
}

/// The PDF: pages, both faces embedded, the outline (title, each puzzle, the answers) and the
/// book's title. No dates and no annotations, so the same pages give the same bytes.
fn write(
    b: &Book,
    faces: &Faces,
    used: &[BTreeMap<u16, char>; 2],
    pages: Vec<Content>,
    (pw, ph): (f32, f32),
    starts: &[usize],
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
    marks.extend(
        starts
            .iter()
            .enumerate()
            .map(|(i, page)| (format!("No. {}", i + 1), *page)),
    );
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
