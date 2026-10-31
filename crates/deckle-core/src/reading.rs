// SPDX-License-Identifier: GPL-3.0-or-later
//! What a reader says about each book: how far through, finished, when last read, the stars
//! (calibre bridge design §5.1). Read from KOReader's sidecars and a Kobo's database as each is
//! read for the shelf; the calibre write-back takes it from there.

use std::path::Path;

use serde::{Deserialize, Serialize};

/// One book on one reader.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Record {
    pub title: String,
    #[serde(default)]
    pub authors: Vec<String>,
    /// The book's calibre id (its `uuid`), read from the book file on the reader.
    #[serde(default)]
    pub calibre_id: Option<String>,
    /// Where the book is on the reader, relative to its root, `/`-separated.
    #[serde(default)]
    pub path: Option<String>,
    /// How far through, 0–100.
    #[serde(default)]
    pub percent: Option<f64>,
    #[serde(default)]
    pub finished: bool,
    /// When it was last read, `YYYY-MM-DDTHH:MM:SSZ`.
    #[serde(default)]
    pub last_read: Option<String>,
    /// 1–5 stars, when the reader has them.
    #[serde(default)]
    pub rating: Option<u8>,
    /// `koreader`, `kobo` or `sync` (KOReader's progress sync).
    pub from: String,
}

/// The calibre id inside the EPUB at `root`/`rel`, if it is one and calibre put one there.
pub fn calibre_id_of(root: &Path, rel: &str) -> Option<String> {
    let lower = rel.to_ascii_lowercase();
    if !lower.ends_with(".epub") {
        return None;
    }
    let path = root.join(rel.trim_start_matches('/'));
    crate::inspect::inspect(&path).ok()?.calibre_id
}
