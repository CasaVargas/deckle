// SPDX-License-Identifier: GPL-3.0-or-later
//! The Inbox folder (save-to-paper design §4): links and saved pages dropped here become
//! queue entries. Processed files move to `Saved/`; ones Deckle can't read move to
//! `Unreadable/` with a `.txt` saying why. Point it at iCloud Drive to save from a phone.
//!
//! Books (`.epub`, `.docx`, `.kfx`) dropped here are sent later (design §10): the caller
//! converts each one and adds it to the outbox, then it moves to `Sent/`. Converting needs
//! `deckle-core`, which this crate doesn't depend on, so that step is passed in.
//!
//! `Recipes/` is the cookbook's: what is dropped there goes to the cookbook, never to Your
//! Reading ([`scan_recipes`], cookbook design §8.3). It keeps its own `Sent/` and `Unreadable/`.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{LazyLock, Mutex};
use std::time::{Duration, Instant, SystemTime};

use chrono::Utc;
use regex::Regex;

use crate::cookbook::add::{AddError, AddOptions, add_classified};
use crate::cookbook::store::Store;
use crate::fetch::Net;
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
    /// Recipes added to (or refreshed in) the cookbook, from [`scan_recipes`].
    pub recipes: usize,
    /// Things the owner should hear about once (a Kindle called `Recipes`).
    pub warnings: Vec<String>,
}

/// The Inbox subfolder for the cookbook.
pub const RECIPES_FOLDER: &str = "Recipes";

/// Said when a Kindle is named like the cookbook's folder.
pub const RECIPES_KINDLE_WARNING: &str =
    "A Kindle named Recipes can't have an Inbox folder: Recipes/ is for the cookbook";

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
pub const BOOK_EXTENSIONS: [&str; 5] = ["epub", "docx", "kfx", "cbz", "cbr"];

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
pub(crate) fn free_name(dir: &Path, name: &str) -> PathBuf {
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

/// Takes a settled book, with the id of the Kindle it's meant for (from its folder), if any.
pub type SendLater<'a> = dyn FnMut(&Path, Option<&str>) -> Result<(), Skip> + 'a;

/// Folders the Inbox makes itself, never a Kindle's.
const OWN_FOLDERS: [&str; 4] = ["Sent", "Saved", "Unreadable", RECIPES_FOLDER];

/// Like [`scan_with_books`], also scanning each subfolder named after a Kindle in `kindles`
/// ((name, id), names matched ignoring case): its books go to `send_later` with that Kindle's
/// id (Kindle profiles design §2). Each folder keeps its own `Sent/`, `Saved/` and
/// `Unreadable/`. Other folders are left alone and listed in the report.
pub fn scan_for_kindles(
    dir: &Path,
    queue: &ReadingQueue,
    now: SystemTime,
    kindles: &[(String, String)],
    send_later: &mut SendLater,
) -> ScanReport {
    let mut report = ScanReport::default();
    if std::fs::create_dir_all(dir).is_err() {
        return report;
    }
    // Made on first run, so there is somewhere to drop a recipe (a Shortcut points at it).
    let _ = std::fs::create_dir_all(dir.join(RECIPES_FOLDER));
    if kindles
        .iter()
        .any(|(n, _)| n.trim().eq_ignore_ascii_case(RECIPES_FOLDER))
    {
        report.warnings.push(RECIPES_KINDLE_WARNING.to_owned());
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

/// Whether `file` was last changed long enough ago to be done being written.
fn settled(file: &Path, now: SystemTime) -> bool {
    std::fs::metadata(file)
        .and_then(|m| m.modified())
        .is_ok_and(|m| now.duration_since(m).is_ok_and(|age| age >= SETTLE))
}

/// The settled files directly in `dir`: books to `send_later` (with `owner`), articles queued.
fn scan_files(
    dir: &Path,
    queue: &ReadingQueue,
    now: SystemTime,
    owner: Option<&str>,
    send_later: &mut SendLater,
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
        if !settled(&file, now) {
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

/// Files directly in `dir` that are visible, in name order.
fn visible_files(dir: &Path) -> Vec<PathBuf> {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return Vec::new();
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
    files
}

/// Least time between tries at one link file that couldn't be fetched, so a dead site
/// doesn't hold up every watch tick.
pub const RECIPE_RETRY_GAP: Duration = Duration::from_secs(2 * 60);

/// How long a link that can't be fetched keeps being tried before it goes to `Unreadable/`.
const RECIPE_GIVE_UP: Duration = Duration::from_secs(24 * 60 * 60);

/// When each link file that failed to fetch may next be tried (this process only).
static NEXT_TRY: LazyLock<Mutex<HashMap<PathBuf, Instant>>> =
    LazyLock::new(|| Mutex::new(HashMap::new()));

/// Adds what was dropped in `inbox/Recipes/` to the cookbook (never to Your Reading): each
/// settled `.webloc`, `.url` or `.txt` (one address per line) is fetched, each `.html` or
/// `.htm` page is read from disk. Added files move to `Recipes/Sent/`; ones that can't be
/// read move to `Recipes/Unreadable/` with a `.txt` saying why. A fetch that fails (offline,
/// a timeout, a server error: phone-saved links sync in before Wi-Fi is up) leaves the file
/// for a later scan, tried at most every [`RECIPE_RETRY_GAP`] and for a day; a server that
/// says no (404, 403…) doesn't. The owner picks recipe or article by the folder; nothing is
/// guessed from the content. An empty folder costs one directory read and touches neither the
/// network nor the book.
pub fn scan_recipes(inbox: &Path, store: &Store, net: &dyn Net, now: SystemTime) -> ScanReport {
    scan_recipes_spaced(inbox, store, net, now, RECIPE_RETRY_GAP)
}

/// [`scan_recipes`] with `gap` between tries at a link that can't be fetched.
pub fn scan_recipes_spaced(
    inbox: &Path,
    store: &Store,
    net: &dyn Net,
    now: SystemTime,
    gap: Duration,
) -> ScanReport {
    let mut report = ScanReport::default();
    for file in visible_files(&inbox.join(RECIPES_FOLDER)) {
        if !settled(&file, now) {
            continue;
        }
        let waiting = NEXT_TRY
            .lock()
            .ok()
            .and_then(|m| m.get(&file).copied())
            .is_some_and(|at| Instant::now() < at);
        if waiting {
            continue;
        }
        let ext = file
            .extension()
            .map(|e| e.to_string_lossy().to_ascii_lowercase())
            .unwrap_or_default();
        let outcome = recipe_file(&file, &ext, store, net, &mut report);
        match outcome {
            Err(RecipeFail::Fetch(why)) => {
                let age = std::fs::metadata(&file)
                    .and_then(|m| m.modified())
                    .ok()
                    .and_then(|m| now.duration_since(m).ok())
                    .unwrap_or_default();
                if age >= RECIPE_GIVE_UP {
                    forget(&file);
                    report.skip(
                        file,
                        Skip::Unreadable(format!("Couldn't fetch for a day: {why}")),
                    );
                } else {
                    if let Ok(mut m) = NEXT_TRY.lock() {
                        m.insert(file.clone(), Instant::now() + gap);
                    }
                    report.skip(file, Skip::Retry(why));
                }
            }
            Err(RecipeFail::Other(why)) => {
                forget(&file);
                report.skip(file, Skip::Unreadable(why));
            }
            Ok(()) => {
                forget(&file);
                move_to(&file, "Sent");
            }
        }
    }
    report
}

fn forget(file: &Path) {
    if let Ok(mut m) = NEXT_TRY.lock() {
        m.remove(file);
    }
}

enum RecipeFail {
    Fetch(String),
    Other(String),
}

/// Adds each recipe in one file. Only when every address was added is the file done; a
/// failure leaves the earlier ones added, and the reason lists them (adding again is safe).
fn recipe_file(
    file: &Path,
    ext: &str,
    store: &Store,
    net: &dyn Net,
    report: &mut ScanReport,
) -> Result<(), RecipeFail> {
    let targets = match ext {
        "webloc" | "url" | "txt" => {
            // A synced file that hasn't downloaded yet won't open: that is the moment, not
            // the content.
            std::fs::read(file).map_err(|e| RecipeFail::Fetch(e.to_string()))?;
            urls_in(file, ext).map_err(RecipeFail::Other)?
        }
        "html" | "htm" => vec![file.to_string_lossy().into_owned()],
        _ => {
            return Err(RecipeFail::Other(
                "the cookbook reads .webloc, .url, .txt and .html files, not other kinds".into(),
            ));
        }
    };
    let opts = AddOptions {
        chapter: None,
        note: None,
    };
    let mut added: Vec<String> = Vec::new();
    for t in targets {
        match add_classified(store, &t, net, &opts, Utc::now()) {
            Ok(_) => {
                report.recipes += 1;
                added.push(t);
            }
            Err(e) => {
                let (fetch, e) = match e {
                    AddError::Fetch(e) => (true, e),
                    AddError::Other(e) => (false, e),
                };
                let why = if added.is_empty() {
                    format!("{t}: {e}")
                } else {
                    format!(
                        "Added before this failed: {}. Failed: {t}: {e}. \
                         Dropping the file back in is safe.",
                        added.join(", ")
                    )
                };
                return Err(if fetch {
                    RecipeFail::Fetch(why)
                } else {
                    RecipeFail::Other(why)
                });
            }
        }
    }
    Ok(())
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
