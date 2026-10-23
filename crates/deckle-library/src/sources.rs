// SPDX-License-Identifier: GPL-3.0-or-later
//! Where a Kindle book's original might be on this computer (Books and Kindle design §4): a
//! calibre library, opened read-only, and folders the owner names. Deckle's own library is
//! matched earlier, by [`crate::shelf::shelve`].

use std::path::{Path, PathBuf};

use deckle_core::inventory::DeviceBook;
use serde::{Deserialize, Serialize};

use crate::LibraryConfig;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Origin {
    Calibre,
    Folder,
    /// Rebuilt from the Kindle's own DRM-free copy (Books and Kindle design §5).
    #[serde(rename = "kindle-copy")]
    KindleCopy,
}

/// An original found on this computer.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Source {
    pub path: PathBuf,
    pub origin: Origin,
    pub title: String,
    pub authors: Vec<String>,
    pub isbn: Option<String>,
    /// An Amazon ASIN calibre knows for the book (`mobi-asin` or `amazon`).
    pub asin: Option<String>,
    /// calibre's book id (its `uuid`).
    pub calibre_id: Option<String>,
}

/// Formats a calibre book counts as an original, best first.
const CALIBRE_FORMATS: [&str; 3] = ["EPUB", "DOCX", "TXT"];
/// How deep a named folder is searched.
const FOLDER_DEPTH: usize = 4;

/// Every original Deckle can see, and what went wrong looking (a calibre library it couldn't
/// open, say). Nothing here fails a listing.
#[derive(Clone, Debug, Default)]
pub struct Sources {
    pub list: Vec<Source>,
    pub warnings: Vec<String>,
}

/// The user's home folder: `HOME`, else `USERPROFILE` (Windows).
pub fn home() -> Option<PathBuf> {
    std::env::var_os("HOME")
        .or_else(|| std::env::var_os("USERPROFILE"))
        .map(PathBuf::from)
}

/// The calibre library `cfg` points at, else `~/Calibre Library` under `home` when it has a
/// `metadata.db`.
pub fn calibre_root(cfg: &LibraryConfig, home: Option<&Path>) -> Option<PathBuf> {
    if !cfg.find_in_calibre {
        return None;
    }
    cfg.calibre
        .clone()
        .or_else(|| home.map(|h| h.join("Calibre Library")))
        .filter(|r| r.join("metadata.db").is_file())
}

impl Sources {
    /// Gathers the originals in calibre and the configured folders.
    pub fn gather(cfg: &LibraryConfig, home: Option<&Path>) -> Sources {
        let mut out = Sources::default();
        if let Some(root) = calibre_root(cfg, home) {
            match calibre_books(&root) {
                Ok(books) => out.list.extend(books),
                Err(e) => out
                    .warnings
                    .push(format!("calibre library {}: {e}", root.display())),
            }
        }
        for dir in &cfg.find_in {
            if dir.is_dir() {
                out.list.extend(folder_books(dir));
            } else {
                out.warnings
                    .push(format!("{}: not a folder", dir.display()));
            }
        }
        out
    }

    /// The original of `book`, if one is here: by calibre id, ASIN, ISBN, then title and first
    /// author.
    pub fn find(&self, book: &DeviceBook) -> Option<&Source> {
        if let Some(id) = &book.calibre_id
            && let Some(s) = self.list.iter().find(|s| s.calibre_id.as_ref() == Some(id))
        {
            return Some(s);
        }
        if let Some(asin) = book
            .asin
            .as_deref()
            .filter(|a| !deckle_core::inventory::is_generated_asin(a))
            && let Some(s) = self.list.iter().find(|s| {
                s.asin
                    .as_deref()
                    .is_some_and(|x| x.eq_ignore_ascii_case(asin))
            })
        {
            return Some(s);
        }
        if let Some(isbn) = book
            .isbn
            .as_deref()
            .map(isbn_digits)
            .filter(|i| i.len() >= 10)
            && let Some(s) = self
                .list
                .iter()
                .find(|s| s.isbn.as_deref().map(isbn_digits).as_deref() == Some(isbn.as_str()))
        {
            return Some(s);
        }
        let title = norm(book.title.as_deref()?);
        if title.is_empty() {
            return None;
        }
        let author = book.authors.first().map(|a| surname(a));
        self.list.iter().find(|s| {
            norm(&s.title) == title
                && match (&author, s.authors.first()) {
                    (Some(a), Some(b)) => !a.is_empty() && *a == surname(b),
                    // A book with no author on either side matches on title alone.
                    (None, None) => true,
                    _ => false,
                }
        })
    }
}

/// Letters and digits, lower case: "The Count of Monte-Cristo" and "the count of monte cristo"
/// are the same.
fn norm(s: &str) -> String {
    s.chars()
        .filter(|c| c.is_alphanumeric())
        .flat_map(char::to_lowercase)
        .collect()
}

/// An author's surname, normalized, for either "Eliot, George" or "George Eliot".
fn surname(author: &str) -> String {
    let a = author.trim();
    let last = match a.split_once(',') {
        Some((last, _)) => last,
        None => a.rsplit(' ').next().unwrap_or(a),
    };
    norm(last)
}

fn isbn_digits(s: &str) -> String {
    s.chars()
        .filter(|c| c.is_ascii_digit() || *c == 'X' || *c == 'x')
        .collect::<String>()
        .to_uppercase()
}

/// `metadata.db` as a read-only SQLite URI: `immutable=1` takes no locks, so a running calibre
/// is never disturbed and nothing is ever written.
fn calibre_uri(db: &Path) -> String {
    let mut path = db.to_string_lossy().replace('\\', "/");
    if !path.starts_with('/') {
        path.insert(0, '/');
    }
    let mut out = String::from("file://");
    for c in path.chars() {
        match c {
            '%' => out.push_str("%25"),
            ' ' => out.push_str("%20"),
            '?' => out.push_str("%3F"),
            '#' => out.push_str("%23"),
            c => out.push(c),
        }
    }
    out + "?immutable=1"
}

/// Every book in the calibre library at `root` that has an EPUB, DOCX or TXT format.
pub fn calibre_books(root: &Path) -> Result<Vec<Source>, String> {
    use rusqlite::{Connection, OpenFlags};
    let db = Connection::open_with_flags(
        calibre_uri(&root.join("metadata.db")),
        OpenFlags::SQLITE_OPEN_READ_ONLY
            | OpenFlags::SQLITE_OPEN_URI
            | OpenFlags::SQLITE_OPEN_NO_MUTEX,
    )
    .map_err(|e| e.to_string())?;
    let err = |e: rusqlite::Error| e.to_string();
    struct Row {
        id: i64,
        title: String,
        path: String,
        uuid: Option<String>,
    }
    let mut books: Vec<Row> = db
        .prepare("SELECT id, title, path, uuid FROM books")
        .map_err(err)?
        .query_map([], |r| {
            Ok(Row {
                id: r.get(0)?,
                title: r.get(1)?,
                path: r.get(2)?,
                uuid: r.get(3)?,
            })
        })
        .map_err(err)?
        .filter_map(Result::ok)
        .collect();
    books.sort_by_key(|b| b.id);
    let pairs = |sql: &str| -> Result<Vec<(i64, String, String)>, String> {
        Ok(db
            .prepare(sql)
            .map_err(err)?
            .query_map([], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)))
            .map_err(err)?
            .filter_map(Result::ok)
            .collect())
    };
    let authors = pairs(
        "SELECT l.book, a.name, '' FROM books_authors_link l JOIN authors a ON a.id = l.author ORDER BY l.id",
    )?;
    let ids = pairs("SELECT book, type, val FROM identifiers")?;
    let files = pairs("SELECT book, format, name FROM data")?;
    let mut out = Vec::new();
    for b in books {
        let Some((format, name)) = CALIBRE_FORMATS.iter().find_map(|f| {
            files
                .iter()
                .find(|(id, fmt, _)| *id == b.id && fmt.eq_ignore_ascii_case(f))
                .map(|(_, fmt, name)| (fmt.to_ascii_lowercase(), name.clone()))
        }) else {
            continue;
        };
        let path = root.join(&b.path).join(format!("{name}.{format}"));
        let id_of = |kinds: &[&str]| {
            ids.iter()
                .find(|(id, t, _)| *id == b.id && kinds.contains(&t.as_str()))
                .map(|(_, _, v)| v.clone())
        };
        out.push(Source {
            path,
            origin: Origin::Calibre,
            title: b.title,
            authors: authors
                .iter()
                .filter(|(id, _, _)| *id == b.id)
                .map(|(_, a, _)| a.clone())
                .collect(),
            isbn: id_of(&["isbn"]),
            asin: id_of(&["mobi-asin", "amazon"]),
            calibre_id: b.uuid,
        });
    }
    Ok(out)
}

/// The EPUBs under `dir`, a few folders deep, with the title and authors their OPF gives.
pub fn folder_books(dir: &Path) -> Vec<Source> {
    let mut out = Vec::new();
    let mut dirs = vec![(dir.to_path_buf(), 0usize)];
    while let Some((d, depth)) = dirs.pop() {
        let Ok(entries) = std::fs::read_dir(&d) else {
            continue;
        };
        let mut entries: Vec<_> = entries.flatten().collect();
        entries.sort_by_key(|e| e.file_name());
        for e in entries {
            let path = e.path();
            let name = e.file_name().to_string_lossy().into_owned();
            if name.starts_with('.') {
                continue;
            }
            if path.is_dir() {
                if depth < FOLDER_DEPTH {
                    dirs.push((path, depth + 1));
                }
            } else if path
                .extension()
                .is_some_and(|x| x.eq_ignore_ascii_case("epub"))
                && let Ok(info) = deckle_core::inspect::inspect(&path)
            {
                out.push(Source {
                    title: info.title.clone(),
                    authors: info.creators.iter().map(|c| c.name.clone()).collect(),
                    isbn: info.isbn.clone(),
                    asin: None,
                    calibre_id: None,
                    origin: Origin::Folder,
                    path,
                });
            }
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn src(title: &str, author: &str) -> Source {
        Source {
            path: PathBuf::from(format!("/books/{title}.epub")),
            origin: Origin::Folder,
            title: title.into(),
            authors: vec![author.into()],
            isbn: None,
            asin: None,
            calibre_id: None,
        }
    }

    fn book(title: &str, author: &str) -> DeviceBook {
        DeviceBook {
            title: Some(title.into()),
            authors: vec![author.into()],
            ..Default::default()
        }
    }

    #[test]
    fn titles_match_loosely_and_authors_by_surname() {
        let s = Sources {
            list: vec![
                src("The Count of Monte Cristo", "Alexandre Dumas"),
                src("Middlemarch", "George Eliot"),
            ],
            warnings: Vec::new(),
        };
        let found = s
            .find(&book("The Count of Monte-Cristo", "Dumas, Alexandre"))
            .unwrap();
        assert_eq!(found.title, "The Count of Monte Cristo");
        assert!(s.find(&book("Middlemarch", "Someone Else")).is_none());
        assert!(s.find(&book("Daniel Deronda", "George Eliot")).is_none());
    }

    #[test]
    fn ids_win_over_titles() {
        let mut a = src("Retitled", "Nobody");
        a.calibre_id = Some("uuid-1".into());
        let mut b = src("Other", "Nobody");
        b.isbn = Some("978-0-14-143954-9".into());
        let mut c = src("Third", "Nobody");
        c.asin = Some("B000JQUSTM".into());
        let s = Sources {
            list: vec![a, b, c],
            warnings: Vec::new(),
        };
        let mut d = book("Middlemarch", "George Eliot");
        d.calibre_id = Some("uuid-1".into());
        assert_eq!(s.find(&d).unwrap().title, "Retitled");
        d.calibre_id = None;
        d.isbn = Some("9780141439549".into());
        assert_eq!(s.find(&d).unwrap().title, "Other");
        d.isbn = None;
        d.asin = Some("b000jqustm".into());
        assert_eq!(s.find(&d).unwrap().title, "Third");
        // A Deckle ASIN never matches calibre.
        d.asin = Some("B0ABCDDECK".into());
        assert!(s.find(&d).is_none());
    }

    #[test]
    fn a_calibre_library_is_read_without_writing() {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path().join("Calibre Library");
        std::fs::create_dir_all(root.join("George Eliot/Middlemarch (1)")).unwrap();
        let db = rusqlite::Connection::open(root.join("metadata.db")).unwrap();
        db.execute_batch(
            "CREATE TABLE books (id INTEGER PRIMARY KEY, title TEXT, path TEXT, uuid TEXT);
             CREATE TABLE authors (id INTEGER PRIMARY KEY, name TEXT);
             CREATE TABLE books_authors_link (id INTEGER PRIMARY KEY, book INTEGER, author INTEGER);
             CREATE TABLE identifiers (id INTEGER PRIMARY KEY, book INTEGER, type TEXT, val TEXT);
             CREATE TABLE data (id INTEGER PRIMARY KEY, book INTEGER, format TEXT, name TEXT);
             INSERT INTO books VALUES (1, 'Middlemarch', 'George Eliot/Middlemarch (1)', 'uuid-mm');
             INSERT INTO books VALUES (2, 'A PDF Only', 'X/Y (2)', 'uuid-pdf');
             INSERT INTO authors VALUES (1, 'George Eliot');
             INSERT INTO books_authors_link VALUES (1, 1, 1);
             INSERT INTO identifiers VALUES (1, 1, 'isbn', '9780141439549');
             INSERT INTO identifiers VALUES (2, 1, 'mobi-asin', 'B000JQUSTM');
             INSERT INTO data VALUES (1, 1, 'AZW3', 'Middlemarch - George Eliot');
             INSERT INTO data VALUES (2, 1, 'EPUB', 'Middlemarch - George Eliot');
             INSERT INTO data VALUES (3, 2, 'PDF', 'A PDF Only');",
        )
        .unwrap();
        drop(db);
        let before = std::fs::read(root.join("metadata.db")).unwrap();
        let books = calibre_books(&root).unwrap();
        assert_eq!(books.len(), 1, "only books with an EPUB, DOCX or TXT");
        let b = &books[0];
        assert_eq!(b.title, "Middlemarch");
        assert_eq!(b.authors, ["George Eliot"]);
        assert_eq!(b.isbn.as_deref(), Some("9780141439549"));
        assert_eq!(b.asin.as_deref(), Some("B000JQUSTM"));
        assert_eq!(b.calibre_id.as_deref(), Some("uuid-mm"));
        assert!(
            b.path
                .ends_with("George Eliot/Middlemarch (1)/Middlemarch - George Eliot.epub")
        );
        assert_eq!(std::fs::read(root.join("metadata.db")).unwrap(), before);
        assert!(
            !root.join("metadata.db-journal").exists() && !root.join("metadata.db-wal").exists()
        );

        let cfg = LibraryConfig::default();
        assert_eq!(calibre_root(&cfg, Some(tmp.path())), Some(root.clone()));
        let off = LibraryConfig {
            find_in_calibre: false,
            ..Default::default()
        };
        assert_eq!(calibre_root(&off, Some(tmp.path())), None);
        let found = Sources::gather(&cfg, Some(tmp.path()));
        assert_eq!(found.list.len(), 1);
    }

    #[test]
    fn a_uri_survives_spaces_and_odd_characters() {
        assert_eq!(
            calibre_uri(Path::new("/Users/me/Calibre Library/metadata.db")),
            "file:///Users/me/Calibre%20Library/metadata.db?immutable=1"
        );
        assert_eq!(
            calibre_uri(Path::new("/a/100%#?/m.db")),
            "file:///a/100%25%23%3F/m.db?immutable=1"
        );
    }
}
