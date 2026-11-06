// SPDX-License-Identifier: GPL-3.0-or-later
//! Deckle's puzzle generators, on their own: the word lists, the crossword fill, the word
//! search, the word ladder, the cryptogram, sudoku, nonograms and mazes. Each is seeded, so the
//! same inputs make the same puzzle on any machine.
//!
//! Nothing here touches the network, a file or the OS, so the crate builds for iOS as it does
//! for the desktop (docs/superpowers/specs/2026-11-06-deckle-puzzles-ios-roadmap.md, phase 0).
//! The paper's puzzle page, the books and the AI that writes clues live in `deckle-digest` and
//! use these through `deckle_digest::puzzle`.

pub mod cross;
pub mod cryptogram;
pub mod grid;
pub mod ladder;
pub mod maze;
pub mod nonogram;
pub mod search;
pub mod sudoku;
pub mod words;

use serde::{Deserialize, Serialize};

use grid::Dir;

/// One crossword clue and its answer.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Clue {
    pub number: u8,
    pub dir: Dir,
    pub answer: String,
    pub clue: String,
    /// A headline with the answer blanked.
    pub theme: bool,
}
