// SPDX-License-Identifier: GPL-3.0-or-later
//! Deckle's own `.klld` dictionary, derived from the Word Wise database and installed in the
//! Kindle's `kll.en.zh.klld` slot when the device has no English dictionary of its own. The
//! layout mirrors Amazon's (as rebuilt by Proficiency): definitions are base64 UTF-8.

use std::path::Path;

use base64::Engine;
use base64::engine::general_purpose::STANDARD as B64;
use rusqlite::{Connection, OpenFlags, params};

use crate::{POS_TYPES, pos_id};

/// Where Deckle's dictionary goes on the device (the Chinese-gloss slot; the glosses are English).
pub const DEVICE_PATH: [&str; 3] = ["system", "kll", "kll.en.zh.klld"];
/// Amazon's English dictionary on the device.
pub const KINDLE_EN_PATH: [&str; 3] = ["system", "kll", "kll.en.en.klld"];
/// Metadata key that marks a `.klld` as Deckle's (value: the database version).
pub const MARKER: &str = "deckleDictionary";

#[derive(Debug, thiserror::Error)]
pub enum KlldError {
    #[error("SQLite error: {0}")]
    Sqlite(#[from] rusqlite::Error),
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),
}

const SCHEMA: &str = "
CREATE TABLE pos_types (id integer NOT NULL DEFAULT '0', label varchar(100) DEFAULT NULL, PRIMARY KEY (id));
CREATE TABLE sources (id integer NOT NULL DEFAULT '0', label varchar(200) DEFAULT NULL, PRIMARY KEY (id));
CREATE TABLE metadata (key TEXT PRIMARY KEY, value TEXT);
CREATE TABLE lemmas (id INTEGER PRIMARY KEY, lemma TEXT);
CREATE TABLE senses (id INTEGER PRIMARY KEY, display_lemma_id INTEGER, term_id INTEGER, term_lemma_id INTEGER, pos_type INTEGER, source_id INTEGER, sense_number REAL, synset_id INTEGER, corpus_count INTEGER, full_def TEXT, short_def TEXT, example_sentence TEXT);
";

/// Builds the `.klld` from a Deckle Word Wise database. Sense ids are the database's, so a
/// sidecar matched against the same database points at the right rows.
pub fn write_from_db(db: &Path, out: &Path) -> Result<(), KlldError> {
    if out.exists() {
        std::fs::remove_file(out)?;
    }
    if let Some(p) = out.parent() {
        std::fs::create_dir_all(p)?;
    }
    let src = Connection::open_with_flags(db, OpenFlags::SQLITE_OPEN_READ_ONLY)?;
    let meta = |k: &str| -> Option<String> {
        src.query_row("SELECT value FROM metadata WHERE key = ?1", [k], |r| {
            r.get(0)
        })
        .ok()
    };
    // same string as `Lexicon::version`, so a device slot is matched to this exact build
    let version = match (meta("version"), meta("build")) {
        (Some(v), Some(b)) => format!("{v}+{b}"),
        (v, _) => v.unwrap_or_default(),
    };
    let mut conn = Connection::open(out)?;
    conn.execute_batch(SCHEMA)?;
    let tx = conn.transaction()?;
    {
        let mut ins = tx.prepare("INSERT INTO pos_types VALUES (?1, ?2)")?;
        for (i, p) in POS_TYPES.iter().enumerate() {
            ins.execute(params![i as i64, p])?;
        }
        let mut ins = tx.prepare("INSERT INTO sources VALUES (?1, ?2)")?;
        for (i, s) in [None, Some("Merriam-Webster"), None, Some("Wiktionary")]
            .iter()
            .enumerate()
        {
            ins.execute(params![i as i64, s])?;
        }
        let mut ins = tx.prepare("INSERT INTO metadata VALUES (?1, ?2)")?;
        for (k, v) in [
            ("maxTermLength", Some("3")),
            (
                "termTerminatorList",
                Some(",    ;       .       \"       '       !       ?"),
            ),
            ("definitionLanguage", Some("en")),
            ("id", Some("kll.en.zh")),
            ("lemmaLanguage", Some("en")),
            ("version", Some(version.as_str())),
            ("revision", Some("57")),
            ("tokenSeparator", None),
            ("encoding", Some("1")),
            (MARKER, Some(version.as_str())),
        ] {
            ins.execute(params![k, v])?;
        }

        let mut lemma_ids: std::collections::HashMap<String, i64> = Default::default();
        let mut ins_lemma = tx.prepare("INSERT INTO lemmas (lemma) VALUES (?1)")?;
        let mut ins_sense = tx.prepare(
            "INSERT INTO senses (id, display_lemma_id, term_id, term_lemma_id, pos_type, source_id, sense_number, corpus_count, full_def, short_def, example_sentence)
             VALUES (?1, ?2, ?2, ?2, ?3, 3, 1.0, 0, ?4, ?5, ?6)",
        )?;
        let mut st = src.prepare(
            "SELECT id, lemma, pos, short_def, full_def, example FROM senses ORDER BY id",
        )?;
        let mut rows = st.query([])?;
        while let Some(r) = rows.next()? {
            let id: i64 = r.get(0)?;
            let lemma: String = r.get(1)?;
            let pos: String = r.get(2)?;
            let short: String = r.get(3)?;
            let full: String = r.get(4)?;
            let example: Option<String> = r.get(5)?;
            let lemma_id = match lemma_ids.get(&lemma) {
                Some(&l) => l,
                None => {
                    ins_lemma.execute([&lemma])?;
                    let l = tx.last_insert_rowid();
                    lemma_ids.insert(lemma, l);
                    l
                }
            };
            let example = example
                .filter(|e| !e.is_empty())
                .map(|e| B64.encode(e.as_bytes()));
            ins_sense.execute(params![
                id,
                lemma_id,
                pos_id(&pos),
                B64.encode(full.as_bytes()),
                B64.encode(short.as_bytes()),
                example
            ])?;
        }
        tx.execute_batch(
            "CREATE INDEX senses_synset_id_index ON senses(synset_id);
             CREATE INDEX senses_term_lemma_id_index ON senses(term_lemma_id);",
        )?;
    }
    tx.commit()?;
    conn.execute_batch("PRAGMA optimize;")?;
    Ok(())
}

/// The Deckle marker of a `.klld` (its database version), `None` for anyone else's dictionary.
pub fn deckle_version(klld: &Path) -> Option<String> {
    let conn = Connection::open_with_flags(klld, OpenFlags::SQLITE_OPEN_READ_ONLY).ok()?;
    conn.query_row("SELECT value FROM metadata WHERE key = ?1", [MARKER], |r| {
        r.get(0)
    })
    .ok()
}
