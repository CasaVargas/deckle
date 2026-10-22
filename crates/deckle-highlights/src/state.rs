// SPDX-License-Identifier: GPL-3.0-or-later
//! `highlights.json` (highlights export design §5): the folder, and per book the file Deckle
//! writes and the entries already in it.

use std::collections::{BTreeMap, BTreeSet};
use std::io;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct State {
    pub folder: Option<PathBuf>,
    pub books: BTreeMap<String, BookState>,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct BookState {
    /// The file's name inside the folder.
    pub file: String,
    /// Ids ([`crate::entry_id`]) of the entries written to it.
    pub written: BTreeSet<String>,
}

impl State {
    /// A missing or unreadable file is an empty state: the worst case is a fresh file per book.
    pub fn load(path: &Path) -> State {
        std::fs::read(path)
            .ok()
            .and_then(|b| serde_json::from_slice(&b).ok())
            .unwrap_or_default()
    }

    pub fn save(&self, path: &Path) -> io::Result<()> {
        let tmp = path.with_extension("json.tmp");
        std::fs::write(&tmp, serde_json::to_vec_pretty(self)?)?;
        std::fs::rename(&tmp, path)
    }
}
