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
    rect: Rect,
    size: f32,
    bold: bool,
    math: f32,
    text: String,
    /// The text ends with a hyphen the PDF marks as a break.
    soft_end: bool,
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
}

/// A gap wider than this many ems ends a line run: a column gutter or a table cell.
const RUN_GAP_EM: f32 = 1.5;
/// A gap wider than this many ems between glyphs is a word space.
const SPACE_EM: f32 = 0.18;

/// The blocks of every page, in reading order, and what was done to get there.
pub fn blocks(pages: &[Page]) -> (Vec<Block>, Stats) {
    let mut stats = Stats::default();
    let mut lines: Vec<Vec<Line>> = pages
        .iter()
        .enumerate()
        .map(|(i, p)| lines_of(i, p))
        .collect();
    stats.running_lines = strip_running(&mut lines, pages);
    let body = body_size(lines.iter().flatten());
    let mut flow: Vec<(usize, Vec<Item>)> = Vec::new();
    for (i, page) in pages.iter().enumerate() {
        let mut page_lines = std::mem::take(&mut lines[i]);
        let mut figures = figures_of(page, &mut page_lines, &mut stats);
        figures.extend(tables_of(&mut page_lines, &mut stats));
        figures.extend(equations_of(&mut page_lines, &mut stats));
        let (items, cols) = reading_order(page, page_lines, figures);
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

/// Glyphs grouped into rows by vertical overlap, then cut into runs at wide gaps.
fn lines_of(page: usize, p: &Page) -> Vec<Line> {
    let mut glyphs: Vec<&Glyph> = p
        .glyphs
        .iter()
        .filter(|g| g.y1 > g.y0 && g.x1 >= g.x0 && g.size > 0.0)
        .collect();
    glyphs.sort_by(|a, b| a.y0.total_cmp(&b.y0).then(a.x0.total_cmp(&b.x0)));
    // Rows: a glyph joins the row its vertical middle falls inside.
    let mut rows: Vec<(f32, f32, Vec<&Glyph>)> = Vec::new();
    for g in glyphs {
        let mid = (g.y0 + g.y1) / 2.0;
        match rows
            .iter_mut()
            .rev()
            .take(4)
            .find(|(top, bottom, _)| mid > *top && mid < *bottom)
        {
            Some(row) => {
                row.0 = row.0.min(g.y0);
                row.1 = row.1.max(g.y1);
                row.2.push(g);
            }
            None => rows.push((g.y0, g.y1, vec![g])),
        }
    }
    let mut out = Vec::new();
    for (_, _, mut row) in rows {
        row.sort_by(|a, b| a.x0.total_cmp(&b.x0));
        let mut run: Vec<&Glyph> = Vec::new();
        let mut pending_space = false;
        let mut text = String::new();
        let flush = |run: &mut Vec<&Glyph>, text: &mut String, out: &mut Vec<Line>| {
            let t = text.trim().to_owned();
            if !t.is_empty() {
                let ink: Vec<&&Glyph> = run.iter().filter(|g| !g.c.is_whitespace()).collect();
                let n = ink.len().max(1) as f32;
                out.push(Line {
                    page,
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
                    size: median(ink.iter().map(|g| g.size).collect()),
                    bold: ink.iter().filter(|g| g.bold).count() as f32 / n > 0.6,
                    math: ink.iter().filter(|g| g.math).count() as f32 / n,
                    soft_end: ink.last().is_some_and(|g| g.soft_hyphen),
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
                if gap > RUN_GAP_EM * g.size {
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
    out
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
fn equations_of(lines: &mut Vec<Line>, stats: &mut Stats) -> Vec<(Figure, Rect)> {
    let is_math = |l: &Line| {
        l.math >= 0.3
            || (l.math > 0.0
                && l.text.chars().filter(|c| c.is_alphabetic()).count() * 2
                    < l.text.chars().count())
    };
    // Each box with its lines and how many characters they hold.
    let mut boxes: Vec<(Rect, Vec<usize>, usize)> = Vec::new();
    for (i, l) in lines.iter().enumerate().filter(|(_, l)| is_math(l)) {
        let n = l.text.chars().filter(|c| !c.is_whitespace()).count();
        match boxes.iter_mut().find(|b| b.0.near(&l.rect, l.size)) {
            Some(b) => {
                b.0 = b.0.union(&l.rect);
                b.1.push(i);
                b.2 += n;
            }
            None => boxes.push((l.rect, vec![i], n)),
        }
    }
    // A lone symbol (a footnote star, a dagger) stays text.
    boxes.retain(|b| b.2 >= 3);
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
/// band's left column before its right. Returns how many columns the page has.
fn reading_order(p: &Page, lines: Vec<Line>, figures: Vec<(Figure, Rect)>) -> (Vec<Item>, usize) {
    let mut items: Vec<Item> = lines.into_iter().map(Item::Text).collect();
    items.extend(figures.into_iter().map(|(k, r)| Item::Figure(k, r)));
    let by_top = |a: &Item, b: &Item| {
        a.rect()
            .y0
            .total_cmp(&b.rect().y0)
            .then(a.rect().x0.total_cmp(&b.rect().x0))
    };
    let Some(gutter) = gutter(p, &items) else {
        items.sort_by(by_top);
        return (items, 1);
    };
    let spans = |r: Rect| r.x0 < gutter && r.x1 > gutter;
    items.sort_by(by_top);
    let mut out = Vec::new();
    let (mut left, mut right): (Vec<Item>, Vec<Item>) = (Vec::new(), Vec::new());
    for it in items {
        let r = it.rect();
        if spans(r) {
            out.append(&mut left);
            out.append(&mut right);
            out.push(it);
        } else if r.x1 <= gutter {
            left.push(it);
        } else {
            right.push(it);
        }
    }
    out.append(&mut left);
    out.append(&mut right);
    (out, 2)
}

/// The x of an empty vertical channel in the middle of the page with text on both sides: the
/// gutter of a two-column page.
fn gutter(p: &Page, items: &[Item]) -> Option<f32> {
    let text: Vec<Rect> = items
        .iter()
        .filter_map(|i| match i {
            Item::Text(l) => Some(l.rect),
            Item::Figure(..) => None,
        })
        .collect();
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
    let (mut best, mut run): ((f32, f32), Option<f32>) = ((0.0, -1.0), None);
    for (x, c) in &crossings {
        match (run, *c == crossing) {
            (None, true) => run = Some(*x),
            (Some(start), false) => {
                if x - start > best.1 - best.0 {
                    best = (start, *x);
                }
                run = None;
            }
            _ => {}
        }
    }
    if let Some(start) = run
        && to - start > best.1 - best.0
    {
        best = (start, to);
    }
    let x = (best.0 + best.1) / 2.0;
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

/// Lines into paragraphs and headings. A paragraph ends at a wider gap, an indent, a short line
/// that ends a sentence, or a change of size; it runs on across columns and pages when the
/// sentence does.
fn paragraphs(flow: &[(usize, Vec<Item>)], body: f32, heights: &[f32]) -> Vec<Block> {
    let known = vocabulary(flow);
    // Heading sizes, largest first: the three largest sizes above the body's become h1–h3.
    let mut heading_sizes: Vec<i32> = flow
        .iter()
        .flat_map(|(_, items)| items.iter())
        .filter_map(|i| match i {
            Item::Text(l) if l.size >= body * 1.15 => Some((l.size * 2.0).round() as i32),
            _ => None,
        })
        .collect::<HashSet<_>>()
        .into_iter()
        .collect();
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
                && !ends_sentence(&text)
                && !text.contains(". ");
            match level_of(last.size) {
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
                        && para.as_ref().is_some_and(|(t, _, _)| !ends_sentence(t));
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
            let size_change = level_of(prev.size) != level_of(l.size)
                || (prev.size < body * 0.85) != (l.size < body * 0.85);
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
                    || (ends_sentence(&prev.text) && short(prev))
                    || (prev.bold && !l.bold)
            } else {
                // Across a column or page: the sentence decides.
                ends_sentence(&prev.text) && !l.text.starts_with(|c: char| c.is_lowercase())
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
        let lines = lines_of(0, &page(g));
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
