// SPDX-License-Identifier: GPL-3.0-or-later
//! The word search (word search design §4): a 9×9 grid hiding 8–10 words that read forward
//! only, across, down, or diagonally, with filler letters that never spell a blocklisted word.

use rand::Rng;
use rand::rngs::StdRng;
use rand::seq::SliceRandom;
use serde::{Deserialize, Serialize};
use std::collections::HashSet;

use super::words::{Words, blocklist};

/// Squares on a side.
pub const SIZE: usize = 9;
/// Whole placements tried before the day goes without a search. A count, never a clock: a busy
/// machine makes the same puzzle (word search design §3 asks for about 200 ms; this many
/// attempts take a few milliseconds).
pub const ATTEMPTS: usize = 40;
const MIN_WORDS: usize = 8;
const MAX_WORDS: usize = 10;
const MAX_THEMES: usize = 3;
/// List words tried for a place before the fill settles for fewer.
const TRIES: usize = 400;

/// The four ways a hidden word reads: forward only.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Way {
    Across,
    Down,
    DownRight,
    UpRight,
}

impl Way {
    pub const ALL: [Way; 4] = [Way::Across, Way::Down, Way::DownRight, Way::UpRight];

    /// (row, column) moved per letter.
    pub fn step(self) -> (isize, isize) {
        match self {
            Way::Across => (0, 1),
            Way::Down => (1, 0),
            Way::DownRight => (1, 1),
            Way::UpRight => (-1, 1),
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
    /// From yesterday's headlines.
    pub theme: bool,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Search {
    /// [`SIZE`] rows of [`SIZE`] upper-case letters.
    pub rows: Vec<String>,
    /// Sorted by word.
    pub words: Vec<Hidden>,
}

impl Search {
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

type Grid = [[u8; SIZE]; SIZE];

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
    let mut pool: Vec<&String> = words
        .of_len(5)
        .iter()
        .chain(words.of_len(4))
        .filter(|w| !avoid.contains(*w) && !themes.contains(w))
        .collect();
    if themes.len() + pool.len() < MIN_WORDS {
        return None;
    }
    for _ in 0..ATTEMPTS {
        pool.shuffle(rng);
        pool.sort_by_key(|w| std::cmp::Reverse(w.len())); // stable: longest first, shuffled within
        if let Some(s) = attempt(&themes, &pool, rng) {
            return Some(s);
        }
    }
    None
}

fn attempt(themes: &[&String], pool: &[&String], rng: &mut StdRng) -> Option<Search> {
    let mut grid: Grid = [[0; SIZE]; SIZE];
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
            let mut spots: Vec<(usize, usize, Way)> = (0..SIZE)
                .flat_map(|r| (0..SIZE).flat_map(move |c| Way::ALL.map(|w| (r, c, w))))
                .filter(|&(r, c, w)| fits(grid, word, r, c, w))
                .collect();
            spots.shuffle(rng);
            let Some(&(r, c, way)) = spots.first() else {
                return false;
            };
            let (dr, dc) = way.step();
            for (k, b) in word.bytes().enumerate() {
                let (rr, cc) = (r as isize + dr * k as isize, c as isize + dc * k as isize);
                grid[rr as usize][cc as usize] = b;
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
        if hidden.len() == MAX_WORDS {
            break;
        }
        place(w, false, &mut grid, &mut hidden, rng);
    }
    if hidden.len() < MIN_WORDS {
        return None;
    }
    let block = blocklist();
    let lens: Vec<usize> = {
        let mut l: Vec<usize> = block
            .iter()
            .map(String::len)
            .filter(|&n| n <= SIZE)
            .collect();
        l.sort_unstable();
        l.dedup();
        l
    };
    for r in 0..SIZE {
        for c in 0..SIZE {
            if grid[r][c] != 0 {
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
                grid[r][c] = letter;
                if !spells_blocked(&grid, r, c, &lens, block) {
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
    for r in 0..SIZE {
        for c in 0..SIZE {
            if spells_blocked(&grid, r, c, &lens, block) {
                return None;
            }
        }
    }
    // the filler can spell a hidden word a second time: the reader would circle the wrong copy
    if hidden.iter().any(|h| reads(&grid, &h.word) > 1) {
        return None;
    }
    hidden.sort_by(|a, b| a.word.cmp(&b.word));
    Some(Search {
        rows: grid
            .iter()
            .map(|row| row.iter().map(|b| b.to_ascii_uppercase() as char).collect())
            .collect(),
        words: hidden,
    })
}

/// How many times `word` reads forward in the grid, along any of the four ways.
fn reads(grid: &Grid, word: &str) -> usize {
    (0..SIZE)
        .flat_map(|r| (0..SIZE).flat_map(move |c| Way::ALL.map(|w| (r, c, w))))
        .filter(|&(r, c, way)| {
            let (dr, dc) = way.step();
            word.bytes().enumerate().all(|(k, b)| {
                let (rr, cc) = (r as isize + dr * k as isize, c as isize + dc * k as isize);
                (0..SIZE as isize).contains(&rr)
                    && (0..SIZE as isize).contains(&cc)
                    && grid[rr as usize][cc as usize] == b
            })
        })
        .count()
}

/// Whether `word` can go from `(r, c)` along `way`: inside the grid, on empty squares or the
/// same letters.
fn fits(grid: &Grid, word: &str, r: usize, c: usize, way: Way) -> bool {
    let (dr, dc) = way.step();
    word.bytes().enumerate().all(|(k, b)| {
        let (rr, cc) = (r as isize + dr * k as isize, c as isize + dc * k as isize);
        (0..SIZE as isize).contains(&rr)
            && (0..SIZE as isize).contains(&cc)
            && [0, b].contains(&grid[rr as usize][cc as usize])
    })
}

/// Whether a filled line through `(r, c)`, in any of the 8 directions and any blocklisted
/// length, spells a blocklisted word.
fn spells_blocked(
    grid: &Grid,
    r: usize,
    c: usize,
    lens: &[usize],
    block: &HashSet<String>,
) -> bool {
    let mut word = String::with_capacity(SIZE);
    for (dr, dc) in LINES {
        for &len in lens {
            for k in 0..len as isize {
                let (r0, c0) = (r as isize - dr * k, c as isize - dc * k);
                word.clear();
                let filled = (0..len as isize).all(|i| {
                    let (rr, cc) = (r0 + dr * i, c0 + dc * i);
                    if !(0..SIZE as isize).contains(&rr) || !(0..SIZE as isize).contains(&cc) {
                        return false;
                    }
                    let b = grid[rr as usize][cc as usize];
                    word.push(b as char);
                    b != 0
                });
                if filled && block.contains(&word) {
                    return true;
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
