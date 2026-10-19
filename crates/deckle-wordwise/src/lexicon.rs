// SPDX-License-Identifier: GPL-3.0-or-later
//! Deckle's Word Wise database (`deckle-wordwise-en-v1.sqlite`), built offline by `data/wordwise`
//! from Wiktionary (kaikki.org) and wordfreq, downloaded on first use. One sense per lemma and
//! part of speech, and every surface form mapped to its best sense, so the app needs no
//! lemmatizer or tagger.

use std::collections::HashMap;
use std::path::Path;

use rusqlite::{Connection, OpenFlags, params};

/// Schema of the database. `pos` uses Kindle labels (`noun`, `verb`, `adjective`, `adverb`,
/// `other`); `form` is lowercase with straight apostrophes.
pub const SCHEMA: &str = "
CREATE TABLE metadata (key TEXT PRIMARY KEY, value TEXT);
CREATE TABLE senses (id INTEGER PRIMARY KEY, lemma TEXT NOT NULL, pos TEXT NOT NULL, short_def TEXT NOT NULL, full_def TEXT NOT NULL, example TEXT, difficulty INTEGER NOT NULL);
CREATE TABLE forms (form TEXT PRIMARY KEY, sense_id INTEGER NOT NULL) WITHOUT ROWID;
";

/// Format version the app understands (`metadata.format`).
pub const FORMAT: &str = "1";

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Sense {
    pub id: i64,
    pub lemma: String,
    pub pos: String,
    pub difficulty: i64,
}

#[derive(Debug, thiserror::Error)]
pub enum LexiconError {
    #[error("SQLite error: {0}")]
    Sqlite(#[from] rusqlite::Error),
    #[error("not a Deckle Word Wise database (format {0:?}, expected {FORMAT})")]
    Format(Option<String>),
}

/// Surface form → sense, loaded into memory once per run.
#[derive(Debug, Default)]
pub struct Lexicon {
    senses: Vec<Sense>,
    forms: HashMap<String, u32>,
    /// Longest form in words (phrases like "take off" are 2).
    pub max_words: usize,
    /// `metadata.version`, plus `+<build>` when the database carries a build fingerprint.
    pub version: String,
}

/// Lowercase, curly apostrophes straightened: the key both the builder and the matcher use.
pub fn normalize_form(s: &str) -> String {
    s.chars()
        .map(|c| match c {
            '\u{2019}' | '\u{2018}' | '\u{02BC}' => '\'',
            '\u{2010}' | '\u{2011}' => '-',
            c => c,
        })
        .flat_map(char::to_lowercase)
        .collect()
}

impl Lexicon {
    pub fn open(path: &Path) -> Result<Lexicon, LexiconError> {
        let conn = Connection::open_with_flags(path, OpenFlags::SQLITE_OPEN_READ_ONLY)?;
        let meta = |k: &str| -> Option<String> {
            conn.query_row("SELECT value FROM metadata WHERE key = ?1", [k], |r| {
                r.get(0)
            })
            .ok()
        };
        let format = meta("format");
        if format.as_deref() != Some(FORMAT) {
            return Err(LexiconError::Format(format));
        }
        // `build` fingerprints the rows: sense ids are only valid for one build, so a rebuilt
        // database with the same release version is still told apart.
        let version = match (meta("version"), meta("build")) {
            (Some(v), Some(b)) => format!("{v}+{b}"),
            (v, _) => v.unwrap_or_default(),
        };
        let mut senses = Vec::new();
        let mut index: HashMap<i64, u32> = HashMap::new();
        {
            let mut st = conn.prepare("SELECT id, lemma, pos, difficulty FROM senses")?;
            let rows = st.query_map([], |r| {
                Ok(Sense {
                    id: r.get(0)?,
                    lemma: r.get(1)?,
                    pos: r.get(2)?,
                    difficulty: r.get(3)?,
                })
            })?;
            for s in rows {
                let s = s?;
                index.insert(s.id, senses.len() as u32);
                senses.push(s);
            }
        }
        let mut forms = HashMap::new();
        let mut max_words = 1;
        {
            let mut st = conn.prepare("SELECT form, sense_id FROM forms")?;
            let rows = st.query_map([], |r| Ok((r.get::<_, String>(0)?, r.get::<_, i64>(1)?)))?;
            for row in rows {
                let (form, id) = row?;
                if let Some(&i) = index.get(&id) {
                    max_words = max_words.max(form.split(' ').count());
                    forms.insert(form, i);
                }
            }
        }
        Ok(Lexicon {
            senses,
            forms,
            max_words,
            version,
        })
    }

    /// The sense for a normalized form (see [`normalize_form`]).
    pub fn get(&self, form: &str) -> Option<&Sense> {
        self.forms.get(form).map(|&i| &self.senses[i as usize])
    }

    pub fn senses(&self) -> &[Sense] {
        &self.senses
    }

    pub fn len(&self) -> usize {
        self.forms.len()
    }

    pub fn is_empty(&self) -> bool {
        self.forms.is_empty()
    }
}

/// Creates an empty database with [`SCHEMA`] and the metadata rows (builder and tests).
pub fn create(path: &Path, version: &str) -> Result<Connection, rusqlite::Error> {
    if path.exists() {
        let _ = std::fs::remove_file(path);
    }
    let conn = Connection::open(path)?;
    conn.execute_batch(SCHEMA)?;
    for (k, v) in [
        ("format", FORMAT),
        ("version", version),
        ("lemmaLanguage", "en"),
        ("definitionLanguage", "en"),
        (
            "license",
            "Definitions from Wiktionary (CC BY-SA 4.0) via kaikki.org; frequencies from wordfreq (CC BY-SA 4.0)",
        ),
    ] {
        conn.execute("INSERT INTO metadata VALUES (?1, ?2)", params![k, v])?;
    }
    Ok(conn)
}
