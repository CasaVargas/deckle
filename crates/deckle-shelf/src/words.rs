// SPDX-License-Identifier: GPL-3.0-or-later
//! The words looked up on a Kindle, from its vocabulary builder's `vocab.db`.

use std::path::PathBuf;

use chrono::{DateTime, Utc};
use deckle_core::vocab::VocabFiles;
use deckle_core::wordwise::{DB_ENV, DB_FILE, default_cache_dir};
use deckle_digest::shelf::store::WordSnap;
use rusqlite::{Connection, OpenFlags};

const LOOKUPS: &str = "SELECT w.word, w.stem, l.usage, b.title, l.timestamp
    FROM LOOKUPS l
    JOIN WORDS w ON l.word_key = w.id
    LEFT JOIN BOOK_INFO b ON l.book_key = b.id
    WHERE w.lang = 'en' AND COALESCE(w.category, 0) != 100 AND l.timestamp >= ?1
    ORDER BY l.timestamp DESC";

/// The English lookups since `since`, newest first. The database and its write-ahead log are
/// copied together and opened there, so the newest lookups, still in the log, show. `define`
/// supplies a short definition for a word's stem.
pub fn read(
    files: &VocabFiles,
    since: DateTime<Utc>,
    define: &dyn Fn(&str) -> Option<String>,
) -> Result<Vec<WordSnap>, String> {
    let dir = tempfile::tempdir().map_err(|e| e.to_string())?;
    let db = dir.path().join("vocab.db");
    std::fs::write(&db, &files.db).map_err(|e| e.to_string())?;
    if let Some(wal) = &files.wal {
        std::fs::write(dir.path().join("vocab.db-wal"), wal).map_err(|e| e.to_string())?;
    }
    // Read-write on our own copy: a WAL database needs its shared-memory file next to it.
    let conn = Connection::open_with_flags(&db, OpenFlags::SQLITE_OPEN_READ_WRITE)
        .map_err(|e| e.to_string())?;
    let mut st = conn.prepare(LOOKUPS).map_err(|e| e.to_string())?;
    let rows = st
        .query_map([since.timestamp_millis()], |r| {
            Ok((
                r.get::<_, String>(0)?,
                r.get::<_, Option<String>>(1)?,
                r.get::<_, Option<String>>(2)?,
                r.get::<_, Option<String>>(3)?,
                r.get::<_, i64>(4)?,
            ))
        })
        .map_err(|e| e.to_string())?;
    let mut out = Vec::new();
    for row in rows {
        let (word, stem, usage, book, ms) = row.map_err(|e| e.to_string())?;
        let stem = stem
            .filter(|s| !s.is_empty())
            .unwrap_or_else(|| word.clone());
        let Some(at) = DateTime::from_timestamp_millis(ms) else {
            continue;
        };
        out.push(WordSnap {
            definition: define(&stem),
            word,
            stem,
            usage: trim_usage(usage.unwrap_or_default().trim()),
            book: book.unwrap_or_default(),
            at: at.format("%Y-%m-%dT%H:%M:%SZ").to_string(),
        });
    }
    Ok(out)
}

/// The longest sentence kept with a looked-up word.
const USAGE_CHARS: usize = 160;

/// `s` cut at a word boundary to at most [`USAGE_CHARS`], with an ellipsis when cut.
fn trim_usage(s: &str) -> String {
    if s.chars().count() <= USAGE_CHARS {
        return s.to_owned();
    }
    let cut: String = s.chars().take(USAGE_CHARS).collect();
    let at = cut.rfind(char::is_whitespace).unwrap_or(cut.len());
    format!("{}\u{2026}", cut[..at].trim_end())
}

/// Looks up a short definition for a word's stem.
pub type Definer = Box<dyn Fn(&str) -> Option<String>>;

/// Short definitions from the Word Wise database already on this Mac (`DECKLE_WORDWISE_DB`, or
/// the cached download), never downloading one; every word is undefined when there is none.
pub fn wordwise_definer() -> Definer {
    let path: PathBuf = std::env::var_os(DB_ENV)
        .filter(|p| !p.is_empty())
        .map(PathBuf::from)
        .unwrap_or_else(|| default_cache_dir().join(DB_FILE));
    let conn = path
        .is_file()
        .then(|| Connection::open_with_flags(&path, OpenFlags::SQLITE_OPEN_READ_ONLY).ok())
        .flatten();
    Box::new(move |stem| {
        conn.as_ref()?
            .query_row(
                "SELECT s.short_def FROM forms f JOIN senses s ON s.id = f.sense_id
                 WHERE f.form = ?1",
                [stem.to_lowercase()],
                |r| r.get::<_, String>(0),
            )
            .ok()
            .filter(|d| !d.is_empty())
    })
}
