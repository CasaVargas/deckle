// SPDX-License-Identifier: GPL-3.0-or-later
//! Nonograms (Puzzles tab design §2): shade the squares so each row and column has runs of
//! the lengths given, in order. The picture is one of Deckle's, or one of yours, thresholded.
//! Every puzzle a book prints solves by line logic alone, checked here.

use rand::Rng;
use rand::rngs::StdRng;
use serde::{Deserialize, Serialize};

/// Squares on a side a nonogram takes: 5 for a child, 25 for a long one.
pub const SIZES: std::ops::RangeInclusive<usize> = 5..=25;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Nonogram {
    /// Rows of squares, `true` shaded.
    pub cells: Vec<Vec<bool>>,
    /// What the picture is, printed with the answer ("Cat", or a photo's file name).
    pub title: String,
}

impl Nonogram {
    pub fn size(&self) -> usize {
        self.cells.len()
    }

    /// The run lengths of each row, left to right; an empty row is `[0]`.
    pub fn row_clues(&self) -> Vec<Vec<usize>> {
        self.cells.iter().map(|r| runs(r)).collect()
    }

    /// The run lengths of each column, top to bottom.
    pub fn col_clues(&self) -> Vec<Vec<usize>> {
        (0..self.size())
            .map(|c| runs(&self.cells.iter().map(|r| r[c]).collect::<Vec<_>>()))
            .collect()
    }

    /// Whether row and column logic alone fills every square: each line's possible fillings
    /// are intersected, round and round, until nothing changes.
    pub fn line_solvable(&self) -> bool {
        let n = self.size();
        let rows = self.row_clues();
        let cols = self.col_clues();
        // None: unknown; Some(true/false): decided
        let mut known: Vec<Vec<Option<bool>>> = vec![vec![None; n]; n];
        loop {
            let mut changed = false;
            for r in 0..n {
                let line: Vec<Option<bool>> = known[r].clone();
                if let Some(next) = settle(&rows[r], &line) {
                    for c in 0..n {
                        if known[r][c] != next[c] {
                            known[r][c] = next[c];
                            changed = true;
                        }
                    }
                } else {
                    return false;
                }
            }
            for c in 0..n {
                let line: Vec<Option<bool>> = (0..n).map(|r| known[r][c]).collect();
                if let Some(next) = settle(&cols[c], &line) {
                    for r in 0..n {
                        if known[r][c] != next[r] {
                            known[r][c] = next[r];
                            changed = true;
                        }
                    }
                } else {
                    return false;
                }
            }
            if !changed {
                break;
            }
        }
        known.iter().all(|r| r.iter().all(Option::is_some))
    }
}

fn runs(line: &[bool]) -> Vec<usize> {
    let mut out = Vec::new();
    let mut run = 0;
    for &c in line {
        if c {
            run += 1;
        } else if run > 0 {
            out.push(run);
            run = 0;
        }
    }
    if run > 0 {
        out.push(run);
    }
    if out.is_empty() {
        out.push(0);
    }
    out
}

/// What every filling of a line with `clues` that agrees with `known` has in common: a square
/// is decided when it's the same in all of them. `None` when no filling agrees.
fn settle(clues: &[usize], known: &[Option<bool>]) -> Option<Vec<Option<bool>>> {
    let n = known.len();
    let clues: Vec<usize> = clues.iter().copied().filter(|&c| c > 0).collect();
    let mut can_be = vec![[false; 2]; n];
    let mut any = false;
    let mut line = vec![false; n];
    place(&clues, 0, 0, known, &mut line, &mut can_be, &mut any);
    if !any {
        return None;
    }
    Some(
        can_be
            .iter()
            .map(|c| match (c[0], c[1]) {
                (true, false) => Some(false),
                (false, true) => Some(true),
                _ => None,
            })
            .collect(),
    )
}

/// Places clue `k` at or after `from`, trying each start; a full placement that agrees with
/// `known` marks what each square can be.
fn place(
    clues: &[usize],
    k: usize,
    from: usize,
    known: &[Option<bool>],
    line: &mut Vec<bool>,
    can_be: &mut [[bool; 2]],
    any: &mut bool,
) {
    let n = known.len();
    if k == clues.len() {
        // the rest is blank
        if (from..n).any(|i| known[i] == Some(true)) {
            return;
        }
        line[from..n].fill(false);
        for i in 0..n {
            can_be[i][line[i] as usize] = true;
        }
        *any = true;
        return;
    }
    let len = clues[k];
    // room for this and the clues after it
    let rest: usize = clues[k + 1..].iter().sum::<usize>() + clues.len() - k - 1;
    let mut start = from;
    while start + len + rest <= n {
        // blanks before it, the run, a blank after (unless the line ends)
        let fits = (from..start).all(|i| known[i] != Some(true))
            && (start..start + len).all(|i| known[i] != Some(false))
            && (start + len == n || known[start + len] != Some(true));
        if fits {
            line[from..start].fill(false);
            line[start..start + len].fill(true);
            if start + len < n {
                line[start + len] = false;
            }
            place(
                clues,
                k + 1,
                (start + len + 1).min(n),
                known,
                line,
                can_be,
                any,
            );
        }
        // a known shaded square before the run's start can't be skipped
        if known[start] == Some(true) {
            break;
        }
        start += 1;
    }
}

/// Deckle's own pictures, 15 squares a side, drawn in `#`.
pub const PICTOGRAMS: &[(&str, &str)] = &[
    (
        "Heart",
        "...............\
         ...###...###...\
         ..#####.#####..\
         .#############.\
         .#############.\
         .#############.\
         ..###########..\
         ...#########...\
         ....#######....\
         .....#####.....\
         ......###......\
         .......#.......\
         ...............\
         ...............\
         ...............",
    ),
    (
        "House",
        "...............\
         .......#.......\
         ......###......\
         .....#####.....\
         ....#######....\
         ...#########...\
         ..###########..\
         ..#.........#..\
         ..#..##.###.#..\
         ..#..##.#.#.#..\
         ..#.....###.#..\
         ..#.....#.#.#..\
         ..#.....#.#.#..\
         ..###########..\
         ...............",
    ),
    (
        "Cat",
        "..#.........#..\
         ..##.......##..\
         ..###.....###..\
         ..###########..\
         .#############.\
         .#.###.###.##.#\
         .###.#.#.###.#.\
         .#############.\
         ..####.#.####..\
         ...##..#..##...\
         ....#######....\
         ....#.#.#.#....\
         ....#######....\
         ...###...###...\
         ...............",
    ),
    (
        "Boat",
        "...............\
         .......#.......\
         ......##.......\
         .....###.......\
         ....####.......\
         ...#####.......\
         ..######.......\
         .#######.......\
         .......#.......\
         ###############\
         .#############.\
         ..###########..\
         ...#########...\
         ...............\
         ...............",
    ),
    (
        "Star",
        ".......#.......\
         .......#.......\
         ......###......\
         ......###......\
         .....#####.....\
         ###############\
         .#############.\
         ..###########..\
         ...#########...\
         ....#######....\
         ....#######....\
         ...####.####...\
         ...###...###...\
         ..##.......##..\
         ...............",
    ),
    (
        "Tree",
        ".......#.......\
         ......###......\
         .....#####.....\
         ....#######....\
         ......###......\
         ....#######....\
         ...#########...\
         ..###########..\
         .....#####.....\
         ...#########...\
         .#############.\
         ###############\
         .......#.......\
         ......###......\
         ....#######....",
    ),
    (
        "Fish",
        "...............\
         ...............\
         ......####.....\
         .....######....\
         ....########.#.\
         ...##.#######.#\
         ..###.########.\
         .############.#\
         ..###########.#\
         ...##########..\
         ....########...\
         .....######....\
         ......####.....\
         ...............\
         ...............",
    ),
    (
        "Bird",
        "...............\
         ....##.........\
         ...####........\
         ..#####........\
         ..######.......\
         ...######......\
         ....########...\
         ....##########.\
         ...############\
         ..#############\
         .############..\
         ...########....\
         .....###.......\
         ....#.#........\
         ...............",
    ),
];

/// A pictogram by name, or `None`.
pub fn pictogram(name: &str) -> Option<Nonogram> {
    PICTOGRAMS
        .iter()
        .find(|(n, _)| *n == name)
        .map(|(n, art)| from_art(n, art))
}

fn from_art(name: &str, art: &str) -> Nonogram {
    let flat: Vec<bool> = art
        .chars()
        .filter(|c| !c.is_whitespace())
        .map(|c| c == '#')
        .collect();
    let size = (flat.len() as f64).sqrt() as usize;
    Nonogram {
        cells: flat.chunks(size).map(<[bool]>::to_vec).collect(),
        title: name.to_owned(),
    }
}

/// `n` resized to `size` squares a side by the nearest square.
pub fn scaled(n: &Nonogram, size: usize) -> Nonogram {
    let from = n.size();
    let cells = (0..size)
        .map(|r| {
            (0..size)
                .map(|c| n.cells[r * from / size][c * from / size])
                .collect()
        })
        .collect();
    Nonogram {
        cells,
        title: n.title.clone(),
    }
}

/// A picture file (PNG, JPEG, GIF, WebP) as a `size`×`size` nonogram: greyscale, scaled to
/// fit, the darker half shaded. `title` is printed with the answer.
pub fn from_picture(bytes: &[u8], size: usize, title: &str) -> Result<Nonogram, String> {
    let img = deckle_core::image_cap::load(bytes).map_err(|e| format!("not a picture: {e}"))?;
    let grey = img.to_luma8();
    let (w, h) = (grey.width() as usize, grey.height() as usize);
    if w == 0 || h == 0 {
        return Err("an empty picture".into());
    }
    // the square in the middle, so a wide photo isn't squashed
    let side = w.min(h);
    let (ox, oy) = ((w - side) / 2, (h - side) / 2);
    let mut levels = vec![0u32; size * size];
    for r in 0..size {
        for c in 0..size {
            let (y0, y1) = (oy + r * side / size, oy + (r + 1) * side / size);
            let (x0, x1) = (ox + c * side / size, ox + (c + 1) * side / size);
            let mut sum = 0u64;
            let mut n = 0u64;
            for y in y0..y1.max(y0 + 1) {
                for x in x0..x1.max(x0 + 1) {
                    sum += grey.get_pixel(x as u32, y as u32).0[0] as u64;
                    n += 1;
                }
            }
            levels[r * size + c] = (sum / n.max(1)) as u32;
        }
    }
    // the darker half of the squares: a median split, so any photo shades about half
    let mut sorted = levels.clone();
    sorted.sort_unstable();
    let cut = sorted[sorted.len() / 2];
    let cells: Vec<Vec<bool>> = (0..size)
        .map(|r| (0..size).map(|c| levels[r * size + c] < cut).collect())
        .collect();
    let shaded = cells.iter().flatten().filter(|&&x| x).count();
    if shaded == 0 || shaded == size * size {
        return Err("the picture is all one shade".into());
    }
    Ok(Nonogram {
        cells,
        title: title.to_owned(),
    })
}

/// A random `size` nonogram that solves by line logic, from `rng`: about half shaded in
/// clumps, retried until it settles.
pub fn random(size: usize, rng: &mut StdRng) -> Nonogram {
    loop {
        let mut cells = vec![vec![false; size]; size];
        // a few random rectangles and lines, so the runs are long enough to reason about
        let strokes = size / 2 + 3;
        for _ in 0..strokes {
            let r = rng.random_range(0..size);
            let c = rng.random_range(0..size);
            let h = rng.random_range(1..=size / 3 + 1);
            let w = rng.random_range(1..=size / 3 + 1);
            for row in cells.iter_mut().take((r + h).min(size)).skip(r) {
                row[c..(c + w).min(size)].fill(true);
            }
        }
        let n = Nonogram {
            cells,
            title: "Shapes".to_owned(),
        };
        let shaded = n.cells.iter().flatten().filter(|&&x| x).count();
        if shaded * 10 < size * size * 3 || shaded * 10 > size * size * 7 {
            continue;
        }
        if n.line_solvable() {
            return n;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rand::SeedableRng;

    #[test]
    fn clues_are_run_lengths() {
        let n = from_art("t", "##.#.....");
        assert_eq!(n.size(), 3);
        assert_eq!(n.row_clues(), vec![vec![2], vec![1], vec![0]]);
        assert_eq!(n.col_clues(), vec![vec![2], vec![1], vec![0]]);
    }

    #[test]
    fn every_pictogram_is_square_and_solves_by_lines() {
        let mut guessy = Vec::new();
        for (name, art) in PICTOGRAMS {
            let n = from_art(name, art);
            assert_eq!(n.size(), 15, "{name}");
            assert!(n.cells.iter().all(|r| r.len() == 15), "{name}");
            if !n.line_solvable() {
                guessy.push(*name);
            }
        }
        assert!(guessy.is_empty(), "need a guess: {guessy:?}");
        assert!(PICTOGRAMS.len() >= 8);
        assert!(pictogram("Cat").is_some() && pictogram("Dragon").is_none());
    }

    #[test]
    fn an_ambiguous_grid_is_not_line_solvable() {
        // two shaded squares on a diagonal: the rows and columns all say "1", either diagonal fits
        let n = from_art("x", "#..#");
        assert!(!n.line_solvable());
        let n = from_art("x", "##.#");
        assert!(n.line_solvable());
    }

    #[test]
    fn scaling_keeps_the_picture() {
        let heart = pictogram("Heart").unwrap();
        let big = scaled(&heart, 30);
        assert_eq!(big.size(), 30);
        assert_eq!(scaled(&big, 15).cells, heart.cells);
    }

    #[test]
    fn random_ones_solve() {
        for seed in 0..4 {
            let n = random(10, &mut StdRng::seed_from_u64(seed));
            assert!(n.line_solvable());
        }
    }

    #[test]
    fn a_picture_becomes_a_grid() {
        // a 2×2 image: black top-left, white elsewhere
        let mut img = image::GrayImage::new(2, 2);
        img.put_pixel(0, 0, image::Luma([0]));
        img.put_pixel(1, 0, image::Luma([255]));
        img.put_pixel(0, 1, image::Luma([255]));
        img.put_pixel(1, 1, image::Luma([200]));
        let mut bytes = Vec::new();
        image::DynamicImage::ImageLuma8(img)
            .write_to(
                &mut std::io::Cursor::new(&mut bytes),
                image::ImageFormat::Png,
            )
            .unwrap();
        let n = from_picture(&bytes, 2, "test.png").unwrap();
        assert_eq!(n.cells, vec![vec![true, false], vec![false, true]]);
        assert!(from_picture(b"nope", 5, "x").is_err());
    }
}
