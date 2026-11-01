// SPDX-License-Identifier: GPL-3.0-or-later
//! Crosswords of any size for puzzle books (puzzle books §3.3): symmetric block patterns and a
//! constraint fill over the long word list (`words-long.tsv.gz`). The Daily's 5×5 mini is
//! `grid.rs`, untouched by this.

use std::collections::{HashMap, HashSet, VecDeque};
use std::io::Read;
use std::sync::OnceLock;

use rand::rngs::StdRng;
use rand::seq::IndexedRandom;
use rand::{Rng, SeedableRng};

use super::grid::{Dir, Slot, TEMPLATES};
use super::store::Clue;

/// `word<TAB>zipf<TAB>clue` lines, 3 to 15 letters, built offline by `data/wordwise`.
static EMBEDDED: &[u8] = include_bytes!("../../assets/puzzle/words-long.tsv.gz");

/// Longest entry a list holds.
pub const MAX_LEN: usize = 15;

/// The crossword word list: words by length, each with its frequency (zipf) and a clue.
#[derive(Clone, Debug, Default)]
pub struct Lexicon {
    by_len: Vec<Vec<String>>,
    zipf: HashMap<String, f32>,
    clues: HashMap<String, String>,
}

impl Lexicon {
    /// The list built into the app, unpacked once.
    pub fn embedded() -> &'static Lexicon {
        static LEX: OnceLock<Lexicon> = OnceLock::new();
        LEX.get_or_init(|| {
            let mut text = String::new();
            flate2::read::GzDecoder::new(EMBEDDED)
                .read_to_string(&mut text)
                .expect("the embedded crossword word list");
            Lexicon::from_tsv(&text)
        })
    }

    /// `word<TAB>zipf<TAB>clue` lines; anything else (not 3–15 lowercase letters, no zipf, no
    /// clue) is skipped.
    pub fn from_tsv(text: &str) -> Lexicon {
        let mut lex = Lexicon {
            by_len: vec![Vec::new(); MAX_LEN + 1],
            ..Lexicon::default()
        };
        for line in text.lines() {
            let mut parts = line.splitn(3, '\t');
            let (Some(word), Some(zipf), Some(clue)) = (parts.next(), parts.next(), parts.next())
            else {
                continue;
            };
            let Ok(zipf) = zipf.parse::<f32>() else {
                continue;
            };
            if !(3..=MAX_LEN).contains(&word.len())
                || !word.bytes().all(|b| b.is_ascii_lowercase())
                || clue.trim().is_empty()
                || lex.clues.contains_key(word)
            {
                continue;
            }
            lex.by_len[word.len()].push(word.to_owned());
            lex.zipf.insert(word.to_owned(), zipf);
            lex.clues.insert(word.to_owned(), clue.trim().to_owned());
        }
        for list in &mut lex.by_len {
            list.sort_unstable();
        }
        lex
    }

    pub fn len(&self) -> usize {
        self.clues.len()
    }

    pub fn is_empty(&self) -> bool {
        self.clues.is_empty()
    }

    pub fn of_len(&self, n: usize) -> &[String] {
        self.by_len.get(n).map(Vec::as_slice).unwrap_or_default()
    }

    pub fn clue(&self, w: &str) -> Option<&str> {
        self.clues.get(w).map(String::as_str)
    }

    fn zipf(&self, w: &str) -> f32 {
        self.zipf.get(w).copied().unwrap_or(0.0)
    }
}

/// A filled crossword: `#` for black squares, upper-case letters, and each entry's clue in
/// [`slots`] order (Across, then Down, each by number).
#[derive(Clone, Debug, PartialEq)]
pub struct Crossword {
    pub rows: Vec<String>,
    pub clues: Vec<Clue>,
}

/// The entries of any square grid where `#` is black, numbered as `grid::slots_in` numbers a
/// mini: Across, then Down, each by number.
pub fn slots<S: AsRef<str>>(rows: &[S]) -> Vec<Slot> {
    let n = rows.len();
    let white = |r: usize, c: usize| r < n && c < n && rows[r].as_ref().as_bytes()[c] != b'#';
    let run = |mut r: usize, mut c: usize, dir: Dir| {
        let mut k = 0;
        while white(r, c) {
            k += 1;
            match dir {
                Dir::Across => c += 1,
                Dir::Down => r += 1,
            }
        }
        k
    };
    let mut out = Vec::new();
    let mut number = 0;
    for r in 0..n {
        for c in 0..n {
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

/// Black squares as a share of the grid. Tuned against the list: about a fifth black, and
/// entries of six letters at most, fill most patterns at every size; open grids with long
/// entries rarely fill from a list of everyday words.
const DENSITY: f64 = 0.20;
/// Longest entry a pattern keeps.
const LONGEST: usize = 6;

/// Whether `rows` (`true` black) keeps every white square in an across and a down entry of 3
/// or more, and every white square joined to every other.
fn sound(black: &[bool], n: usize) -> bool {
    let at = |r: usize, c: usize| black[r * n + c];
    for r in 0..n {
        for c in 0..n {
            if at(r, c) {
                continue;
            }
            let across = (0..c).rev().take_while(|&k| !at(r, k)).count()
                + 1
                + (c + 1..n).take_while(|&k| !at(r, k)).count();
            let down = (0..r).rev().take_while(|&k| !at(k, c)).count()
                + 1
                + (r + 1..n).take_while(|&k| !at(k, c)).count();
            if across < 3 || down < 3 {
                return false;
            }
        }
    }
    let whites = black.iter().filter(|b| !**b).count();
    let Some(start) = black.iter().position(|b| !b) else {
        return false;
    };
    let mut seen = vec![false; n * n];
    seen[start] = true;
    let mut queue = VecDeque::from([start]);
    let mut reached = 1;
    while let Some(i) = queue.pop_front() {
        let (r, c) = (i / n, i % n);
        let near = [
            (r > 0).then(|| i - n),
            (r + 1 < n).then(|| i + n),
            (c > 0).then(|| i - 1),
            (c + 1 < n).then(|| i + 1),
        ];
        for j in near.into_iter().flatten() {
            if !black[j] && !seen[j] {
                seen[j] = true;
                reached += 1;
                queue.push_back(j);
            }
        }
    }
    reached == whites
}

/// The longest run of white squares across or down, and a square in it.
fn longest_run(black: &[bool], n: usize) -> (usize, Vec<usize>) {
    let mut best = (0, Vec::new());
    for dir in [Dir::Across, Dir::Down] {
        for a in 0..n {
            let mut run: Vec<usize> = Vec::new();
            for b in 0..=n {
                let i = match dir {
                    Dir::Across => a * n + b,
                    Dir::Down => b * n + a,
                };
                if b < n && !black[i] {
                    run.push(i);
                    continue;
                }
                if run.len() > best.0 {
                    best = (run.len(), run.clone());
                }
                run.clear();
            }
        }
    }
    best
}

/// Tries at building one pattern before starting again.
const PATTERN_TRIES: usize = 4_000;

/// A block pattern for a `size` grid (`#` black, `.` white): turned upside down it's the same,
/// every white square is in an across and a down entry of 3 or more, and the white squares are
/// joined up. A 5×5 is one of the mini's templates.
pub fn pattern(size: usize, rng: &mut StdRng) -> Vec<String> {
    if size == 5 {
        let t = TEMPLATES.choose(rng).expect("templates");
        return t.0.iter().map(|r| r.to_string()).collect();
    }
    let n = size;
    let target = (DENSITY * (n * n) as f64).round() as usize;
    let max = LONGEST.min(n);
    loop {
        let mut black = vec![false; n * n];
        let mut blacks = 0;
        for _ in 0..PATTERN_TRIES {
            let (long, run) = longest_run(&black, n);
            if blacks >= target && long <= max {
                return black
                    .chunks(n)
                    .map(|r| r.iter().map(|b| if *b { '#' } else { '.' }).collect())
                    .collect();
            }
            // break the longest entry while it's too long, else anywhere
            let i = if long > max {
                *run.choose(rng).expect("a run")
            } else {
                rng.random_range(0..n * n)
            };
            let j = n * n - 1 - i;
            if black[i] {
                continue;
            }
            black[i] = true;
            black[j] = true;
            if sound(&black, n) {
                blacks += if i == j { 1 } else { 2 };
            } else {
                black[i] = false;
                black[j] = false;
            }
        }
    }
}

/// Placements one pattern's fill may try before a new pattern is drawn. A count, never a
/// clock: a busy machine makes the same puzzle.
const BUDGET: u64 = 20_000;
/// Words that fit an entry ranked by the room they leave, at most: the rest aren't tried.
const LOOKAHEAD: usize = 48;
/// Patterns tried before the fill gives up.
const PATTERNS: usize = 24;

/// A `size` crossword filled from `lex`, or `None` when [`PATTERNS`] patterns won't fill. Words
/// are tried commonest first, each moved down the order by up to `spread` zipf at random: a
/// small spread keeps to everyday words, a large one mixes in rarer ones. The same `rng` makes
/// the same crossword on any machine.
pub fn fill(lex: &Lexicon, size: usize, spread: f32, rng: &mut StdRng) -> Option<Crossword> {
    for _ in 0..PATTERNS {
        let rows = pattern(size, rng);
        let seed = rng.random::<u64>();
        if let Some(x) = fill_pattern(lex, &rows, spread, &mut StdRng::seed_from_u64(seed)) {
            return Some(x);
        }
    }
    None
}

/// Backtracking over a pattern's entries, the one with fewest words left first. Each entry
/// keeps the bitset of words that still fit it; placing a word narrows its crossings'.
struct Search<'a> {
    slots: Vec<Slot>,
    /// For each slot and position, the crossing slot and its position there.
    cross: Vec<Vec<Option<(usize, usize)>>>,
    /// Each length's words, in the order this fill tries them.
    lists: Vec<Vec<&'a str>>,
    /// `index[len][pos][letter]`: the words of `len` with `letter` at `pos`.
    index: Vec<Vec<Vec<Vec<u64>>>>,
    fits: Vec<Vec<u64>>,
    chosen: Vec<Option<usize>>,
    used: HashSet<&'a str>,
    steps: u64,
}

fn count(bits: &[u64]) -> u32 {
    bits.iter().map(|b| b.count_ones()).sum()
}

impl<'a> Search<'a> {
    fn new(lex: &'a Lexicon, rows: &[String], spread: f32, rng: &mut StdRng) -> Search<'a> {
        let slots = slots(rows);
        let n = rows.len();
        let mut owner: HashMap<((usize, usize), Dir), (usize, usize)> = HashMap::new();
        for (i, s) in slots.iter().enumerate() {
            for k in 0..s.len {
                owner.insert((s.cell(k), s.dir), (i, k));
            }
        }
        let cross = slots
            .iter()
            .map(|s| {
                let other = match s.dir {
                    Dir::Across => Dir::Down,
                    Dir::Down => Dir::Across,
                };
                (0..s.len)
                    .map(|k| owner.get(&(s.cell(k), other)).copied())
                    .collect()
            })
            .collect();
        // common words first, shuffled among their like
        let lists: Vec<Vec<&str>> = (0..=n.min(MAX_LEN))
            .map(|len| {
                let mut l: Vec<(f32, &str)> = lex
                    .of_len(len)
                    .iter()
                    .map(|w| (lex.zipf(w) - rng.random::<f32>() * spread, w.as_str()))
                    .collect();
                l.sort_by(|a, b| b.0.total_cmp(&a.0).then(a.1.cmp(b.1)));
                l.into_iter().map(|(_, w)| w).collect()
            })
            .collect();
        let index = lists
            .iter()
            .enumerate()
            .map(|(len, words)| {
                let blocks = words.len().div_ceil(64);
                let mut idx = vec![vec![vec![0u64; blocks]; 26]; len];
                for (i, w) in words.iter().enumerate() {
                    for (p, b) in w.bytes().enumerate() {
                        idx[p][(b - b'a') as usize][i / 64] |= 1 << (i % 64);
                    }
                }
                idx
            })
            .collect();
        let fits = slots
            .iter()
            .map(|s| {
                let k = lists.get(s.len).map_or(0, Vec::len);
                let mut bits = vec![u64::MAX; k.div_ceil(64)];
                if k % 64 != 0 {
                    *bits.last_mut().expect("a block") = (1u64 << (k % 64)) - 1;
                }
                bits
            })
            .collect();
        Search {
            chosen: vec![None; slots.len()],
            slots,
            cross,
            lists,
            index,
            fits,
            used: HashSet::new(),
            steps: 0,
        }
    }

    /// The fewest words any open crossing of slot `i` would keep with word `w` in it; `None`
    /// when one would keep none.
    fn room(&self, i: usize, w: usize) -> Option<u32> {
        let word = self.lists[self.slots[i].len][w].as_bytes();
        let mut least = u32::MAX;
        for (k, b) in word.iter().enumerate() {
            let Some((j, p)) = self.cross[i][k] else {
                continue;
            };
            if self.chosen[j].is_some() {
                continue;
            }
            let idx = &self.index[self.slots[j].len][p][(b - b'a') as usize];
            let n: u32 = self.fits[j]
                .iter()
                .zip(idx)
                .map(|(x, y)| (x & y).count_ones())
                .sum();
            if n == 0 {
                return None;
            }
            least = least.min(n);
        }
        Some(least)
    }

    fn solve(&mut self) -> bool {
        let open = (0..self.slots.len())
            .filter(|&i| self.chosen[i].is_none())
            .min_by_key(|&i| (count(&self.fits[i]), i));
        let Some(i) = open else {
            return true;
        };
        let len = self.slots[i].len;
        let words: Vec<usize> = self.fits[i]
            .iter()
            .enumerate()
            .flat_map(|(b, bits)| {
                (0..64)
                    .filter(move |k| bits & (1 << k) != 0)
                    .map(move |k| b * 64 + k)
            })
            .collect();
        // of the first few that fit, the ones leaving their crossings most room go first
        let mut ranked: Vec<(u32, usize)> = words
            .into_iter()
            .filter(|&w| !self.used.contains(self.lists[len][w]))
            .take(LOOKAHEAD)
            .filter_map(|w| self.room(i, w).map(|r| (r, w)))
            .collect();
        ranked.sort_by(|a, b| b.0.cmp(&a.0).then(a.1.cmp(&b.1)));
        for (_, w) in ranked {
            let word = self.lists[len][w];
            self.steps += 1;
            if self.steps > BUDGET {
                return false;
            }
            // narrow each open crossing to the words with this word's letter there
            let mut saved: Vec<(usize, Vec<u64>)> = Vec::new();
            let mut dead = false;
            for (k, b) in word.bytes().enumerate() {
                let Some((j, p)) = self.cross[i][k] else {
                    continue;
                };
                if self.chosen[j].is_some() {
                    continue;
                }
                let jl = self.slots[j].len;
                saved.push((j, self.fits[j].clone()));
                for (x, y) in self.fits[j]
                    .iter_mut()
                    .zip(&self.index[jl][p][(b - b'a') as usize])
                {
                    *x &= y;
                }
                if count(&self.fits[j]) == 0 {
                    dead = true;
                    break;
                }
            }
            if !dead {
                self.chosen[i] = Some(w);
                self.used.insert(word);
                if self.solve() {
                    return true;
                }
                self.chosen[i] = None;
                self.used.remove(word);
            }
            for (j, bits) in saved.into_iter().rev() {
                self.fits[j] = bits;
            }
            if self.steps > BUDGET {
                return false;
            }
        }
        false
    }
}

fn fill_pattern(
    lex: &Lexicon,
    rows: &[String],
    spread: f32,
    rng: &mut StdRng,
) -> Option<Crossword> {
    let mut search = Search::new(lex, rows, spread, rng);
    if !search.solve() {
        return None;
    }
    let mut grid: Vec<Vec<u8>> = rows.iter().map(|r| r.bytes().collect()).collect();
    let mut clues = Vec::with_capacity(search.slots.len());
    for (s, w) in search.slots.iter().zip(&search.chosen) {
        let word = search.lists[s.len][w.expect("filled")];
        for (k, b) in word.bytes().enumerate() {
            let (r, c) = s.cell(k);
            grid[r][c] = b.to_ascii_uppercase();
        }
        clues.push(Clue {
            number: s.number,
            dir: s.dir,
            answer: word.to_owned(),
            clue: lex.clue(word).expect("a list word").to_owned(),
            theme: false,
        });
    }
    Some(Crossword {
        rows: grid
            .into_iter()
            .map(|r| String::from_utf8(r).expect("ascii"))
            .collect(),
        clues,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::puzzle::grid::{self, TEMPLATES};
    use rand::SeedableRng;
    use std::collections::{HashSet, VecDeque};

    fn rng(seed: u64) -> StdRng {
        StdRng::seed_from_u64(seed)
    }

    #[test]
    fn slots_match_the_minis() {
        for t in TEMPLATES {
            let rows: Vec<String> = t.0.iter().map(|r| r.to_string()).collect();
            assert_eq!(slots(&rows), grid::slots(t));
        }
    }

    /// Symmetric, every white square in an across and a down entry of 3 or more, and all the
    /// white squares joined up.
    fn assert_valid(rows: &[String]) {
        let n = rows.len();
        let at = |r: usize, c: usize| rows[r].as_bytes()[c];
        for r in 0..n {
            assert_eq!(rows[r].len(), n);
            for c in 0..n {
                assert_eq!(
                    at(r, c) == b'#',
                    at(n - 1 - r, n - 1 - c) == b'#',
                    "{rows:?}"
                );
            }
        }
        let slots = slots(rows);
        assert!(slots.iter().all(|s| s.len >= 3), "{rows:?}");
        let mut covered = HashSet::new();
        for s in &slots {
            for k in 0..s.len {
                covered.insert((s.cell(k), s.dir));
            }
        }
        let whites: Vec<(usize, usize)> = (0..n)
            .flat_map(|r| (0..n).map(move |c| (r, c)))
            .filter(|&(r, c)| at(r, c) != b'#')
            .collect();
        for &w in &whites {
            assert!(covered.contains(&(w, grid::Dir::Across)), "{w:?} {rows:?}");
            assert!(covered.contains(&(w, grid::Dir::Down)), "{w:?} {rows:?}");
        }
        let mut seen = HashSet::from([whites[0]]);
        let mut queue = VecDeque::from([whites[0]]);
        while let Some((r, c)) = queue.pop_front() {
            let near = [
                (r.wrapping_sub(1), c),
                (r + 1, c),
                (r, c.wrapping_sub(1)),
                (r, c + 1),
            ];
            for (rr, cc) in near {
                if rr < n && cc < n && at(rr, cc) != b'#' && seen.insert((rr, cc)) {
                    queue.push_back((rr, cc));
                }
            }
        }
        assert_eq!(seen.len(), whites.len(), "joined up: {rows:?}");
    }

    #[test]
    fn patterns_are_valid() {
        for size in 5..=15 {
            for seed in 0..6 {
                let rows = pattern(size, &mut rng(seed));
                assert_eq!(rows.len(), size);
                assert_valid(&rows);
            }
        }
    }

    /// Every size fills from the whole list: each entry a list word with its clue, none
    /// twice, and the grid's letters are the answers'.
    #[test]
    fn fills_every_size() {
        let lex = Lexicon::embedded();
        for size in 5..=15 {
            for seed in 0..2 {
                let x = fill(lex, size, 1.5, &mut rng(seed))
                    .unwrap_or_else(|| panic!("size {size} seed {seed}"));
                assert_eq!(x.rows.len(), size);
                let pattern: Vec<String> = x
                    .rows
                    .iter()
                    .map(|r| {
                        r.chars()
                            .map(|c| if c == '#' { '#' } else { '.' })
                            .collect()
                    })
                    .collect();
                assert_valid(&pattern);
                let slots = slots(&x.rows);
                assert_eq!(x.clues.len(), slots.len());
                let mut seen = HashSet::new();
                for (s, c) in slots.iter().zip(&x.clues) {
                    assert_eq!((s.number, s.dir), (c.number, c.dir));
                    let read: String = (0..s.len)
                        .map(|k| {
                            let (r, cc) = s.cell(k);
                            x.rows[r].as_bytes()[cc].to_ascii_lowercase() as char
                        })
                        .collect();
                    assert_eq!(read, c.answer);
                    assert_eq!(lex.clue(&c.answer), Some(c.clue.as_str()));
                    assert!(!c.clue.to_lowercase().contains(&c.answer), "{c:?}");
                    assert!(seen.insert(c.answer.clone()), "{} twice", c.answer);
                }
            }
        }
    }

    #[test]
    fn deterministic() {
        let lex = Lexicon::embedded();
        assert_eq!(
            fill(lex, 11, 1.5, &mut rng(4)),
            fill(lex, 11, 1.5, &mut rng(4))
        );
    }

    /// The fill counts steps, never time: sixteen fills at once all agree.
    #[test]
    fn a_busy_machine_makes_the_same_fill() {
        let lex = Lexicon::embedded();
        let alone = fill(lex, 13, 1.5, &mut rng(2));
        assert!(alone.is_some());
        let crowd: Vec<Option<Crossword>> = std::thread::scope(|s| {
            let hs: Vec<_> = (0..16)
                .map(|_| s.spawn(move || fill(lex, 13, 1.5, &mut rng(2))))
                .collect();
            hs.into_iter().map(|h| h.join().unwrap()).collect()
        });
        assert!(crowd.iter().all(|c| *c == alone));
    }

    /// Spec §7's sweep: every size fills for 30 seeds at each difficulty's spread. Slow in a
    /// debug build, so run on demand: `cargo test --release -p deckle-digest --lib
    /// sweep_every_size_and_spread -- --ignored --nocapture`.
    #[test]
    #[ignore]
    fn sweep_every_size_and_spread() {
        let lex = Lexicon::embedded();
        for size in 5..=15 {
            let mut line = format!("{size:>2}×{size:<2}");
            for spread in [0.5, 1.5, 4.0] {
                let made = (0..30)
                    .filter(|&seed| fill(lex, size, spread, &mut rng(seed)).is_some())
                    .count();
                line += &format!("  spread {spread}: {made}/30");
                assert_eq!(made, 30, "size {size} spread {spread}");
            }
            println!("{line}");
        }
    }
}
