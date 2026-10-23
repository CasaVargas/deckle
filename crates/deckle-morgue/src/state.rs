// SPDX-License-Identifier: GPL-3.0-or-later
//! `morgue.json`: what the Morgue learns at each Kindle connection and what has been sent again
//! (design §5, plan rulings). It sits beside `digest.toml`.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use chrono::NaiveDate;
use serde::{Deserialize, Serialize};

/// An edition sent to the Kindle again. Times are RFC 3339 UTC.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Resent {
    pub queued: String,
    /// When a Kindle took it; `None` while it waits in the outbox.
    pub delivered: Option<String>,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct MorgueState {
    /// The cached editions were imported once.
    pub imported: bool,
    pub resent: BTreeMap<NaiveDate, Resent>,
    /// Passages highlighted per edition, from the last `My Clippings.txt` read.
    pub highlights: BTreeMap<NaiveDate, usize>,
    /// Whether each edition (by stem) was on the Kindle at the last connection.
    pub on_kindle: BTreeMap<String, bool>,
}

/// `morgue.json` beside `digest.toml`.
pub fn state_path() -> PathBuf {
    deckle_digest::config::default_path().with_file_name("morgue.json")
}

impl MorgueState {
    /// The state at `path`; a missing or unreadable file is an empty state.
    pub fn load(path: &Path) -> MorgueState {
        std::fs::read(path)
            .ok()
            .and_then(|b| serde_json::from_slice(&b).ok())
            .unwrap_or_default()
    }

    /// Writes through a temp file and a rename.
    pub fn save(&self, path: &Path) -> std::io::Result<()> {
        if let Some(dir) = path.parent() {
            std::fs::create_dir_all(dir)?;
        }
        let tmp = crate::folder::unique_tmp(path);
        let result = std::fs::write(
            &tmp,
            serde_json::to_vec_pretty(self).map_err(std::io::Error::other)?,
        )
        .and_then(|()| std::fs::rename(&tmp, path));
        if result.is_err() {
            let _ = std::fs::remove_file(&tmp);
        }
        result
    }
}
