// SPDX-License-Identifier: GPL-3.0-or-later
//! The Kindle's own English dictionary (`system/kll/kll.en.en.klld`, Amazon's, downloaded by the
//! device). Deckle reads it at send time to translate its matches into Amazon sense ids, so
//! hints look exactly like a store book's. It is never stored or shipped by Deckle.

use std::collections::HashMap;
use std::path::Path;

use rusqlite::{Connection, OpenFlags};

use crate::kll::Gloss;
use crate::lexicon::Lexicon;

#[derive(Debug, thiserror::Error)]
pub enum DeviceDictError {
    #[error("SQLite error: {0}")]
    Sqlite(#[from] rusqlite::Error),
    #[error("the Kindle's dictionary has an unexpected layout ({0})")]
    Schema(String),
    #[error("the Kindle's dictionary is Deckle's, not Amazon's")]
    IsDeckle,
}

/// (lemma, pos label) → Amazon sense id, the lowest `sense_number` per pair.
#[derive(Debug, Default)]
pub struct DeviceDictionary {
    by_lemma_pos: HashMap<(String, String), i64>,
    by_lemma: HashMap<String, i64>,
}

impl DeviceDictionary {
    /// Reads an Amazon-shaped `.klld`; a missing table or column is a schema error so the caller
    /// can fall back to Deckle's dictionary.
    pub fn open(path: &Path) -> Result<DeviceDictionary, DeviceDictError> {
        let conn = Connection::open_with_flags(path, OpenFlags::SQLITE_OPEN_READ_ONLY)?;
        if crate::klld::deckle_version(path).is_some() {
            return Err(DeviceDictError::IsDeckle);
        }
        for (table, cols) in [
            ("lemmas", &["id", "lemma"][..]),
            (
                "senses",
                &["id", "display_lemma_id", "pos_type", "sense_number"][..],
            ),
            ("pos_types", &["id", "label"][..]),
        ] {
            let mut st = conn.prepare(&format!("PRAGMA table_info({table})"))?;
            let have: Vec<String> = st
                .query_map([], |r| r.get::<_, String>(1))?
                .collect::<Result<_, _>>()?;
            if have.is_empty() {
                return Err(DeviceDictError::Schema(format!("no {table} table")));
            }
            for c in cols {
                if !have.iter().any(|h| h == c) {
                    return Err(DeviceDictError::Schema(format!("no {table}.{c}")));
                }
            }
        }
        let mut st = conn.prepare(
            "SELECT lower(l.lemma), p.label, s.id FROM senses s
             JOIN lemmas l ON l.id = s.display_lemma_id
             LEFT JOIN pos_types p ON p.id = s.pos_type
             ORDER BY s.sense_number, s.id",
        )?;
        let mut d = DeviceDictionary::default();
        let rows = st.query_map([], |r| {
            Ok((
                r.get::<_, String>(0)?,
                r.get::<_, Option<String>>(1)?.unwrap_or_default(),
                r.get::<_, i64>(2)?,
            ))
        })?;
        for row in rows {
            let (lemma, pos, id) = row?;
            d.by_lemma_pos.entry((lemma.clone(), pos)).or_insert(id);
            d.by_lemma.entry(lemma).or_insert(id);
        }
        if d.by_lemma.is_empty() {
            return Err(DeviceDictError::Schema("no senses".into()));
        }
        Ok(d)
    }

    pub fn len(&self) -> usize {
        self.by_lemma.len()
    }

    pub fn is_empty(&self) -> bool {
        self.by_lemma.is_empty()
    }

    /// Amazon's sense for a lemma, preferring the same part of speech.
    pub fn sense(&self, lemma: &str, pos: &str) -> Option<i64> {
        let lemma = lemma.to_lowercase();
        self.by_lemma_pos
            .get(&(lemma.clone(), pos.to_owned()))
            .or_else(|| self.by_lemma.get(&lemma))
            .copied()
    }

    /// Rewrites Deckle sense ids to Amazon's; words Amazon's dictionary lacks are dropped.
    pub fn remap(&self, glosses: Vec<Gloss>, lexicon: &Lexicon) -> Vec<Gloss> {
        let by_id: HashMap<i64, &crate::lexicon::Sense> =
            lexicon.senses().iter().map(|s| (s.id, s)).collect();
        glosses
            .into_iter()
            .filter_map(|mut g| {
                let s = by_id.get(&g.sense_id)?;
                g.sense_id = self.sense(&s.lemma, &s.pos)?;
                Some(g)
            })
            .collect()
    }
}
