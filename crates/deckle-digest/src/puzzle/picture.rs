// SPDX-License-Identifier: GPL-3.0-or-later
//! The mini's grid as a picture for the EPUB. A Kindle sets any HTML table as a full-width,
//! zoomable table: the squares stretch into wide boxes and a zoom button sits under them, so
//! the crossword reads as a spreadsheet. A PNG keeps the squares square on every reader.
//! The PDF edition still draws the grid from the table, in vector.

use ab_glyph::FontRef;

use super::grid;
use crate::cover::{Canvas, png, width_of};

const BLACK: u8 = 0;

/// A grid printed as a picture: the table the section's HTML holds, and what replaces it.
pub(crate) struct GridPicture {
    /// The `<table>` exactly as the section prints it.
    pub table: String,
    /// Where the PNG goes in the EPUB, relative to the OPF.
    pub href: String,
    pub png: Vec<u8>,
    alt: String,
    class: &'static str,
}

impl GridPicture {
    /// The XHTML that takes the table's place.
    pub fn html(&self) -> String {
        format!(
            "<div class=\"{}\"><img src=\"{}\" alt=\"{}\"/></div>\n",
            self.class,
            self.href,
            crate::esc(&self.alt)
        )
    }
}

/// Today's grid (numbers) and yesterday's answers (letters), each beside the table it
/// replaces, named for `section` (its index in the edition).
pub(crate) fn pictures(p: &super::PuzzleData, section: usize) -> Vec<GridPicture> {
    let mut out = Vec::new();
    if let Some(rows) = &p.today.rows {
        out.push(GridPicture {
            table: super::grid_table(rows, "grid", false),
            href: format!("images/s{}-mini.png", section + 1),
            png: draw(rows, false),
            alt: format!("The Mini · No. {}, a 5 by 5 grid", p.today.number),
            class: "mini",
        });
    }
    if let Some(rows) = p.yesterday.as_ref().and_then(|y| y.rows.as_ref()) {
        out.push(GridPicture {
            table: super::grid_table(rows, "grid small", true),
            href: format!("images/s{}-mini-answers.png", section + 1),
            png: draw(rows, true),
            alt: format!("Yesterday's grid: {}", rows.join(" / ").replace('#', "■")),
            class: "mini small",
        });
    }
    out
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
