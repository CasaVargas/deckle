// SPDX-License-Identifier: GPL-3.0-or-later
//! The `LanguageLayer.en.<ASIN>.kll` SQLite sidecar. It holds positions and sense ids only; the
//! hint text lives in the device's `.klld` dictionary named by `enDictionaryId`.

use std::path::Path;

use rusqlite::{Connection, params};

/// One hint: a word (or phrase) at a KFX position and the dictionary sense it shows.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Gloss {
    /// KFX position of the first character.
    pub start: i64,
    /// KFX position just past the last character of a multi-word phrase; `None` for one word.
    pub end: Option<i64>,
    /// 1 (rarest, shown at "fewer hints") … 5 (commonest, only at "more hints").
    pub difficulty: i64,
    pub sense_id: i64,
}

/// The `enDictionary*` metadata rows. The Kindle picks the `.klld` from its Word Wise language
/// setting, not from these, so WordDumb writes Amazon's 2016 English dictionary values in every
/// mode; Deckle does the same.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DictionaryRef {
    pub id: String,
    pub version: String,
    pub revision: String,
}

impl Default for DictionaryRef {
    fn default() -> Self {
        DictionaryRef {
            id: "kll.en.en".into(),
            version: "2016-09-14".into(),
            revision: "57".into(),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LanguageLayer {
    /// The KFX `asset_id` (WordDumb's `acr`).
    pub acr: String,
    pub dictionary: DictionaryRef,
    pub glosses: Vec<Gloss>,
}

#[derive(Debug, thiserror::Error)]
pub enum KllError {
    #[error("SQLite error: {0}")]
    Sqlite(#[from] rusqlite::Error),
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),
}

pub fn sidecar_name(asin: &str) -> String {
    format!("LanguageLayer.en.{asin}.kll")
}

/// Writes the sidecar, replacing any existing file. Glosses sharing a start keep the first.
pub fn write_kll(path: &Path, layer: &LanguageLayer) -> Result<(), KllError> {
    if path.exists() {
        std::fs::remove_file(path)?;
    }
    if let Some(p) = path.parent() {
        std::fs::create_dir_all(p)?;
    }
    let mut conn = Connection::open(path)?;
    conn.execute_batch(
        "CREATE TABLE metadata (key TEXT, value TEXT);
         CREATE TABLE glosses (start INTEGER PRIMARY KEY, end INTEGER, difficulty INTEGER, sense_id INTEGER, low_confidence BOOLEAN);",
    )?;
    let tx = conn.transaction()?;
    {
        let d = &layer.dictionary;
        let mut md = tx.prepare("INSERT INTO metadata VALUES (?1, ?2)")?;
        for (k, v) in [
            ("acr", layer.acr.as_str()),
            ("targetLanguages", "en"),
            ("sidecarRevision", "9"),
            ("bookRevision", ""),
            ("sourceLanguage", "en"),
            ("enDictionaryVersion", d.version.as_str()),
            ("enDictionaryRevision", d.revision.as_str()),
            ("enDictionaryId", d.id.as_str()),
            ("sidecarFormat", "1.0"),
        ] {
            md.execute(params![k, v])?;
        }
        let mut ins = tx.prepare("INSERT OR IGNORE INTO glosses VALUES (?1, ?2, ?3, ?4, 0)")?;
        for g in &layer.glosses {
            ins.execute(params![g.start, g.end, g.difficulty, g.sense_id])?;
        }
    }
    tx.commit()?;
    conn.execute_batch("PRAGMA optimize;")?;
    Ok(())
}
