// SPDX-License-Identifier: GPL-3.0-or-later
//! The Inbox folder (save-to-paper design §4): links and saved pages dropped here become
//! queue entries. Processed files move to `Saved/`; ones Deckle can't read move to
//! `Unreadable/` with a `.txt` saying why. Point it at iCloud Drive to save from a phone.

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
    pub unreadable: Vec<(PathBuf, String)>,
    /// Left in place for the next scan (the queue was busy, a disk error).
    pub retry: Vec<(PathBuf, String)>,
}

/// Why a file wasn't queued: its content (moved aside) or the moment (left for later).
enum Skip {
    Unreadable(String),
    Retry(String),
}

static URL: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"https?://[^\s<>\x22\x00-\x1f\x7f\u{fffd}]+").unwrap());
static WEBLOC: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"(?s)<key>URL</key>\s*<string>([^<]+)</string>").unwrap());

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
fn urls_in(path: &Path, ext: &str) -> Result<Vec<String>, String> {
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

/// Queues everything readable in `dir` (created when missing). `now` decides which files
/// have settled.
pub fn scan(dir: &Path, queue: &ReadingQueue, now: SystemTime) -> ScanReport {
    let mut report = ScanReport::default();
    if std::fs::create_dir_all(dir).is_err() {
        return report;
    }
    let Ok(entries) = std::fs::read_dir(dir) else {
        return report;
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
        let result =
            match ext.as_str() {
                "html" | "htm" => queue
                    .add_page(&file, "inbox", Utc::now())
                    .map(|a| usize::from(matches!(a, Added::New(_))))
                    .map_err(Skip::Retry),
                "webloc" | "url" | "txt" => urls_in(&file, &ext)
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
                    }),
                _ => Err(Skip::Unreadable(
                    "Deckle reads .webloc, .url, .txt and .html files".into(),
                )),
            };
        match result {
            Ok(n) => {
                report.queued += n;
                move_to(&file, "Saved");
            }
            Err(Skip::Retry(why)) => report.retry.push((file, why)),
            Err(Skip::Unreadable(why)) => {
                if let Some(dest) = move_to(&file, "Unreadable") {
                    let note = dest.with_file_name(format!(
                        "{}.txt",
                        dest.file_name().expect("has a name").to_string_lossy()
                    ));
                    let _ = std::fs::write(note, format!("{why}\n"));
                }
                report.unreadable.push((file, why));
            }
        }
    }
    report
}
