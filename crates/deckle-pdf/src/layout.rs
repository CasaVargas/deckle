// SPDX-License-Identifier: GPL-3.0-or-later
//! From positioned glyphs to a book's blocks, in reading order (PDF spec §4). Pure functions of
//! the [`Page`]s, so every pass is tested without a PDF engine.
//!
//! Coordinates are PDF points with the origin at the top left of the page, y growing down.

use std::collections::{HashMap, HashSet};

/// One character as the page draws it.
#[derive(Clone, Debug, PartialEq)]
pub struct Glyph {
    pub c: char,
    pub x0: f32,
    pub y0: f32,
    pub x1: f32,
    pub y1: f32,
    pub size: f32,
    pub bold: bool,
    pub italic: bool,
    /// Set in a math font (Computer Modern math, Symbol, Cambria Math and the like).
    pub math: bool,
    /// A hyphen the PDF itself marks as a line-end break (a soft hyphen).
    pub soft_hyphen: bool,
    /// The PDF puts a space before this glyph, whatever the gap says.
    pub space_before: bool,
}

/// A rectangle on a page.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Rect {
    pub x0: f32,
    pub y0: f32,
    pub x1: f32,
    pub y1: f32,
}

impl Rect {
    pub fn width(&self) -> f32 {
        self.x1 - self.x0
    }
    pub fn height(&self) -> f32 {
        self.y1 - self.y0
    }
    fn area(&self) -> f32 {
        self.width().max(0.0) * self.height().max(0.0)
    }
    fn union(&self, o: &Rect) -> Rect {
        Rect {
            x0: self.x0.min(o.x0),
            y0: self.y0.min(o.y0),
            x1: self.x1.max(o.x1),
            y1: self.y1.max(o.y1),
        }
    }
    fn contains(&self, o: &Rect, slack: f32) -> bool {
        o.x0 >= self.x0 - slack
            && o.x1 <= self.x1 + slack
            && o.y0 >= self.y0 - slack
            && o.y1 <= self.y1 + slack
    }
    fn near(&self, o: &Rect, gap: f32) -> bool {
        o.x0 <= self.x1 + gap
            && o.x1 >= self.x0 - gap
            && o.y0 <= self.y1 + gap
            && o.y1 >= self.y0 - gap
    }
}

/// One page: its size, its text and where its pictures and drawings are.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Page {
    pub width: f32,
    pub height: f32,
    pub glyphs: Vec<Glyph>,
    /// Embedded raster images.
    pub images: Vec<Rect>,
    /// Vector drawing (paths): rules, boxes, charts.
    pub paths: Vec<Rect>,
}

/// What a picture block shows; all are cut from the rendered page.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Figure {
    Image,
    Table,
    Equation,
}

/// The book, in reading order.
#[derive(Clone, Debug, PartialEq)]
pub enum Block {
    /// `top`: it starts in the top third of its page, where a chapter starts. `page`: the
    /// page it starts on, from 0.
    Heading {
        level: u8,
        text: String,
        top: bool,
        page: usize,
    },
    Para {
        text: String,
        small: bool,
        page: usize,
    },
    Figure {
        kind: Figure,
        page: usize,
        rect: Rect,
    },
}

/// What the passes removed or turned into pictures, for the report.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Stats {
    pub running_lines: usize,
    pub tables: usize,
    pub equations: usize,
    pub images: usize,
    pub columns: usize,
}

/// A bookmark in the PDF's outline: its depth (1 for the top level), title, and the page it
/// opens, from 0.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Mark {
    pub level: u8,
    pub title: String,
    pub page: usize,
}

/// Lowercase letters and digits, one space between words: for matching a bookmark's title
/// with the text the page sets.
fn words(s: &str) -> String {
    s.to_lowercase()
        .split(|c: char| !c.is_alphanumeric())
        .filter(|w| !w.is_empty())
        .collect::<Vec<_>>()
        .join(" ")
}

/// The PDF's own outline as the book's headings (structure-recovery spec §4). Each bookmark of
/// the top two levels becomes an h1 or h2: the block on its page that reads the same, else a
/// new heading with the bookmark's title before the page's first block. The layout's own
/// headings drop to h3, under them. `None` when fewer than two top-level bookmarks land, so the
/// layout's headings stand.
pub fn apply_outline(blocks: &[Block], marks: &[Mark]) -> Option<Vec<Block>> {
    let page_of = |b: &Block| match b {
        Block::Heading { page, .. } | Block::Para { page, .. } | Block::Figure { page, .. } => {
            *page
        }
    };
    // (block index, mark, whether the mark names that block or goes before it)
    let mut anchors: Vec<(usize, &Mark, bool)> = Vec::new();
    let (mut cursor, mut last_page) = (0, 0);
    for m in marks.iter().filter(|m| (1..=2).contains(&m.level)) {
        if m.page < last_page {
            continue;
        }
        let title = words(&m.title);
        let named = (cursor..blocks.len())
            .take_while(|&i| page_of(&blocks[i]) <= m.page)
            .filter(|&i| page_of(&blocks[i]) == m.page)
            .find(|&i| match &blocks[i] {
                Block::Heading { text, .. } | Block::Para { text, .. } => {
                    !title.is_empty() && words(text) == title
                }
                Block::Figure { .. } => false,
            });
        let anchor = match named {
            Some(i) => Some((i, true)),
            None => (cursor..blocks.len())
                .find(|&i| page_of(&blocks[i]) >= m.page)
                .map(|i| (i, false)),
        };
        let Some((i, names)) = anchor else { continue };
        anchors.push((i, m, names));
        cursor = if names { i + 1 } else { i };
        last_page = m.page;
    }
    if anchors.iter().filter(|(_, m, _)| m.level == 1).count() < 2 {
        return None;
    }
    let mut out = Vec::with_capacity(blocks.len() + anchors.len());
    let mut next = anchors.iter().peekable();
    for (i, b) in blocks.iter().enumerate() {
        let mut named = None;
        while let Some(&&(at, m, names)) = next.peek()
            && at == i
        {
            next.next();
            if names {
                named = Some(m);
            } else {
                out.push(Block::Heading {
                    level: m.level,
                    text: m.title.trim().to_owned(),
                    top: true,
                    page: m.page,
                });
            }
        }
        out.push(match (named, b) {
            (Some(m), Block::Heading { text, page, .. } | Block::Para { text, page, .. }) => {
                Block::Heading {
                    level: m.level,
                    text: text.clone(),
                    top: true,
                    page: *page,
                }
            }
            (
                None,
                Block::Heading {
                    level,
                    text,
                    top,
                    page,
                },
            ) => Block::Heading {
                level: (*level).max(3),
                text: text.clone(),
                top: *top,
                page: *page,
            },
            _ => b.clone(),
        });
    }
    Some(out)
}

/// A run of text on one line, with no gap wider than a column gutter inside it.
#[derive(Clone, Debug, PartialEq)]
struct Line {
    page: usize,
    /// The top of the row it was cut from: the runs of one row share it, so they keep their
    /// order left to right whatever a glyph's box says.
    row: f32,
    rect: Rect,
    size: f32,
    bold: bool,
    math: f32,
    text: String,
    /// The text ends with a hyphen the PDF marks as a break.
    soft_end: bool,
    /// How many of the text's last characters (spaces aside) are set small: a citation after
    /// the sentence's full stop ("it.¹²⁻¹⁵").
    script_end: usize,
}

impl Line {
    /// Whether the line ends a sentence, a citation set small after its full stop aside.
    fn ends_sentence(&self) -> bool {
        let mut text = self.text.trim_end();
        for _ in 0..self.script_end {
            let mut chars = text.chars();
            chars.next_back();
            text = chars.as_str().trim_end();
        }
        ends_sentence(text)
    }
}

/// A line or a picture, the units reading order arranges.
#[derive(Clone, Debug)]
enum Item {
    Text(Line),
    Figure(Figure, Rect),
}

impl Item {
    fn rect(&self) -> Rect {
        match self {
            Item::Text(l) => l.rect,
            Item::Figure(_, r) => *r,
        }
    }
    /// Where it sorts from top to bottom: a line by its row, so a row's runs stay in order.
    fn top(&self) -> f32 {
        match self {
            Item::Text(l) => l.row,
            Item::Figure(_, r) => r.y0,
        }
    }
}

/// A gap wider than this many ems ends a line run: a column gutter or a table cell.
const RUN_GAP_EM: f32 = 1.5;
/// A gap wider than this many ems across the page's gutter ends a line run: wider than any word
/// space, narrower than the tightest column gutter.
const GUTTER_EM: f32 = 0.8;
/// A gutter wider than this many ems is found by [`gutter`] from the lines: one this wide
/// already ends a run.
const GUTTER_MAX_EM: f32 = 3.0;
/// Text smaller than this share of the page's usual size is a superscript, a subscript or a
/// footnote.
const SCRIPT_SIZE: f32 = 0.75;
/// A row of text is at most this many times as tall as its text (its size, or its glyphs' boxes
/// when they're taller): more would take in the line below.
const ROW_SPAN: f32 = 1.5;
/// A gap wider than this many ems between glyphs is a word space.
const SPACE_EM: f32 = 0.18;

/// The blocks of every page, in reading order, and what was done to get there.
pub fn blocks(pages: &[Page]) -> (Vec<Block>, Stats) {
    let mut stats = Stats::default();
    let (mut lines, channels): (Vec<Vec<Line>>, Vec<Option<f32>>) = pages
        .iter()
        .enumerate()
        .map(|(i, p)| lines_of(i, p))
        .unzip();
    stats.running_lines = strip_running(&mut lines, pages);
    let body = body_size(lines.iter().flatten());
    let mut flow: Vec<(usize, Vec<Item>)> = Vec::new();
    for (i, page) in pages.iter().enumerate() {
        let mut page_lines = std::mem::take(&mut lines[i]);
        let mut figures = figures_of(page, &mut page_lines, &mut stats);
        figures.extend(tables_of(&mut page_lines, &mut stats));
        // The page's gutter, for equations and reading order: the one the rows showed, else the
        // one the lines show (columns whose lines don't share rows).
        let column = channels[i].or_else(|| {
            let text: Vec<Rect> = page_lines.iter().map(|l| l.rect).collect();
            gutter(page, &text)
        });
        figures.extend(equations_of(&mut page_lines, column, &mut stats));
        let (items, cols) = reading_order(page_lines, figures, column);
        stats.columns = stats.columns.max(cols);
        flow.push((i, items));
    }
    let heights: Vec<f32> = pages.iter().map(|p| p.height).collect();
    (paragraphs(&flow, body, &heights), stats)
}

// ---- lines ----------------------------------------------------------------------------------

fn median(mut v: Vec<f32>) -> f32 {
    if v.is_empty() {
        return 0.0;
    }
    v.sort_by(f32::total_cmp);
    v[v.len() / 2]
}

/// Glyphs grouped into rows by vertical overlap, then cut into runs at wide gaps, and the
/// page's column gutter when the rows show one.
fn lines_of(page: usize, p: &Page) -> (Vec<Line>, Option<f32>) {
    let mut glyphs: Vec<&Glyph> = p
        .glyphs
        .iter()
        .filter(|g| g.y1 > g.y0 && g.x1 >= g.x0 && g.size > 0.0)
        .collect();
    glyphs.sort_by(|a, b| a.y0.total_cmp(&b.y0).then(a.x0.total_cmp(&b.x0)));
    // Superscripts and subscripts (a citation's "53–55") are placed after the full-size text,
    // so they join the line they sit on instead of starting a row of their own above it, which
    // would cut that line in two around the hole they leave.
    let usual = median(glyphs.iter().map(|g| g.size).collect());
    let (full, small): (Vec<&Glyph>, Vec<&Glyph>) = glyphs
        .into_iter()
        .partition(|g| g.size >= usual * SCRIPT_SIZE);
    // Rows: a glyph joins the row its vertical middle falls inside, unless that would make the
    // row taller than the smaller text in it allows: a tall bracket in an equation or a drop cap
    // would otherwise take in the lines beside it and shuffle their letters together.
    let mut rows: Vec<(f32, f32, Vec<&Glyph>)> = Vec::new();
    for g in full {
        let mid = (g.y0 + g.y1) / 2.0;
        // Each glyph's height is its size or its box, whichever is taller: some fonts' boxes
        // are well over an em, and a row of them is that tall. The smaller of the two sets the
        // limit, so a tall bracket can't stretch a row of ordinary text.
        let tall = |g: &Glyph| g.size.max(g.y1 - g.y0);
        let fits = |(top, bottom, row): &&mut (f32, f32, Vec<&Glyph>)| {
            let h = row.first().map_or(tall(g), |f| tall(f).min(tall(g)));
            mid > *top && mid < *bottom && bottom.max(g.y1) - top.min(g.y0) <= ROW_SPAN * h
        };
        match rows.iter_mut().rev().take(4).find(fits) {
            Some(row) => {
                row.0 = row.0.min(g.y0);
                row.1 = row.1.max(g.y1);
                row.2.push(g);
            }
            None => rows.push((g.y0, g.y1, vec![g])),
        }
    }
    // Small text joins a full-size row without stretching it; small text on lines of its own
    // (footnotes) makes rows as above.
    let full_rows = rows.len();
    for g in small {
        let mid = (g.y0 + g.y1) / 2.0;
        match rows
            .iter_mut()
            .enumerate()
            .rev()
            .find(|(_, (top, bottom, _))| mid > *top && mid < *bottom)
        {
            Some((i, row)) => {
                if i >= full_rows {
                    row.0 = row.0.min(g.y0);
                    row.1 = row.1.max(g.y1);
                }
                row.2.push(g);
            }
            None => rows.push((g.y0, g.y1, vec![g])),
        }
    }
    for row in &mut rows {
        row.2.sort_by(|a, b| a.x0.total_cmp(&b.x0));
    }
    // Each row's marks, looked up once: a chart's thousands of small paths aren't searched for
    // every gap.
    let marks = marks(p);
    let row_marks: Vec<Vec<Rect>> = rows
        .iter()
        .map(|(top, bottom, _)| {
            marks
                .iter()
                .filter(|m| m.y0 < *bottom && m.y1 > *top)
                .copied()
                .collect()
        })
        .collect();
    let gutter = channel(p, &rows, &row_marks);
    let mut out = Vec::new();
    for ((row_top, _, row), marks) in rows.into_iter().zip(&row_marks) {
        let mut run: Vec<&Glyph> = Vec::new();
        let mut pending_space = false;
        let mut text = String::new();
        let flush = |run: &mut Vec<&Glyph>, text: &mut String, out: &mut Vec<Line>| {
            let t = text.trim().to_owned();
            if !t.is_empty() {
                let ink: Vec<&&Glyph> = run.iter().filter(|g| !g.c.is_whitespace()).collect();
                let n = ink.len().max(1) as f32;
                let most = ink.iter().map(|g| g.size).fold(0.0, f32::max);
                let small = |g: &Glyph| g.size < most * SCRIPT_SIZE;
                out.push(Line {
                    page,
                    row: row_top,
                    rect: run
                        .iter()
                        .fold(None::<Rect>, |r, g| {
                            let gr = Rect {
                                x0: g.x0,
                                y0: g.y0,
                                x1: g.x1,
                                y1: g.y1,
                            };
                            Some(r.map_or(gr, |r| r.union(&gr)))
                        })
                        .expect("a run has glyphs"),
                    // The size of its full-size text: a last line that's mostly a citation
                    // ("it.¹²⁻¹⁵") is the size of its sentence.
                    size: median(ink.iter().filter(|g| !small(g)).map(|g| g.size).collect()),
                    bold: ink.iter().filter(|g| g.bold).count() as f32 / n > 0.6,
                    math: ink.iter().filter(|g| g.math).count() as f32 / n,
                    soft_end: ink.last().is_some_and(|g| g.soft_hyphen),
                    script_end: ink.iter().rev().take_while(|g| small(g)).count(),
                    text: t,
                });
            }
            run.clear();
            text.clear();
        };
        let mut last_x1: Option<f32> = None;
        for g in row {
            if g.c.is_whitespace() || g.c.is_control() {
                pending_space = true;
                continue;
            }
            if let Some(x1) = last_x1 {
                let gap = g.x0 - x1;
                let across = gutter.is_some_and(|(c, least)| {
                    x1 <= c && g.x0 >= c && gap > least.max(GUTTER_EM * g.size)
                });
                let wide = gap > RUN_GAP_EM * g.size || across;
                if wide && !holds_mark(marks, x1, g) {
                    flush(&mut run, &mut text, &mut out);
                } else if pending_space || g.space_before || gap > SPACE_EM * g.size {
                    text.push(' ');
                }
            }
            pending_space = false;
            text.push(if g.soft_hyphen { '-' } else { g.c });
            run.push(g);
            last_x1 = Some(g.x1);
        }
        flush(&mut run, &mut text, &mut out);
    }
    (out, gutter.map(|(x, _)| x))
}

/// Small drawings and images the size of a letter: an ORCID or e-mail icon after an author's
/// name, a checkbox. Text either side of one is the same line.
fn marks(p: &Page) -> Vec<Rect> {
    p.images
        .iter()
        .chain(&p.paths)
        .filter(|r| (2.0..=20.0).contains(&r.width()) && (2.0..=20.0).contains(&r.height()))
        .copied()
        .collect()
}

/// Whether the gap from `x1` to glyph `g` is filled by marks on `g`'s line, no bigger than its
/// text, with no more than a word space's room between them and the text either side: an icon
/// after a name, not one at the end of a column with the gutter still to cross.
fn holds_mark(marks: &[Rect], x1: f32, g: &Glyph) -> bool {
    let most = 2.0 * g.size.max(g.y1 - g.y0);
    let room = GUTTER_EM * g.size;
    let mut inside: Vec<&Rect> = marks
        .iter()
        .filter(|m| {
            m.x0 >= x1 - 1.0
                && m.x1 <= g.x0 + 1.0
                && m.y0 < g.y1
                && m.y1 > g.y0
                && m.width() <= most
                && m.height() <= most
        })
        .collect();
    if inside.is_empty() {
        return false;
    }
    inside.sort_by(|a, b| a.x0.total_cmp(&b.x0));
    let mut reach = x1;
    for m in inside {
        if m.x0 - reach > room {
            return false;
        }
        reach = reach.max(m.x1);
    }
    g.x0 - reach <= room
}

/// The x of the page's column gutter, found from its rows of glyphs before they are cut into
/// runs: a position in the middle of the page that most rows with text on both sides of it pass
/// with a gap wider than a word space. A tight gutter (a journal's 1.3 em) is narrower than
/// [`RUN_GAP_EM`], and without this each left-column line would be glued to the right-column
/// line beside it.
///
/// Returns the x and the narrowest gap, in points, that counts as the gutter there: a little
/// under the gutter's narrow end, so a quad between two names in a line across it isn't cut.
/// Points, not ems: the gutter is as wide beside small type (references) as beside large.
fn channel(
    p: &Page,
    rows: &[(f32, f32, Vec<&Glyph>)],
    row_marks: &[Vec<Rect>],
) -> Option<(f32, f32)> {
    let (from, to) = (p.width * 0.3, p.width * 0.7);
    if to <= from {
        return None;
    }
    // Per x: (rows that pass it with a gutter-wide gap, rows whose text runs on across it).
    let mut counts: Vec<(f32, usize, usize)> = (0..=((to - from) as usize))
        .map(|i| (from + i as f32, 0, 0))
        .collect();
    // Each row's extent and its gaps wider than a word space: (from, to, width in ems).
    type Gaps = (f32, f32, Vec<(f32, f32, f32)>);
    let gaps: Vec<Gaps> = rows
        .iter()
        .zip(row_marks)
        .filter_map(|((_, _, row), marks)| {
            // Sorted by x0: gaps from the furthest any glyph reached so far.
            let first = row.first()?;
            let mut reach = first.x1;
            let mut wide = Vec::new();
            for g in &row[1..] {
                if g.x0 - reach > GUTTER_EM * g.size && !holds_mark(marks, reach, g) {
                    wide.push((reach, g.x0, (g.x0 - reach) / g.size));
                }
                reach = reach.max(g.x1);
            }
            Some((first.x0, reach, wide))
        })
        .collect();
    for (start, end, wide) in &gaps {
        for c in counts.iter_mut().filter(|c| c.0 > *start && c.0 < *end) {
            match wide.iter().find(|&&(l, r, _)| c.0 >= l && c.0 <= r) {
                // A tight gutter's gap. Wider ones (a list's labels and values, a short last
                // line) say nothing: a gutter that wide is the old rule's to find.
                Some(&(_, _, em)) if em <= GUTTER_MAX_EM => c.1 += 1,
                Some(_) => {}
                None => c.2 += 1,
            }
        }
    }
    // Text across the gutter (a title, an abstract, a footer) sits in bands of its own, which
    // reading order keeps apart, so a third of the rows splitting there is enough: no word
    // space is gutter-wide, and body text never lines its gaps up like that.
    let ok = |c: &(f32, usize, usize)| c.1 >= 4 && c.2 <= c.1 * 2;
    // The widest run of positions where the most rows split, and the fewest run on.
    let most = counts
        .iter()
        .filter(|c| ok(c))
        .map(|c| (c.1, std::cmp::Reverse(c.2)))
        .max()?;
    let best = |c: &(f32, usize, usize)| ok(c) && (c.1, std::cmp::Reverse(c.2)) == most;
    let x = middle_of_widest(counts.iter().map(|c| (c.0, best(c))))?;
    let widths: Vec<f32> = gaps
        .iter()
        .flat_map(|(_, _, wide)| wide.iter())
        .filter(|&&(l, r, em)| x >= l && x <= r && em <= GUTTER_MAX_EM)
        .map(|&(l, r, _)| r - l)
        .collect();
    // From the narrow end: beside a ragged column (references) the gap varies, and the rows
    // whose lines reach the gutter are the ones to cut. One row in ten (a quad between names in
    // a line across the gutter) doesn't set it.
    let mut widths = widths;
    widths.sort_by(f32::total_cmp);
    let narrow = widths.get(widths.len() / 10).copied().unwrap_or(0.0);
    Some((x, narrow * 0.85))
}

/// The middle of the widest run of consecutive positions that `hit`, from its first to its last.
fn middle_of_widest(positions: impl Iterator<Item = (f32, bool)>) -> Option<f32> {
    let mut best: Option<(f32, f32)> = None;
    let mut run: Option<(f32, f32)> = None;
    let mut end = |run: &mut Option<(f32, f32)>| {
        if let Some((a, b)) = run.take()
            && best.is_none_or(|(c, d)| b - a > d - c)
        {
            best = Some((a, b));
        }
    };
    for (x, hit) in positions {
        if hit {
            run = Some(run.map_or((x, x), |(a, _)| (a, x)));
        } else {
            end(&mut run);
        }
    }
    end(&mut run);
    best.map(|(a, b)| (a + b) / 2.0)
}

/// The size most of the text is set in, weighted by characters.
fn body_size<'a>(lines: impl Iterator<Item = &'a Line>) -> f32 {
    let mut by_size: HashMap<i32, usize> = HashMap::new();
    for l in lines {
        *by_size.entry((l.size * 2.0).round() as i32).or_default() += l.text.chars().count();
    }
    by_size
        .into_iter()
        .max_by_key(|(s, n)| (*n, -s))
        .map_or(10.0, |(s, _)| s as f32 / 2.0)
}

// ---- running headers, footers and page numbers --------------------------------------------

/// The top and bottom of a page where running heads and folios sit.
const MARGIN_ZONE: f32 = 0.1;

fn in_margin(l: &Line, p: &Page) -> bool {
    l.rect.y1 < p.height * MARGIN_ZONE || l.rect.y0 > p.height * (1.0 - MARGIN_ZONE)
}

/// A line with its numbers blanked: "Page 3 of 10" and "Page 4 of 10" are the same running line.
fn shape(text: &str) -> String {
    text.to_lowercase()
        .chars()
        .map(|c| if c.is_ascii_digit() { '#' } else { c })
        .collect::<String>()
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
}

/// "12", "- 12 -", "Page 12", "12 of 40", "xiv".
fn is_folio(text: &str) -> bool {
    let t = text
        .trim_matches(|c: char| c == '-' || c == '–' || c == '—' || c == '|' || c.is_whitespace())
        .to_lowercase();
    let t = t.strip_prefix("page").map(str::trim).unwrap_or(&t);
    let t = t.split(" of ").next().unwrap_or(t).trim();
    let t = t.split('/').next().unwrap_or(t).trim();
    (!t.is_empty() && t.len() <= 5 && t.chars().all(|c| c.is_ascii_digit()))
        || (!t.is_empty() && t.len() <= 6 && t.chars().all(|c| "ivxlcdm".contains(c)))
}

/// Removes page numbers and lines repeated in the same margin on many pages. Returns how many.
fn strip_running(lines: &mut [Vec<Line>], pages: &[Page]) -> usize {
    let mut seen: HashMap<String, HashSet<usize>> = HashMap::new();
    for (i, page_lines) in lines.iter().enumerate() {
        for l in page_lines.iter().filter(|l| in_margin(l, &pages[i])) {
            seen.entry(shape(&l.text)).or_default().insert(i);
        }
    }
    let many = 2.max((pages.len() as f32 * 0.4).ceil() as usize);
    let running: HashSet<String> = seen
        .into_iter()
        .filter(|(_, on)| on.len() >= many)
        .map(|(k, _)| k)
        .collect();
    let mut removed = 0;
    for (i, page_lines) in lines.iter_mut().enumerate() {
        let before = page_lines.len();
        page_lines.retain(|l| {
            !(in_margin(l, &pages[i]) && (is_folio(&l.text) || running.contains(&shape(&l.text))))
        });
        removed += before - page_lines.len();
    }
    removed
}

// ---- pictures: images, drawings, tables and equations ---------------------------------------

/// Images and drawn figures on the page, merged into boxes; the text inside a box is part of
/// the picture and leaves the flow.
fn figures_of(p: &Page, lines: &mut Vec<Line>, stats: &mut Stats) -> Vec<(Figure, Rect)> {
    let page_area = p.width * p.height;
    let mut boxes: Vec<Rect> = Vec::new();
    for r in p.images.iter().chain(&p.paths) {
        // A frame around the whole page, or a background, is not a figure.
        if r.area() > page_area * 0.8 {
            continue;
        }
        let mut merged = *r;
        loop {
            let before = boxes.len();
            boxes.retain(|b| {
                if b.near(&merged, 4.0) {
                    merged = merged.union(b);
                    false
                } else {
                    true
                }
            });
            if boxes.len() == before {
                break;
            }
        }
        boxes.push(merged);
    }
    let mut out = Vec::new();
    for b in boxes {
        if b.width() < 40.0 || b.height() < 30.0 || b.area() > page_area * 0.8 {
            continue;
        }
        // A box around running text (a sidebar, a shaded note) stays text.
        let inside: usize = lines
            .iter()
            .filter(|l| b.contains(&l.rect, 2.0))
            .map(|l| l.text.chars().count())
            .sum();
        let is_image = p.images.iter().any(|i| b.contains(i, 1.0));
        if inside > 400 && !is_image {
            continue;
        }
        lines.retain(|l| !b.contains(&l.rect, 2.0));
        let kind = if is_image {
            Figure::Image
        } else {
            Figure::Table
        };
        match kind {
            Figure::Image => stats.images += 1,
            _ => stats.tables += 1,
        }
        out.push((kind, b));
    }
    out
}

/// Rows of three or more short, aligned cells: a table without rules.
fn tables_of(lines: &mut Vec<Line>, stats: &mut Stats) -> Vec<(Figure, Rect)> {
    // Rows of runs that share a vertical band.
    let mut rows: Vec<Vec<usize>> = Vec::new();
    let mut order: Vec<usize> = (0..lines.len()).collect();
    order.sort_by(|&a, &b| lines[a].rect.y0.total_cmp(&lines[b].rect.y0));
    for i in order {
        let r = lines[i].rect;
        let mid = (r.y0 + r.y1) / 2.0;
        match rows.last_mut() {
            Some(row)
                if {
                    let f = lines[row[0]].rect;
                    mid > f.y0 && mid < f.y1
                } =>
            {
                row.push(i)
            }
            _ => rows.push(vec![i]),
        }
    }
    let cellish = |row: &[usize]| {
        row.len() >= 3
            && row
                .iter()
                .filter(|&&i| lines[i].text.chars().count() <= 30)
                .count()
                * 2
                >= row.len()
    };
    let aligned = |a: &[usize], b: &[usize]| {
        a.iter()
            .filter(|&&i| {
                b.iter().any(|&j| {
                    (lines[i].rect.x0 - lines[j].rect.x0).abs() < 4.0
                        || (lines[i].rect.x1 - lines[j].rect.x1).abs() < 4.0
                })
            })
            .count()
            >= 2
    };
    let mut out = Vec::new();
    let mut taken: HashSet<usize> = HashSet::new();
    let mut k = 0;
    while k < rows.len() {
        if !cellish(&rows[k]) {
            k += 1;
            continue;
        }
        let mut end = k + 1;
        while end < rows.len() && cellish(&rows[end]) && aligned(&rows[end - 1], &rows[end]) {
            end += 1;
        }
        if end - k >= 3 {
            let mut rect = lines[rows[k][0]].rect;
            for row in &rows[k..end] {
                for &i in row {
                    rect = rect.union(&lines[i].rect);
                    taken.insert(i);
                }
            }
            // Pad a little so the cut picture doesn't clip ascenders.
            out.push((Figure::Table, pad(rect, 3.0)));
            stats.tables += 1;
        }
        k = end;
    }
    let mut i = 0;
    lines.retain(|_| {
        let keep = !taken.contains(&i);
        i += 1;
        keep
    });
    out
}

/// Runs set mostly in math fonts, merged with their neighbours: a displayed equation.
/// `channel` is the page's column gutter, if it has one.
fn equations_of(
    lines: &mut Vec<Line>,
    channel: Option<f32>,
    stats: &mut Stats,
) -> Vec<(Figure, Rect)> {
    let is_math = |l: &Line| {
        l.math >= 0.3
            || (l.math > 0.0
                && l.text.chars().filter(|c| c.is_alphabetic()).count() * 2
                    < l.text.chars().count())
    };
    // Each box with its lines.
    let mut boxes: Vec<(Rect, Vec<usize>)> = Vec::new();
    for (i, l) in lines.iter().enumerate().filter(|(_, l)| is_math(l)) {
        match boxes.iter_mut().find(|b| b.0.near(&l.rect, l.size)) {
            Some(b) => {
                b.0 = b.0.union(&l.rect);
                b.1.push(i);
            }
            None => boxes.push((l.rect, vec![i])),
        }
    }
    // The equation's pieces in a text font ("T" with its subscript "day", "heat =") sit on its
    // own lines, beside its symbols: short runs inside the box's height, in its column.
    // A displayed equation has nothing else beside it in its column.
    let same_column = |b: &Rect, r: &Rect| match channel {
        Some(c) if b.x1 <= c + 1.0 => r.x1 <= c,
        Some(c) if b.x0 >= c - 1.0 => r.x0 >= c,
        _ => true,
    };
    // The left edge of a box's column, where its body text starts.
    let col_left = |b: &Rect| {
        lines
            .iter()
            .filter(|l| same_column(b, &l.rect))
            .map(|l| l.rect.x0)
            .fold(f32::MAX, f32::min)
    };
    loop {
        let mut grew = false;
        for b in boxes.iter_mut() {
            let left = col_left(&b.0);
            for (i, l) in lines.iter().enumerate() {
                let r = l.rect;
                let overlap = r.y1.min(b.0.y1) - r.y0.max(b.0.y0);
                // Mostly inside the box's height; or half inside and set in from the column's
                // edge. A line of text at the edge that a tall sign only reaches ("where" under
                // a sum) stays text.
                let inside = overlap >= r.height() * 0.75
                    || (overlap >= r.height() * 0.5 && r.x0 > left + l.size);
                if b.1.contains(&i)
                    || is_math(l)
                    || l.text.chars().count() > 30
                    || !inside
                    || !same_column(&b.0, &r)
                {
                    continue;
                }
                b.0 = b.0.union(&r);
                b.1.push(i);
                grew = true;
            }
        }
        // Boxes that now touch are one equation.
        let mut merged: Vec<(Rect, Vec<usize>)> = Vec::new();
        for b in boxes.drain(..) {
            match merged.iter_mut().find(|m| m.0.near(&b.0, 0.0)) {
                Some(m) => {
                    m.0 = m.0.union(&b.0);
                    for i in b.1 {
                        if !m.1.contains(&i) {
                            m.1.push(i);
                        }
                    }
                    grew = true;
                }
                None => merged.push(b),
            }
        }
        boxes = merged;
        if !grew {
            break;
        }
    }
    // A lone symbol (a footnote star, a dagger) stays text, with what it gathered. A box needs
    // three characters of mathematics: every character of its math runs, and the equals signs
    // of the text-font pieces it took in, each line counted once.
    let score = |b: &(Rect, Vec<usize>)| -> usize {
        b.1.iter()
            .map(|&i| {
                let l = &lines[i];
                if is_math(l) {
                    l.text.chars().filter(|c| !c.is_whitespace()).count()
                } else {
                    l.text.chars().filter(|&c| c == '=').count()
                }
            })
            .sum()
    };
    boxes.retain(|b| score(b) >= 3);
    let taken: HashSet<usize> = boxes.iter().flat_map(|b| b.1.iter().copied()).collect();
    let mut i = 0;
    lines.retain(|_| {
        i += 1;
        !taken.contains(&(i - 1))
    });
    stats.equations += boxes.len();
    boxes
        .into_iter()
        .map(|b| (Figure::Equation, pad(b.0, 3.0)))
        .collect()
}

fn pad(r: Rect, by: f32) -> Rect {
    Rect {
        x0: r.x0 - by,
        y0: r.y0 - by,
        x1: r.x1 + by,
        y1: r.y1 + by,
    }
}

// ---- reading order --------------------------------------------------------------------------

/// The page's items in reading order: bands separated by anything spanning the gutter, each
/// band's left column before its right. `column` is the page's gutter, if it has one. Returns
/// how many columns the page has.
fn reading_order(
    lines: Vec<Line>,
    figures: Vec<(Figure, Rect)>,
    column: Option<f32>,
) -> (Vec<Item>, usize) {
    let mut items: Vec<Item> = lines.into_iter().map(Item::Text).collect();
    items.extend(figures.into_iter().map(|(k, r)| Item::Figure(k, r)));
    let by_top = |a: &Item, b: &Item| {
        a.top()
            .total_cmp(&b.top())
            .then(a.rect().x0.total_cmp(&b.rect().x0))
    };
    let Some(gutter) = column else {
        items.sort_by(by_top);
        return (items, 1);
    };
    let spans = |r: Rect| r.x0 < gutter && r.x1 > gutter;
    items.sort_by(by_top);
    let mut out = Vec::new();
    let (mut left, mut right): (Vec<Item>, Vec<Item>) = (Vec::new(), Vec::new());
    let place = |it: Item, left: &mut Vec<Item>, right: &mut Vec<Item>| {
        if it.rect().x1 <= gutter {
            left.push(it);
        } else {
            right.push(it);
        }
    };
    let mut spanning: Option<Spanning> = None;
    for it in items {
        let r = it.rect();
        if spans(r) {
            // The run goes on only if what waited beside it is beside this item too; a label
            // under a centred title, above the abstract, is not a sidebar.
            if let Some(s) = spanning.take_if(|s| !s.pending.iter().all(|p| clear(r, p.rect()))) {
                for rest in s.close(&mut out) {
                    place(rest, &mut left, &mut right);
                }
            }
            match &mut spanning {
                Some(s) => s.extend(it),
                None => {
                    out.append(&mut left);
                    out.append(&mut right);
                    spanning = Some(Spanning::new(it));
                }
            }
        } else if let Some(s) = spanning.as_mut().filter(|s| s.beside(r).is_some()) {
            s.pending.push(it);
        } else {
            if let Some(s) = spanning.take() {
                for rest in s.close(&mut out) {
                    place(rest, &mut left, &mut right);
                }
            }
            place(it, &mut left, &mut right);
        }
    }
    if let Some(s) = spanning.take() {
        for rest in s.close(&mut out) {
            place(rest, &mut left, &mut right);
        }
    }
    out.append(&mut left);
    out.append(&mut right);
    (out, 2)
}

/// Whether `b` is clear of `a` side to side.
fn clear(a: Rect, b: Rect) -> bool {
    b.x1 < a.x0 || b.x0 > a.x1
}

/// A run of items across the gutter (a title, an abstract, a wide figure) and the narrow text
/// set entirely beside it: a sidebar of dates next to a paper's authors and abstract. The
/// sidebar is read as a column of its own, before the run if it's on the left, after it if on
/// the right, instead of line by line between its lines.
struct Spanning {
    rect: Rect,
    items: Vec<Item>,
    left: Vec<Item>,
    right: Vec<Item>,
    /// Beside the run so far, waiting to see whether the run goes on past them.
    pending: Vec<Item>,
}

impl Spanning {
    fn new(it: Item) -> Self {
        Spanning {
            rect: it.rect(),
            items: vec![it],
            left: Vec::new(),
            right: Vec::new(),
            pending: Vec::new(),
        }
    }

    /// Left (`true`) or right of the run, clear of it side to side.
    /// Narrow, too: at most half the run's width. A column beside a figure that crosses the
    /// gutter is as wide as the figure, and is a column, not a sidebar.
    fn beside(&self, r: Rect) -> Option<bool> {
        if r.width() > self.rect.width() * 0.5 {
            None
        } else if r.x1 < self.rect.x0 {
            Some(true)
        } else if r.x0 > self.rect.x1 {
            Some(false)
        } else {
            None
        }
    }

    /// The run goes on: what waited beside it is its sidebar.
    fn extend(&mut self, it: Item) {
        for p in std::mem::take(&mut self.pending) {
            self.side(p);
        }
        self.rect = self.rect.union(&it.rect());
        self.items.push(it);
    }

    fn side(&mut self, it: Item) {
        match self.beside(it.rect()) {
            Some(true) => self.left.push(it),
            _ => self.right.push(it),
        }
    }

    /// Writes the run and its sidebar. Items that waited beside it but start below its end
    /// aren't a sidebar; they come back for the columns.
    fn close(mut self, out: &mut Vec<Item>) -> Vec<Item> {
        let mut rest = Vec::new();
        for p in std::mem::take(&mut self.pending) {
            if p.rect().y0 < self.rect.y1 {
                self.side(p);
            } else {
                rest.push(p);
            }
        }
        out.append(&mut self.left);
        out.append(&mut self.items);
        out.append(&mut self.right);
        rest
    }
}

/// The x of an empty vertical channel in the middle of the page with text on both sides: the
/// gutter of a two-column page.
fn gutter(p: &Page, text: &[Rect]) -> Option<f32> {
    if text.len() < 8 {
        return None;
    }
    let (from, to) = (p.width * 0.3, p.width * 0.7);
    let crossings: Vec<(f32, usize)> = (0..=((to - from) as usize))
        .map(|i| {
            let x = from + i as f32;
            (x, text.iter().filter(|r| r.x0 < x && r.x1 > x).count())
        })
        .collect();
    let crossing = crossings.iter().map(|c| c.1).min()?;
    // The middle of the widest run of positions with that few crossings.
    let x = middle_of_widest(crossings.iter().map(|&(x, c)| (x, c == crossing)))?;
    let left = text.iter().filter(|r| r.x1 <= x).count();
    let right = text.iter().filter(|r| r.x0 >= x).count();
    // Most lines sit on one side or the other, and both sides carry real text.
    (crossing * 10 <= text.len()
        && left >= 4
        && right >= 4
        && left * 5 >= right
        && right * 5 >= left)
        .then_some(x)
}

// ---- paragraphs, headings and hyphens -------------------------------------------------------

/// Common first halves of hyphenated compounds, so "well-" + "known" keeps its hyphen.
const COMPOUND_HEADS: [&str; 30] = [
    "well", "self", "ill", "all", "ex", "half", "full", "long", "short", "high", "low", "cross",
    "far", "near", "ever", "never", "first", "second", "third", "one", "two", "three", "four",
    "five", "much", "old", "new", "so", "non", "co",
];

/// Joins a line ending in a hyphen to the next. `known` holds the document's own words, so a
/// word that also appears whole elsewhere is rejoined whatever the rule says.
fn join_hyphen(prev: &str, next: &str, soft: bool, known: &HashSet<String>) -> String {
    let stem = prev.trim_end_matches('-');
    let head: String = stem
        .chars()
        .rev()
        .take_while(|c| c.is_alphabetic())
        .collect::<Vec<_>>()
        .into_iter()
        .rev()
        .collect();
    let tail: String = next.chars().take_while(|c| c.is_alphabetic()).collect();
    let starts_lower = next.chars().next().is_some_and(|c| c.is_lowercase());
    let whole = format!("{head}{tail}").to_lowercase();
    let next_token = next.split_whitespace().next().unwrap_or("");
    let keep = !soft
        && (!starts_lower
            // "English-" + "to-German": the next word is a compound itself.
            || next_token
                .trim_end_matches(|c: char| !c.is_alphanumeric())
                .contains('-')
            || head.is_empty()
            || (!known.contains(&whole)
                && (known.contains(&format!("{}-{}", head.to_lowercase(), tail.to_lowercase()))
                    || COMPOUND_HEADS.contains(&head.to_lowercase().as_str()))));
    if keep {
        format!("{prev}{next}")
    } else {
        format!("{stem}{next}")
    }
}

/// Words the document uses whole, and hyphenated words it uses mid-line.
fn vocabulary(flow: &[(usize, Vec<Item>)]) -> HashSet<String> {
    let mut out = HashSet::new();
    for (_, items) in flow {
        for it in items {
            if let Item::Text(l) = it {
                let words: Vec<&str> = l.text.split_whitespace().collect();
                for (n, w) in words.iter().enumerate() {
                    let w: String = w
                        .trim_matches(|c: char| !c.is_alphanumeric() && c != '-')
                        .to_lowercase();
                    // The last word of a line may be half of a broken word.
                    if n + 1 == words.len() && w.ends_with('-') {
                        continue;
                    }
                    if !w.is_empty() {
                        out.insert(w);
                    }
                }
            }
        }
    }
    out
}

/// A list item's first line: a bullet, or "1." / "(a)" before the text.
fn starts_item(t: &str) -> bool {
    let t = t.trim_start();
    if t.starts_with(['•', '◦', '▪', '‣']) {
        return true;
    }
    let marker = t.split_whitespace().next().unwrap_or("");
    let inner = marker.trim_start_matches('(').trim_end_matches(['.', ')']);
    marker.len() > inner.len()
        && marker.len() <= 5
        && t.len() > marker.len()
        // "1." or "12)", not "2014)" at the start of a wrapped citation.
        && (inner.chars().all(|c| c.is_ascii_digit()) && (1..=2).contains(&inner.len())
            || inner.len() == 1 && inner.chars().all(|c| c.is_ascii_lowercase()))
}

fn ends_sentence(t: &str) -> bool {
    t.trim_end()
        .trim_end_matches(['"', '\'', '”', '’', ')'])
        .ends_with(['.', '!', '?', ':'])
}

#[derive(Clone, Copy, Debug, Default)]
struct Opening {
    top: bool,
    spaced: bool,
    page: usize,
}

/// A heading is no longer than this many characters.
const HEADING_CHARS: usize = 150;

/// Lines into paragraphs and headings. A paragraph ends at a wider gap, an indent, a short line
/// that ends a sentence, or a change of size; it runs on across columns and pages when the
/// sentence does.
fn paragraphs(flow: &[(usize, Vec<Item>)], body: f32, heights: &[f32]) -> Vec<Block> {
    let known = vocabulary(flow);
    // Heading sizes, largest first: the three largest sizes above the body's become h1–h3. A
    // size counts only if some run of lines in it is short enough to be a heading, so an
    // abstract set large doesn't take a level from the section heads.
    let mut heading_sizes: HashSet<i32> = HashSet::new();
    let mut run: Option<(i32, usize)> = None;
    let mut end_run = |run: &mut Option<(i32, usize)>| {
        if let Some((key, chars)) = run.take()
            && chars <= HEADING_CHARS
        {
            heading_sizes.insert(key);
        }
    };
    for it in flow.iter().flat_map(|(_, items)| items.iter()) {
        let key = match it {
            Item::Text(l) if l.size >= body * 1.15 => Some((l.size * 2.0).round() as i32),
            _ => None,
        };
        match (key, &mut run) {
            (Some(k), Some((r, chars))) if *r == k => {
                if let Item::Text(l) = it {
                    *chars += l.text.chars().count() + 1;
                }
            }
            (Some(k), _) => {
                end_run(&mut run);
                if let Item::Text(l) = it {
                    run = Some((k, l.text.chars().count()));
                }
            }
            (None, _) => end_run(&mut run),
        }
    }
    end_run(&mut run);
    let mut heading_sizes: Vec<i32> = heading_sizes.into_iter().collect();
    heading_sizes.sort_by(|a, b| b.cmp(a));
    let level_of = |size: f32| {
        let key = (size * 2.0).round() as i32;
        heading_sizes
            .iter()
            .position(|s| *s == key)
            .map(|i| (i as u8 + 1).min(3))
    };

    // The usual distance from one line to the next, for spotting paragraph gaps.
    let mut steps = Vec::new();
    for (_, items) in flow {
        let mut prev: Option<&Line> = None;
        for it in items {
            if let Item::Text(l) = it {
                if let Some(p) = prev
                    && (l.size - body).abs() < 0.5
                    && (p.size - body).abs() < 0.5
                    && l.rect.y0 > p.rect.y0
                    && (l.rect.x0 - p.rect.x0).abs() < body * 4.0
                {
                    steps.push(l.rect.y0 - p.rect.y0);
                }
                prev = Some(l);
            } else {
                prev = None;
            }
        }
    }
    steps.sort_by(f32::total_cmp);
    // The lower median: paragraph gaps are the larger steps and mustn't set the norm.
    let step = match steps
        .get(steps.len().saturating_sub(1) / 2)
        .copied()
        .unwrap_or(0.0)
    {
        s if s > 0.0 => s,
        _ => body * 1.2,
    };

    let mut out: Vec<Block> = Vec::new();
    let mut para: Option<(String, Line, bool)> = None; // text, last line, soft hyphen at end
    // Where the paragraph opens: in the top third of its page; its page's first text, or at
    // least three lines' space below the text above; and the page.
    let mut first = Opening::default();
    let mut last: Option<(usize, f32)> = None;
    let flush = |para: &mut Option<(String, Line, bool)>, out: &mut Vec<Block>, first: Opening| {
        let Opening { top, spaced, page } = first;
        if let Some((text, last, _)) = para.take() {
            let small = last.size < body * 0.85;
            // A superscript or footnote mark left on a line of its own.
            if small && text.chars().count() <= 3 && !text.chars().any(char::is_alphabetic) {
                return;
            }
            let lone_bold = last.bold
                && text.chars().count() < 100
                && !last.ends_sentence()
                && !text.contains(". ");
            // Large type that runs on (an abstract, a paper's list of authors) is a paragraph
            // set large, not a heading.
            let level = level_of(last.size).filter(|_| text.chars().count() <= HEADING_CHARS);
            match level {
                Some(level) => out.push(Block::Heading {
                    level,
                    text,
                    top,
                    page,
                }),
                // Set apart by space, a bold line ranks above one inside the text
                // (structure-recovery spec §5).
                None if lone_bold && !small => out.push(Block::Heading {
                    level: if spaced { 2 } else { 3 },
                    text,
                    top,
                    page,
                }),
                None => out.push(Block::Para { text, small, page }),
            }
        }
    };
    let mut lines_in_para = 0usize;
    for (page_no, items) in flow {
        // The right edge of the column a line sits in: the furthest any line starting near it
        // reaches on this page.
        let texts: Vec<Rect> = items
            .iter()
            .filter_map(|i| match i {
                Item::Text(l) => Some(l.rect),
                Item::Figure(..) => None,
            })
            .collect();
        let col_right = |l: &Line| {
            texts
                .iter()
                .filter(|r| (r.x0 - l.rect.x0).abs() < body * 3.0)
                .map(|r| r.x1)
                .fold(l.rect.x1, f32::max)
        };
        // A line that stops before three quarters of its column's width: the last of a
        // paragraph, even in ragged-right text.
        let short = |l: &Line| l.rect.x1 < l.rect.x0 + (col_right(l) - l.rect.x0) * 0.75;
        for it in items {
            let l = match it {
                Item::Figure(kind, rect) => {
                    // An image or table in the middle of a sentence floats ahead of the
                    // paragraph, which carries on after it; an equation stays where it is.
                    let open = *kind != Figure::Equation
                        && para
                            .as_ref()
                            .is_some_and(|(_, last, _)| !last.ends_sentence());
                    let held = if open { para.take() } else { None };
                    last = Some((*page_no, rect.y1));
                    flush(&mut para, &mut out, first);
                    out.push(Block::Figure {
                        kind: *kind,
                        page: *page_no,
                        rect: *rect,
                    });
                    para = held;
                    continue;
                }
                Item::Text(l) => l,
            };
            let top = l.rect.y0 < heights.get(l.page).copied().unwrap_or(f32::MAX) / 3.0;
            let spaced =
                last.is_none_or(|(page, y0)| page != l.page || l.rect.y0 - y0 >= step * 3.0);
            last = Some((l.page, l.rect.y0));
            let Some((text, prev, _)) = &para else {
                para = Some((l.text.clone(), l.clone(), l.soft_end));
                first = Opening {
                    top,
                    spaced,
                    page: l.page,
                };
                lines_in_para = 1;
                continue;
            };
            let same_place = prev.page == l.page
                && l.rect.y0 > prev.rect.y0
                && (l.rect.x0 - prev.rect.x0).abs() < step * 6.0;
            let gap = l.rect.y0 - prev.rect.y0;
            // A change of size among the large ones too, heading or not (an author list set
            // larger than the dates beside it), and any clear change of size: a table's title
            // doesn't run on into a caption set smaller.
            let large = |l: &Line| (l.size >= body * 1.15).then(|| (l.size * 2.0).round() as i32);
            let size_change = level_of(prev.size) != level_of(l.size)
                || large(prev) != large(l)
                || (prev.size < body * 0.85) != (l.size < body * 0.85)
                || (prev.size - l.size).abs() >= body * 0.15;
            // A heading set over two lines, centred or not: the lines overlap side to side.
            let heading_run = level_of(l.size).is_some()
                && !size_change
                && prev.page == l.page
                && gap > 0.0
                && gap < l.size * 2.0
                && l.rect.x0 < prev.rect.x1
                && l.rect.x1 > prev.rect.x0;
            let new_para = if heading_run {
                false
            } else if size_change || level_of(l.size).is_some() {
                true
            } else if same_place {
                let col_left = prev.rect.x0.min(l.rect.x0);
                gap > step * 1.4
                    || starts_item(&l.text)
                    || (l.rect.x0 > col_left + body * 0.8
                        && prev.rect.x0 <= col_left + 1.0
                        // A list item's own lines hang under its first.
                        && !(lines_in_para == 1 && starts_item(&prev.text)))
                    || (prev.ends_sentence() && short(prev))
                    || (prev.bold && !l.bold)
            } else {
                // Across a column or page: the sentence decides.
                prev.ends_sentence() && !l.text.starts_with(|c: char| c.is_lowercase())
            };
            if new_para {
                flush(&mut para, &mut out, first);
                para = Some((l.text.clone(), l.clone(), l.soft_end));
                first = Opening {
                    top,
                    spaced,
                    page: l.page,
                };
                lines_in_para = 1;
                continue;
            }
            let joined = if text.ends_with('-') && !text.ends_with(" -") {
                join_hyphen(text, &l.text, prev.soft_end, &known)
            } else if text.ends_with('/') && !l.text.starts_with(char::is_whitespace) {
                // A URL broken after a slash.
                format!("{text}{}", l.text)
            } else {
                format!("{text} {}", l.text)
            };
            para = Some((joined, l.clone(), l.soft_end));
            lines_in_para += 1;
        }
    }
    flush(&mut para, &mut out, first);
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A line of 10 pt text at `(x, y)`, one glyph per character, 5 pt wide.
    fn text_at(glyphs: &mut Vec<Glyph>, x: f32, y: f32, s: &str) {
        styled(glyphs, x, y, s, 10.0, false);
    }

    fn styled(glyphs: &mut Vec<Glyph>, x: f32, y: f32, s: &str, size: f32, bold: bool) {
        let w = size * 0.5;
        for (i, c) in s.chars().enumerate() {
            glyphs.push(Glyph {
                c,
                x0: x + i as f32 * w,
                y0: y,
                x1: x + i as f32 * w + w * 0.9,
                y1: y + size,
                size,
                bold,
                italic: false,
                math: false,
                soft_hyphen: false,
                space_before: false,
            });
        }
    }

    fn page(glyphs: Vec<Glyph>) -> Page {
        Page {
            width: 612.0,
            height: 792.0,
            glyphs,
            ..Default::default()
        }
    }

    fn texts(b: &[Block]) -> Vec<String> {
        b.iter()
            .map(|b| match b {
                Block::Heading { level, text, .. } => format!("h{level}:{text}"),
                Block::Para { text, .. } => text.clone(),
                Block::Figure { kind, .. } => format!("[{kind:?}]"),
            })
            .collect()
    }

    #[test]
    fn spaces_come_from_gaps_and_wide_gaps_split_runs() {
        let mut g = Vec::new();
        text_at(&mut g, 72.0, 100.0, "ab");
        text_at(&mut g, 72.0 + 2.0 * 5.0 + 2.5, 100.0, "cd");
        text_at(&mut g, 300.0, 100.0, "far");
        let (lines, _) = lines_of(0, &page(g));
        let t: Vec<&str> = lines.iter().map(|l| l.text.as_str()).collect();
        assert_eq!(t, ["ab cd", "far"]);
    }

    #[test]
    fn two_columns_read_left_then_right() {
        let mut g = Vec::new();
        for i in 0..6 {
            let y = 120.0 + i as f32 * 12.0;
            let end = if i == 5 { "." } else { "" };
            text_at(&mut g, 72.0, y, &format!("Left line {i} of the text{end}"));
            text_at(&mut g, 330.0, y, &format!("Right line {i} of the text"));
        }
        // A title across both columns, above them.
        styled(&mut g, 150.0, 80.0, "A Title Across", 18.0, true);
        let (b, stats) = blocks(&[page(g)]);
        let t = texts(&b);
        assert_eq!(t[0], "h1:A Title Across");
        assert!(
            t[1].starts_with("Left line 0") && t[1].ends_with("Left line 5 of the text."),
            "{t:?}"
        );
        assert!(t.last().unwrap().starts_with("Right line 0"), "{t:?}");
        assert_eq!(stats.columns, 2);
    }

    #[test]
    fn a_gutter_narrower_than_a_run_gap_still_splits_the_columns() {
        // A journal's gutter: 12 pt between columns of 10 pt type, under RUN_GAP_EM.
        let mut g = Vec::new();
        for i in 0..8 {
            let y = 120.0 + i as f32 * 12.0;
            // 31 glyphs, ending at 226.5; the right column starts at 239.
            text_at(
                &mut g,
                72.0,
                y,
                &format!("Left {i} of the first column text"),
            );
            text_at(&mut g, 239.0, y, &format!("Right {i}"));
        }
        // A title across both columns, and an abstract line under it.
        styled(
            &mut g,
            100.0,
            60.0,
            "A Title Across Both Columns",
            18.0,
            true,
        );
        text_at(
            &mut g,
            72.0,
            90.0,
            "An abstract line that runs across the gutter too.",
        );
        let (lines, channel) = lines_of(0, &page(g.clone()));
        assert!(
            channel.is_some_and(|c| c > 226.5 && c < 239.0),
            "{channel:?}"
        );
        assert!(
            lines
                .iter()
                .any(|l| l.text == "Left 0 of the first column text")
        );
        let (b, stats) = blocks(&[page(g)]);
        let t = texts(&b);
        assert_eq!(stats.columns, 2);
        let all = t.join(" | ");
        let lefts: String = (0..8)
            .map(|i| format!("Left {i} of the first column text"))
            .collect::<Vec<_>>()
            .join(" ");
        let rights: String = (0..8)
            .map(|i| format!("Right {i}"))
            .collect::<Vec<_>>()
            .join(" ");
        assert!(all.ends_with(&format!("{lefts} {rights}")), "{t:?}");
    }

    #[test]
    fn superscripts_stay_in_their_line_and_its_halves_in_order() {
        let mut g = Vec::new();
        text_at(&mut g, 72.0, 100.0, "generations");
        // "53-55" raised and small, then ". Acknowledging" a hair higher than the first half.
        styled(&mut g, 127.0, 98.0, "53-55", 6.0, false);
        text_at(&mut g, 145.0, 99.99, ". Acknowledging");
        let (lines, _) = lines_of(0, &page(g.clone()));
        let t: Vec<&str> = lines.iter().map(|l| l.text.as_str()).collect();
        assert_eq!(t, ["generations53-55 . Acknowledging"]);
        let mut g = Vec::new();
        text_at(&mut g, 72.0, 100.0, "first half");
        text_at(&mut g, 300.0, 99.99, "second half");
        let (b, _) = blocks(&[page(g)]);
        assert_eq!(texts(&b), ["first half second half"]);
    }

    #[test]
    fn a_tall_glyph_does_not_merge_the_lines_beside_it() {
        let mut g = Vec::new();
        text_at(&mut g, 330.0, 100.0, "upper line");
        text_at(&mut g, 330.0, 111.0, "lower line");
        // A bracket in the other column, as tall as both lines.
        styled(&mut g, 100.0, 99.0, "(", 22.0, false);
        let (lines, _) = lines_of(0, &page(g));
        let t: Vec<&str> = lines.iter().map(|l| l.text.as_str()).collect();
        assert!(
            t.contains(&"upper line") && t.contains(&"lower line"),
            "{t:?}"
        );
    }

    #[test]
    fn an_icon_after_a_name_keeps_the_line_whole() {
        let mut g = Vec::new();
        text_at(&mut g, 72.0, 100.0, "Ada Lovelace");
        // An ORCID mark (a small drawing, a point from the text either side) fills a gap wider
        // than RUN_GAP_EM: "Lovelace" ends at 131.5, the next text starts at 148.
        text_at(&mut g, 148.0, 100.0, "1,2 & Charles Babbage");
        let mut p = page(g);
        p.paths.push(Rect {
            x0: 133.0,
            y0: 101.0,
            x1: 146.5,
            y1: 109.0,
        });
        let (lines, _) = lines_of(0, &p);
        let t: Vec<&str> = lines.iter().map(|l| l.text.as_str()).collect();
        assert_eq!(t, ["Ada Lovelace 1,2 & Charles Babbage"]);
    }

    #[test]
    fn a_sidebar_beside_the_abstract_is_read_as_its_own_column() {
        let mut g = Vec::new();
        // Under the title: dates in a narrow sidebar beside full-width authors and abstract.
        text_at(&mut g, 40.0, 100.0, "Received: 1 May");
        text_at(
            &mut g,
            200.0,
            100.0,
            "Ada Lovelace, Charles Babbage and Mary Somerville",
        );
        text_at(&mut g, 40.0, 112.0, "Accepted: 2 June");
        text_at(
            &mut g,
            200.0,
            124.0,
            "An abstract across the page, from the sidebar's edge on",
        );
        text_at(&mut g, 40.0, 124.0, "Open access");
        text_at(
            &mut g,
            200.0,
            136.0,
            "to the right margin of the page, two lines long.",
        );
        // Two columns below, the left one reaching nearly to the middle (x 290).
        for i in 0..6 {
            let y = 170.0 + i as f32 * 12.0;
            let filler = "of the body text in this column";
            text_at(&mut g, 40.0, y, &format!("Left column line {i} {filler} ."));
            text_at(&mut g, 310.0, y, &format!("Right column line {i} here"));
        }
        let (b, _) = blocks(&[page(g)]);
        let all = texts(&b).join(" | ");
        let at = |s: &str| all.find(s).unwrap_or_else(|| panic!("{s} in {all}"));
        assert!(at("Received") < at("Accepted") && at("Accepted") < at("Open access"));
        assert!(at("Open access") < at("Ada Lovelace"), "{all}");
        assert!(at("Ada Lovelace") < at("An abstract"), "{all}");
        assert!(at("two lines long.") < at("Left column line 0"), "{all}");
        assert!(
            at("Left column line 5") < at("Right column line 0"),
            "{all}"
        );
    }

    #[test]
    fn long_large_type_is_a_paragraph_not_a_heading() {
        let mut g = Vec::new();
        styled(&mut g, 72.0, 60.0, "A Title", 20.0, false);
        let abstract_ = "An abstract set larger than the body, which runs on for a while";
        for i in 0..3 {
            styled(&mut g, 72.0, 90.0 + i as f32 * 14.0, abstract_, 12.0, false);
        }
        for i in 0..6 {
            text_at(
                &mut g,
                72.0,
                150.0 + i as f32 * 12.0,
                "Body text set in the usual size",
            );
        }
        let (b, _) = blocks(&[page(g)]);
        assert!(matches!(&b[0], Block::Heading { text, .. } if text == "A Title"));
        assert!(matches!(&b[1], Block::Para { text, .. } if text.starts_with("An abstract")));
    }

    /// `s` at `(x, y)` in a math font, `h` tall: a sum sign or a big bracket.
    fn math_at(glyphs: &mut Vec<Glyph>, x: f32, y: f32, s: &str, h: f32) {
        for (i, c) in s.chars().enumerate() {
            glyphs.push(Glyph {
                c,
                x0: x + i as f32 * 5.0,
                y0: y,
                x1: x + i as f32 * 5.0 + 4.0,
                y1: y + h,
                size: 10.0,
                bold: false,
                italic: false,
                math: true,
                soft_hyphen: false,
                space_before: false,
            });
        }
    }

    #[test]
    fn an_equation_in_a_text_font_around_math_symbols_is_one_picture() {
        let mut g = Vec::new();
        text_at(&mut g, 72.0, 100.0, "The cumulative heat of an event is:");
        // heat = Σ ( T - Txpct )   (2), with the sum and brackets in a math font, taller than
        // a line, so each sits on a row of its own.
        text_at(&mut g, 100.0, 130.0, "heat =");
        math_at(&mut g, 135.0, 125.0, "∑(", 17.0);
        text_at(&mut g, 147.0, 130.0, "T");
        math_at(&mut g, 155.0, 130.0, "−", 10.0);
        text_at(&mut g, 162.0, 130.0, "Txpct");
        math_at(&mut g, 190.0, 125.0, ")", 17.0);
        math_at(&mut g, 270.0, 125.0, "(", 17.0);
        text_at(&mut g, 276.0, 130.0, "2");
        math_at(&mut g, 282.0, 125.0, ")", 17.0);
        text_at(&mut g, 72.0, 160.0, "summed over the days of the heatwave.");
        let (b, stats) = blocks(&[page(g)]);
        assert_eq!(stats.equations, 1);
        assert_eq!(
            texts(&b),
            [
                "The cumulative heat of an event is:",
                "[Equation]",
                "summed over the days of the heatwave."
            ]
        );
    }

    #[test]
    fn a_lone_math_symbol_does_not_take_the_text_beside_it() {
        let mut g = Vec::new();
        text_at(&mut g, 72.0, 100.0, "Ada Lovelace");
        math_at(&mut g, 160.0, 98.0, "†", 14.0);
        text_at(&mut g, 200.0, 100.0, "Charles Babbage");
        let (b, stats) = blocks(&[page(g)]);
        assert_eq!(stats.equations, 0);
        let all = texts(&b).join(" ");
        assert!(
            all.contains("Ada Lovelace") && all.contains("Charles Babbage"),
            "{all}"
        );
    }

    /// Eight rows of two columns 12 pt apart (a tight gutter) from `y`: the left column ends at
    /// 226.5, the right starts at 239.
    fn tight_columns(g: &mut Vec<Glyph>, y: f32) {
        for i in 0..8 {
            let y = y + i as f32 * 12.0;
            text_at(g, 72.0, y, &format!("Left {i} of the first column text"));
            text_at(g, 239.0, y, &format!("Right {i} and more of it"));
        }
    }

    #[test]
    fn an_icon_at_a_column_end_does_not_bridge_the_gutter() {
        let mut g = Vec::new();
        // The right column starts further off: 20 pt after the icon.
        for i in 0..8 {
            let y = 120.0 + i as f32 * 12.0;
            text_at(
                &mut g,
                72.0,
                y,
                &format!("Left {i} of the first column text"),
            );
            text_at(&mut g, 260.0, y, &format!("Right {i}"));
        }
        let mut p = page(g);
        p.paths.push(Rect {
            x0: 230.0,
            y0: 121.0,
            x1: 238.0,
            y1: 129.0,
        });
        let (lines, _) = lines_of(0, &p);
        let t: Vec<&str> = lines.iter().map(|l| l.text.as_str()).collect();
        assert!(t.contains(&"Left 0 of the first column text"), "{t:?}");
        assert!(t.contains(&"Right 0"), "{t:?}");
    }

    #[test]
    fn a_label_under_a_centred_title_is_not_its_sidebar() {
        let mut g = Vec::new();
        styled(&mut g, 200.0, 60.0, "A Centred Title", 18.0, true);
        text_at(&mut g, 50.0, 90.0, "Abstract");
        for i in 0..3 {
            let y = 102.0 + i as f32 * 12.0;
            // From 50 to 375, across the gutter at 233.
            let text = format!("Abstract line {i} runs across the page and over the gutter.");
            text_at(&mut g, 50.0, y, &text);
        }
        tight_columns(&mut g, 170.0);
        let (b, _) = blocks(&[page(g)]);
        let all = texts(&b).join(" | ");
        let title = all.find("A Centred Title").expect("title");
        let label = all.find("Abstract").expect("label");
        assert!(title < label, "{all}");
        assert!(
            all[label..].starts_with("Abstract Abstract line 0"),
            "{all}"
        );
    }

    #[test]
    fn a_short_equation_in_computer_modern_is_a_picture() {
        // (x + y)2 with only x and y in the math font, as Computer Modern sets it.
        let mut g = Vec::new();
        text_at(&mut g, 72.0, 100.0, "We square the sum:");
        text_at(&mut g, 150.0, 120.0, "(");
        math_at(&mut g, 155.0, 120.0, "x", 10.0);
        text_at(&mut g, 160.0, 120.0, "+");
        math_at(&mut g, 165.0, 120.0, "y", 10.0);
        text_at(&mut g, 170.0, 120.0, ")2");
        text_at(&mut g, 72.0, 140.0, "and expand it.");
        let (b, stats) = blocks(&[page(g)]);
        assert_eq!(stats.equations, 1, "{:?}", texts(&b));
    }

    #[test]
    fn an_aligned_list_on_a_one_column_page_reads_row_by_row() {
        let mut g = Vec::new();
        let body = "Body text runs across the whole single column of this page here.";
        for i in 0..6 {
            text_at(&mut g, 72.0, 100.0 + i as f32 * 12.0, body);
        }
        for (i, k) in ["Name", "Born", "Field", "Known for", "Died"]
            .iter()
            .enumerate()
        {
            let y = 190.0 + i as f32 * 12.0;
            text_at(&mut g, 72.0, y, k);
            text_at(&mut g, 280.0, y, &format!("value {i}"));
        }
        let (b, stats) = blocks(&[page(g)]);
        assert_eq!(stats.columns, 1);
        let all = texts(&b).join(" ");
        assert!(all.contains("Name value 0 Born value 1"), "{all}");
    }

    #[test]
    fn a_quad_between_names_across_the_gutter_stays_in_the_line() {
        let mut g = Vec::new();
        // Authors across both columns with a 1 em quad (228 to 238) right on the gutter's x
        // (233); the gutter itself is 1.25 em.
        text_at(&mut g, 93.5, 90.0, "Ada Lovelace and Mary Somer");
        text_at(&mut g, 238.0, 90.0, "Charles Babbage");
        // Sixteen rows of columns: a page's worth, so the one quad isn't its narrow end.
        tight_columns(&mut g, 120.0);
        tight_columns(&mut g, 220.0);
        let (lines, _) = lines_of(0, &page(g));
        let t: Vec<&str> = lines.iter().map(|l| l.text.as_str()).collect();
        assert!(
            t.contains(&"Ada Lovelace and Mary Somer Charles Babbage"),
            "{t:?}"
        );
    }

    #[test]
    fn an_equation_keeps_to_its_column_without_shared_rows() {
        let mut g = Vec::new();
        // Two columns on offset baselines: no row has both, so only the lines show the gutter.
        for i in 0..8 {
            let y = 100.0 + i as f32 * 12.0;
            text_at(&mut g, 72.0, y, "Left column text in a line.");
            text_at(&mut g, 330.0, y + 6.0, "Right column body text line.");
        }
        // A tall equation in the left column, beside a short right-column line.
        math_at(&mut g, 100.0, 200.0, "∑(x)=", 22.0);
        text_at(&mut g, 330.0, 206.0, "Results");
        let (b, stats) = blocks(&[page(g)]);
        assert_eq!(stats.equations, 1);
        assert!(
            texts(&b).iter().any(|t| t.contains("Results")),
            "{:?}",
            texts(&b)
        );
    }

    #[test]
    fn a_large_abstract_does_not_take_a_heading_level() {
        let mut g = Vec::new();
        styled(&mut g, 72.0, 40.0, "A Title", 20.0, false);
        let line = "An abstract set larger than the body, which runs on for a while";
        for i in 0..3 {
            styled(&mut g, 72.0, 70.0 + i as f32 * 14.0, line, 12.0, false);
        }
        styled(&mut g, 72.0, 130.0, "Results", 11.5, false);
        // More body than abstract, so the body sets the usual size.
        for i in 0..12 {
            text_at(
                &mut g,
                72.0,
                150.0 + i as f32 * 12.0,
                "Body text set in the usual size",
            );
        }
        let (b, _) = blocks(&[page(g)]);
        assert!(
            texts(&b).contains(&"h2:Results".to_owned()),
            "{:?}",
            texts(&b)
        );
    }

    /// `s` on baseline `base`, its glyph boxes `em` ems tall, as fonts with a tall ascent and
    /// descent give them.
    fn boxed_at(glyphs: &mut Vec<Glyph>, x: f32, base: f32, s: &str, size: f32, em: f32) {
        let w = size * 0.5;
        for (i, c) in s.chars().enumerate() {
            glyphs.push(Glyph {
                c,
                x0: x + i as f32 * w,
                y0: base - size * em * 0.8,
                x1: x + i as f32 * w + w * 0.9,
                y1: base + size * em * 0.2,
                size,
                bold: false,
                italic: false,
                math: false,
                soft_hyphen: false,
                space_before: false,
            });
        }
    }

    #[test]
    fn a_font_with_tall_boxes_still_makes_lines() {
        let mut g = Vec::new();
        boxed_at(&mut g, 72.0, 110.0, "Hello world", 10.0, 1.6);
        // A smaller acronym in a 1.22 em font, on the same baseline.
        boxed_at(&mut g, 72.0, 130.0, "The", 10.0, 1.22);
        boxed_at(&mut g, 92.0, 130.0, "NASA", 8.0, 1.22);
        boxed_at(&mut g, 115.0, 130.0, "mission flew.", 10.0, 1.22);
        let (lines, _) = lines_of(0, &page(g));
        let t: Vec<&str> = lines.iter().map(|l| l.text.as_str()).collect();
        assert_eq!(t, ["Hello world", "The NASA mission flew."]);
    }

    #[test]
    fn a_column_beside_a_wide_figure_is_not_a_sidebar() {
        let mut g = Vec::new();
        // Two columns, with a figure a little wider than the left one, across the gutter.
        for i in 0..20 {
            let y = 80.0 + i as f32 * 12.0;
            if !(150.0..300.0).contains(&y) {
                text_at(
                    &mut g,
                    72.0,
                    y,
                    &format!("L{i} left body text runs on in here ok"),
                );
            }
            text_at(&mut g, 310.0, y, &format!("R{i} right body text in here"));
        }
        let mut p = page(g);
        p.images.push(Rect {
            x0: 72.0,
            y0: 150.0,
            x1: 304.0,
            y1: 290.0,
        });
        let (b, _) = blocks(&[p]);
        let all = texts(&b).join(" ");
        let at = |s: &str| all.find(s).unwrap_or_else(|| panic!("{s} in {all}"));
        // The left column's last line isn't pushed behind the right column's lines beside the
        // figure.
        assert!(at("L19 ") < at("R6 "), "{all}");
    }

    #[test]
    fn a_last_line_mostly_citation_stays_in_its_paragraph() {
        let mut g = Vec::new();
        text_at(
            &mut g,
            72.0,
            100.0,
            "A sentence that runs the whole width of the column",
        );
        text_at(&mut g, 72.0, 112.0, "it.");
        styled(&mut g, 87.0, 110.0, "12-15,18", 6.0, false);
        text_at(&mut g, 72.0, 124.0, "Next sentence goes on and on here.");
        let (b, _) = blocks(&[page(g)]);
        let t = texts(&b);
        assert_eq!(
            t,
            [
                "A sentence that runs the whole width of the column it.12-15,18",
                "Next sentence goes on and on here."
            ]
        );
    }

    #[test]
    fn a_number_set_full_size_at_a_line_end_does_not_end_the_sentence() {
        let mut g = Vec::new();
        text_at(
            &mut g,
            72.0,
            100.0,
            "A sentence that runs the whole width of the column",
        );
        text_at(&mut g, 72.0, 112.0, "pi is 3.14");
        text_at(&mut g, 72.0, 124.0, "Roughly, and it goes on here.");
        let (b, _) = blocks(&[page(g)]);
        assert_eq!(texts(&b).len(), 1, "{:?}", texts(&b));
    }

    #[test]
    fn where_under_a_tall_sum_stays_text() {
        let mut g = Vec::new();
        text_at(
            &mut g,
            72.0,
            100.0,
            "The total over all the days of the event is",
        );
        // A centred sum whose box reaches half into the next line.
        math_at(&mut g, 150.0, 114.0, "∑", 20.0);
        math_at(&mut g, 160.0, 120.0, "x=y", 10.0);
        text_at(&mut g, 72.0, 128.0, "where");
        text_at(&mut g, 105.0, 128.0, "x is the heat of a day.");
        let (b, stats) = blocks(&[page(g)]);
        assert_eq!(stats.equations, 1);
        assert!(
            texts(&b).iter().any(|t| t.contains("where")),
            "{:?}",
            texts(&b)
        );
    }

    #[test]
    fn running_heads_and_folios_go() {
        let pages: Vec<Page> = (1..=4)
            .map(|n| {
                let mut g = Vec::new();
                text_at(&mut g, 72.0, 30.0, "Journal of Tests");
                text_at(&mut g, 72.0, 120.0, &format!("Body text on page {n}."));
                text_at(&mut g, 300.0, 760.0, &n.to_string());
                text_at(&mut g, 72.0, 772.0, &format!("Page {n} of 4"));
                page(g)
            })
            .collect();
        let (b, stats) = blocks(&pages);
        let all = texts(&b).join("|");
        assert!(!all.contains("Journal"), "{all}");
        assert!(!all.contains("of 4"), "{all}");
        assert!(all.contains("Body text on page 3."), "{all}");
        assert_eq!(stats.running_lines, 12);
    }

    #[test]
    fn hyphens_rejoin_but_compounds_stay() {
        let known: HashSet<String> = ["information", "the"]
            .iter()
            .map(|s| s.to_string())
            .collect();
        assert_eq!(
            join_hyphen("the informa-", "tion age", false, &known),
            "the information age"
        );
        assert_eq!(
            join_hyphen("a well-", "known fact", false, &known),
            "a well-known fact"
        );
        assert_eq!(
            join_hyphen("pre-", "War Europe", false, &known),
            "pre-War Europe"
        );
        assert_eq!(
            join_hyphen("an unusu-", "ally long", false, &known),
            "an unusually long"
        );
        assert_eq!(
            join_hyphen("a self-", "help book", true, &known),
            "a selfhelp book",
            "a soft hyphen always joins"
        );
    }

    #[test]
    fn list_items_are_paragraphs_with_hanging_indents() {
        let mut g = Vec::new();
        text_at(&mut g, 72.0, 100.0, "• First item that wraps");
        text_at(&mut g, 82.0, 112.0, "onto a second line");
        text_at(&mut g, 72.0, 124.0, "• Second item");
        let (b, _) = blocks(&[page(g)]);
        assert_eq!(
            texts(&b),
            [
                "• First item that wraps onto a second line",
                "• Second item"
            ]
        );
    }

    fn para(text: &str, page: usize) -> Block {
        Block::Para {
            text: text.into(),
            small: false,
            page,
        }
    }

    fn mark(level: u8, title: &str, page: usize) -> Mark {
        Mark {
            level,
            title: title.into(),
            page,
        }
    }

    #[test]
    fn bookmarks_name_their_blocks_or_open_their_pages() {
        let blocks = vec![
            para("A Bookmarked Book", 0),
            para("Opening words of the first part.", 1),
            Block::Heading {
                level: 1,
                text: "A Loud Title".into(),
                top: true,
                page: 1,
            },
            para("THE SECOND PART", 2),
            para("More words.", 2),
            para("An Aside", 2),
            para("Last words.", 2),
        ];
        let marks = [
            mark(1, "Part One: Beginnings", 1),
            mark(1, "The Second Part", 2),
            mark(2, "An aside!", 2),
            mark(3, "Too deep", 2),
        ];
        let t = texts(&apply_outline(&blocks, &marks).unwrap());
        assert_eq!(
            t,
            [
                "A Bookmarked Book",
                "h1:Part One: Beginnings",
                "Opening words of the first part.",
                "h3:A Loud Title",
                "h1:THE SECOND PART",
                "More words.",
                "h2:An Aside",
                "Last words.",
            ]
        );
    }

    #[test]
    fn one_bookmark_or_none_leaves_the_layout_alone() {
        let blocks = vec![para("Text.", 0), para("More.", 1)];
        assert_eq!(apply_outline(&blocks, &[]), None);
        assert_eq!(apply_outline(&blocks, &[mark(1, "Only", 0)]), None);
        // A bookmark past the last page lands nowhere.
        assert_eq!(
            apply_outline(&blocks, &[mark(1, "One", 0), mark(1, "Gone", 9)]),
            None
        );
    }

    #[test]
    fn a_bold_line_set_apart_outranks_one_inside_the_text() {
        let mut pages = Vec::new();
        for ch in ["Chapter One", "Chapter Two"] {
            let mut g = Vec::new();
            styled(&mut g, 72.0, 100.0, ch, 10.0, true);
            let mut y = 124.0;
            let para = |g: &mut Vec<Glyph>, y: &mut f32| {
                for i in 0..4 {
                    text_at(
                        g,
                        72.0,
                        *y,
                        &format!("Body text line {i} of the chapter goes on"),
                    );
                    *y += 12.0;
                }
                text_at(g, 72.0, *y, "Short end.");
                *y += 12.0;
            };
            para(&mut g, &mut y);
            // A section title with a little space above it, then one with a lot.
            y += 6.0;
            styled(&mut g, 72.0, y, "A Section", 10.0, true);
            y += 12.0;
            para(&mut g, &mut y);
            y += 30.0;
            styled(&mut g, 72.0, y, "A Part", 10.0, true);
            y += 12.0;
            para(&mut g, &mut y);
            pages.push(page(g));
        }
        let t = texts(&blocks(&pages).0);
        assert!(t.contains(&"h2:Chapter One".to_owned()), "{t:?}");
        assert!(t.contains(&"h2:Chapter Two".to_owned()), "{t:?}");
        assert!(t.contains(&"h2:A Part".to_owned()), "{t:?}");
        assert_eq!(
            t.iter().filter(|x| *x == "h3:A Section").count(),
            2,
            "{t:?}"
        );
    }

    #[test]
    fn a_centred_two_line_title_is_one_heading() {
        let mut g = Vec::new();
        styled(&mut g, 100.0, 80.0, "A Rather Long Title That", 18.0, true);
        styled(&mut g, 160.0, 102.0, "Wraps Twice", 18.0, true);
        text_at(
            &mut g,
            72.0,
            140.0,
            "2014) is a year, not a list item, and this line",
        );
        text_at(&mut g, 72.0, 152.0, "goes on to https://example.com/");
        text_at(&mut g, 72.0, 164.0, "path for a while.");
        let (b, _) = blocks(&[page(g)]);
        assert_eq!(
            texts(&b),
            [
                "h1:A Rather Long Title That Wraps Twice",
                "2014) is a year, not a list item, and this line goes on to https://example.com/path for a while."
            ]
        );
    }

    #[test]
    fn a_hyphenated_next_word_keeps_the_break() {
        let known = HashSet::new();
        assert_eq!(
            join_hyphen("English-", "to-German pairs", false, &known),
            "English-to-German pairs"
        );
    }

    #[test]
    fn aligned_short_cells_are_a_table() {
        let mut g = Vec::new();
        text_at(&mut g, 72.0, 100.0, "Before the table comes a sentence.");
        for (i, row) in [
            ["Name", "Qty", "Price"],
            ["Apple", "3", "1.20"],
            ["Pear", "10", "0.80"],
        ]
        .iter()
        .enumerate()
        {
            let y = 130.0 + i as f32 * 14.0;
            for (j, cell) in row.iter().enumerate() {
                text_at(&mut g, 72.0 + j as f32 * 120.0, y, cell);
            }
        }
        text_at(&mut g, 72.0, 200.0, "After it, more text.");
        let (b, stats) = blocks(&[page(g)]);
        assert_eq!(
            texts(&b),
            [
                "Before the table comes a sentence.",
                "[Table]",
                "After it, more text."
            ]
        );
        assert_eq!(stats.tables, 1);
    }

    #[test]
    fn paragraphs_split_on_gaps_and_run_on_across_pages() {
        let mut a = Vec::new();
        text_at(&mut a, 72.0, 100.0, "One paragraph that keeps");
        text_at(&mut a, 72.0, 112.0, "going on this line.");
        text_at(&mut a, 72.0, 140.0, "A second one that runs");
        let mut b = Vec::new();
        text_at(&mut b, 72.0, 100.0, "onto the next page.");
        let (blocks, _) = blocks(&[page(a), page(b)]);
        assert_eq!(
            texts(&blocks),
            [
                "One paragraph that keeps going on this line.",
                "A second one that runs onto the next page."
            ]
        );
    }

    #[test]
    fn math_lines_become_equations() {
        let mut g = Vec::new();
        text_at(&mut g, 72.0, 100.0, "We have");
        for (i, c) in "x=y+1".chars().enumerate() {
            g.push(Glyph {
                c,
                x0: 200.0 + i as f32 * 6.0,
                y0: 120.0,
                x1: 205.0 + i as f32 * 6.0,
                y1: 130.0,
                size: 10.0,
                bold: false,
                italic: true,
                math: true,
                soft_hyphen: false,
                space_before: false,
            });
        }
        text_at(&mut g, 72.0, 140.0, "for all x.");
        let (b, stats) = blocks(&[page(g)]);
        assert_eq!(texts(&b), ["We have", "[Equation]", "for all x."]);
        assert_eq!(stats.equations, 1);
    }
}
