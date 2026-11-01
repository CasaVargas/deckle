// SPDX-License-Identifier: GPL-3.0-or-later
//! Sudoku for puzzle books (puzzle books §3.4): a seeded full grid, givens taken away in
//! symmetric pairs while the puzzle stays unique and within its grade's techniques, and a
//! logical grader that says which techniques a puzzle needs.

use std::sync::OnceLock;

use rand::rngs::StdRng;
use rand::seq::SliceRandom;

/// 81 squares, row by row: 0 for an empty one, else 1–9.
pub type Grid = [u8; 81];

/// A puzzle and its one solution.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Sudoku {
    pub givens: Grid,
    pub solution: Grid,
}

/// Which techniques a puzzle needs (puzzle books §3.4).
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum Grade {
    /// Naked and hidden singles.
    Easy,
    /// Also naked and hidden pairs, pointing and claiming.
    Medium,
    /// More than those.
    Hard,
}

/// Givens an easy puzzle keeps, at least.
pub const EASY_GIVENS: usize = 36;
/// Full grids dug before [`make`] gives up on a grade.
const ATTEMPTS: usize = 200;

/// The 27 units (rows, columns, boxes), each its 9 squares.
fn units() -> &'static [[usize; 9]; 27] {
    static UNITS: OnceLock<[[usize; 9]; 27]> = OnceLock::new();
    UNITS.get_or_init(|| {
        std::array::from_fn(|u| {
            let k = u % 9;
            std::array::from_fn(|j| match u / 9 {
                0 => k * 9 + j,
                1 => j * 9 + k,
                _ => (k / 3 * 3 + j / 3) * 9 + k % 3 * 3 + j % 3,
            })
        })
    })
}

/// Each square's 20 peers: the squares sharing its row, column or box.
fn peers() -> &'static [[usize; 20]; 81] {
    static PEERS: OnceLock<[[usize; 20]; 81]> = OnceLock::new();
    PEERS.get_or_init(|| {
        std::array::from_fn(|i| {
            let mut out = [0; 20];
            let mut n = 0;
            for j in 0..81 {
                let same = j / 9 == i / 9
                    || j % 9 == i % 9
                    || (j / 27 == i / 27 && j % 9 / 3 == i % 9 / 3);
                if j != i && same {
                    out[n] = j;
                    n += 1;
                }
            }
            out
        })
    })
}

fn bit(d: u8) -> u16 {
    1 << d
}

const ALL: u16 = 0b11_1111_1110;

/// The digits `g` leaves open at each empty square, or `None` when two givens clash.
fn candidates(g: &Grid) -> Option<[u16; 81]> {
    let mut cand = [ALL; 81];
    for i in 0..81 {
        if g[i] != 0 {
            if cand[i] & bit(g[i]) == 0 {
                return None;
            }
            cand[i] = 0;
            for &p in &peers()[i] {
                if g[p] == g[i] {
                    return None;
                }
                cand[p] &= !bit(g[i]);
            }
        }
    }
    Some(cand)
}

/// Backtracking, the square with fewest digits left first, digits in `order`.
fn search(g: &mut Grid, order: &[u8; 9], limit: usize, found: &mut usize, keep: &mut Option<Grid>) {
    let mut best: Option<(usize, u16)> = None;
    for i in 0..81 {
        if g[i] != 0 {
            continue;
        }
        let mut open = ALL;
        for &p in &peers()[i] {
            if g[p] != 0 {
                open &= !bit(g[p]);
            }
        }
        if open == 0 {
            return;
        }
        if best.is_none_or(|(_, b)| open.count_ones() < b.count_ones()) {
            best = Some((i, open));
        }
    }
    let Some((i, open)) = best else {
        *found += 1;
        if keep.is_none() {
            *keep = Some(*g);
        }
        return;
    };
    for &d in order {
        if open & bit(d) != 0 {
            g[i] = d;
            search(g, order, limit, found, keep);
            g[i] = 0;
            if *found >= limit {
                return;
            }
        }
    }
}

/// How many solutions `g` has, counting no further than `limit`.
pub fn count_solutions(g: &Grid, limit: usize) -> usize {
    if candidates(g).is_none() {
        return 0;
    }
    let mut work = *g;
    let mut found = 0;
    search(
        &mut work,
        &[1, 2, 3, 4, 5, 6, 7, 8, 9],
        limit,
        &mut found,
        &mut None,
    );
    found
}

/// A full grid from `rng`.
pub fn full(rng: &mut StdRng) -> Grid {
    let mut order = [1, 2, 3, 4, 5, 6, 7, 8, 9];
    order.shuffle(rng);
    let mut g = [0u8; 81];
    // a shuffled first row, then the search fills the rest in a shuffled digit order
    g[..9].copy_from_slice(&order);
    order.shuffle(rng);
    let mut found = 0;
    let mut keep = None;
    search(&mut g, &order, 1, &mut found, &mut keep);
    keep.expect("a sudoku with one row set always completes")
}

/// A logical solver: digits placed, and the digits still open at each empty square.
struct Logic {
    val: Grid,
    cand: [u16; 81],
}

impl Logic {
    fn place(&mut self, i: usize, d: u8) {
        self.val[i] = d;
        self.cand[i] = 0;
        for &p in &peers()[i] {
            self.cand[p] &= !bit(d);
        }
    }

    /// Naked singles (a square with one digit left) and hidden singles (a digit with one
    /// square left in a unit). `None` on a contradiction.
    fn singles(&mut self) -> Option<bool> {
        let mut moved = false;
        loop {
            let mut step = false;
            for i in 0..81 {
                if self.val[i] == 0 {
                    match self.cand[i].count_ones() {
                        0 => return None,
                        1 => {
                            self.place(i, self.cand[i].trailing_zeros() as u8);
                            step = true;
                        }
                        _ => {}
                    }
                }
            }
            for unit in units() {
                for d in 1..=9u8 {
                    if unit.iter().any(|&i| self.val[i] == d) {
                        continue;
                    }
                    let mut at = unit.iter().filter(|&&i| self.cand[i] & bit(d) != 0);
                    match (at.next(), at.next()) {
                        (None, _) => return None,
                        (Some(&i), None) => {
                            self.place(i, d);
                            step = true;
                        }
                        _ => {}
                    }
                }
            }
            if !step {
                return Some(moved);
            }
            moved = true;
        }
    }

    /// The squares of `open` that can still take `d`.
    fn places(&self, open: &[usize], d: u8) -> Vec<usize> {
        open.iter()
            .copied()
            .filter(|&i| self.cand[i] & bit(d) != 0)
            .collect()
    }

    /// Takes `mask`'s digits from square `i`; whether any went.
    fn strike(&mut self, i: usize, mask: u16) -> bool {
        let before = self.cand[i];
        self.cand[i] &= !mask;
        self.cand[i] != before
    }

    /// Naked and hidden pairs, pointing and claiming: one round. Whether anything changed.
    fn pairs_and_lines(&mut self) -> bool {
        let mut moved = false;
        for unit in units() {
            let open: Vec<usize> = unit.iter().copied().filter(|&i| self.val[i] == 0).collect();
            // naked pair: two squares left with the same two digits
            for (a, &i) in open.iter().enumerate() {
                for &j in &open[a + 1..] {
                    let m = self.cand[i];
                    if m.count_ones() == 2 && self.cand[j] == m {
                        for &k in &open {
                            if k != i && k != j {
                                moved |= self.strike(k, m);
                            }
                        }
                    }
                }
            }
            // hidden pair: two digits that only two squares of the unit can take
            for d in 1..=9u8 {
                let pd = self.places(&open, d);
                if pd.len() != 2 {
                    continue;
                }
                for e in d + 1..=9 {
                    if self.places(&open, e) == pd {
                        let keep = bit(d) | bit(e);
                        for &i in &pd {
                            moved |= self.strike(i, !keep);
                        }
                    }
                }
            }
        }
        // pointing (a box's digit in one row or column) and claiming (a line's digit in one box)
        for b in 0..9 {
            let boxed = &units()[18 + b];
            for line in units()[..18].iter() {
                let inside: Vec<usize> =
                    line.iter().copied().filter(|i| boxed.contains(i)).collect();
                if inside.len() != 3 {
                    continue;
                }
                for d in 1..=9u8 {
                    let has = |i: &usize| self.val[*i] == 0 && self.cand[*i] & bit(d) != 0;
                    let in_both = inside.iter().any(has);
                    if !in_both {
                        continue;
                    }
                    let box_rest = boxed.iter().filter(|i| !inside.contains(i)).any(has);
                    let line_rest = line.iter().filter(|i| !inside.contains(i)).any(has);
                    if !box_rest && line_rest {
                        for &i in line.iter().filter(|i| !inside.contains(i)) {
                            moved |= self.strike(i, bit(d));
                        }
                    } else if !line_rest && box_rest {
                        for &i in boxed.iter().filter(|i| !inside.contains(i)) {
                            moved |= self.strike(i, bit(d));
                        }
                    }
                }
            }
        }
        moved
    }
}

/// `g` solved by logic alone, using the medium techniques when `medium`; `None` when they
/// get stuck.
fn logic(g: &Grid, medium: bool) -> Option<Grid> {
    let mut l = Logic {
        val: *g,
        cand: candidates(g)?,
    };
    loop {
        l.singles()?;
        if l.val.iter().all(|d| *d != 0) {
            return Some(l.val);
        }
        if !(medium && l.pairs_and_lines()) {
            return None;
        }
    }
}

fn solves(g: &Grid, medium: bool) -> bool {
    logic(g, medium).is_some()
}

/// The techniques a unique puzzle needs.
pub fn grade(g: &Grid) -> Grade {
    if solves(g, false) {
        Grade::Easy
    } else if solves(g, true) {
        Grade::Medium
    } else {
        Grade::Hard
    }
}

/// A puzzle of `want`'s grade: givens taken from a full grid in symmetric pairs, in an order
/// shuffled by `rng`, while the puzzle stays unique and (for easy and medium) within its
/// techniques. `None` when [`ATTEMPTS`] grids don't give one.
pub fn make(want: Grade, rng: &mut StdRng) -> Option<Sudoku> {
    for _ in 0..ATTEMPTS {
        let solution = full(rng);
        let mut givens = solution;
        let mut order: Vec<usize> = (0..=40).collect();
        order.shuffle(rng);
        let mut left = 81;
        for i in order {
            let pair = if i == 40 { 1 } else { 2 };
            if want == Grade::Easy && left - pair < EASY_GIVENS {
                continue;
            }
            let (a, b) = (givens[i], givens[80 - i]);
            givens[i] = 0;
            givens[80 - i] = 0;
            let fits = count_solutions(&givens, 2) == 1
                && match want {
                    Grade::Easy => solves(&givens, false),
                    Grade::Medium => solves(&givens, true),
                    Grade::Hard => true,
                };
            if fits {
                left -= pair;
            } else {
                givens[i] = a;
                givens[80 - i] = b;
            }
        }
        if grade(&givens) == want {
            return Some(Sudoku { givens, solution });
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use rand::SeedableRng;

    fn grid(s: &str) -> Grid {
        let mut g = [0u8; 81];
        for (i, b) in s.bytes().enumerate() {
            g[i] = b - b'0';
        }
        g
    }

    fn rng(seed: u64) -> StdRng {
        StdRng::seed_from_u64(seed)
    }

    /// Project Euler 96's first grid: naked and hidden singles finish it.
    const SINGLES: &str =
        "003020600900305001001806400008102900700000008006708200002609500800203009005010300";
    /// Arto Inkala's "hardest sudoku": far past pairs and pointing.
    const INKALA: &str =
        "800000000003600000070090200050007000000045700000100030001000068008500010090000400";

    fn is_solution(g: &Grid) -> bool {
        let unit = |cells: [usize; 9]| {
            let mut seen = [false; 10];
            cells.iter().all(|&i| {
                let d = g[i] as usize;
                (1..=9).contains(&d) && !std::mem::replace(&mut seen[d], true)
            })
        };
        (0..9).all(|k| {
            unit(std::array::from_fn(|j| k * 9 + j))
                && unit(std::array::from_fn(|j| j * 9 + k))
                && unit(std::array::from_fn(|j| {
                    (k / 3 * 3 + j / 3) * 9 + k % 3 * 3 + j % 3
                }))
        })
    }

    #[test]
    fn a_full_grid_is_a_sudoku() {
        for seed in 0..10 {
            assert!(is_solution(&full(&mut rng(seed))));
        }
    }

    #[test]
    fn count_finds_one_or_two() {
        assert_eq!(count_solutions(&grid(SINGLES), 2), 1);
        assert_eq!(count_solutions(&grid(INKALA), 2), 1);
        let mut open = grid(SINGLES);
        open[2] = 0;
        open[4] = 0;
        open[6] = 0;
        open[9] = 0;
        open[13] = 0;
        open[14] = 0;
        assert!(count_solutions(&open, 2) >= 1);
        assert_eq!(count_solutions(&[0; 81], 2), 2, "an empty grid has many");
    }

    #[test]
    fn grades_known_puzzles() {
        assert_eq!(grade(&grid(SINGLES)), Grade::Easy);
        assert_eq!(grade(&grid(INKALA)), Grade::Hard);
    }

    /// Every grade comes out unique, symmetric, true to its solution and of the grade asked.
    #[test]
    fn every_grade_makes_unique_symmetric_puzzles() {
        for want in [Grade::Easy, Grade::Medium, Grade::Hard] {
            for seed in 0..12 {
                let s =
                    make(want, &mut rng(seed)).unwrap_or_else(|| panic!("{want:?} seed {seed}"));
                assert!(is_solution(&s.solution));
                assert_eq!(count_solutions(&s.givens, 2), 1, "{want:?} {seed}");
                for i in 0..81 {
                    assert!(s.givens[i] == 0 || s.givens[i] == s.solution[i]);
                    assert_eq!(s.givens[i] == 0, s.givens[80 - i] == 0, "symmetric");
                }
                assert_eq!(grade(&s.givens), want, "seed {seed}");
                // the techniques that grade it also solve it, to the one solution
                if want != Grade::Hard {
                    assert_eq!(logic(&s.givens, want == Grade::Medium), Some(s.solution));
                }
                let givens = s.givens.iter().filter(|d| **d != 0).count();
                if want == Grade::Easy {
                    assert!(givens >= EASY_GIVENS, "{givens}");
                }
            }
        }
    }

    #[test]
    fn deterministic() {
        assert_eq!(
            make(Grade::Medium, &mut rng(5)),
            make(Grade::Medium, &mut rng(5))
        );
    }

    /// Attempts are counted, never timed: sixteen at once agree.
    #[test]
    fn a_busy_machine_makes_the_same_sudoku() {
        let alone = make(Grade::Hard, &mut rng(3));
        assert!(alone.is_some());
        let crowd: Vec<Option<Sudoku>> = std::thread::scope(|s| {
            let hs: Vec<_> = (0..16)
                .map(|_| s.spawn(|| make(Grade::Hard, &mut rng(3))))
                .collect();
            hs.into_iter().map(|h| h.join().unwrap()).collect()
        });
        assert!(crowd.iter().all(|c| *c == alone));
    }
}
