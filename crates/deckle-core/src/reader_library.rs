// SPDX-License-Identifier: GPL-3.0-or-later
//! What's on a Kobo, a PocketBook or a reMarkable, for the Devices tab: every book with how
//! far it's read, and the two things an owner does with a library, make room and keep a copy.
//!
//! - **Kobo and PocketBook** are drives. The books are the files on them; a Kobo's database
//!   (`.kobo/KoboReader.sqlite`, read from a copy, never in place) adds titles, authors and
//!   progress, KOReader's sidecars add progress on either. A book is removed by deleting its
//!   file and its KOReader sidecar; on a Kobo the database is copied aside first, and the Kobo
//!   drops the book from its library when it next scans for new content (on unplugging).
//!   Books bought from Kobo live in Kobo's own storage, not as files: they're listed, never
//!   removed here.
//! - **A reMarkable over its USB web interface** lists folders and documents and hands each
//!   one back as a PDF with its handwriting drawn in; it can't delete.
//! - **A reMarkable over SSH** lists every document from its `.metadata` files, removes one by
//!   marking it deleted (as the tablet's own trash does), and backs one up as a `.tar` of its
//!   files (the document, its pages and its handwriting), which is what xochitl keeps.

use std::collections::HashSet;
use std::path::{Path, PathBuf};

use serde::Serialize;

use crate::reader::Reader;
use crate::remarkable::{self, Remarkable};
use crate::remarkable_ssh::{self, Shell};

/// One book or document on a reader.
#[derive(Clone, Debug, Default, PartialEq, Serialize)]
pub struct Book {
    /// What removing or backing it up names it by: its path on the drive (`/`-separated, from
    /// the drive's top), a Kobo store book's content id, or a reMarkable document's id.
    pub id: String,
    pub title: String,
    pub authors: Vec<String>,
    /// The folder it's in (`/`-separated), empty at the top.
    pub folder: String,
    /// `epub`, `kepub`, `pdf`, `notebook`, …; empty when unknown.
    pub format: String,
    pub bytes: Option<u64>,
    /// 0 to 100.
    pub percent: Option<f64>,
    pub finished: bool,
    /// RFC 3339, UTC.
    pub last_read: Option<String>,
    /// Whether Deckle can remove it here.
    pub removable: bool,
    /// Why not, or anything else worth a line.
    pub note: Option<String>,
}

/// A reader's library.
#[derive(Clone, Debug, Default, PartialEq, Serialize)]
pub struct Library {
    /// Most recently read first, then by title.
    pub books: Vec<Book>,
    pub warnings: Vec<String>,
    pub total: Option<u64>,
    pub free: Option<u64>,
}

/// What a remove did.
#[derive(Clone, Debug, Default, PartialEq, Serialize)]
pub struct Removed {
    pub removed: Vec<String>,
    /// `(id, why)`
    pub failed: Vec<(String, String)>,
    /// Where the Kobo's database was copied before anything was deleted.
    pub database_backup: Option<PathBuf>,
}

/// What a backup did.
#[derive(Clone, Debug, Default, PartialEq, Serialize)]
pub struct Backup {
    pub dir: PathBuf,
    pub copied: Vec<String>,
    /// `(id, why)`
    pub failed: Vec<(String, String)>,
}

/// The extensions read as books on a drive.
const BOOK_EXTENSIONS: &[&str] = &[
    "epub", "pdf", "mobi", "azw3", "fb2", "djvu", "djv", "cbz", "cbr", "txt", "rtf", "doc", "docx",
    "html", "htm", "chm", "odt",
];

/// Folders at a drive's top that are the reader's own, not books.
const SYSTEM_FOLDERS: &[&str] = &["system", "System Volume Information", "dictionaries"];

/// How deep and how wide a drive is walked: a reader is never this big, a mistaken drive may be.
const MAX_DEPTH: usize = 8;
const MAX_ENTRIES: usize = 50_000;

fn is_book_file(name: &str) -> bool {
    !name.starts_with('.')
        && name
            .rsplit_once('.')
            .is_some_and(|(_, ext)| BOOK_EXTENSIONS.contains(&ext.to_ascii_lowercase().as_str()))
}

fn format_of(name: &str) -> String {
    let lower = name.to_ascii_lowercase();
    if lower.ends_with(".kepub.epub") || lower.ends_with(".kepub") {
        return "kepub".into();
    }
    lower
        .rsplit_once('.')
        .map(|(_, e)| e.to_owned())
        .unwrap_or_default()
}

/// The title a file gives when nothing else does: its name without the extension(s).
fn title_of_file(name: &str) -> String {
    let stem = name
        .strip_suffix(".kepub.epub")
        .or_else(|| name.rsplit_once('.').map(|(s, _)| s))
        .unwrap_or(name);
    stem.replace('_', " ").trim().to_owned()
}

/// Every book file on the drive at `root`: `(path from the top, bytes)`.
fn book_files(root: &Path) -> Vec<(String, u64)> {
    let mut out = Vec::new();
    let mut seen = 0usize;
    let mut stack = vec![(root.to_path_buf(), String::new(), 0usize)];
    while let Some((dir, rel, depth)) = stack.pop() {
        let Ok(rd) = std::fs::read_dir(&dir) else {
            continue;
        };
        for entry in rd.flatten() {
            seen += 1;
            if seen > MAX_ENTRIES {
                return out;
            }
            let name = entry.file_name().to_string_lossy().into_owned();
            if name.starts_with('.') {
                continue;
            }
            let path_rel = if rel.is_empty() {
                name.clone()
            } else {
                format!("{rel}/{name}")
            };
            let Ok(kind) = entry.file_type() else {
                continue;
            };
            if kind.is_dir() {
                let system = depth == 0 && SYSTEM_FOLDERS.contains(&name.as_str());
                // a KOReader sidecar is the book's, not a folder of books
                if !system && !name.ends_with(".sdr") && depth + 1 < MAX_DEPTH {
                    stack.push((entry.path(), path_rel, depth + 1));
                }
            } else if kind.is_file() && is_book_file(&name) {
                let bytes = entry.metadata().map(|m| m.len()).unwrap_or(0);
                out.push((path_rel, bytes));
            }
        }
    }
    out
}

/// A Kobo's book rows: `(content id, title, authors, percent, last read, finished, bytes)`.
type KoboRow = (
    String,
    String,
    Vec<String>,
    f64,
    Option<String>,
    bool,
    Option<u64>,
);

fn kobo_rows(db: &Path) -> Result<Vec<KoboRow>, String> {
    let tmp = tempfile::tempdir().map_err(|e| e.to_string())?;
    let copy = tmp.path().join("KoboReader.sqlite");
    std::fs::copy(db, &copy).map_err(|e| format!("{}: {e}", db.display()))?;
    for side in ["-wal", "-shm"] {
        let from = db.with_file_name(format!("KoboReader.sqlite{side}"));
        if from.is_file() {
            let _ = std::fs::copy(&from, tmp.path().join(format!("KoboReader.sqlite{side}")));
        }
    }
    let conn = rusqlite::Connection::open_with_flags(
        &copy,
        rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY | rusqlite::OpenFlags::SQLITE_OPEN_NO_MUTEX,
    )
    .map_err(|e| format!("KoboReader.sqlite: {e}"))?;
    let read = |with_size: bool| -> rusqlite::Result<Vec<KoboRow>> {
        let size = if with_size { "___FileSize" } else { "NULL" };
        let mut q = conn.prepare(&format!(
            "SELECT ContentID, Title, Attribution, ___PercentRead, DateLastRead, ReadStatus, \
             {size} FROM content WHERE ContentType = 6"
        ))?;
        let rows = q.query_map([], |r| {
            Ok((
                r.get::<_, String>(0)?,
                r.get::<_, Option<String>>(1)?.unwrap_or_default(),
                crate::kobo::authors(&r.get::<_, Option<String>>(2)?.unwrap_or_default()),
                r.get::<_, Option<f64>>(3)?.unwrap_or(0.0).clamp(0.0, 100.0),
                r.get::<_, Option<String>>(4)?
                    .as_deref()
                    .and_then(crate::kobo::iso_utc),
                r.get::<_, Option<i64>>(5)? == Some(2),
                r.get::<_, Option<i64>>(6)?
                    .and_then(|n| u64::try_from(n).ok()),
            ))
        })?;
        rows.collect()
    };
    // an older database without the size column still lists its books
    read(true)
        .or_else(|_| read(false))
        .map_err(|e| format!("KoboReader.sqlite: {e}"))
}

const ONBOARD: &str = "file:///mnt/onboard/";

/// What's on a Kobo or a PocketBook.
pub fn on_drive(reader: &Reader) -> Library {
    let root = reader.root();
    let mut lib = Library::default();
    if let Some(s) = space(root) {
        lib.total = Some(s.0);
        lib.free = Some(s.1);
    }
    let ko = crate::koreader::read(root);
    lib.warnings
        .extend(ko.warnings.iter().take(3).map(|w| format!("KOReader: {w}")));
    let mut rows: Vec<KoboRow> = Vec::new();
    if let Reader::Kobo(k) = reader {
        match kobo_rows(&k.root.join(".kobo").join("KoboReader.sqlite")) {
            Ok(r) => rows = r,
            Err(e) => lib.warnings.push(e),
        }
    }
    for (rel, bytes) in book_files(root) {
        let name = rel.rsplit('/').next().unwrap_or(&rel).to_owned();
        let folder = rel
            .rsplit_once('/')
            .map(|(f, _)| f.to_owned())
            .unwrap_or_default();
        let mut book = Book {
            id: rel.clone(),
            title: title_of_file(&name),
            folder,
            format: format_of(&name),
            bytes: Some(bytes),
            removable: true,
            ..Default::default()
        };
        if let Some(r) = rows
            .iter()
            .find(|r| r.0.strip_prefix(ONBOARD) == Some(rel.as_str()))
        {
            if !r.1.trim().is_empty() {
                book.title = r.1.clone();
            }
            book.authors = r.2.clone();
            book.finished = r.5;
            book.percent = Some(if r.5 { 100.0 } else { r.3 });
            book.last_read = r.4.clone();
        }
        if let Some(k) = ko
            .records
            .iter()
            .find(|k| k.path.as_deref() == Some(rel.as_str()))
        {
            // KOReader's own place wins when it read the book more recently
            let newer = match (&k.last_read, &book.last_read) {
                (Some(a), Some(b)) => a > b,
                (Some(_), None) => true,
                _ => false,
            };
            if newer || book.percent.is_none() {
                book.percent = k.percent;
                book.finished = k.finished;
                book.last_read = k.last_read.clone();
            }
            if book.authors.is_empty() {
                book.authors = k.authors.clone();
            }
            if !k.title.trim().is_empty() && book.title == title_of_file(&name) {
                book.title = k.title.clone();
            }
        }
        if rel.starts_with("Deckle/") {
            book.note = Some("Sent by Deckle".into());
        }
        lib.books.push(book);
    }
    // a Kobo's own purchases: in its database, not files on the drive
    for r in rows.iter().filter(|r| !r.0.starts_with("file://")) {
        if r.1.trim().is_empty() {
            continue;
        }
        lib.books.push(Book {
            id: r.0.clone(),
            title: r.1.clone(),
            authors: r.2.clone(),
            format: "kepub".into(),
            bytes: r.6,
            percent: Some(if r.5 { 100.0 } else { r.3 }),
            finished: r.5,
            last_read: r.4.clone(),
            removable: false,
            note: Some(
                "Bought from Kobo: remove it on the Kobo, which keeps it in your account".into(),
            ),
            ..Default::default()
        });
    }
    sort(&mut lib.books);
    lib
}

fn sort(books: &mut [Book]) {
    books.sort_by(|a, b| {
        b.last_read
            .cmp(&a.last_read)
            .then_with(|| a.title.to_lowercase().cmp(&b.title.to_lowercase()))
    });
}

/// `(total, free)` bytes on the drive at `root`.
fn space(root: &Path) -> Option<(u64, u64)> {
    let s = fs4::statvfs(root).ok()?;
    Some((s.total_space(), s.available_space()))
}

/// `id` as a file on the drive at `root`, refused unless it's a book file inside the drive,
/// under no hidden folder.
pub fn book_path(root: &Path, id: &str) -> Result<PathBuf, String> {
    let parts: Vec<&str> = id.split('/').collect();
    let safe = !id.is_empty()
        && !id.starts_with('/')
        && !id.contains('\\')
        && parts
            .iter()
            .all(|p| !p.is_empty() && *p != "." && *p != ".." && !p.starts_with('.'));
    let name = parts.last().copied().unwrap_or_default();
    if !safe || !is_book_file(name) {
        return Err("not a book on this reader".into());
    }
    let path = parts
        .iter()
        .fold(root.to_path_buf(), |p, part| p.join(part));
    if !path.is_file() {
        return Err("it isn't there any more".into());
    }
    Ok(path)
}

/// Removes the books `ids` from a Kobo or PocketBook. A Kobo's database is first copied into
/// `backups` (`<stamp>-KoboReader.sqlite`); when that copy fails nothing is removed.
pub fn remove_on_drive(reader: &Reader, ids: &[String], backups: &Path, stamp: &str) -> Removed {
    let root = reader.root();
    let mut out = Removed::default();
    if let Reader::Kobo(k) = reader {
        let db = k.root.join(".kobo").join("KoboReader.sqlite");
        if db.is_file() {
            let to = backups.join(format!("{stamp}-KoboReader.sqlite"));
            let copied = std::fs::create_dir_all(backups).and_then(|_| std::fs::copy(&db, &to));
            match copied {
                Ok(_) => out.database_backup = Some(to),
                Err(e) => {
                    let why = format!("the Kobo's database couldn't be copied aside first: {e}");
                    out.failed = ids.iter().map(|i| (i.clone(), why.clone())).collect();
                    return out;
                }
            }
        }
    }
    for id in ids {
        let path = match book_path(root, id) {
            Ok(p) => p,
            Err(e) => {
                out.failed.push((id.clone(), e));
                continue;
            }
        };
        match std::fs::remove_file(&path) {
            Ok(()) => {
                // Only this book's own metadata file goes: one `.sdr` folder can hold another
                // format's (`Dune.sdr` for both `Dune.epub` and `Dune.pdf`).
                let _ = crate::koreader::remove_sidecar(&path);
                out.removed.push(id.clone());
            }
            Err(e) => out.failed.push((id.clone(), e.to_string())),
        }
    }
    out
}

fn copy_dir(from: &Path, to: &Path) -> std::io::Result<()> {
    std::fs::create_dir_all(to)?;
    for entry in std::fs::read_dir(from)?.flatten() {
        let target = to.join(entry.file_name());
        if entry.file_type()?.is_dir() {
            copy_dir(&entry.path(), &target)?;
        } else {
            std::fs::copy(entry.path(), &target)?;
        }
    }
    Ok(())
}

/// Copies every book on a Kobo or PocketBook, with its KOReader sidecar, into `dir` (made), at
/// the same paths; a Kobo's database goes to `.kobo/` there too. A `backup.json` lists them.
pub fn backup_drive(reader: &Reader, dir: &Path) -> Backup {
    let root = reader.root();
    let mut out = Backup {
        dir: dir.to_path_buf(),
        ..Default::default()
    };
    if let Err(e) = std::fs::create_dir_all(dir) {
        out.failed
            .push((String::new(), format!("{}: {e}", dir.display())));
        return out;
    }
    for (rel, _) in book_files(root) {
        let from = rel.split('/').fold(root.to_path_buf(), |p, s| p.join(s));
        let to = rel.split('/').fold(dir.to_path_buf(), |p, s| p.join(s));
        let copied = to
            .parent()
            .map_or(Ok(()), std::fs::create_dir_all)
            .and_then(|_| std::fs::copy(&from, &to).map(|_| ()))
            .and_then(|_| {
                let sdr = crate::koreader::sidecar_dir(&from);
                match sdr.is_dir() {
                    true => copy_dir(&sdr, &crate::koreader::sidecar_dir(&to)),
                    false => Ok(()),
                }
            });
        match copied {
            Ok(()) => out.copied.push(rel),
            Err(e) => out.failed.push((rel, e.to_string())),
        }
    }
    if let Reader::Kobo(k) = reader {
        let db = k.root.join(".kobo").join("KoboReader.sqlite");
        if db.is_file() {
            let to = dir.join(".kobo").join("KoboReader.sqlite");
            let copied = std::fs::create_dir_all(dir.join(".kobo"))
                .and_then(|_| std::fs::copy(&db, &to).map(|_| ()));
            match copied {
                Ok(()) => out.copied.push(".kobo/KoboReader.sqlite".into()),
                Err(e) => out
                    .failed
                    .push((".kobo/KoboReader.sqlite".into(), e.to_string())),
            }
        }
    }
    write_manifest(dir, &reader.name(), &out);
    out
}

fn write_manifest(dir: &Path, reader: &str, b: &Backup) {
    let manifest = serde_json::json!({ "reader": reader, "copied": b.copied, "failed": b.failed });
    let _ = std::fs::write(
        dir.join("backup.json"),
        serde_json::to_vec_pretty(&manifest).unwrap_or_default(),
    );
}

/// `2026-11-03T20:41:00Z` for epoch milliseconds.
fn iso_from_ms(ms: u64) -> Option<String> {
    if ms == 0 {
        return None;
    }
    let secs = (ms / 1000) as i64;
    let (days, rem) = (secs.div_euclid(86_400), secs.rem_euclid(86_400));
    // civil from days (Howard Hinnant's algorithm)
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = yoe + era * 400 + i64::from(m <= 2);
    Some(format!(
        "{y:04}-{m:02}-{d:02}T{:02}:{:02}:{:02}Z",
        rem / 3600,
        rem % 3600 / 60,
        rem % 60
    ))
}

/// Folder paths by id, from `(id, name, parent)`.
fn folder_paths(folders: &[(String, String, String)]) -> std::collections::HashMap<String, String> {
    let mut out = std::collections::HashMap::new();
    for (id, _, _) in folders {
        let mut parts = Vec::new();
        let mut at = id.clone();
        // a loop in the parents never hangs: no path is longer than the folders there are
        for _ in 0..=folders.len() {
            let Some((_, name, parent)) = folders.iter().find(|f| f.0 == at) else {
                break;
            };
            parts.push(name.clone());
            if parent.is_empty() || parent == "trash" {
                break;
            }
            at = parent.clone();
        }
        parts.reverse();
        out.insert(id.clone(), parts.join("/"));
    }
    out
}

/// What's on a reMarkable over its USB web interface: every document in every folder.
pub fn on_remarkable_web(r: &Remarkable) -> Library {
    let mut lib = Library::default();
    let mut stack: Vec<(String, String, usize)> = vec![(String::new(), String::new(), 0)];
    while let Some((id, path, depth)) = stack.pop() {
        let entries = match remarkable::list_in(r, &id) {
            Ok(e) => e,
            Err(e) => {
                lib.warnings.push(e);
                continue;
            }
        };
        for e in entries {
            if e.is_folder() {
                if depth + 1 < MAX_DEPTH {
                    let sub = if path.is_empty() {
                        e.name.clone()
                    } else {
                        format!("{path}/{}", e.name)
                    };
                    stack.push((e.id.clone(), sub, depth + 1));
                }
                continue;
            }
            let percent = match (e.current_page, e.page_count) {
                (Some(c), Some(n)) if n > 0 => Some(((c + 1) as f64 * 100.0 / n as f64).min(100.0)),
                _ => None,
            };
            lib.books.push(Book {
                id: e.id.clone(),
                title: e.name.clone(),
                folder: path.clone(),
                format: e.file_type.clone().unwrap_or_default(),
                bytes: e.size(),
                percent,
                last_read: e.modified.clone(),
                removable: false,
                note: None,
                ..Default::default()
            });
            if lib.books.len() >= MAX_ENTRIES {
                break;
            }
        }
    }
    sort(&mut lib.books);
    lib
}

/// A file name that's safe on any computer.
fn safe_name(name: &str) -> String {
    let cleaned: String = name
        .chars()
        .map(|c| match c {
            '/' | '\\' | ':' | '*' | '?' | '"' | '<' | '>' | '|' => '_',
            c if c.is_control() => '_',
            c => c,
        })
        .collect();
    let trimmed = cleaned.trim().trim_matches('.').to_owned();
    if trimmed.is_empty() {
        "Untitled".into()
    } else {
        trimmed.chars().take(120).collect()
    }
}

/// A file name in `folder` no other document of this backup has taken: `Title.ext`, then
/// `Title (2).ext` and so on. reMarkable titles need not be unique, and [`safe_name`] maps some
/// different titles to one name; names are compared without case, as many disks do.
fn unused_name(folder: &Path, title: &str, ext: &str, taken: &mut HashSet<String>) -> PathBuf {
    let base = safe_name(title);
    let mut n = 1;
    loop {
        let name = match n {
            1 => format!("{base}.{ext}"),
            n => format!("{base} ({n}).{ext}"),
        };
        let to = folder.join(&name);
        if taken.insert(to.to_string_lossy().to_lowercase()) && !to.exists() {
            return to;
        }
        n += 1;
    }
}

/// Downloads every document of `lib` (from [`on_remarkable_web`]) as a PDF into `dir`, in its
/// folders: the tablet draws the handwriting into each.
pub fn backup_remarkable_web(r: &Remarkable, lib: &Library, dir: &Path) -> Backup {
    let mut out = Backup {
        dir: dir.to_path_buf(),
        ..Default::default()
    };
    let mut taken = HashSet::new();
    for b in &lib.books {
        let folder = b
            .folder
            .split('/')
            .filter(|p| !p.is_empty())
            .fold(dir.to_path_buf(), |p, s| p.join(safe_name(s)));
        let to = unused_name(&folder, &b.title, "pdf", &mut taken);
        let got = remarkable::download(r, &b.id).and_then(|bytes| {
            std::fs::create_dir_all(&folder)
                .and_then(|_| std::fs::write(&to, bytes))
                .map_err(|e| e.to_string())
        });
        match got {
            Ok(()) => out.copied.push(
                to.strip_prefix(dir)
                    .unwrap_or(&to)
                    .to_string_lossy()
                    .into_owned(),
            ),
            Err(e) => out.failed.push((b.title.clone(), e)),
        }
    }
    write_manifest(dir, "reMarkable", &out);
    out
}

/// What's on a reMarkable over SSH: every document not in the trash.
pub fn on_remarkable_ssh(sh: &mut dyn Shell) -> Library {
    let mut lib = Library::default();
    let docs = match remarkable_ssh::list(sh) {
        Ok(d) => d,
        Err(e) => {
            lib.warnings.push(e);
            return lib;
        }
    };
    let folders: Vec<(String, String, String)> = docs
        .iter()
        .filter(|d| d.is_folder() && !d.deleted)
        .map(|d| (d.uuid.clone(), d.name.clone(), d.parent.clone()))
        .collect();
    let paths = folder_paths(&folders);
    for d in docs
        .iter()
        .filter(|d| !d.is_folder() && !d.deleted && d.parent != "trash")
    {
        lib.books.push(Book {
            id: d.uuid.clone(),
            title: d.name.clone(),
            folder: paths.get(&d.parent).cloned().unwrap_or_default(),
            last_read: iso_from_ms(d.last_modified),
            removable: true,
            ..Default::default()
        });
    }
    sort(&mut lib.books);
    lib
}

/// A reMarkable document id: letters, digits and dashes only, so it's safe in a command.
pub fn is_uuid(id: &str) -> bool {
    !id.is_empty() && id.len() <= 64 && id.chars().all(|c| c.is_ascii_alphanumeric() || c == '-')
}

/// Renames the document `id` on the tablet, then restarts its reader to show it.
pub fn rename_remarkable_ssh(sh: &mut dyn Shell, id: &str, name: &str) -> Result<(), String> {
    if !is_uuid(id) {
        return Err("not a document on the tablet".into());
    }
    let name = name.trim();
    if name.is_empty() {
        return Err("a document needs a name".into());
    }
    remarkable_ssh::rename(sh, id, name)?;
    remarkable_ssh::restart(sh)
}

/// Moves the documents `ids` to the tablet's trash (as its own Delete does), then restarts
/// the tablet's reader so it shows the change.
pub fn remove_remarkable_ssh(sh: &mut dyn Shell, ids: &[String]) -> Removed {
    let mut out = Removed::default();
    for id in ids {
        if !is_uuid(id) {
            out.failed
                .push((id.clone(), "not a document on the tablet".into()));
            continue;
        }
        match remarkable_ssh::mark_deleted(sh, id) {
            Ok(()) => out.removed.push(id.clone()),
            Err(e) => out.failed.push((id.clone(), e)),
        }
    }
    if !out.removed.is_empty()
        && let Err(e) = remarkable_ssh::restart(sh)
    {
        out.failed.push((
            String::new(),
            format!("removed, but the tablet didn't restart its reader: {e}"),
        ));
    }
    out
}

/// Backs up each document of `lib` (from [`on_remarkable_ssh`]) into `dir` as
/// `<folder>/<title>.tar`: the tablet's own files for it, handwriting included, as xochitl
/// keeps them (unpack them into its document folder to put one back).
pub fn backup_remarkable_ssh(sh: &mut dyn Shell, lib: &Library, dir: &Path) -> Backup {
    let mut out = Backup {
        dir: dir.to_path_buf(),
        ..Default::default()
    };
    let mut taken = HashSet::new();
    for b in &lib.books {
        if !is_uuid(&b.id) {
            out.failed
                .push((b.title.clone(), "not a document on the tablet".into()));
            continue;
        }
        let folder = b
            .folder
            .split('/')
            .filter(|p| !p.is_empty())
            .fold(dir.to_path_buf(), |p, s| p.join(safe_name(s)));
        let to = unused_name(&folder, &b.title, "tar", &mut taken);
        let cmd = format!(
            "cd {} && tar -cf - $(ls -d {id} {id}.* 2>/dev/null)",
            remarkable_ssh::DIR,
            id = b.id
        );
        let got = sh.run(&cmd, b"").and_then(|o| {
            if o.status != 0 || o.stdout.is_empty() {
                return Err(format!("tar: exit {}", o.status));
            }
            std::fs::create_dir_all(&folder)
                .and_then(|_| std::fs::write(&to, &o.stdout))
                .map_err(|e| e.to_string())
        });
        match got {
            Ok(()) => out.copied.push(
                to.strip_prefix(dir)
                    .unwrap_or(&to)
                    .to_string_lossy()
                    .into_owned(),
            ),
            Err(e) => out.failed.push((b.title.clone(), e)),
        }
    }
    write_manifest(dir, "reMarkable", &out);
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::reader::PocketBook;

    fn drive() -> (tempfile::TempDir, Reader) {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path();
        std::fs::create_dir_all(root.join("Books/Fiction")).unwrap();
        std::fs::create_dir_all(root.join("system/config")).unwrap();
        std::fs::create_dir_all(root.join(".hidden")).unwrap();
        std::fs::write(root.join("Books/Fiction/Moby_Dick.epub"), b"EPUB").unwrap();
        std::fs::create_dir_all(root.join("Books/Fiction/Moby_Dick.sdr")).unwrap();
        std::fs::write(
            root.join("Books/Fiction/Moby_Dick.sdr/metadata.epub.lua"),
            b"--",
        )
        .unwrap();
        std::fs::write(root.join("notes.pdf"), b"%PDF").unwrap();
        std::fs::write(root.join("system/config/settings.pdf"), b"%PDF").unwrap();
        std::fs::write(root.join(".hidden/secret.epub"), b"EPUB").unwrap();
        std::fs::write(root.join("Books/cover.jpg"), b"JPG").unwrap();
        let r = Reader::PocketBook(PocketBook {
            root: root.to_path_buf(),
        });
        (tmp, r)
    }

    #[test]
    fn a_drive_lists_its_books_and_nothing_of_the_system() {
        let (_tmp, r) = drive();
        let lib = on_drive(&r);
        let mut ids: Vec<&str> = lib.books.iter().map(|b| b.id.as_str()).collect();
        ids.sort();
        assert_eq!(ids, ["Books/Fiction/Moby_Dick.epub", "notes.pdf"]);
        let moby = lib.books.iter().find(|b| b.id.ends_with(".epub")).unwrap();
        assert_eq!(moby.title, "Moby Dick");
        assert_eq!(moby.folder, "Books/Fiction");
        assert_eq!(moby.format, "epub");
        assert_eq!(moby.bytes, Some(4));
        assert!(moby.removable);
    }

    #[test]
    fn remove_takes_the_book_and_its_sidecar_and_refuses_anything_else() {
        let (tmp, r) = drive();
        let ids = vec![
            "Books/Fiction/Moby_Dick.epub".to_owned(),
            "../outside.epub".to_owned(),
            ".hidden/secret.epub".to_owned(),
            "Books/cover.jpg".to_owned(),
            "gone.epub".to_owned(),
        ];
        let out = remove_on_drive(&r, &ids, &tmp.path().join("backups"), "x");
        assert_eq!(out.removed, ["Books/Fiction/Moby_Dick.epub"]);
        assert_eq!(out.failed.len(), 4);
        assert!(!tmp.path().join("Books/Fiction/Moby_Dick.epub").exists());
        assert!(!tmp.path().join("Books/Fiction/Moby_Dick.sdr").exists());
        assert!(tmp.path().join(".hidden/secret.epub").exists());
        assert!(tmp.path().join("Books/cover.jpg").exists());
    }

    #[test]
    fn remove_leaves_another_formats_sidecar_and_takes_a_kepubs_own() {
        // #72: removing one format took the whole `<stem>.sdr`, which KOReader shares between
        // formats of a name, and missed a KEPUB's `<name>.kepub.sdr`.
        let (tmp, r) = drive();
        let root = tmp.path();
        let w = |p: &str, b: &[u8]| {
            std::fs::create_dir_all(root.join(p).parent().unwrap()).unwrap();
            std::fs::write(root.join(p), b).unwrap();
        };
        w("Books/Emma.pdf", b"%PDF");
        w("Books/Emma.epub", b"EPUB");
        w("Books/Emma.sdr/metadata.pdf.lua", b"--");
        w("Books/Emma.sdr/metadata.epub.lua", b"--");
        w("Books/Dune.kepub.epub", b"EPUB");
        w("Books/Dune.kepub.sdr/metadata.epub.lua", b"--");
        w("Books/Dune.pdf", b"%PDF");
        w("Books/Dune.sdr/metadata.pdf.lua", b"--");
        let ids = vec![
            "Books/Emma.pdf".to_owned(),
            "Books/Dune.kepub.epub".to_owned(),
        ];
        let out = remove_on_drive(&r, &ids, &root.join("backups"), "x");
        assert_eq!(out.removed, ids);
        assert!(!root.join("Books/Emma.sdr/metadata.pdf.lua").exists());
        assert!(root.join("Books/Emma.sdr/metadata.epub.lua").is_file());
        assert!(!root.join("Books/Dune.kepub.sdr").exists());
        assert!(root.join("Books/Dune.sdr/metadata.pdf.lua").is_file());
    }

    #[test]
    fn a_backup_takes_a_kepubs_sidecar() {
        let (tmp, r) = drive();
        let root = tmp.path();
        std::fs::write(root.join("Books/Ulysses.kepub.epub"), b"EPUB").unwrap();
        std::fs::create_dir_all(root.join("Books/Ulysses.kepub.sdr")).unwrap();
        std::fs::write(
            root.join("Books/Ulysses.kepub.sdr/metadata.epub.lua"),
            b"--",
        )
        .unwrap();
        let dest = tmp.path().join("out/backup");
        let b = backup_drive(&r, &dest);
        assert!(b.failed.is_empty(), "{:?}", b.failed);
        assert!(
            dest.join("Books/Ulysses.kepub.sdr/metadata.epub.lua")
                .is_file()
        );
    }

    #[test]
    fn a_backup_keeps_the_paths_and_the_sidecars() {
        let (tmp, r) = drive();
        let dest = tmp.path().join("out/backup");
        let b = backup_drive(&r, &dest);
        assert_eq!(b.failed, []);
        assert_eq!(b.copied.len(), 2);
        assert!(dest.join("Books/Fiction/Moby_Dick.epub").is_file());
        assert!(
            dest.join("Books/Fiction/Moby_Dick.sdr/metadata.epub.lua")
                .is_file()
        );
        assert!(dest.join("backup.json").is_file());
    }

    #[test]
    fn a_kobo_lists_its_files_with_the_database_and_its_store_books_unremovable() {
        let tmp = tempfile::tempdir().unwrap();
        let k = crate::kobo::tests::fake_kobo(
            tmp.path(),
            "INSERT INTO content VALUES ('file:///mnt/onboard/Deckle/paper.kepub.epub', '6', \
             'The Daily', 'Deckle', 40, '2026-11-02T07:00:00.000', 1, NULL); \
             INSERT INTO content VALUES ('0a1b-store', '6', 'Bought Book', 'Someone', 0, NULL, 0, NULL);",
        );
        std::fs::create_dir_all(tmp.path().join("Deckle")).unwrap();
        std::fs::write(tmp.path().join("Deckle/paper.kepub.epub"), b"EPUB").unwrap();
        let r = Reader::Kobo(k);
        let lib = on_drive(&r);
        assert_eq!(lib.books.len(), 2, "{lib:?}");
        let paper = &lib.books[0];
        assert_eq!(paper.title, "The Daily");
        assert_eq!(paper.format, "kepub");
        assert_eq!(paper.percent, Some(40.0));
        assert_eq!(paper.last_read.as_deref(), Some("2026-11-02T07:00:00Z"));
        assert!(paper.removable);
        let store = &lib.books[1];
        assert_eq!(store.title, "Bought Book");
        assert!(!store.removable);
        // removing copies the database aside first
        let out = remove_on_drive(
            &r,
            std::slice::from_ref(&paper.id),
            &tmp.path().join("bk"),
            "s",
        );
        assert_eq!(out.removed, ["Deckle/paper.kepub.epub"]);
        assert!(out.database_backup.unwrap().is_file());
    }

    #[test]
    fn remarkable_folders_and_dates_read() {
        let folders = vec![
            ("a".to_owned(), "Books".to_owned(), String::new()),
            ("b".to_owned(), "Novels".to_owned(), "a".to_owned()),
            ("c".to_owned(), "Loop".to_owned(), "c".to_owned()),
        ];
        let p = folder_paths(&folders);
        assert_eq!(p["b"], "Books/Novels");
        assert_eq!(p["a"], "Books");
        assert!(p["c"].starts_with("Loop"));
        assert_eq!(
            iso_from_ms(1_762_202_460_000).as_deref(),
            Some("2025-11-03T20:41:00Z")
        );
        assert_eq!(iso_from_ms(0), None);
        assert!(is_uuid("0f1e2d3c-aaaa-bbbb-cccc-1234567890ab"));
        assert!(!is_uuid("x; rm -rf /"));
        assert_eq!(safe_name("a/b: c?"), "a_b_ c_");
    }

    struct Fake {
        meta: String,
        ran: Vec<String>,
    }

    impl Shell for Fake {
        fn run(&mut self, cmd: &str, _stdin: &[u8]) -> Result<remarkable_ssh::Output, String> {
            self.ran.push(cmd.to_owned());
            let stdout = if cmd.contains("for f in *.metadata") {
                self.meta.clone().into_bytes()
            } else if cmd.starts_with("cat ") {
                br#"{"visibleName":"Notes","type":"DocumentType","parent":"","deleted":false,"version":1}"#.to_vec()
            } else if cmd.contains("tar -cf") {
                b"TARBYTES".to_vec()
            } else {
                Vec::new()
            };
            Ok(remarkable_ssh::Output { status: 0, stdout })
        }
    }

    #[test]
    fn a_remarkable_over_ssh_lists_removes_and_backs_up() {
        let meta = concat!(
            "== f1.metadata\n{\"visibleName\":\"Work\",\"type\":\"CollectionType\",\"parent\":\"\"}\n",
            "== d1.metadata\n{\"visibleName\":\"Notes\",\"type\":\"DocumentType\",\"parent\":\"f1\",\"lastModified\":\"1762202460000\"}\n",
            "== d2.metadata\n{\"visibleName\":\"Old\",\"type\":\"DocumentType\",\"parent\":\"trash\"}\n",
            "== d3.metadata\n{\"visibleName\":\"Gone\",\"type\":\"DocumentType\",\"parent\":\"\",\"deleted\":true}\n",
        );
        let mut sh = Fake {
            meta: meta.into(),
            ran: Vec::new(),
        };
        let lib = on_remarkable_ssh(&mut sh);
        assert_eq!(lib.books.len(), 1);
        assert_eq!(lib.books[0].folder, "Work");
        assert_eq!(
            lib.books[0].last_read.as_deref(),
            Some("2025-11-03T20:41:00Z")
        );
        let tmp = tempfile::tempdir().unwrap();
        let b = backup_remarkable_ssh(&mut sh, &lib, tmp.path());
        assert_eq!(b.failed, []);
        assert_eq!(
            std::fs::read(tmp.path().join("Work/Notes.tar")).unwrap(),
            b"TARBYTES"
        );
        let out = remove_remarkable_ssh(&mut sh, &["d1".to_owned(), "bad id;".to_owned()]);
        assert_eq!(out.removed, ["d1"]);
        assert_eq!(out.failed.len(), 1);
        assert!(
            sh.ran
                .iter()
                .any(|c| c.contains("systemctl restart xochitl"))
        );
    }

    #[test]
    fn a_remarkable_backup_keeps_documents_that_share_a_name() {
        // #73: titles need not be unique and `safe_name` maps `a/b` and `a_b` alike; each
        // document overwrote the one before and both were reported copied.
        let mut sh = Fake {
            meta: String::new(),
            ran: Vec::new(),
        };
        let book = |id: &str, title: &str| Book {
            id: id.into(),
            title: title.into(),
            ..Default::default()
        };
        let lib = Library {
            books: vec![
                book("d1", "Notebook"),
                book("d2", "Notebook"),
                book("d3", "a/b"),
                book("d4", "a_b"),
            ],
            ..Default::default()
        };
        let tmp = tempfile::tempdir().unwrap();
        let b = backup_remarkable_ssh(&mut sh, &lib, tmp.path());
        assert_eq!(b.failed, []);
        assert_eq!(
            b.copied,
            ["Notebook.tar", "Notebook (2).tar", "a_b.tar", "a_b (2).tar"]
        );
        for f in &b.copied {
            assert!(tmp.path().join(f).is_file(), "{f}");
        }
    }
}
