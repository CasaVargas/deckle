// SPDX-License-Identifier: GPL-3.0-or-later
//! The `XRAY.entities.<ASIN>.asc` SQLite sidecar, in the layout the Kindle firmware reads
//! (schema reverse-engineered by the WordDumb project, GPL-3).

use std::path::Path;

use rusqlite::{Connection, params};
use serde::{Deserialize, Serialize};

/// WordDumb's localized string table (ids 0..20, 1000, 1001).
const STRINGS_JSON: &str = include_str!("../data/x_ray_strings.json");

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum EntityKind {
    Person,
    Term,
}

impl EntityKind {
    fn type_id(self) -> i64 {
        match self {
            EntityKind::Person => 1,
            EntityKind::Term => 2,
        }
    }
}

/// Where a description came from; the Kindle shows the source under the info card.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum DescriptionSource {
    /// Written by Deckle (LLM) or quoted from the book: no source row.
    Deckle,
    Wikipedia,
    /// Another MediaWiki (source row 2).
    MediaWiki,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Entity {
    pub label: String,
    pub kind: EntityKind,
    pub description: String,
    pub source: DescriptionSource,
    /// `(start position, length in characters)`.
    pub occurrences: Vec<(i64, i64)>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct XRayBook {
    pub asin: String,
    /// Book language (`en`, `de`, …) used for the UI strings and Wikipedia URL.
    pub lang: String,
    /// End position of the book (last run position + its length).
    pub erl: i64,
    pub entities: Vec<Entity>,
}

#[derive(Debug, thiserror::Error)]
pub enum AscError {
    #[error("SQLite error: {0}")]
    Sqlite(#[from] rusqlite::Error),
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),
    #[error("string table is invalid: {0}")]
    Strings(#[from] serde_json::Error),
}

const SCHEMA: &str = "
PRAGMA user_version = 1;
CREATE TABLE book_metadata (srl INTEGER, erl INTEGER, has_images TINYINT, has_excerpts TINYINT, show_spoilers_default TINYINT, num_people INTEGER, num_terms INTEGER, num_images INTEGER, preview_images TEXT);
CREATE TABLE entity (id INTEGER PRIMARY KEY, label TEXT, loc_label INTEGER, type INTEGER, count INTEGER, has_info_card TINYINT);
CREATE TABLE entity_description (text TEXT, source_wildcard TEXT, source INTEGER, entity INTEGER PRIMARY KEY);
CREATE TABLE entity_excerpt (entity INTEGER, excerpt INTEGER);
CREATE TABLE excerpt (id INTEGER PRIMARY KEY, start INTEGER, length INTEGER, image TEXT, related_entities TEXT, goto INTEGER);
CREATE TABLE occurrence (entity INTEGER, start INTEGER, length INTEGER);
CREATE TABLE source (id INTEGER PRIMARY KEY, label INTEGER, url INTEGER, license_label INTEGER, license_url INTEGER);
CREATE TABLE string (id INTEGER, language TEXT, text TEXT);
CREATE TABLE type (id INTEGER PRIMARY KEY, label INTEGER, singular_label INTEGER, icon INTEGER, top_mentioned_entities TEXT);
INSERT INTO entity (id, loc_label, has_info_card) VALUES (0, 1, 0);
INSERT INTO source (id, label, url) VALUES (0, 5, 20);
INSERT INTO source VALUES (1, 6, 21, 7, 8);
INSERT INTO source (id, label, url, license_label, license_url) VALUES (2, 4, 22, 7, 8);
";

/// Writes the sidecar, replacing any existing file.
pub fn write_asc(path: &Path, book: &XRayBook) -> Result<(), AscError> {
    if path.exists() {
        std::fs::remove_file(path)?;
    }
    if let Some(p) = path.parent() {
        std::fs::create_dir_all(p)?;
    }
    let mut conn = Connection::open(path)?;
    conn.execute_batch(SCHEMA)?;
    let tx = conn.transaction()?;
    {
        let strings: Vec<(i64, String, String)> = serde_json::from_str(STRINGS_JSON)?;
        let mut ins = tx.prepare("INSERT INTO string VALUES (?1, ?2, ?3)")?;
        for (id, lang, text) in &strings {
            ins.execute(params![id, lang, text])?;
        }
        let lang = if book.lang.is_empty() {
            "en"
        } else {
            book.lang.split('-').next().unwrap_or("en")
        };
        ins.execute(params![
            21,
            "en",
            format!("https://{lang}.wikipedia.org/wiki/%s")
        ])?;
        ins.execute(params![22, "en", "https://www.wikipedia.org/wiki/%s"])?;

        let mut ent = tx.prepare(
            "INSERT INTO entity (id, label, type, count, has_info_card) VALUES (?1, ?2, ?3, ?4, 1)",
        )?;
        let mut desc = tx.prepare("INSERT INTO entity_description VALUES (?1, ?2, ?3, ?4)")?;
        let mut occ = tx.prepare("INSERT INTO occurrence VALUES (?1, ?2, ?3)")?;
        for (i, e) in book.entities.iter().enumerate() {
            let id = i as i64 + 1;
            ent.execute(params![
                id,
                e.label,
                e.kind.type_id(),
                e.occurrences.len() as i64
            ])?;
            let source: Option<i64> = match e.source {
                DescriptionSource::Deckle => None,
                DescriptionSource::Wikipedia => Some(1),
                DescriptionSource::MediaWiki => Some(2),
            };
            desc.execute(params![e.description, e.label, source, id])?;
            for (start, len) in &e.occurrences {
                occ.execute(params![id, start, len])?;
            }
        }
        let num_people = book
            .entities
            .iter()
            .filter(|e| e.kind == EntityKind::Person)
            .count() as i64;
        let num_terms = book.entities.len() as i64 - num_people;
        tx.execute(
            "INSERT INTO book_metadata VALUES (0, ?1, 0, 0, 0, ?2, ?3, 0, NULL)",
            params![book.erl, num_people, num_terms],
        )?;
        for (kind, label, singular, icon) in [
            (EntityKind::Person, 14, 15, 1),
            (EntityKind::Term, 16, 17, 2),
        ] {
            let mut ids: Vec<(i64, usize)> = book
                .entities
                .iter()
                .enumerate()
                .filter(|(_, e)| e.kind == kind)
                .map(|(i, e)| (i as i64 + 1, e.occurrences.len()))
                .collect();
            ids.sort_by(|a, b| b.1.cmp(&a.1).then(a.0.cmp(&b.0)));
            let top: Vec<String> = ids.iter().take(10).map(|(id, _)| id.to_string()).collect();
            tx.execute(
                "INSERT INTO type VALUES (?1, ?2, ?3, ?4, ?5)",
                params![kind.type_id(), label, singular, icon, top.join(",")],
            )?;
        }
        tx.execute_batch(
            "CREATE INDEX idx_entity_type ON entity(type ASC);
             CREATE INDEX idx_entity_excerpt ON entity_excerpt(entity ASC);
             CREATE INDEX idx_occurrence_start ON occurrence(start ASC);",
        )?;
    }
    tx.commit()?;
    conn.execute_batch("PRAGMA optimize;")?;
    Ok(())
}
