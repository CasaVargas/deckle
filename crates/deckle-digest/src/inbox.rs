// SPDX-License-Identifier: GPL-3.0-or-later
//! The Inbox folder (save-to-paper design §4): links and saved pages dropped here become
//! queue entries. Processed files move to `Saved/`; ones Deckle can't read move to
//! `Unreadable/` with a `.txt` saying why. Point it at iCloud Drive to save from a phone.
//!
//! Books (`.epub`, `.docx`, `.kfx`) dropped here are sent later (design §10): the caller
//! converts each one and adds it to the outbox, then it moves to `Sent/`. Converting needs
//! `deckle-core`, which this crate doesn't depend on, so that step is passed in.

use std::path::{Path, PathBuf};
use std::sync::LazyLock;
use std::time::{Duration, SystemTime};

use chrono::Utc;
use regex::Regex;

use crate::reading::{Added, ReadingQueue};

/// Files modified more recently than this may still be being written.
const SETTLE: Duration = Duration::from_secs(5);

#[derive(Debug, Default)]
pub struct ScanReport {
    /// New queue entries (duplicates of pending saves don't count).
    pub queued: usize,
    /// Books handed on to be sent later (where they were in the Inbox).
    pub sent: Vec<PathBuf>,
    pub unreadable: Vec<(PathBuf, String)>,
    /// Left in place for the next scan (the queue was busy, a disk error).
    pub retry: Vec<(PathBuf, String)>,
    /// Subfolders that aren't the Inbox's own and aren't named after a Kindle: left alone.
    pub other_folders: Vec<PathBuf>,
}

/// Why a file wasn't queued: its content (moved aside) or the moment (left for later).
#[derive(Debug)]
pub enum Skip {
    Unreadable(String),
    Retry(String),
}

static URL: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"https?://[^\s<>\x22\x00-\x1f\x7f\u{fffd}]+").unwrap());
static WEBLOC: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"(?s)<key>URL</key>\s*<string>([^<]+)</string>").unwrap());

/// Extensions the Inbox treats as books rather than articles.
pub const BOOK_EXTENSIONS: [&str; 3] = ["epub", "docx", "kfx"];

/// A book to send later (by extension, any case), not an article.
pub fn is_book(path: &Path) -> bool {
    path.extension().is_some_and(|e| {
        BOOK_EXTENSIONS
            .iter()
            .any(|b| e.to_string_lossy().eq_ignore_ascii_case(b))
    })
}

/// `~/…` → the home folder.
pub fn expand_home(p: &str) -> PathBuf {
    match (p.strip_prefix("~/"), std::env::var_os("HOME")) {
        (Some(rest), Some(home)) => PathBuf::from(home).join(rest),
        _ => PathBuf::from(p),
    }
}

/// `dir/name`, or `dir/name (2).ext`… when taken.
fn free_name(dir: &Path, name: &str) -> PathBuf {
    let first = dir.join(name);
    if !first.exists() {
        return first;
    }
    let (stem, ext) = match name.rsplit_once('.') {
        Some((s, e)) => (s.to_owned(), format!(".{e}")),
        None => (name.to_owned(), String::new()),
    };
    (2..)
        .map(|n| dir.join(format!("{stem} ({n}){ext}")))
        .find(|p| !p.exists())
        .expect("some name is free")
}

fn move_to(file: &Path, sub: &str) -> Option<PathBuf> {
    let dir = file.parent()?.join(sub);
    std::fs::create_dir_all(&dir).ok()?;
    let dest = free_name(&dir, &file.file_name()?.to_string_lossy());
    std::fs::rename(file, &dest).ok()?;
    Some(dest)
}

/// The URLs in a link file, by kind.
pub(crate) fn urls_in(path: &Path, ext: &str) -> Result<Vec<String>, String> {
    let text = std::fs::read(path).map_err(|e| e.to_string())?;
    let text = String::from_utf8_lossy(&text);
    let urls: Vec<String> = match ext {
        // XML property lists; binary ones (Finder drags) keep the URL as plain ASCII.
        "webloc" => match WEBLOC.captures(&text) {
            Some(c) => vec![html_escape::decode_html_entities(c[1].trim()).into_owned()],
            None => URL
                .find_iter(&text)
                .take(1)
                .map(|m| m.as_str().to_owned())
                .collect(),
        },
        "url" => text
            .lines()
            .filter_map(|l| l.trim().strip_prefix("URL="))
            .map(|u| u.trim().to_owned())
            .collect(),
        _ => URL
            .find_iter(&text)
            .map(|m| m.as_str().to_owned())
            .collect(),
    };
    if urls.is_empty() {
        Err("no web address found in it".into())
    } else {
        Ok(urls)
    }
}

/// Queues the articles in `dir` (created when missing). `now` decides which files have
/// settled. Books are left where they are for [`scan_with_books`].
pub fn scan(dir: &Path, queue: &ReadingQueue, now: SystemTime) -> ScanReport {
    scan_with_books(dir, queue, now, &mut |_| {
        Err(Skip::Retry("books are sent by `deckle digest`".into()))
    })
}

/// Like [`scan`], handing each settled book to `send_later`; `Ok` moves it to `Sent/`.
pub fn scan_with_books(
    dir: &Path,
    queue: &ReadingQueue,
    now: SystemTime,
    send_later: &mut dyn FnMut(&Path) -> Result<(), Skip>,
) -> ScanReport {
    scan_for_kindles(dir, queue, now, &[], &mut |book, _| send_later(book))
}

/// Folders the Inbox makes itself, never a Kindle's.
const OWN_FOLDERS: [&str; 3] = ["Sent", "Saved", "Unreadable"];

/// Like [`scan_with_books`], also scanning each subfolder named after a Kindle in `kindles`
/// ((name, id), names matched ignoring case): its books go to `send_later` with that Kindle's
/// id (Kindle profiles design §2). Each folder keeps its own `Sent/`, `Saved/` and
/// `Unreadable/`. Other folders are left alone and listed in the report.
pub fn scan_for_kindles(
    dir: &Path,
    queue: &ReadingQueue,
    now: SystemTime,
    kindles: &[(String, String)],
    send_later: &mut dyn FnMut(&Path, Option<&str>) -> Result<(), Skip>,
) -> ScanReport {
    let mut report = ScanReport::default();
    if std::fs::create_dir_all(dir).is_err() {
        return report;
    }
    scan_files(dir, queue, now, None, send_later, &mut report);
    let Ok(entries) = std::fs::read_dir(dir) else {
        return report;
    };
    let mut folders: Vec<PathBuf> = entries
        .flatten()
        .filter(|e| e.file_type().is_ok_and(|t| t.is_dir()))
        .map(|e| e.path())
        .filter(|p| {
            let name = p.file_name().unwrap_or_default().to_string_lossy();
            !name.starts_with('.') && !OWN_FOLDERS.iter().any(|o| o.eq_ignore_ascii_case(&name))
        })
        .collect();
    folders.sort();
    for folder in folders {
        let name = folder
            .file_name()
            .unwrap_or_default()
            .to_string_lossy()
            .trim()
            .to_owned();
        match kindles
            .iter()
            .find(|(n, _)| n.trim().eq_ignore_ascii_case(&name))
        {
            Some((_, id)) => scan_files(&folder, queue, now, Some(id), send_later, &mut report),
            None => report.other_folders.push(folder),
        }
    }
    report
}

/// The settled files directly in `dir`: books to `send_later` (with `owner`), articles queued.
fn scan_files(
    dir: &Path,
    queue: &ReadingQueue,
    now: SystemTime,
    owner: Option<&str>,
    send_later: &mut dyn FnMut(&Path, Option<&str>) -> Result<(), Skip>,
    report: &mut ScanReport,
) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    let mut files: Vec<PathBuf> = entries
        .flatten()
        .filter(|e| e.file_type().is_ok_and(|t| t.is_file()))
        .map(|e| e.path())
        .filter(|p| {
            !p.file_name()
                .is_some_and(|n| n.to_string_lossy().starts_with('.'))
        })
        .collect();
    files.sort();
    for file in files {
        let settled = std::fs::metadata(&file)
            .and_then(|m| m.modified())
            .is_ok_and(|m| now.duration_since(m).is_ok_and(|age| age >= SETTLE));
        if !settled {
            continue;
        }
        let ext = file
            .extension()
            .map(|e| e.to_string_lossy().to_ascii_lowercase())
            .unwrap_or_default();
        if is_book(&file) {
            match send_later(&file, owner) {
                Ok(()) => {
                    move_to(&file, "Sent");
                    report.sent.push(file);
                }
                Err(skip) => report.skip(file, skip),
            }
            continue;
        }
        let result = match ext.as_str() {
            "html" | "htm" => queue
                .add_page(&file, "inbox", Utc::now())
                .map(|a| usize::from(matches!(a, Added::New(_))))
                .map_err(Skip::Retry),
            "webloc" | "url" | "txt" => {
                urls_in(&file, &ext)
                    .map_err(Skip::Unreadable)
                    .and_then(|urls| {
                        let mut n = 0;
                        for u in urls {
                            let added = queue
                                .add_url(&u, None, "inbox", Utc::now())
                                .map_err(Skip::Retry)?;
                            n += usize::from(matches!(added, Added::New(_)));
                        }
                        Ok(n)
                    })
            }
            _ => Err(Skip::Unreadable(
                "Deckle reads .webloc, .url, .txt and .html files, and .epub, .docx and .kfx books"
                    .into(),
            )),
        };
        match result {
            Ok(n) => {
                report.queued += n;
                move_to(&file, "Saved");
            }
            Err(skip) => report.skip(file, skip),
        }
    }
}

impl ScanReport {
    /// Leaves `file` for the next scan, or moves it to `Unreadable/` with a `.txt` saying why.
    fn skip(&mut self, file: PathBuf, skip: Skip) {
        match skip {
            Skip::Retry(why) => self.retry.push((file, why)),
            Skip::Unreadable(why) => {
                if let Some(dest) = move_to(&file, "Unreadable") {
                    let note = dest.with_file_name(format!(
                        "{}.txt",
                        dest.file_name().expect("has a name").to_string_lossy()
                    ));
                    let _ = std::fs::write(note, format!("{why}\n"));
                }
                self.unreadable.push((file, why));
            }
        }
    }
}
