// SPDX-License-Identifier: GPL-3.0-or-later
//! The puzzle page's grids as pictures for the EPUB. A Kindle sets any HTML table as a
//! full-width, zoomable table: the squares stretch into wide boxes and a zoom button sits under
//! them, so the crossword and the word search read as spreadsheets. A PNG keeps the squares
//! square on every reader. The PDF edition still draws the grids from the tables, in vector.

use ab_glyph::FontRef;

use super::{grid, search};
use crate::cover::{Canvas, png, width_of};
use crate::esc;

const BLACK: u8 = 0;
/// The loop around a found word in yesterday's word search.
const LOOP: u8 = 205;

/// A table on the puzzle page and what the EPUB prints in its place.
pub(crate) struct GridPicture {
    /// The `<table>` exactly as the section prints it.
    pub table: String,
    /// The XHTML that takes its place.
    pub html: String,
    /// The PNG it shows (where it goes in the EPUB, relative to the OPF, and its bytes); a
    /// table that becomes plain text has none.
    pub image: Option<(String, Vec<u8>)>,
}

impl GridPicture {
    fn picture(table: String, href: String, png: Vec<u8>, alt: &str, class: &str) -> Self {
        GridPicture {
            table,
            html: format!(
                "<div class=\"{class}\"><img src=\"{href}\" alt=\"{}\"/></div>\n",
                esc(alt)
            ),
            image: Some((href, png)),
        }
    }
}

/// The puzzle page's tables as the EPUB prints them, named for `section` (its index in the
/// edition): today's mini (numbers) and word search as pictures, the search's words as one
/// paragraph, and yesterday's mini (letters) and word search (its words looped) as pictures.
pub(crate) fn pictures(p: &super::PuzzleData, section: usize) -> Vec<GridPicture> {
    let href = |name: &str| format!("images/s{}-{name}.png", section + 1);
    let mut out = Vec::new();
    if let Some(rows) = &p.today.rows {
        out.push(GridPicture::picture(
            super::grid_table(rows, "grid", false),
            href("mini"),
            draw(rows, false),
            &format!("The Mini · No. {}, a 5 by 5 grid", p.today.number),
            "mini",
        ));
    }
    if let Some(ws) = &p.today.search {
        out.push(GridPicture::picture(
            super::search_table(ws, "grid search", false),
            href("search"),
            draw_search(ws, false),
            &format!(
                "Word search · No. {}, {n} by {n} letters",
                p.today.number,
                n = ws.size()
            ),
            "mini search",
        ));
        let words: Vec<String> = ws
            .words
            .iter()
            .map(|h| {
                let w = esc(&h.word.to_uppercase());
                match h.theme {
                    true => format!("<b>{w}</b>"),
                    false => w,
                }
            })
            .collect();
        let theme = match ws.words.iter().any(|h| h.theme) {
            true => {
                "<br/><span class=\"small\">Words in bold are from yesterday&#39;s paper.</span>"
                    .to_owned()
            }
            false => String::new(),
        };
        out.push(GridPicture {
            table: super::word_columns(ws),
            html: format!("<p class=\"words\">{}{theme}</p>\n", words.join(" · ")),
            image: None,
        });
    }
    if let Some(y) = &p.yesterday {
        if let Some(rows) = &y.rows {
            out.push(GridPicture::picture(
                super::grid_table(rows, "grid small", true),
                href("mini-answers"),
                draw(rows, true),
                &format!("Yesterday's grid: {}", rows.join(" / ").replace('#', "■")),
                "mini small",
            ));
        }
        if let Some(ws) = &y.search {
            out.push(GridPicture::picture(
                super::search_table(ws, "grid search small", true),
                href("search-answers"),
                draw_search(ws, true),
                "Yesterday's word search, its words looped",
                "mini search small",
            ));
        }
    }
    out
}

/// The word search: its letters in a square frame, no rules between them (it's a page of
/// letters, not a form). With `answers`, each hidden word is looped in gray and the letters
/// outside every word are faint.
pub(crate) fn draw_search(ws: &search::Search, answers: bool) -> Vec<u8> {
    let font = FontRef::try_from_slice(crate::pdf::BOLD).expect("bundled font");
    let n = ws.size().max(1);
    let frame = if answers { 4.0 } else { 6.0 };
    let inner = if answers { 660.0 } else { 1100.0_f32 };
    let cell = inner / n as f32;
    let pad = cell * 0.25;
    let side = (inner + 2.0 * (frame + pad)).round() as u32;
    let mut c = Canvas::new(side, side);
    let origin = frame + pad;
    let center = |r: usize, col: usize| {
        (
            origin + cell * (col as f32 + 0.5),
            origin + cell * (r as f32 + 0.5),
        )
    };
    let mut in_word = vec![vec![!answers; n]; n];
    if answers {
        for h in &ws.words {
            let cells = ws.cells(h);
            if let (Some(&a), Some(&b)) = (cells.first(), cells.last()) {
                c.line(center(a.0, a.1), center(b.0, b.1), cell * 0.78, LOOP);
            }
            for (r, col) in cells {
                if let Some(x) = in_word.get_mut(r).and_then(|row| row.get_mut(col)) {
                    *x = true;
                }
            }
        }
    }
    let size = cell * 0.56;
    for (r, row) in ws.rows.iter().enumerate().take(n) {
        for (col, ch) in row.chars().enumerate().take(n) {
            let t = ch.to_uppercase().to_string();
            let (x, y) = center(r, col);
            let w = width_of(&font, size, &t);
            let ink = if in_word[r][col] { BLACK } else { 170 };
            c.text(&font, size, x - w / 2.0, y + size * 0.36, &t, ink);
        }
    }
    let s = side as f32;
    c.fill(0.0, 0.0, s, frame, BLACK);
    c.fill(0.0, s - frame, s, s, BLACK);
    c.fill(0.0, 0.0, frame, s, BLACK);
    c.fill(s - frame, 0.0, s, s, BLACK);
    png(c)
}

/// The grid drawn in black on white: black squares filled, clue numbers small in each
/// numbered square's corner, or (`letters`) each answer letter centered.
pub(crate) fn draw(rows: &[String; 5], letters: bool) -> Vec<u8> {
    let regular = FontRef::try_from_slice(crate::pdf::REGULAR).expect("bundled font");
    let bold = FontRef::try_from_slice(crate::pdf::BOLD).expect("bundled font");
    // Big enough to stay sharp at a Scribe's 300 ppi across most of its column.
    let (cell, rule, frame) = match letters {
        false => (200.0_f32, 3.0_f32, 8.0_f32),
        true => (120.0, 2.0, 6.0),
    };
    let n = rows.len();
    let side = (cell * n as f32 + frame * 2.0).round() as u32;
    let mut c = Canvas::new(side, side);
    let at = |k: usize| frame + cell * k as f32;
    let mut numbers = [[None; 5]; 5];
    for s in grid::slots_in(rows) {
        numbers[s.row][s.col] = Some(s.number);
    }
    for (r, row) in rows.iter().enumerate() {
        for (col, ch) in row.chars().enumerate().take(n) {
            let (x, y) = (at(col), at(r));
            if ch == '#' {
                c.fill(x, y, x + cell, y + cell, BLACK);
                continue;
            }
            if letters {
                let size = cell * 0.62;
                let t = ch.to_uppercase().to_string();
                let w = width_of(&bold, size, &t);
                c.text(
                    &bold,
                    size,
                    x + (cell - w) / 2.0,
                    y + cell * 0.72,
                    &t,
                    BLACK,
                );
            } else if let Some(num) = numbers[r][col] {
                let size = cell * 0.26;
                c.text(
                    &regular,
                    size,
                    x + cell * 0.07,
                    y + cell * 0.07 + size * 0.8,
                    &num.to_string(),
                    BLACK,
                );
            }
        }
    }
    // the rules between squares, then the heavier frame over their ends
    for k in 1..n {
        let p = at(k);
        c.fill(
            p - rule / 2.0,
            frame,
            p + rule / 2.0,
            side as f32 - frame,
            BLACK,
        );
        c.fill(
            frame,
            p - rule / 2.0,
            side as f32 - frame,
            p + rule / 2.0,
            BLACK,
        );
    }
    let s = side as f32;
    c.fill(0.0, 0.0, s, frame, BLACK);
    c.fill(0.0, s - frame, s, s, BLACK);
    c.fill(0.0, 0.0, frame, s, BLACK);
    c.fill(s - frame, 0.0, s, s, BLACK);
    png(c)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rows() -> [String; 5] {
        ["#ASAP", "#AERO", "STRAP", "ALSO#", "PEER#"].map(String::from)
    }

    #[test]
    fn a_grid_is_a_square_grayscale_png_with_black_squares() {
        for letters in [false, true] {
            let img = image::load_from_memory(&draw(&rows(), letters)).unwrap();
            assert_eq!(img.color(), image::ColorType::L8);
            assert_eq!(img.width(), img.height());
            let g = img.to_luma8();
            let cell = (g.width() as f32 / 5.0) as u32;
            // the middle of the top-left square is black, of the one beside it white or a letter
            assert_eq!(g.get_pixel(cell / 2, cell / 2).0[0], 0);
            // a plain square's bottom-right corner area is white
            let (x, y) = (cell * 3 - cell / 8, cell * 2 - cell / 8);
            assert_eq!(g.get_pixel(x, y).0[0], 255);
        }
    }

    fn search() -> search::Search {
        search::Search {
            rows: ["CATX", "XDOG", "XXXX", "XXXX"].map(String::from).to_vec(),
            words: vec![search::Hidden {
                word: "cat".into(),
                row: 0,
                col: 0,
                way: search::Way::Across,
                theme: false,
            }],
        }
    }

    #[test]
    fn a_word_search_is_square_and_loops_its_answers_only_when_asked() {
        let ws = search();
        let plain = image::load_from_memory(&draw_search(&ws, false))
            .unwrap()
            .to_luma8();
        let solved = image::load_from_memory(&draw_search(&ws, true))
            .unwrap()
            .to_luma8();
        assert_eq!(plain.width(), plain.height());
        assert_eq!(solved.width(), solved.height());
        // below the letters, between C and A: the loop's gray on the answers, white on today's
        let at = |g: &image::GrayImage| {
            let (side, n) = (g.width() as f32, 4.0);
            let (frame_pad, cell) = (side / (n + 0.5) * 0.25, side / (n + 0.5));
            let x = frame_pad + cell;
            let y = frame_pad + cell * 0.83;
            g.get_pixel(x as u32, y as u32).0[0]
        };
        assert_eq!(at(&plain), 255);
        assert_eq!(at(&solved), LOOP);
    }

    #[test]
    fn numbers_are_drawn_in_numbered_squares_only() {
        let g = image::load_from_memory(&draw(&rows(), false))
            .unwrap()
            .to_luma8();
        let cell = g.width() / 5;
        let inked = |r: u32, c: u32| {
            // the square's top-left quarter, clear of the rules
            (c * cell + cell / 20..c * cell + cell / 2).any(|x| {
                (r * cell + cell / 20..r * cell + cell / 2).any(|y| g.get_pixel(x, y).0[0] < 128)
            })
        };
        assert!(inked(0, 1), "1 across");
        assert!(inked(2, 0), "6 across");
        assert!(!inked(1, 2), "no number in the middle of 5 across");
    }
}
