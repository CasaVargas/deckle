// SPDX-License-Identifier: GPL-3.0-or-later
//! `library.json` in Deckle's config folder (library design §3): where the library is, and
//! whether every send is added. No file, or no root, means no library.

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct LibraryConfig {
    pub version: u32,
    pub root: Option<PathBuf>,
    /// Add each book sent (CLI `run`, the app's Send) to the library.
    pub auto_add: bool,
    /// Look for originals of the Kindle's books in a calibre library (Books and Kindle design
    /// §4): `calibre`, else `~/Calibre Library` when it exists.
    pub find_in_calibre: bool,
    pub calibre: Option<PathBuf>,
    /// Folders to look for originals in (EPUBs), searched a few levels deep.
    pub find_in: Vec<PathBuf>,
}

impl Default for LibraryConfig {
    fn default() -> Self {
        LibraryConfig {
            version: 1,
            root: None,
            auto_add: true,
            find_in_calibre: true,
            calibre: None,
            find_in: Vec::new(),
        }
    }
}

/// `~/Library/Application Support/deckle/library.json`, `%APPDATA%\deckle\…`, or
/// `$XDG_CONFIG_HOME/deckle/…` (`~/.config`): beside `digest.toml`.
pub fn default_path() -> PathBuf {
    let home = || std::env::var_os("HOME").map(PathBuf::from);
    let base = if cfg!(target_os = "macos") {
        home().map(|h| h.join("Library/Application Support"))
    } else if cfg!(target_os = "windows") {
        std::env::var_os("APPDATA").map(PathBuf::from)
    } else {
        std::env::var_os("XDG_CONFIG_HOME")
            .map(PathBuf::from)
            .or_else(|| home().map(|h| h.join(".config")))
    };
    base.unwrap_or_else(std::env::temp_dir)
        .join("deckle")
        .join("library.json")
}

impl LibraryConfig {
    /// The config at `path`; the default (no library) when it's missing or unreadable.
    pub fn load(path: &Path) -> Self {
        std::fs::read(path)
            .ok()
            .and_then(|b| serde_json::from_slice(&b).ok())
            .unwrap_or_default()
    }

    pub fn save(&self, path: &Path) -> Result<(), String> {
        let json = serde_json::to_vec_pretty(self).map_err(|e| e.to_string())?;
        crate::write_atomic(path, &json)
    }
}
