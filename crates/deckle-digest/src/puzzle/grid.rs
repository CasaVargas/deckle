// SPDX-License-Identifier: GPL-3.0-or-later
//! The 5×5 mini: symmetric templates, crossword numbering, and a backtracking fill
//! (puzzle design §4, "The grid").

use std::time::{Duration, Instant};

use rand::rngs::StdRng;
use rand::seq::SliceRandom;
use serde::{Deserialize, Serialize};

use super::words::Words;

/// A 5×5 layout: `#` is a black square, `.` a white one.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Template(pub [&'static str; 5]);

/// Standard mini layouts, each the same turned upside down, every entry 3 to 5 letters. The
/// all-white grid is left out: ten crossing 5-letter words fill from this list less than half
/// the time.
pub const TEMPLATES: &[Template] = &[
    Template(["#...#", ".....", ".....", ".....", "#...#"]),
    Template(["#....", ".....", ".....", ".....", "....#"]),
    Template(["##...", ".....", ".....", ".....", "...##"]),
    Template(["#....", "#....", ".....", "....#", "....#"]),
    Template(["...##", "....#", ".....", "#....", "##..."]),
];

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Dir {
    Across,
    Down,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Slot {
    pub number: u8,
    pub dir: Dir,
    pub row: usize,
    pub col: usize,
    pub len: usize,
}

impl Slot {
    /// The grid cell holding the slot's `k`th letter.
    pub fn cell(&self, k: usize) -> (usize, usize) {
        match self.dir {
            Dir::Across => (self.row, self.col + k),
            Dir::Down => (self.row + k, self.col),
        }
    }
}

/// The template's entries, numbered in standard crossword order: Across, then Down, each by
/// number.
pub fn slots(t: &Template) -> Vec<Slot> {
    let white = |r: usize, c: usize| r < 5 && c < 5 && t.0[r].as_bytes()[c] != b'#';
    let run = |mut r: usize, mut c: usize, dir: Dir| {
        let mut n = 0;
        while white(r, c) {
            n += 1;
            match dir {
                Dir::Across => c += 1,
                Dir::Down => r += 1,
            }
        }
        n
    };
    let mut out = Vec::new();
    let mut number = 0;
    for r in 0..5 {
        for c in 0..5 {
            if !white(r, c) {
                continue;
            }
            let across = (c == 0 || !white(r, c - 1)) && run(r, c, Dir::Across) > 1;
            let down = (r == 0 || !white(r - 1, c)) && run(r, c, Dir::Down) > 1;
            if !(across || down) {
                continue;
            }
            number += 1;
            for (starts, dir) in [(across, Dir::Across), (down, Dir::Down)] {
                if starts {
                    out.push(Slot {
                        number,
                        dir,
                        row: r,
                        col: c,
                        len: run(r, c, dir),
                    });
                }
            }
        }
    }
    out.sort_by_key(|s| (s.dir == Dir::Down, s.number));
    out
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Fill {
    /// Index into [`TEMPLATES`].
    pub template: usize,
    /// `#` for black squares, upper-case letters for the answers.
    pub rows: [String; 5],
    /// Each entry with its (lower-case) answer, in [`slots`] order.
    pub answers: Vec<(Slot, String)>,
}

/// Searches a template visits before giving up on it: the fill is the same on any machine
/// unless the time cap is reached first.
const NODE_BUDGET: u64 = 400_000;

/// Fills a mini: the templates in an order shuffled by `rng`, each searched for at most `cap`.
/// Seeds (theme words) go into the grid first, longest first; one that can't be placed is left
/// out.
pub fn fill(words: &Words, seeds: &[String], rng: &mut StdRng, cap: Duration) -> Option<Fill> {
    let mut order: Vec<usize> = (0..TEMPLATES.len()).collect();
    order.shuffle(rng);
    order
        .into_iter()
        .find_map(|t| fill_template(words, t, seeds, rng, Instant::now() + cap))
}

/// One template's fill, or `None` when it can't be done before `deadline`.
pub(crate) fn fill_template(
    words: &Words,
    template: usize,
    seeds: &[String],
    rng: &mut StdRng,
    deadline: Instant,
) -> Option<Fill> {
    let slots = slots(&TEMPLATES[template]);
    let mut seeds: Vec<&str> = seeds
        .iter()
        .map(String::as_str)
        .filter(|s| words.contains(s))
        .collect();
    seeds.sort_by_key(|s| std::cmp::Reverse(s.len()));
    seeds.dedup();
    let mut search = Search::new(words, &slots, rng, deadline);
    if !search.seed(&seeds) {
        return None;
    }
    let mut rows: [String; 5] = Default::default();
    for (r, row) in rows.iter_mut().enumerate() {
        *row = (0..5)
            .map(|c| match search.grid[r][c] {
                0 => '#',
                b => b.to_ascii_uppercase() as char,
            })
            .collect();
    }
    let answers = slots
        .iter()
        .zip(&search.chosen)
        .map(|(s, w)| (*s, search.lists[s.len][w.expect("filled")].clone()))
        .collect();
    Some(Fill {
        template,
        rows,
        answers,
    })
}

/// Backtracking over the slots, most constrained first. Candidates are bitsets indexed by
/// (length, position, letter).
struct Search<'a> {
    slots: &'a [Slot],
    lists: [Vec<String>; 6],
    /// `index[len][pos][letter]`: the words of `len` with `letter` at `pos`.
    index: [Vec<Vec<Vec<u64>>>; 6],
    /// Each length's words in the order this fill tries them.
    order: [Vec<usize>; 6],
    /// Words already in the grid, as bitsets like `index`'s.
    used: [Vec<u64>; 6],
    grid: [[u8; 5]; 5],
    chosen: Vec<Option<usize>>,
    nodes: u64,
    deadline: Instant,
    gave_up: bool,
}

impl<'a> Search<'a> {
    fn new(words: &Words, slots: &'a [Slot], rng: &mut StdRng, deadline: Instant) -> Self {
        let lists: [Vec<String>; 6] = std::array::from_fn(|n| words.of_len(n).to_vec());
        let index = std::array::from_fn(|n| {
            let blocks = lists[n].len().div_ceil(64);
            let mut idx = vec![vec![vec![0u64; blocks]; 26]; n];
            for (i, w) in lists[n].iter().enumerate() {
                for (p, b) in w.bytes().enumerate() {
                    idx[p][(b - b'a') as usize][i / 64] |= 1 << (i % 64);
                }
            }
            idx
        });
        let order = std::array::from_fn(|n| {
            let mut o: Vec<usize> = (0..lists[n].len()).collect();
            o.shuffle(rng);
            o
        });
        let used = std::array::from_fn(|n| vec![0u64; lists[n].len().div_ceil(64)]);
        // white squares start empty (b' '), black ones are 0
        let mut grid = [[0u8; 5]; 5];
        for s in slots {
            for k in 0..s.len {
                let (r, c) = s.cell(k);
                grid[r][c] = b' ';
            }
        }
        Search {
            slots,
            lists,
            index,
            order,
            used,
            grid,
            chosen: vec![None; slots.len()],
            nodes: 0,
            deadline,
            gave_up: false,
        }
    }

    /// Words that fit slot `i` as the grid stands and aren't used yet.
    fn candidates(&self, i: usize) -> Vec<u64> {
        let s = &self.slots[i];
        let mut bits: Vec<u64> = self.used[s.len].iter().map(|u| !u).collect();
        for k in 0..s.len {
            let (r, c) = s.cell(k);
            let b = self.grid[r][c];
            if b.is_ascii_lowercase() {
                for (x, y) in bits
                    .iter_mut()
                    .zip(&self.index[s.len][k][(b - b'a') as usize])
                {
                    *x &= y;
                }
            }
        }
        bits
    }

    fn fits(&self, bits: &[u64], len: usize, w: usize) -> bool {
        bits[w / 64] & (1 << (w % 64)) != 0 && self.used[len][w / 64] & (1 << (w % 64)) == 0
    }

    /// Writes word `w` into slot `i`, returning the squares it filled (to undo).
    fn place(&mut self, i: usize, w: usize) -> Vec<(usize, usize)> {
        let s = self.slots[i];
        let mut filled = Vec::new();
        for (k, b) in self.lists[s.len][w].bytes().enumerate() {
            let (r, c) = s.cell(k);
            if self.grid[r][c] == b' ' {
                self.grid[r][c] = b;
                filled.push((r, c));
            }
        }
        self.used[s.len][w / 64] |= 1 << (w % 64);
        self.chosen[i] = Some(w);
        filled
    }

    fn undo(&mut self, i: usize, filled: Vec<(usize, usize)>) {
        let s = self.slots[i];
        for (r, c) in filled {
            self.grid[r][c] = b' ';
        }
        if let Some(w) = self.chosen[i].take() {
            self.used[s.len][w / 64] &= !(1 << (w % 64));
        }
    }

    /// Places the seeds in turn, each in the first slot of its length that lets the rest fill,
    /// or nowhere.
    fn seed(&mut self, seeds: &[&str]) -> bool {
        let Some((seed, rest)) = seeds.split_first() else {
            return self.solve();
        };
        let Some(w) = self.lists[seed.len()].iter().position(|x| x == seed) else {
            return self.seed(rest);
        };
        for i in 0..self.slots.len() {
            if self.chosen[i].is_some() || self.slots[i].len != seed.len() {
                continue;
            }
            let bits = self.candidates(i);
            if !self.fits(&bits, seed.len(), w) {
                continue;
            }
            let filled = self.place(i, w);
            if self.seed(rest) {
                return true;
            }
            self.undo(i, filled);
            if self.gave_up {
                return false;
            }
        }
        self.seed(rest)
    }

    fn solve(&mut self) -> bool {
        // the open slot with the fewest candidates
        let mut best: Option<(usize, usize, Vec<u64>)> = None;
        for i in 0..self.slots.len() {
            if self.chosen[i].is_some() {
                continue;
            }
            let bits = self.candidates(i);
            let n: usize = bits.iter().map(|b| b.count_ones() as usize).sum();
            if n == 0 {
                return false;
            }
            if best.as_ref().is_none_or(|b| n < b.1) {
                best = Some((i, n, bits));
            }
        }
        let Some((i, _, bits)) = best else {
            return true; // every slot filled
        };
        let len = self.slots[i].len;
        for k in 0..self.order[len].len() {
            let w = self.order[len][k];
            if !self.fits(&bits, len, w) {
                continue;
            }
            self.nodes += 1;
            if self.nodes > NODE_BUDGET
                || (self.nodes.is_multiple_of(1000) && Instant::now() > self.deadline)
            {
                self.gave_up = true;
            }
            if self.gave_up {
                return false;
            }
            let filled = self.place(i, w);
            if self.solve() {
                return true;
            }
            self.undo(i, filled);
        }
        false
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::puzzle::words::Words;
    use rand::SeedableRng;
    use std::time::Instant;

    const CAP: Duration = Duration::from_millis(500);

    fn day_rng(day: u64) -> StdRng {
        // the dates of late 2026, as `num_days_from_ce` gives them
        StdRng::seed_from_u64(739_900 + day)
    }

    fn runs(cells: impl Iterator<Item = u8>) -> Vec<usize> {
        let mut out = Vec::new();
        let mut n = 0;
        for c in cells.chain(std::iter::once(b'#')) {
            if c == b'#' {
                if n > 0 {
                    out.push(n);
                }
                n = 0;
            } else {
                n += 1;
            }
        }
        out
    }

    #[test]
    fn templates_are_symmetric_and_have_no_short_runs() {
        assert!(TEMPLATES.len() >= 4);
        for (i, t) in TEMPLATES.iter().enumerate() {
            let cell = |r: usize, c: usize| t.0[r].as_bytes()[c];
            for r in 0..5 {
                assert_eq!(t.0[r].len(), 5, "template {i}");
                for c in 0..5 {
                    assert_eq!(cell(r, c), cell(4 - r, 4 - c), "template {i} ({r},{c})");
                }
                for len in runs((0..5).map(|c| cell(r, c)))
                    .into_iter()
                    .chain(runs((0..5).map(|c| cell(c, r))))
                {
                    assert!(len >= 3, "template {i}: a run of {len}");
                }
            }
        }
    }

    #[test]
    fn slots_number_like_a_crossword() {
        let open = Template([".....", ".....", ".....", ".....", "....."]);
        let s = slots(&open);
        let nums = |d: Dir| {
            s.iter()
                .filter(|x| x.dir == d)
                .map(|x| x.number)
                .collect::<Vec<_>>()
        };
        assert_eq!(nums(Dir::Across), [1, 6, 7, 8, 9]);
        assert_eq!(nums(Dir::Down), [1, 2, 3, 4, 5]);
        assert!(s.iter().all(|x| x.len == 5));
        let six = s.iter().find(|x| x.number == 6).unwrap();
        assert_eq!((six.row, six.col, six.dir), (1, 0, Dir::Across));
        // a corner template: 1 Across starts at column 1
        let corner = Template(["#....", ".....", ".....", ".....", "....#"]);
        let s = slots(&corner);
        let one = s
            .iter()
            .find(|x| x.number == 1 && x.dir == Dir::Across)
            .unwrap();
        assert_eq!((one.row, one.col, one.len), (0, 1, 4));
        let five = s.iter().find(|x| x.number == 5).unwrap();
        assert_eq!(
            (five.row, five.col, five.dir, five.len),
            (1, 0, Dir::Across, 5)
        );
    }

    fn check(words: &Words, f: &Fill) {
        let mut seen = std::collections::HashSet::new();
        for (slot, answer) in &f.answers {
            assert!(words.contains(answer), "{answer} not in the list");
            assert!(seen.insert(answer.clone()), "{answer} twice");
            assert_eq!(answer.len(), slot.len);
            for (k, ch) in answer.chars().enumerate() {
                let (r, c) = match slot.dir {
                    Dir::Across => (slot.row, slot.col + k),
                    Dir::Down => (slot.row + k, slot.col),
                };
                assert_eq!(f.rows[r].as_bytes()[c], ch.to_ascii_uppercase() as u8);
            }
        }
        let t = &TEMPLATES[f.template];
        assert_eq!(f.answers.len(), slots(t).len());
        for r in 0..5 {
            for c in 0..5 {
                let black = t.0[r].as_bytes()[c] == b'#';
                assert_eq!(f.rows[r].as_bytes()[c] == b'#', black);
            }
        }
    }

    #[test]
    fn fills_thirty_days_within_cap() {
        let words = Words::embedded();
        for day in 0..30 {
            let start = Instant::now();
            let f = fill(words, &[], &mut day_rng(day), CAP)
                .unwrap_or_else(|| panic!("day {day} didn't fill"));
            let took = start.elapsed();
            check(words, &f);
            // every template gets the cap at most, and the spec wants the puzzle in a second
            if !cfg!(debug_assertions) {
                assert!(took < Duration::from_secs(1), "day {day}: {took:?}");
            }
        }
    }

    #[test]
    fn every_template_fills() {
        let words = Words::embedded();
        // this is about the list, not the speed: a debug build is ten times slower
        let cap = if cfg!(debug_assertions) {
            CAP * 10
        } else {
            CAP
        };
        for t in 0..TEMPLATES.len() {
            let filled = (0..10).filter(|&d| {
                fill_template(words, t, &[], &mut day_rng(d), Instant::now() + cap).is_some()
            });
            assert!(filled.count() >= 8, "template {t} fills too rarely");
        }
    }

    #[test]
    fn deterministic() {
        let words = Words::embedded();
        let seeds = vec!["leaf".to_owned()];
        assert_eq!(
            fill(words, &seeds, &mut day_rng(3), CAP),
            fill(words, &seeds, &mut day_rng(3), CAP)
        );
    }

    #[test]
    fn seed_is_placed() {
        let words = Words::embedded();
        for day in 0..5 {
            let f = fill(words, &["leaf".to_owned()], &mut day_rng(day), CAP).unwrap();
            assert!(f.answers.iter().any(|(_, a)| a == "leaf"), "day {day}");
            check(words, &f);
        }
    }

    #[test]
    fn impossible_seed_still_fills() {
        let words = Words::embedded();
        let f = fill(words, &["zzzzz".to_owned()], &mut day_rng(1), CAP).unwrap();
        assert!(f.answers.iter().all(|(_, a)| a != "zzzzz"));
        check(words, &f);
    }

    #[test]
    fn tiny_list_gives_up() {
        let words = Words::from_tsv("card\tA piece of paper\nleaf\tA green organ\n");
        assert_eq!(fill(&words, &[], &mut day_rng(1), CAP), None);
    }
}
