// SPDX-License-Identifier: GPL-3.0-or-later
//! Kobo e-readers (docs/superpowers/specs/2026-10-23-other-readers-design.md §8): found as a USB
//! drive with a `.kobo/` folder, sent KEPUBs (through [`crate::reader`], which delivers and
//! prunes), and read for highlights and books in progress from `.kobo/KoboReader.sqlite`.
//!
//! The database is never opened where it lies: it is copied out first and the copy opened
//! read-only, so the Kobo's own file is never locked or changed. Its tables are Kobo's, not
//! documented by Kobo; what's read here follows what the e-reader community has long relied on
//! and is checked on a device by spike 1 (§11). A table or column that isn't there makes that
//! read report an error, never a failed delivery.

use std::path::{Path, PathBuf};

use serde::Serialize;

/// A Kobo mounted as a drive.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct Kobo {
    pub root: PathBuf,
    /// The serial number from `.kobo/version`, when it reads.
    pub serial: Option<String>,
    /// The firmware version from `.kobo/version`.
    pub firmware: Option<String>,
    /// Kobo's model id from `.kobo/version` (its last field).
    pub model_id: Option<String>,
}

impl Kobo {
    /// The key `[sync.devices."<id>"]` and the device registry use: `kobo/<serial>`, else the
    /// volume name.
    pub fn id(&self) -> String {
        match self.serial.as_deref().filter(|s| !s.is_empty()) {
            Some(s) => format!("kobo/{s}"),
            None => format!(
                "kobo/{}",
                self.root.file_name().unwrap_or_default().to_string_lossy()
            ),
        }
    }

    /// What the reports call it.
    pub fn name(&self) -> String {
        "Kobo".to_owned()
    }
}

/// Kobos among explicit mount roots: a `.kobo/` folder holding `KoboReader.sqlite`.
pub fn detect_in(roots: &[PathBuf]) -> Vec<Kobo> {
    roots
        .iter()
        .filter(|r| r.join(".kobo").join("KoboReader.sqlite").is_file())
        .map(|root| {
            let version = std::fs::read_to_string(root.join(".kobo").join("version"))
                .ok()
                .unwrap_or_default();
            let (serial, firmware, model_id) = parse_version(&version);
            Kobo {
                root: root.clone(),
                serial,
                firmware,
                model_id,
            }
        })
        .collect()
}

/// Connected Kobos, on the same mount points as a Kindle ([`crate::deliver::mount_roots`]).
pub fn detect() -> Vec<Kobo> {
    detect_in(&crate::deliver::mount_roots())
}

/// `.kobo/version`: one line of comma-separated fields, the serial first, the firmware third
/// and the model id last (`N418170012345,4.1.15,4.38.21908,4.1.15,4.1.15,00000000-…-390`).
pub fn parse_version(text: &str) -> (Option<String>, Option<String>, Option<String>) {
    let line = text.lines().next().unwrap_or_default();
    let fields: Vec<&str> = line.split(',').map(str::trim).collect();
    let field = |i: usize| {
        fields
            .get(i)
            .filter(|f| !f.is_empty())
            .map(|f| (*f).to_owned())
    };
    let model = (fields.len() > 3)
        .then(|| field(fields.len() - 1))
        .flatten();
    (field(0), field(2), model)
}

/// A book in progress on the Kobo.
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct Book {
    /// Kobo's id for it: a `file:///mnt/onboard/…` path for a sideloaded book.
    pub content_id: String,
    pub title: String,
    pub authors: Vec<String>,
    /// How far through it, 0–100.
    pub percent: f64,
    /// When it was last read, `YYYY-MM-DDTHH:MM:SSZ`, when the Kobo says.
    pub last_read: Option<String>,
}

/// What one read of the Kobo's database found.
#[derive(Clone, Debug, Default)]
pub struct Read {
    /// Books being read, most recently read first.
    pub books: Vec<Book>,
    /// The highlights and notes, written as a Kindle's `My Clippings.txt`, so the highlights
    /// export and From Your Shelf read them as they read a Kindle's.
    pub clippings: Option<Vec<u8>>,
    /// Every sideloaded book opened or finished (calibre bridge design §5.1).
    pub records: Vec<crate::reading::Record>,
}

/// Reads books in progress and highlights from a copy of `.kobo/KoboReader.sqlite`.
pub fn read(kobo: &Kobo) -> Result<Read, String> {
    let db = kobo.root.join(".kobo").join("KoboReader.sqlite");
    let tmp = tempfile::tempdir().map_err(|e| e.to_string())?;
    let copy = tmp.path().join("KoboReader.sqlite");
    std::fs::copy(&db, &copy).map_err(|e| format!("{}: {e}", db.display()))?;
    // A write-ahead log holds the newest changes; it travels with the copy when there is one.
    for side in ["-wal", "-shm"] {
        let from = db.with_file_name(format!("KoboReader.sqlite{side}"));
        if from.is_file() {
            let _ = std::fs::copy(&from, tmp.path().join(format!("KoboReader.sqlite{side}")));
        }
    }
    let mut read = read_db(&copy)?;
    for r in &mut read.records {
        r.calibre_id = r
            .path
            .as_deref()
            .and_then(|p| crate::reading::calibre_id_of(&kobo.root, p));
    }
    Ok(read)
}

/// [`read`] on a database file already copied out.
pub fn read_db(path: &Path) -> Result<Read, String> {
    let conn = rusqlite::Connection::open_with_flags(
        path,
        rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY | rusqlite::OpenFlags::SQLITE_OPEN_NO_MUTEX,
    )
    .map_err(|e| format!("KoboReader.sqlite: {e}"))?;
    let books = books(&conn).map_err(|e| format!("books: {e}"))?;
    let clippings = clippings(&conn).map_err(|e| format!("highlights: {e}"))?;
    // An older database without a column the record wants still gives books and highlights.
    let records = records(&conn).unwrap_or_default();
    Ok(Read {
        books,
        clippings,
        records,
    })
}

/// Sideloaded books (`file:///mnt/onboard/…`) opened (`ReadStatus` 1) or finished (2).
fn records(conn: &rusqlite::Connection) -> rusqlite::Result<Vec<crate::reading::Record>> {
    let mut q = conn.prepare(
        "SELECT ContentID, Title, Attribution, ___PercentRead, DateLastRead, ReadStatus \
         FROM content WHERE ContentType = 6 AND ReadStatus IN (1, 2) \
         AND ContentID LIKE 'file:///mnt/onboard/%' ORDER BY DateLastRead DESC",
    )?;
    let rows = q.query_map([], |r| {
        let id: String = r.get(0)?;
        let finished = r.get::<_, Option<i64>>(5)? == Some(2);
        Ok(crate::reading::Record {
            title: r.get::<_, Option<String>>(1)?.unwrap_or_default(),
            authors: authors(&r.get::<_, Option<String>>(2)?.unwrap_or_default()),
            calibre_id: None,
            path: id.strip_prefix("file:///mnt/onboard/").map(str::to_owned),
            percent: Some(if finished {
                100.0
            } else {
                r.get::<_, Option<f64>>(3)?.unwrap_or(0.0).clamp(0.0, 100.0)
            }),
            finished,
            last_read: r.get::<_, Option<String>>(4)?.as_deref().and_then(iso_utc),
            rating: None,
            from: "kobo".into(),
        })
    })?;
    rows.filter(|b| b.as_ref().map_or(true, |b| !b.title.trim().is_empty()))
        .collect()
}

/// `content` rows of books (`ContentType = 6`) being read (`ReadStatus = 1`).
fn books(conn: &rusqlite::Connection) -> rusqlite::Result<Vec<Book>> {
    let mut q = conn.prepare(
        "SELECT ContentID, Title, Attribution, ___PercentRead, DateLastRead FROM content \
         WHERE ContentType = 6 AND ReadStatus = 1 ORDER BY DateLastRead DESC",
    )?;
    let rows = q.query_map([], |r| {
        Ok(Book {
            content_id: r.get::<_, String>(0)?,
            title: r.get::<_, Option<String>>(1)?.unwrap_or_default(),
            authors: authors(&r.get::<_, Option<String>>(2)?.unwrap_or_default()),
            percent: r.get::<_, Option<f64>>(3)?.unwrap_or(0.0).clamp(0.0, 100.0),
            last_read: r.get::<_, Option<String>>(4)?.as_deref().and_then(iso_utc),
        })
    })?;
    rows.filter(|b| b.as_ref().map_or(true, |b| !b.title.trim().is_empty()))
        .collect()
}

/// `Attribution` names the authors, comma-separated.
fn authors(attribution: &str) -> Vec<String> {
    attribution
        .split(',')
        .map(str::trim)
        .filter(|a| !a.is_empty())
        .map(str::to_owned)
        .collect()
}

/// Kobo's `2026-10-20T21:15:03.000` (or with a `Z`, or a space) as `2026-10-20T21:15:03Z`.
fn iso_utc(raw: &str) -> Option<String> {
    let raw = raw.trim();
    let date = raw.get(..10)?;
    let time = raw.get(11..19)?;
    let ok = date.as_bytes().iter().enumerate().all(|(i, b)| match i {
        4 | 7 => *b == b'-',
        _ => b.is_ascii_digit(),
    }) && time.as_bytes().iter().enumerate().all(|(i, b)| match i {
        2 | 5 => *b == b':',
        _ => b.is_ascii_digit(),
    });
    ok.then(|| format!("{date}T{time}Z"))
}

/// Every highlight and note in `Bookmark`, oldest first, as `My Clippings.txt` entries.
/// `None` when there are none, as a Kindle with no clippings file.
fn clippings(conn: &rusqlite::Connection) -> rusqlite::Result<Option<Vec<u8>>> {
    let mut q = conn.prepare(
        "SELECT c.Title, c.Attribution, b.Text, b.Annotation, b.DateCreated \
         FROM Bookmark b JOIN content c ON c.ContentID = b.VolumeID AND c.ContentType = 6 \
         WHERE (b.Text IS NOT NULL AND trim(b.Text) <> '') \
            OR (b.Annotation IS NOT NULL AND trim(b.Annotation) <> '') \
         ORDER BY b.DateCreated",
    )?;
    let rows = q.query_map([], |r| {
        Ok((
            r.get::<_, Option<String>>(0)?.unwrap_or_default(),
            r.get::<_, Option<String>>(1)?.unwrap_or_default(),
            r.get::<_, Option<String>>(2)?.unwrap_or_default(),
            r.get::<_, Option<String>>(3)?.unwrap_or_default(),
            r.get::<_, Option<String>>(4)?.unwrap_or_default(),
        ))
    })?;
    let mut out = String::new();
    for row in rows {
        let (title, author, text, note, created) = row?;
        out.push_str(&clipping(
            &title,
            &author,
            "Highlight",
            None,
            &text,
            &created,
        ));
        out.push_str(&clipping(&title, &author, "Note", None, &note, &created));
    }
    Ok((!out.is_empty()).then(|| out.into_bytes()))
}

/// One `My Clippings.txt` entry as a Kindle writes it, or nothing when `title` or `body` is
/// blank. `kind` is `Highlight` or `Note`; `created` is `YYYY-MM-DD[T ]HH:MM:SS`.
pub(crate) fn clipping(
    title: &str,
    author: &str,
    kind: &str,
    page: Option<&str>,
    body: &str,
    created: &str,
) -> String {
    let (title, body) = (title.trim(), body.trim());
    if title.is_empty() || body.is_empty() {
        return String::new();
    }
    let head = match author.trim() {
        "" => title.to_owned(),
        a => format!("{title} ({a})"),
    };
    let page = page.map(|p| format!(" on page {p}")).unwrap_or_default();
    let added = english_date(created)
        .map(|d| format!(" | Added on {d}"))
        .unwrap_or_default();
    format!("{head}\n- Your {kind}{page}{added}\n\n{body}\n==========\n")
}

const MONTHS: [&str; 12] = [
    "January",
    "February",
    "March",
    "April",
    "May",
    "June",
    "July",
    "August",
    "September",
    "October",
    "November",
    "December",
];
const WEEKDAYS: [&str; 7] = [
    "Sunday",
    "Monday",
    "Tuesday",
    "Wednesday",
    "Thursday",
    "Friday",
    "Saturday",
];

/// Kobo's `2026-10-20T21:15:03.000` as a Kindle writes it: `Tuesday, October 20, 2026
/// 9:15:03 PM`.
fn english_date(raw: &str) -> Option<String> {
    let iso = iso_utc(raw)?;
    let num = |a: usize, b: usize| iso.get(a..b)?.parse::<u32>().ok();
    let (y, m, d) = (num(0, 4)?, num(5, 7)?, num(8, 10)?);
    let (h, min, s) = (num(11, 13)?, num(14, 16)?, num(17, 19)?);
    if !(1..=12).contains(&m) || !(1..=31).contains(&d) {
        return None;
    }
    let (h12, ampm) = match h {
        0 => (12, "AM"),
        1..=11 => (h, "AM"),
        12 => (12, "PM"),
        _ => (h - 12, "PM"),
    };
    Some(format!(
        "{}, {} {d}, {y} {h12}:{min:02}:{s:02} {ampm}",
        WEEKDAYS[weekday(y, m, d)],
        MONTHS[m as usize - 1]
    ))
}

/// 0 = Sunday (Sakamoto's method).
fn weekday(y: u32, m: u32, d: u32) -> usize {
    const T: [u32; 12] = [0, 3, 2, 5, 0, 3, 5, 1, 4, 6, 2, 4];
    let y = if m < 3 { y - 1 } else { y };
    ((y + y / 4 - y / 100 + y / 400 + T[m as usize - 1] + d) % 7) as usize
}

#[cfg(test)]
pub mod tests {
    use super::*;

    /// A Kobo on disk: `.kobo/` with a database holding what `sql` inserts.
    pub fn fake_kobo(root: &Path, sql: &str) -> Kobo {
        std::fs::create_dir_all(root.join(".kobo")).unwrap();
        std::fs::write(
            root.join(".kobo/version"),
            "N418170012345,4.1.15,4.38.21908,4.1.15,4.1.15,00000000-0000-0000-0000-000000000390\n",
        )
        .unwrap();
        let conn = rusqlite::Connection::open(root.join(".kobo/KoboReader.sqlite")).unwrap();
        conn.execute_batch(&format!("{SCHEMA}\n{sql}")).unwrap();
        detect_in(&[root.to_path_buf()]).remove(0)
    }

    /// The columns read here, as Kobo names them.
    pub const SCHEMA: &str = "CREATE TABLE content (ContentID TEXT, ContentType TEXT, \
         Title TEXT, Attribution TEXT, ___PercentRead INTEGER, DateLastRead TEXT, \
         ReadStatus INTEGER, BookID TEXT); \
         CREATE TABLE Bookmark (BookmarkID TEXT, VolumeID TEXT, ContentID TEXT, Text TEXT, \
         Annotation TEXT, DateCreated TEXT);";

    #[test]
    fn sideloaded_books_opened_or_finished_are_records() {
        let tmp = tempfile::tempdir().unwrap();
        let k = fake_kobo(
            tmp.path(),
            "INSERT INTO content VALUES ('file:///mnt/onboard/Books/Dune.epub', '6', 'Dune', 'Frank Herbert', 37, '2026-10-20T21:15:03.000', 1, NULL);
             INSERT INTO content VALUES ('file:///mnt/onboard/Books/Emma.epub', '6', 'Emma', 'Jane Austen', 12, '2026-10-21T08:00:00.000', 2, NULL);
             INSERT INTO content VALUES ('file:///mnt/onboard/Books/New.epub', '6', 'New', 'A', 0, NULL, 0, NULL);
             INSERT INTO content VALUES ('a-store-book-id', '6', 'Bought', 'B', 50, '2026-10-22T08:00:00.000', 1, NULL);",
        );
        let r = read(&k).unwrap();
        let got: Vec<(&str, Option<&str>, Option<f64>, bool)> = r
            .records
            .iter()
            .map(|x| (x.title.as_str(), x.path.as_deref(), x.percent, x.finished))
            .collect();
        assert_eq!(
            got,
            [
                ("Emma", Some("Books/Emma.epub"), Some(100.0), true),
                ("Dune", Some("Books/Dune.epub"), Some(37.0), false),
            ]
        );
        assert_eq!(
            r.records[1].last_read.as_deref(),
            Some("2026-10-20T21:15:03Z")
        );
        assert!(
            r.records
                .iter()
                .all(|x| x.from == "kobo" && x.calibre_id.is_none())
        );
    }

    #[test]
    fn a_kobo_is_found_by_its_database_and_named_by_its_version_file() {
        let tmp = tempfile::tempdir().unwrap();
        let k = fake_kobo(tmp.path(), "");
        assert_eq!(k.serial.as_deref(), Some("N418170012345"));
        assert_eq!(k.firmware.as_deref(), Some("4.38.21908"));
        assert_eq!(
            k.model_id.as_deref(),
            Some("00000000-0000-0000-0000-000000000390")
        );
        assert_eq!(k.id(), "kobo/N418170012345");
        // A Kindle, or a drive with an empty `.kobo/`, isn't one.
        let kindle = tmp.path().join("kindle");
        std::fs::create_dir_all(kindle.join("documents")).unwrap();
        std::fs::create_dir_all(kindle.join("system")).unwrap();
        let empty = tmp.path().join("empty");
        std::fs::create_dir_all(empty.join(".kobo")).unwrap();
        assert!(detect_in(&[kindle, empty]).is_empty());
    }

    #[test]
    fn a_kobo_without_a_version_file_is_named_by_its_volume() {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path().join("KOBOeReader");
        fake_kobo(&root, "");
        std::fs::remove_file(root.join(".kobo/version")).unwrap();
        let k = detect_in(std::slice::from_ref(&root)).remove(0);
        assert_eq!(k.serial, None);
        assert_eq!(k.id(), "kobo/KOBOeReader");
    }

    const BOOKS: &str = "INSERT INTO content VALUES \
        ('file:///mnt/onboard/Deckle/mill.kepub.epub', '6', 'The Mill', 'Ann Lee, Bo Ray', 42, \
         '2026-10-20T21:15:03.000', 1, NULL), \
        ('file:///mnt/onboard/done.epub', '6', 'Done', 'X', 100, '2026-10-01T08:00:00Z', 2, NULL), \
        ('file:///mnt/onboard/new.epub', '6', 'New', 'Y', 0, NULL, 0, NULL), \
        ('file:///mnt/onboard/Deckle/mill.kepub.epub#ch1', '9', 'Chapter 1', NULL, 100, NULL, 0, \
         'file:///mnt/onboard/Deckle/mill.kepub.epub');";

    #[test]
    fn books_in_progress_are_read_with_their_percent() {
        let tmp = tempfile::tempdir().unwrap();
        let k = fake_kobo(tmp.path(), BOOKS);
        let r = read(&k).unwrap();
        assert_eq!(
            r.books,
            [Book {
                content_id: "file:///mnt/onboard/Deckle/mill.kepub.epub".into(),
                title: "The Mill".into(),
                authors: vec!["Ann Lee".into(), "Bo Ray".into()],
                percent: 42.0,
                last_read: Some("2026-10-20T21:15:03Z".into()),
            }]
        );
        assert_eq!(r.clippings, None);
    }

    #[test]
    fn highlights_and_notes_come_out_as_clippings() {
        let tmp = tempfile::tempdir().unwrap();
        let sql = format!(
            "{BOOKS} INSERT INTO Bookmark VALUES \
             ('b1', 'file:///mnt/onboard/Deckle/mill.kepub.epub', 'x#ch1', 'The wheel turned.', \
              NULL, '2026-10-20T21:15:03.000'), \
             ('b2', 'file:///mnt/onboard/Deckle/mill.kepub.epub', 'x#ch2', 'It stopped.', \
              'Why?', '2026-10-21T09:05:00Z'), \
             ('b3', 'file:///mnt/onboard/Deckle/mill.kepub.epub', 'x#ch2', NULL, NULL, \
              '2026-10-21T09:06:00Z');"
        );
        let k = fake_kobo(tmp.path(), &sql);
        let text = String::from_utf8(read(&k).unwrap().clippings.unwrap()).unwrap();
        assert_eq!(
            text,
            "The Mill (Ann Lee, Bo Ray)\n- Your Highlight | Added on Tuesday, October 20, 2026 \
             9:15:03 PM\n\nThe wheel turned.\n==========\n\
             The Mill (Ann Lee, Bo Ray)\n- Your Highlight | Added on Wednesday, October 21, 2026 \
             9:05:00 AM\n\nIt stopped.\n==========\n\
             The Mill (Ann Lee, Bo Ray)\n- Your Note | Added on Wednesday, October 21, 2026 \
             9:05:00 AM\n\nWhy?\n==========\n"
        );
    }

    #[test]
    fn the_kobos_own_database_is_left_untouched() {
        let tmp = tempfile::tempdir().unwrap();
        let k = fake_kobo(tmp.path(), BOOKS);
        let db = tmp.path().join(".kobo/KoboReader.sqlite");
        let before = std::fs::read(&db).unwrap();
        read(&k).unwrap();
        assert_eq!(std::fs::read(&db).unwrap(), before);
        assert!(!db.with_file_name("KoboReader.sqlite-journal").exists());
    }

    #[test]
    fn a_database_without_kobos_tables_is_an_error_not_a_panic() {
        let tmp = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(tmp.path().join(".kobo")).unwrap();
        rusqlite::Connection::open(tmp.path().join(".kobo/KoboReader.sqlite"))
            .unwrap()
            .execute_batch("CREATE TABLE other (x INTEGER);")
            .unwrap();
        let k = detect_in(&[tmp.path().to_path_buf()]).remove(0);
        assert!(read(&k).unwrap_err().starts_with("books:"));
    }

    #[test]
    fn dates_and_weekdays() {
        assert_eq!(
            english_date("2026-10-23T00:01:02"),
            Some("Friday, October 23, 2026 12:01:02 AM".into())
        );
        assert_eq!(
            english_date("2024-02-29 12:00:00"),
            Some("Thursday, February 29, 2024 12:00:00 PM".into())
        );
        assert_eq!(english_date("yesterday"), None);
    }

    #[test]
    fn version_lines() {
        assert_eq!(parse_version(""), (None, None, None));
        assert_eq!(
            parse_version("N1,a,4.1,b,c,MODEL"),
            (Some("N1".into()), Some("4.1".into()), Some("MODEL".into()))
        );
    }
}
