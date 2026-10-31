// SPDX-License-Identifier: GPL-3.0-or-later
//! `puzzles.json`: each date's puzzle, made once, with the headlines saved for the next day's
//! theme and the solve once it's known (puzzle design §3).

use std::collections::BTreeMap;
use std::path::Path;

use chrono::{Days, NaiveDate};
use serde::{Deserialize, Serialize};

use super::grid::Dir;
use super::ladder::Ladder;
use super::theme::ThemeWord;

/// Days kept.
pub const KEEP_DAYS: u64 = 60;

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Clue {
    pub number: u8,
    pub dir: Dir,
    pub answer: String,
    pub clue: String,
    /// A headline with the answer blanked.
    pub theme: bool,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Puzzle {
    /// "The Mini · No. N".
    pub number: u32,
    /// `#` for black squares, upper-case letters; `None` when no mini could be made.
    pub rows: Option<[String; 5]>,
    pub clues: Vec<Clue>,
    pub ladder: Option<Ladder>,
    /// The word search (word search design); `None` on days stored before it, or when none
    /// could be made.
    #[serde(default)]
    pub search: Option<super::search::Search>,
}

/// What the Kindle's notes said about a day's puzzle.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Solve {
    pub solved: usize,
    pub of: usize,
    /// A note on the ladder spelled the whole path.
    pub ladder: bool,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Day {
    pub puzzle: Puzzle,
    /// This day's headline words, for tomorrow's theme.
    #[serde(default)]
    pub headlines: Vec<ThemeWord>,
    #[serde(default)]
    pub solve: Option<Solve>,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Store {
    #[serde(default)]
    pub days: BTreeMap<NaiveDate, Day>,
    /// Whether the last Kindle that connected had a readable `My Clippings.txt`; `None` before
    /// any did.
    #[serde(default)]
    pub notes_readable: Option<bool>,
}

impl Store {
    /// The store at `path`; a missing or unreadable file is an empty store.
    pub fn load(path: &Path) -> Store {
        std::fs::read(path)
            .ok()
            .and_then(|b| serde_json::from_slice(&b).ok())
            .unwrap_or_default()
    }

    /// Writes through a temp file and a rename, so a crash never leaves half a file.
    pub fn save(&self, path: &Path) -> std::io::Result<()> {
        if let Some(dir) = path.parent() {
            std::fs::create_dir_all(dir)?;
        }
        let tmp = path.with_extension("json.tmp");
        std::fs::write(&tmp, serde_json::to_vec_pretty(self)?)?;
        std::fs::rename(&tmp, path)
    }

    /// Keeps the last [`KEEP_DAYS`] days up to `today`.
    pub fn prune(&mut self, today: NaiveDate) {
        let first = today - Days::new(KEEP_DAYS - 1);
        self.days.retain(|d, _| *d >= first);
    }
}
