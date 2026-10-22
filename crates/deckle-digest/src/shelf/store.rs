// SPDX-License-Identifier: GPL-3.0-or-later
//! `shelf.json`: what each Kindle shared the last time it was plugged in (shelf design §4).

use std::collections::BTreeMap;
use std::path::Path;

use chrono::{Days, NaiveDate};
use serde::{Deserialize, Serialize};

/// Days a highlight is remembered as shown.
const SHOWN_DAYS: u64 = 90;
/// A snapshot older than this is not printed.
const FRESH_DAYS: u64 = 30;

/// A book on the Kindle and where the reader is in it.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct BookSnap {
    /// The path under `documents/`.
    pub file: String,
    pub title: String,
    pub authors: Vec<String>,
    pub asin: Option<String>,
    /// `lpr`: the last position read.
    pub last: Option<i64>,
    /// `fpr`: the furthest position reached.
    pub furthest: Option<i64>,
    /// When `last` was recorded, UTC.
    pub last_read: Option<String>,
    /// The book's end position, when known.
    pub end: Option<i64>,
    /// The Kindle's own fraction through the book.
    pub kindle_fraction: Option<f64>,
    pub words_read: Option<i64>,
    /// The Kindle's reading-pace samples, kept for a checked follow-up.
    pub wpm: Vec<f64>,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct HighlightSnap {
    pub id: String,
    pub title: String,
    pub author: String,
    pub text: String,
    pub notes: Vec<String>,
    pub page: Option<String>,
    pub added: Option<NaiveDate>,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct WordSnap {
    pub word: String,
    pub stem: String,
    /// The sentence the Kindle saved with the lookup.
    pub usage: String,
    pub book: String,
    pub at: String,
    pub definition: Option<String>,
}

/// One Kindle, as it was when last plugged in.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct KindleSnapshot {
    pub name: String,
    /// When it was taken: `YYYY-MM-DD` first, then anything.
    pub seen: String,
    pub books: Vec<BookSnap>,
    /// `.yjf` left unread past the per-connection cap.
    pub skipped: usize,
    pub highlights_shared: bool,
    pub highlights: Vec<HighlightSnap>,
    pub words_shared: bool,
    pub words: Vec<WordSnap>,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Shelf {
    /// One snapshot per Kindle, by sync id, each replaced whole.
    pub kindles: BTreeMap<String, KindleSnapshot>,
    /// Highlights already printed, by id, and when.
    pub shown: BTreeMap<String, NaiveDate>,
}

impl Shelf {
    /// The store at `path`; a missing or unreadable file is an empty store.
    pub fn load(path: &Path) -> Shelf {
        std::fs::read(path)
            .ok()
            .and_then(|b| serde_json::from_slice(&b).ok())
            .unwrap_or_default()
    }

    /// Writes through a temp file and a rename, so a crash never leaves half a file.
    pub fn save(&self, path: &Path) -> Result<(), String> {
        let io = |e: std::io::Error| format!("{}: {e}", path.display());
        if let Some(dir) = path.parent() {
            std::fs::create_dir_all(dir).map_err(io)?;
        }
        let tmp = path.with_extension("json.tmp");
        let json = serde_json::to_vec_pretty(self).map_err(|e| e.to_string())?;
        std::fs::write(&tmp, json).map_err(io)?;
        std::fs::rename(&tmp, path).map_err(io)
    }

    /// Removes the file; one that isn't there is fine.
    pub fn forget(path: &Path) -> Result<(), String> {
        match std::fs::remove_file(path) {
            Ok(()) => Ok(()),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
            Err(e) => Err(format!("{}: {e}", path.display())),
        }
    }

    /// Replaces the snapshot of the Kindle `id`, and no other.
    pub fn put(&mut self, id: &str, snap: KindleSnapshot) {
        self.kindles.insert(id.to_owned(), snap);
    }

    /// Keeps the last [`SHOWN_DAYS`] days up to `today`.
    pub fn prune_shown(&mut self, today: NaiveDate) {
        let first = today - Days::new(SHOWN_DAYS - 1);
        self.shown.retain(|_, d| *d >= first);
    }

    /// The snapshots taken within [`FRESH_DAYS`] days of `today`, newest first.
    pub fn fresh(&self, today: NaiveDate) -> Vec<(&str, &KindleSnapshot)> {
        let first = today - Days::new(FRESH_DAYS);
        let mut out: Vec<_> = self
            .kindles
            .iter()
            .filter(|(_, k)| seen_on(k).is_some_and(|d| d >= first))
            .map(|(id, k)| (id.as_str(), k))
            .collect();
        out.sort_by(|a, b| b.1.seen.cmp(&a.1.seen));
        out
    }
}

/// The date a snapshot was taken.
pub fn seen_on(k: &KindleSnapshot) -> Option<NaiveDate> {
    NaiveDate::parse_from_str(k.seen.get(..10)?, "%Y-%m-%d").ok()
}
