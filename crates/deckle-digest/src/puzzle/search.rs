// SPDX-License-Identifier: GPL-3.0-or-later
//! The word search (word search design §4): a 9×9 grid hiding 8–10 words that read forward
//! only, across, down, or diagonally, with filler letters that never spell a blocklisted word.
//! Puzzle books (puzzle books §3.2) ask for other sizes and directions through [`Options`].

use rand::Rng;
use rand::rngs::StdRng;
use rand::seq::SliceRandom;
use serde::{Deserialize, Serialize};
use std::collections::HashSet;

use super::words::{Words, blocklist};

/// Squares on a side of the Daily's search.
pub const SIZE: usize = 9;
/// Whole placements tried before the day goes without a search. A count, never a clock: a busy
/// machine makes the same puzzle (word search design §3 asks for about 200 ms; this many
/// attempts take a few milliseconds).
pub const ATTEMPTS: usize = 40;
const MAX_THEMES: usize = 3;
/// List words tried for a place before the fill settles for fewer.
const TRIES: usize = 400;

/// The ways a hidden word reads. The Daily uses the four forward ones ([`Way::ALL`]); a hard
/// puzzle book adds the four backward ones ([`Way::EVERY`]).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Way {
    Across,
    Down,
    DownRight,
    UpRight,
    Left,
    Up,
    UpLeft,
    DownLeft,
}

impl Way {
    /// The forward ways, the Daily's.
    pub const ALL: [Way; 4] = [Way::Across, Way::Down, Way::DownRight, Way::UpRight];
    /// All eight.
    pub const EVERY: [Way; 8] = [
        Way::Across,
        Way::Down,
        Way::DownRight,
        Way::UpRight,
        Way::Left,
        Way::Up,
        Way::UpLeft,
        Way::DownLeft,
    ];

    /// (row, column) moved per letter.
    pub fn step(self) -> (isize, isize) {
        match self {
            Way::Across => (0, 1),
            Way::Down => (1, 0),
            Way::DownRight => (1, 1),
            Way::UpRight => (-1, 1),
            Way::Left => (0, -1),
            Way::Up => (-1, 0),
            Way::UpLeft => (-1, -1),
            Way::DownLeft => (1, -1),
        }
    }
}

/// A word hidden in the grid, lowercase, from its first letter.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Hidden {
    pub word: String,
    pub row: u8,
    pub col: u8,
    pub way: Way,
    /// From yesterday's headlines, or a book's own words.
    pub theme: bool,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Search {
    /// Rows of upper-case letters, as many as each row is long.
    pub rows: Vec<String>,
    /// Sorted by word.
    pub words: Vec<Hidden>,
}

impl Search {
    /// Squares on a side.
    pub fn size(&self) -> usize {
        self.rows.len()
    }

    /// The squares `h` covers, first letter first.
    pub fn cells(&self, h: &Hidden) -> Vec<(usize, usize)> {
        let (dr, dc) = h.way.step();
        (0..h.word.len() as isize)
            .map(|k| {
                (
                    (h.row as isize + dr * k) as usize,
                    (h.col as isize + dc * k) as usize,
                )
            })
            .collect()
    }
}

/// What a fill makes: the grid's size, the ways words may read, and how many words to hide.
#[derive(Clone, Copy, Debug)]
pub struct Options<'a> {
    pub size: usize,
    pub ways: &'a [Way],
    /// Words hidden, at most.
    pub max: usize,
    /// Words hidden, at least, or the attempt fails.
    pub min: usize,
    /// A puzzle book's fill (puzzle books §3.2). A blocklisted string inside one hidden word
    /// (CLASS) isn't the filler's doing and is allowed; a word whose reverse is hidden is
    /// skipped when words read backwards; and the filler never spells a hidden word again.
    /// The Daily's fill stays as it was, so its puzzles don't move.
    pub book: bool,
}

impl Options<'_> {
    /// The Daily's: 9×9, forward, 8 to 10 words.
    pub const DAILY: Options<'static> = Options {
        size: SIZE,
        ways: &Way::ALL,
        max: 10,
        min: 8,
        book: false,
    };
}

/// Every direction a line reads in, the four ways and their reverses.
const LINES: [(isize, isize); 8] = [
    (0, 1),
    (1, 0),
    (1, 1),
    (-1, 1),
    (0, -1),
    (-1, 0),
    (-1, -1),
    (1, -1),
];

/// English letter frequency, per mille rounded, for the filler.
const WEIGHTS: [(u8, u32); 26] = [
    (b'e', 127),
    (b't', 91),
    (b'a', 82),
    (b'o', 75),
    (b'i', 70),
    (b'n', 67),
    (b's', 63),
    (b'h', 61),
    (b'r', 60),
    (b'd', 43),
    (b'l', 40),
    (b'c', 28),
    (b'u', 28),
    (b'm', 24),
    (b'w', 24),
    (b'f', 22),
    (b'g', 20),
    (b'y', 20),
    (b'p', 19),
    (b'b', 15),
    (b'v', 10),
    (b'k', 8),
    (b'j', 2),
    (b'x', 2),
    (b'q', 1),
    (b'z', 1),
];

/// A square grid of lowercase letters, 0 for an empty square.
struct Grid {
    size: usize,
    cells: Vec<u8>,
}

impl Grid {
    fn new(size: usize) -> Grid {
        Grid {
            size,
            cells: vec![0; size * size],
        }
    }

    /// The letter at `(r, c)`, or `None` off the grid.
    fn at(&self, r: isize, c: isize) -> Option<u8> {
        let n = self.size as isize;
        ((0..n).contains(&r) && (0..n).contains(&c)).then(|| self.cells[(r * n + c) as usize])
    }

    fn set(&mut self, r: usize, c: usize, b: u8) {
        self.cells[r * self.size + c] = b;
    }
}

/// A word search: up to three `themes` first (4–5 letters, in the list, not in `avoid`), then
/// list words, 8 to 10 in all; `None` when that many can't be placed and filled in
/// [`ATTEMPTS`] tries. The same inputs and `rng` make the same search, on any machine.
pub fn fill(
    words: &Words,
    themes: &[String],
    avoid: &HashSet<String>,
    rng: &mut StdRng,
) -> Option<Search> {
    let mut themes: Vec<&String> = themes
        .iter()
        .filter(|t| (4..=5).contains(&t.len()) && words.contains(t) && !avoid.contains(*t))
        .collect();
    themes.dedup();
    themes.truncate(MAX_THEMES);
    let pool: Vec<String> = words
        .of_len(5)
        .iter()
        .chain(words.of_len(4))
        .filter(|w| !avoid.contains(*w) && !themes.contains(w))
        .cloned()
        .collect();
    let themes: Vec<String> = themes.into_iter().cloned().collect();
    fill_with(&themes, &pool, &Options::DAILY, rng)
}

/// A search of `opts`: every word of `first` tried first (marked as theme words), then words
/// of `pool`, longest first, until `opts.max` are hidden; `None` when fewer than `opts.min` can
/// be placed and filled in [`ATTEMPTS`] tries. Callers check the words: lowercase a–z, none
/// longer than the grid, none in both lists.
pub fn fill_with(
    first: &[String],
    pool: &[String],
    opts: &Options,
    rng: &mut StdRng,
) -> Option<Search> {
    let first: Vec<&String> = first.iter().collect();
    let mut pool: Vec<&String> = pool.iter().collect();
    if first.len() + pool.len() < opts.min {
        return None;
    }
    for _ in 0..ATTEMPTS {
        pool.shuffle(rng);
        pool.sort_by_key(|w| std::cmp::Reverse(w.len())); // stable: longest first, shuffled within
        if let Some(s) = attempt(&first, &pool, opts, rng) {
            return Some(s);
        }
    }
    None
}

fn attempt(
    themes: &[&String],
    pool: &[&String],
    opts: &Options,
    rng: &mut StdRng,
) -> Option<Search> {
    let size = opts.size;
    let backwards = opts.book && opts.ways.iter().any(|w| !Way::ALL.contains(w));
    let mut grid = Grid::new(size);
    let mut hidden: Vec<Hidden> = Vec::new();
    let place =
        |word: &str, theme: bool, grid: &mut Grid, hidden: &mut Vec<Hidden>, rng: &mut StdRng| {
            // a word inside another (TOTE in TOTEM) would be found twice
            if hidden
                .iter()
                .any(|h| h.word.contains(word) || word.contains(h.word.as_str()))
            {
                return false;
            }
            // read backwards, STAR is RATS: one would be found on the other's squares
            if backwards {
                let rev: String = word.chars().rev().collect();
                if hidden
                    .iter()
                    .any(|h| h.word.contains(&rev) || rev.contains(h.word.as_str()))
                {
                    return false;
                }
            }
            let mut spots: Vec<(usize, usize, Way)> = (0..size)
                .flat_map(|r| {
                    (0..size).flat_map(move |c| opts.ways.iter().map(move |&w| (r, c, w)))
                })
                .filter(|&(r, c, w)| fits(grid, word, r, c, w))
                .collect();
            spots.shuffle(rng);
            let Some(&(r, c, way)) = spots.first() else {
                return false;
            };
            let (dr, dc) = way.step();
            for (k, b) in word.bytes().enumerate() {
                let (rr, cc) = (r as isize + dr * k as isize, c as isize + dc * k as isize);
                grid.set(rr as usize, cc as usize, b);
            }
            hidden.push(Hidden {
                word: word.to_owned(),
                row: r as u8,
                col: c as u8,
                way,
                theme,
            });
            true
        };
    let mut themes = themes.to_vec();
    themes.sort_by_key(|t| std::cmp::Reverse(t.len()));
    for t in themes {
        place(t, true, &mut grid, &mut hidden, rng);
    }
    for w in pool.iter().take(TRIES) {
        if hidden.len() >= opts.max {
            break;
        }
        place(w, false, &mut grid, &mut hidden, rng);
    }
    if hidden.len() < opts.min {
        return None;
    }
    let block = blocklist();
    let lens: Vec<usize> = {
        let mut l: Vec<usize> = block
            .iter()
            .map(String::len)
            .filter(|&n| n <= size)
            .collect();
        l.sort_unstable();
        l.dedup();
        l
    };
    // a book's own words may hold a blocklisted string (CLASS): that isn't the filler's doing
    let own: Vec<Vec<usize>> = if opts.book {
        hidden
            .iter()
            .map(|h| {
                let (dr, dc) = h.way.step();
                (0..h.word.len() as isize)
                    .map(|k| {
                        (h.row as isize + dr * k) as usize * size
                            + (h.col as isize + dc * k) as usize
                    })
                    .collect()
            })
            .collect()
    } else {
        Vec::new()
    };
    for r in 0..size {
        for c in 0..size {
            if grid.cells[r * size + c] != 0 {
                continue;
            }
            let mut letters: Vec<(u8, u32)> = WEIGHTS.to_vec();
            let mut ok = false;
            while !letters.is_empty() {
                let total: u32 = letters.iter().map(|l| l.1).sum();
                let mut pick = rng.random_range(0..total);
                let i = letters
                    .iter()
                    .position(|l| {
                        if pick < l.1 {
                            true
                        } else {
                            pick -= l.1;
                            false
                        }
                    })
                    .expect("pick is under the total");
                let (letter, _) = letters.swap_remove(i);
                grid.set(r, c, letter);
                if !spells_blocked(&grid, r, c, &lens, block, &own)
                    && !(opts.book && spells_hidden(&grid, r, c, &hidden, opts.ways))
                {
                    ok = true;
                    break;
                }
            }
            if !ok {
                return None;
            }
        }
    }
    // two hidden words crossing can spell one with no filler square: check every square once
    for r in 0..size {
        for c in 0..size {
            if spells_blocked(&grid, r, c, &lens, block, &own) {
                return None;
            }
        }
    }
    // the filler can spell a hidden word a second time: the reader would circle the wrong copy
    if hidden.iter().any(|h| reads(&grid, &h.word, opts.ways) > 1) {
        return None;
    }
    hidden.sort_by(|a, b| a.word.cmp(&b.word));
    Some(Search {
        rows: grid
            .cells
            .chunks(size)
            .map(|row| row.iter().map(|b| b.to_ascii_uppercase() as char).collect())
            .collect(),
        words: hidden,
    })
}

/// How many places `word` reads in the grid, along any of `ways`. A palindrome read both ways
/// on the same squares is one place.
fn reads(grid: &Grid, word: &str, ways: &[Way]) -> usize {
    let size = grid.size;
    let last = word.len() as isize - 1;
    (0..size)
        .flat_map(|r| (0..size).flat_map(move |c| ways.iter().map(move |&w| (r, c, w))))
        .filter(|&(r, c, way)| {
            let (dr, dc) = way.step();
            word.bytes().enumerate().all(|(k, b)| {
                grid.at(r as isize + dr * k as isize, c as isize + dc * k as isize) == Some(b)
            })
        })
        .map(|(r, c, way)| {
            let (dr, dc) = way.step();
            let ends = (
                (r as isize, c as isize),
                (r as isize + dr * last, c as isize + dc * last),
            );
            (ends.0.min(ends.1), ends.0.max(ends.1))
        })
        .collect::<HashSet<_>>()
        .len()
}

/// Whether a filled line through `(r, c)` along one of `ways` spells a hidden word: a second
/// copy, which the reader might circle instead.
fn spells_hidden(grid: &Grid, r: usize, c: usize, hidden: &[Hidden], ways: &[Way]) -> bool {
    hidden.iter().any(|h| {
        let len = h.word.len() as isize;
        ways.iter().any(|w| {
            let (dr, dc) = w.step();
            (0..len).any(|k| {
                let (r0, c0) = (r as isize - dr * k, c as isize - dc * k);
                h.word
                    .bytes()
                    .enumerate()
                    .all(|(i, b)| grid.at(r0 + dr * i as isize, c0 + dc * i as isize) == Some(b))
            })
        })
    })
}

/// Whether `word` can go from `(r, c)` along `way`: inside the grid, on empty squares or the
/// same letters.
fn fits(grid: &Grid, word: &str, r: usize, c: usize, way: Way) -> bool {
    let (dr, dc) = way.step();
    word.bytes().enumerate().all(|(k, b)| {
        grid.at(r as isize + dr * k as isize, c as isize + dc * k as isize)
            .is_some_and(|g| g == 0 || g == b)
    })
}

/// Whether a filled line through `(r, c)`, in any of the 8 directions and any blocklisted
/// length, spells a blocklisted word, other than inside one of the `own` words' squares.
fn spells_blocked(
    grid: &Grid,
    r: usize,
    c: usize,
    lens: &[usize],
    block: &HashSet<String>,
    own: &[Vec<usize>],
) -> bool {
    let mut word = String::with_capacity(grid.size);
    for (dr, dc) in LINES {
        for &len in lens {
            for k in 0..len as isize {
                let (r0, c0) = (r as isize - dr * k, c as isize - dc * k);
                word.clear();
                let filled = (0..len as isize).all(|i| match grid.at(r0 + dr * i, c0 + dc * i) {
                    Some(b) => {
                        word.push(b as char);
                        b != 0
                    }
                    None => false,
                });
                if filled && block.contains(&word) {
                    let inside = |cells: &Vec<usize>| {
                        (0..len as isize).all(|i| {
                            let (rr, cc) = (r0 + dr * i, c0 + dc * i);
                            cells.contains(&(rr as usize * grid.size + cc as usize))
                        })
                    };
                    if !own.iter().any(inside) {
                        return true;
                    }
                }
            }
        }
    }
    false
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::puzzle::words::{Words, blocklist};
    use rand::SeedableRng;
    use rand::rngs::StdRng;
    use std::collections::HashSet;
    use std::time::Instant;

    fn day_rng(day: u64) -> StdRng {
        StdRng::seed_from_u64(739_900 + day)
    }

    /// The letters along `way` from `(row, col)`, `len` of them, if they fit.
    fn read(
        rows: &[String],
        row: isize,
        col: isize,
        (dr, dc): (isize, isize),
        len: usize,
    ) -> Option<String> {
        (0..len as isize)
            .map(|k| {
                let (r, c) = (row + dr * k, col + dc * k);
                (r >= 0 && c >= 0 && (r as usize) < SIZE && (c as usize) < SIZE)
                    .then(|| rows[r as usize].as_bytes()[c as usize] as char)
            })
            .collect()
    }

    fn sixty() -> Vec<Search> {
        let words = Words::embedded();
        (0..60)
            .map(|d| fill(words, &[], &HashSet::new(), &mut day_rng(d)).expect("fills"))
            .collect()
    }

    #[test]
    fn placed_words_read_back() {
        let words = Words::embedded();
        for s in sixty() {
            assert_eq!(s.rows.len(), SIZE);
            assert!(
                s.rows
                    .iter()
                    .all(|r| r.len() == SIZE && r.bytes().all(|b| b.is_ascii_uppercase()))
            );
            assert!((8..=10).contains(&s.words.len()), "{}", s.words.len());
            let mut seen = HashSet::new();
            for h in &s.words {
                assert!(
                    (4..=5).contains(&h.word.len()) && words.contains(&h.word),
                    "{}",
                    h.word
                );
                assert!(seen.insert(h.word.clone()), "{} twice", h.word);
                let got = read(
                    &s.rows,
                    h.row as isize,
                    h.col as isize,
                    h.way.step(),
                    h.word.len(),
                );
                assert_eq!(
                    got.as_deref(),
                    Some(h.word.to_uppercase().as_str()),
                    "{h:?}"
                );
            }
        }
    }

    /// Filler letters can spell a hidden word again; the reader would circle the wrong copy.
    /// The first such grid in the seeded days is past day 680, so this walks 800.
    #[test]
    fn each_hidden_word_reads_once() {
        let words = Words::embedded();
        for d in 0..800 {
            let s = fill(words, &[], &HashSet::new(), &mut day_rng(d)).expect("fills");
            for h in &s.words {
                let want = h.word.to_uppercase();
                let mut found = 0;
                for r in 0..SIZE as isize {
                    for c in 0..SIZE as isize {
                        for way in Way::ALL {
                            if read(&s.rows, r, c, way.step(), want.len()).as_deref() == Some(&want)
                            {
                                found += 1;
                            }
                        }
                    }
                }
                assert_eq!(
                    found, 1,
                    "day {d}: {want} reads {found} times in {:?}",
                    s.rows
                );
            }
        }
    }

    #[test]
    fn no_blocklisted_word_in_any_line() {
        let block = blocklist();
        let steps = [
            (0, 1),
            (1, 0),
            (1, 1),
            (-1, 1),
            (0, -1),
            (-1, 0),
            (-1, -1),
            (1, -1),
        ];
        for s in sixty() {
            for r in 0..SIZE as isize {
                for c in 0..SIZE as isize {
                    for step in steps {
                        for len in 3..=5 {
                            if let Some(w) = read(&s.rows, r, c, step, len) {
                                assert!(!block.contains(&w.to_lowercase()), "{w} in {:?}", s.rows);
                            }
                        }
                    }
                }
            }
        }
    }

    #[test]
    fn themes_go_in_and_avoid_stays_out() {
        let avoid: HashSet<String> = ["totem", "anime"].map(String::from).into();
        let s = fill(
            Words::embedded(),
            &["court".into(), "state".into()],
            &avoid,
            &mut day_rng(3),
        )
        .expect("fills");
        for theme in ["court", "state"] {
            assert!(
                s.words.iter().any(|h| h.word == theme && h.theme),
                "{theme}"
            );
        }
        assert!(s.words.iter().filter(|h| h.theme).count() == 2);
        assert!(s.words.iter().all(|h| !avoid.contains(&h.word)));
    }

    #[test]
    fn deterministic() {
        let words = Words::embedded();
        let a = fill(words, &["court".into()], &HashSet::new(), &mut day_rng(9));
        let b = fill(words, &["court".into()], &HashSet::new(), &mut day_rng(9));
        assert!(a.is_some());
        assert_eq!(a, b);
    }

    /// A loaded machine (a load average of 130 while other builds ran) made a different
    /// puzzle, or none, when a wall clock cut the fill short. Sixteen fills of the same day at
    /// once must all agree.
    #[test]
    fn a_busy_machine_makes_the_same_search() {
        let words = Words::embedded();
        for d in 0..20 {
            let alone = fill(words, &[], &HashSet::new(), &mut day_rng(d));
            assert!(alone.is_some());
            let crowd: Vec<Option<Search>> = std::thread::scope(|s| {
                let hs: Vec<_> = (0..16)
                    .map(|_| s.spawn(move || fill(words, &[], &HashSet::new(), &mut day_rng(d))))
                    .collect();
                hs.into_iter().map(|h| h.join().unwrap()).collect()
            });
            assert!(crowd.iter().all(|c| *c == alone), "day {d}");
        }
    }

    fn fnv(text: &str) -> u64 {
        text.bytes().fold(0xcbf2_9ce4_8422_2325, |h, b| {
            (h ^ b as u64).wrapping_mul(0x0100_0000_01b3)
        })
    }

    /// The Daily's searches, frozen before the generator took options (puzzle books §5.1):
    /// sixty days and a themed day hash the same as they did.
    #[test]
    fn daily_output_is_unchanged() {
        let words = Words::embedded();
        let mut all = String::new();
        for d in 0..60 {
            all += &format!("{:?}", fill(words, &[], &HashSet::new(), &mut day_rng(d)));
        }
        let avoid: HashSet<String> = ["totem"].map(String::from).into();
        all += &format!(
            "{:?}",
            fill(
                words,
                &["court".into(), "state".into()],
                &avoid,
                &mut day_rng(3)
            )
        );
        assert_eq!(fnv(&all), 0x0027_5806_7bba_6e66, "{:#x}", fnv(&all));
    }

    /// The letters along `(dr, dc)` from `(row, col)` in a `size`-square grid, if they fit.
    fn read_in(
        rows: &[String],
        size: usize,
        (row, col): (isize, isize),
        (dr, dc): (isize, isize),
        len: usize,
    ) -> Option<String> {
        (0..len as isize)
            .map(|k| {
                let (r, c) = (row + dr * k, col + dc * k);
                (r >= 0 && c >= 0 && (r as usize) < size && (c as usize) < size)
                    .then(|| rows[r as usize].as_bytes()[c as usize] as char)
            })
            .collect()
    }

    fn book_pool() -> Vec<String> {
        let words = Words::embedded();
        words
            .of_len(5)
            .iter()
            .chain(words.of_len(4))
            .cloned()
            .collect()
    }

    /// Every book size at every set of directions fills, and each word reads once along the
    /// directions allowed (puzzle books §3.2).
    #[test]
    fn sizes_and_ways_fill() {
        let block = blocklist();
        let pool = book_pool();
        let easy = [Way::Across, Way::Down];
        for size in 8..=20usize {
            let want = (size * size / 9).clamp(8, 40);
            let min = (want * 4).div_ceil(5).max(8);
            for ways in [&easy[..], &Way::ALL[..], &Way::EVERY[..]] {
                let opts = Options {
                    size,
                    ways,
                    max: want,
                    min,
                    book: true,
                };
                for seed in 0..10 {
                    let s = fill_with(&[], &pool, &opts, &mut StdRng::seed_from_u64(seed))
                        .unwrap_or_else(|| panic!("size {size} ways {} seed {seed}", ways.len()));
                    assert_eq!(s.rows.len(), size);
                    assert!(
                        s.rows
                            .iter()
                            .all(|r| r.len() == size && r.bytes().all(|b| b.is_ascii_uppercase()))
                    );
                    assert!((min..=want).contains(&s.words.len()), "{}", s.words.len());
                    for h in &s.words {
                        assert!(ways.contains(&h.way), "{h:?}");
                        let want = h.word.to_uppercase();
                        // places, not readings: a palindrome reads both ways on its squares
                        let mut found = HashSet::new();
                        for r in 0..size as isize {
                            for c in 0..size as isize {
                                for w in ways {
                                    let (dr, dc) = w.step();
                                    if read_in(&s.rows, size, (r, c), (dr, dc), want.len())
                                        .as_deref()
                                        == Some(&want)
                                    {
                                        let last = want.len() as isize - 1;
                                        let ends = ((r, c), (r + dr * last, c + dc * last));
                                        found.insert((ends.0.min(ends.1), ends.0.max(ends.1)));
                                    }
                                }
                            }
                        }
                        assert_eq!(found.len(), 1, "size {size} seed {seed}: {want} {found:?}");
                    }
                    let squares: Vec<HashSet<(isize, isize)>> = s
                        .words
                        .iter()
                        .map(|h| {
                            s.cells(h)
                                .into_iter()
                                .map(|(r, c)| (r as isize, c as isize))
                                .collect()
                        })
                        .collect();
                    for r in 0..size as isize {
                        for c in 0..size as isize {
                            for w in Way::EVERY {
                                let (dr, dc) = w.step();
                                for len in 3..=5 {
                                    let Some(x) = read_in(&s.rows, size, (r, c), (dr, dc), len)
                                    else {
                                        continue;
                                    };
                                    // a book allows one inside a hidden word (NIGHT), nowhere else
                                    let inside = squares.iter().any(|sq| {
                                        (0..len as isize)
                                            .all(|k| sq.contains(&(r + dr * k, c + dc * k)))
                                    });
                                    assert!(inside || !block.contains(&x.to_lowercase()), "{x}");
                                }
                            }
                        }
                    }
                }
            }
        }
    }

    #[test]
    fn easy_uses_only_across_and_down() {
        let opts = Options {
            size: 12,
            ways: &[Way::Across, Way::Down],
            max: 16,
            min: 13,
            book: true,
        };
        for seed in 0..20 {
            let s = fill_with(&[], &book_pool(), &opts, &mut StdRng::seed_from_u64(seed)).unwrap();
            assert!(
                s.words
                    .iter()
                    .all(|h| matches!(h.way, Way::Across | Way::Down))
            );
        }
    }

    #[test]
    fn first_words_go_in_whatever_their_length() {
        let first: Vec<String> = ["keyboard", "router", "pixel"].map(String::from).into();
        let opts = Options {
            size: 12,
            ways: &Way::EVERY,
            max: 16,
            min: 13,
            book: true,
        };
        let s = fill_with(&first, &book_pool(), &opts, &mut StdRng::seed_from_u64(1)).unwrap();
        for w in &first {
            assert!(s.words.iter().any(|h| &h.word == w && h.theme), "{w}");
        }
    }

    #[test]
    fn a_list_too_small_gives_up_at_once() {
        let small = Words::from_tsv("cold\tx\ncord\tx\ncard\tx\nward\tx\nwarm\tx\n");
        let start = Instant::now();
        assert_eq!(fill(&small, &[], &HashSet::new(), &mut day_rng(1)), None);
        assert!(
            start.elapsed() < std::time::Duration::from_secs(1),
            "{:?}",
            start.elapsed()
        );
    }
}
