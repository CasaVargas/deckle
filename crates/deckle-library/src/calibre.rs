// SPDX-License-Identifier: GPL-3.0-or-later
//! What the write-back reads of a calibre library (calibre bridge design §5): its custom
//! columns, its books, and the values they hold now. All from `metadata.db` opened read-only
//! (`immutable=1`), as the originals are found; nothing here writes.

use std::collections::BTreeMap;
use std::path::Path;

use rusqlite::{Connection, OpenFlags, OptionalExtension};
use serde::Serialize;

/// A custom column.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct Column {
    /// Its internal number: the `custom_column_<id>` table.
    pub id: i64,
    /// Its lookup name without the `#`, what `calibredb set_custom` takes.
    pub label: String,
    /// What calibre shows as its heading.
    pub name: String,
    /// `int`, `float`, `bool`, `datetime`, `rating`, `comments`, `text`, `series`,
    /// `enumeration`, `composite`.
    pub datatype: String,
    pub is_multiple: bool,
    pub normalized: bool,
}

/// A book, as the write-back matches it.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct Book {
    pub id: i64,
    pub uuid: String,
    pub title: String,
    pub authors: Vec<String>,
    /// Its formats, upper case: `EPUB`, `AZW3`…
    pub formats: Vec<String>,
}

fn open(root: &Path) -> Result<Connection, String> {
    Connection::open_with_flags(
        crate::sources::calibre_uri(&root.join("metadata.db")),
        OpenFlags::SQLITE_OPEN_READ_ONLY
            | OpenFlags::SQLITE_OPEN_URI
            | OpenFlags::SQLITE_OPEN_NO_MUTEX,
    )
    .map_err(|e| format!("{}: {e}", root.join("metadata.db").display()))
}

/// The library's custom columns, by label.
pub fn columns(root: &Path) -> Result<Vec<Column>, String> {
    let db = open(root)?;
    let mut q = db
        .prepare("SELECT id, label, name, datatype, is_multiple, normalized FROM custom_columns WHERE mark_for_delete = 0 ORDER BY label")
        .map_err(|e| e.to_string())?;
    let rows = q
        .query_map([], |r| {
            Ok(Column {
                id: r.get(0)?,
                label: r.get(1)?,
                name: r.get(2)?,
                datatype: r.get(3)?,
                is_multiple: r.get::<_, Option<bool>>(4)?.unwrap_or(false),
                normalized: r.get::<_, Option<bool>>(5)?.unwrap_or(false),
            })
        })
        .map_err(|e| e.to_string())?;
    rows.collect::<Result<_, _>>().map_err(|e| e.to_string())
}

/// Every book, with its authors in order.
pub fn books(root: &Path) -> Result<Vec<Book>, String> {
    let db = open(root)?;
    let err = |e: rusqlite::Error| e.to_string();
    let mut authors: BTreeMap<i64, Vec<String>> = BTreeMap::new();
    let mut q = db
        .prepare("SELECT l.book, a.name FROM books_authors_link l JOIN authors a ON a.id = l.author ORDER BY l.id")
        .map_err(err)?;
    for row in q
        .query_map([], |r| Ok((r.get::<_, i64>(0)?, r.get::<_, String>(1)?)))
        .map_err(err)?
        .flatten()
    {
        authors.entry(row.0).or_default().push(row.1);
    }
    let mut formats: BTreeMap<i64, Vec<String>> = BTreeMap::new();
    let mut q = db
        .prepare("SELECT book, format FROM data ORDER BY id")
        .map_err(err)?;
    for row in q
        .query_map([], |r| Ok((r.get::<_, i64>(0)?, r.get::<_, String>(1)?)))
        .map_err(err)?
        .flatten()
    {
        formats.entry(row.0).or_default().push(row.1.to_uppercase());
    }
    let mut q = db
        .prepare("SELECT id, uuid, title FROM books ORDER BY id")
        .map_err(err)?;
    let rows = q
        .query_map([], |r| {
            Ok((
                r.get::<_, i64>(0)?,
                r.get::<_, Option<String>>(1)?,
                r.get::<_, String>(2)?,
            ))
        })
        .map_err(err)?;
    Ok(rows
        .flatten()
        .filter_map(|(id, uuid, title)| {
            Some(Book {
                id,
                uuid: uuid?,
                title,
                authors: authors.remove(&id).unwrap_or_default(),
                formats: formats.remove(&id).unwrap_or_default(),
            })
        })
        .collect())
}

/// What `column` holds for `book` now, written as `calibredb set_custom` takes it: `true` or
/// `false`, a number, an ISO time, text; a rating as calibre keeps it, 0–10. `None`: empty.
pub fn value(root: &Path, column: &Column, book: i64) -> Result<Option<String>, String> {
    let db = open(root)?;
    let table = format!("custom_column_{}", column.id);
    let sql = if column.normalized {
        format!(
            "SELECT t.value FROM books_{table}_link l JOIN {table} t ON t.id = l.value WHERE l.book = ?1"
        )
    } else {
        format!("SELECT value FROM {table} WHERE book = ?1")
    };
    let v: Option<rusqlite::types::Value> = db
        .query_row(&sql, [book], |r| r.get(0))
        .optional()
        .map_err(|e| e.to_string())?;
    use rusqlite::types::Value as V;
    Ok(match v {
        None | Some(V::Null) => None,
        Some(V::Integer(i)) if column.datatype == "bool" => Some((i != 0).to_string()),
        Some(V::Integer(i)) => Some(i.to_string()),
        Some(V::Real(f)) if column.datatype == "int" || column.datatype == "rating" => {
            Some((f as i64).to_string())
        }
        Some(V::Real(f)) => Some(f.to_string()),
        Some(V::Text(t)) => Some(t),
        Some(V::Blob(_)) => None,
    })
}

/// A calibre library's `metadata.db` with the tables and columns these read, for tests in
/// this crate and others.
pub mod fixture {
    use std::path::Path;

    /// `(label, name, datatype, normalized)` for each column; books as `(id, uuid, title,
    /// author)`.
    pub fn library(
        root: &Path,
        columns: &[(&str, &str, &str, bool)],
        books: &[(i64, &str, &str, &str)],
    ) {
        std::fs::create_dir_all(root).unwrap();
        let db = rusqlite::Connection::open(root.join("metadata.db")).unwrap();
        db.execute_batch(
            "CREATE TABLE books (id INTEGER PRIMARY KEY, title TEXT, path TEXT, uuid TEXT, series_index REAL DEFAULT 1);
             CREATE TABLE authors (id INTEGER PRIMARY KEY, name TEXT);
             CREATE TABLE books_authors_link (id INTEGER PRIMARY KEY, book INTEGER, author INTEGER);
             CREATE TABLE identifiers (id INTEGER PRIMARY KEY, book INTEGER, type TEXT, val TEXT);
             CREATE TABLE data (id INTEGER PRIMARY KEY, book INTEGER, format TEXT, name TEXT);
             CREATE TABLE custom_columns (id INTEGER PRIMARY KEY, label TEXT, name TEXT, datatype TEXT,
               mark_for_delete BOOL DEFAULT 0, editable BOOL DEFAULT 1, display TEXT DEFAULT '{}',
               is_multiple BOOL DEFAULT 0, normalized BOOL);",
        )
        .unwrap();
        for (i, (label, name, datatype, normalized)) in columns.iter().enumerate() {
            let id = i as i64 + 1;
            db.execute(
                "INSERT INTO custom_columns (id, label, name, datatype, normalized) VALUES (?1, ?2, ?3, ?4, ?5)",
                rusqlite::params![id, label, name, datatype, normalized],
            )
            .unwrap();
            if *normalized {
                db.execute_batch(&format!(
                    "CREATE TABLE custom_column_{id} (id INTEGER PRIMARY KEY, value);
                     CREATE TABLE books_custom_column_{id}_link (id INTEGER PRIMARY KEY, book INTEGER, value INTEGER);"
                ))
                .unwrap();
            } else {
                db.execute_batch(&format!("CREATE TABLE custom_column_{id} (id INTEGER PRIMARY KEY, book INTEGER UNIQUE, value);"))
                    .unwrap();
            }
        }
        for (id, uuid, title, author) in books {
            db.execute(
                "INSERT INTO books (id, title, path, uuid) VALUES (?1, ?2, ?3, ?4)",
                rusqlite::params![id, title, format!("{author}/{title} ({id})"), uuid],
            )
            .unwrap();
            db.execute(
                "INSERT INTO authors (id, name) VALUES (?1, ?2)",
                rusqlite::params![id, author],
            )
            .unwrap();
            db.execute(
                "INSERT INTO books_authors_link (book, author) VALUES (?1, ?1)",
                [id],
            )
            .unwrap();
        }
    }

    /// Gives a book a format.
    pub fn format(root: &Path, book: i64, format: &str) {
        let db = rusqlite::Connection::open(root.join("metadata.db")).unwrap();
        db.execute(
            "INSERT INTO data (book, format, name) VALUES (?1, ?2, 'x')",
            rusqlite::params![book, format],
        )
        .unwrap();
    }

    /// Sets a plain (not normalized) column's value, as calibre would.
    pub fn set(root: &Path, column_id: i64, book: i64, value: rusqlite::types::Value) {
        let db = rusqlite::Connection::open(root.join("metadata.db")).unwrap();
        db.execute(
            &format!(
                "INSERT OR REPLACE INTO custom_column_{column_id} (book, value) VALUES (?1, ?2)"
            ),
            rusqlite::params![book, value],
        )
        .unwrap();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rusqlite::types::Value as V;

    #[test]
    fn columns_books_and_values_read_as_calibredb_takes_them() {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path();
        fixture::library(
            root,
            &[
                ("read", "Read", "bool", false),
                ("progress", "Progress", "int", false),
                ("stars", "Stars", "rating", true),
            ],
            &[
                (1, "u-1", "Dune", "Frank Herbert"),
                (2, "u-2", "Emma", "Jane Austen"),
            ],
        );
        fixture::set(root, 1, 1, V::Integer(1));
        fixture::set(root, 2, 1, V::Integer(42));
        let db = Connection::open(root.join("metadata.db")).unwrap();
        db.execute("INSERT INTO custom_column_3 (id, value) VALUES (7, 8)", [])
            .unwrap();
        db.execute(
            "INSERT INTO books_custom_column_3_link (book, value) VALUES (1, 7)",
            [],
        )
        .unwrap();
        drop(db);

        let cols = columns(root).unwrap();
        assert_eq!(
            cols.iter().map(|c| c.label.as_str()).collect::<Vec<_>>(),
            ["progress", "read", "stars"]
        );
        let by = |l: &str| cols.iter().find(|c| c.label == l).unwrap().clone();
        assert_eq!(
            value(root, &by("read"), 1).unwrap().as_deref(),
            Some("true")
        );
        assert_eq!(
            value(root, &by("progress"), 1).unwrap().as_deref(),
            Some("42")
        );
        assert_eq!(value(root, &by("stars"), 1).unwrap().as_deref(), Some("8"));
        assert_eq!(value(root, &by("progress"), 2).unwrap(), None);
        let b = books(root).unwrap();
        assert_eq!(
            b[1],
            Book {
                id: 2,
                uuid: "u-2".into(),
                title: "Emma".into(),
                authors: vec!["Jane Austen".into()],
                formats: vec![]
            }
        );
    }
}
