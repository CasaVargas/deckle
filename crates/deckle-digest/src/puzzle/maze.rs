// SPDX-License-Identifier: GPL-3.0-or-later
//! Mazes (Puzzles tab design §2): a perfect maze (one way between any two squares), in from
//! the top-left, out at the bottom-right, with its one solution for the answers.

use rand::Rng;
use rand::rngs::StdRng;
use serde::{Deserialize, Serialize};

/// Squares on a side a maze takes: 6 for a small child, 40 for a long evening.
pub const SIZES: std::ops::RangeInclusive<usize> = 6..=40;

pub const N: u8 = 1;
pub const E: u8 = 2;
pub const S: u8 = 4;
pub const W: u8 = 8;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Maze {
    pub size: usize,
    /// Each square's standing walls, row by row: bits [`N`], [`E`], [`S`], [`W`].
    pub walls: Vec<u8>,
    /// The squares from the entrance (top-left) to the exit (bottom-right), both included.
    pub solution: Vec<(usize, usize)>,
}

impl Maze {
    pub fn at(&self, r: usize, c: usize) -> u8 {
        self.walls[r * self.size + c]
    }

    /// The entrance is a gap in the top wall of the first square; the exit a gap in the
    /// bottom wall of the last.
    pub fn entrance(&self) -> (usize, usize) {
        (0, 0)
    }

    pub fn exit(&self) -> (usize, usize) {
        (self.size - 1, self.size - 1)
    }
}

/// A `size`×`size` maze from `rng`: a growing-tree carve that mostly follows the newest
/// square (long twisting corridors) and sometimes a random one (branches), so the mazes of a
/// book don't all look alike.
pub fn make(size: usize, rng: &mut StdRng) -> Maze {
    let n = size * size;
    let mut walls = vec![N | E | S | W; n];
    let mut seen = vec![false; n];
    let mut active = vec![0usize];
    seen[0] = true;
    while !active.is_empty() {
        let pick = if rng.random_ratio(3, 4) {
            active.len() - 1
        } else {
            rng.random_range(0..active.len())
        };
        let cell = active[pick];
        let (r, c) = (cell / size, cell % size);
        let mut dirs = [
            (N, S, r.wrapping_sub(1), c),
            (E, W, r, c + 1),
            (S, N, r + 1, c),
            (W, E, r, c.wrapping_sub(1)),
        ];
        // shuffle the four
        for i in (1..4).rev() {
            let j = rng.random_range(0..=i);
            dirs.swap(i, j);
        }
        let next = dirs
            .iter()
            .find(|(_, _, nr, nc)| *nr < size && *nc < size && !seen[nr * size + nc]);
        match next {
            Some(&(out, back, nr, nc)) => {
                walls[cell] &= !out;
                walls[nr * size + nc] &= !back;
                seen[nr * size + nc] = true;
                active.push(nr * size + nc);
            }
            None => {
                active.swap_remove(pick);
            }
        }
    }
    // the entrance and the exit
    walls[0] &= !N;
    walls[n - 1] &= !S;
    let mut m = Maze {
        size,
        walls,
        solution: Vec::new(),
    };
    m.solution = solve(&m);
    m
}

/// The one path from the entrance to the exit, by breadth-first search.
fn solve(m: &Maze) -> Vec<(usize, usize)> {
    let size = m.size;
    let n = size * size;
    let mut prev = vec![usize::MAX; n];
    let mut queue = std::collections::VecDeque::from([0usize]);
    prev[0] = 0;
    while let Some(cell) = queue.pop_front() {
        if cell == n - 1 {
            break;
        }
        let (r, c) = (cell / size, cell % size);
        let w = m.walls[cell];
        let steps = [
            (N, r.wrapping_sub(1), c),
            (E, r, c + 1),
            (S, r + 1, c),
            (W, r, c.wrapping_sub(1)),
        ];
        for (wall, nr, nc) in steps {
            if w & wall == 0 && nr < size && nc < size {
                let next = nr * size + nc;
                if prev[next] == usize::MAX {
                    prev[next] = cell;
                    queue.push_back(next);
                }
            }
        }
    }
    let mut path = vec![n - 1];
    while *path.last().unwrap() != 0 {
        let p = prev[*path.last().unwrap()];
        path.push(p);
    }
    path.reverse();
    path.into_iter().map(|k| (k / size, k % size)).collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use rand::SeedableRng;

    #[test]
    fn a_maze_is_perfect_and_solved() {
        for seed in 0..5 {
            let m = make(12, &mut StdRng::seed_from_u64(seed));
            // every square reachable, and the walls agree from both sides
            let n = 144;
            for r in 0..12 {
                for c in 0..12 {
                    let w = m.at(r, c);
                    if c + 1 < 12 {
                        assert_eq!(w & E != 0, m.at(r, c + 1) & W != 0);
                    }
                    if r + 1 < 12 {
                        assert_eq!(w & S != 0, m.at(r + 1, c) & N != 0);
                    }
                }
            }
            // a perfect maze of n squares has n - 1 open inner passages
            let open: usize = m
                .walls
                .iter()
                .enumerate()
                .map(|(k, &w)| {
                    let (r, c) = (k / 12, k % 12);
                    (c + 1 < 12 && w & E == 0) as usize + (r + 1 < 12 && w & S == 0) as usize
                })
                .sum();
            assert_eq!(open, n - 1, "seed {seed}");
            assert_eq!(m.solution.first(), Some(&(0, 0)));
            assert_eq!(m.solution.last(), Some(&(11, 11)));
            assert!(m.solution.len() >= 23, "{}", m.solution.len());
            assert_eq!(m.at(0, 0) & N, 0, "entrance open");
            assert_eq!(m.at(11, 11) & S, 0, "exit open");
        }
        let a = make(8, &mut StdRng::seed_from_u64(1));
        let b = make(8, &mut StdRng::seed_from_u64(1));
        assert_eq!(a, b, "the same seed, the same maze");
    }
}
