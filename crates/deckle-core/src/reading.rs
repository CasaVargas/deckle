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
    /// The highlights and notes, oldest first.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub highlights: Vec<Highlight>,
    /// `koreader`, `kobo` or `sync` (KOReader's progress sync).
    pub from: String,
}

/// One highlight, with its note.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Highlight {
    pub text: String,
    /// Empty for none.
    #[serde(default)]
    pub note: String,
    /// The chapter it's in, when the reader says.
    #[serde(default)]
    pub chapter: Option<String>,
    #[serde(default)]
    pub page: Option<String>,
    /// When it was made: `YYYY-MM-DD HH:MM:SS` or ISO 8601, as the reader wrote it.
    #[serde(default)]
    pub datetime: String,
    /// Which spine file of the EPUB it's in, from 0 (KOReader's `DocFragment[N]` less one).
    #[serde(default)]
    pub spine: Option<u32>,
}

/// The spine file a KOReader position (`/body/DocFragment[N]/…`) is in, from 0.
pub fn spine_of(xpointer: &str) -> Option<u32> {
    let rest = xpointer.split("DocFragment[").nth(1)?;
    let n: u32 = rest.split(']').next()?.parse().ok()?;
    n.checked_sub(1)
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

#[cfg(test)]
mod tests {
    #[test]
    fn a_position_names_its_spine_file() {
        assert_eq!(
            super::spine_of("/body/DocFragment[3]/body/p[2]/text().5"),
            Some(2)
        );
        assert_eq!(super::spine_of("/body/DocFragment[1]"), Some(0));
        assert_eq!(super::spine_of("/body/DocFragment[0]"), None);
        assert_eq!(super::spine_of("12"), None);
    }
}
