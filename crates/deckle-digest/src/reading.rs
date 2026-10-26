// SPDX-License-Identifier: GPL-3.0-or-later
//! The reading queue (save-to-paper design §3): `reading.jsonl`, one saved article per line,
//! plain enough to read and edit by hand. Lines that do not parse are kept as they are.

use std::path::{Path, PathBuf};
use std::sync::LazyLock;
use std::time::{Duration, Instant};

use chrono::{DateTime, NaiveDate, Utc};
use regex::Regex;
use serde::{Deserialize, Serialize};

/// Failed extractions before a saved article prints as a headline and is given up on.
pub const MAX_TRIES: u32 = 3;
/// Editions a withheld entry ([`Outcome::Waiting`]) may wait before it counts as failing.
pub const MAX_WAITS: u32 = 14;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum State {
    Pending,
    Printed,
    /// Became its own book (a long read).
    Book,
    Failed,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Entry {
    pub id: String,
    pub url: Option<String>,
    /// A saved page, copied under `reading/<id>/`.
    pub file: Option<PathBuf>,
    pub title: Option<String>,
    pub saved_at: DateTime<Utc>,
    /// `cli`, `inbox` or `bookmarklet`.
    pub source: String,
    pub state: State,
    pub tries: u32,
    pub printed_in: Option<NaiveDate>,
    pub note: Option<String>,
    /// The edition date of the last failed try (a same-day rebuild doesn't count twice).
    #[serde(default)]
    pub tried_on: Option<NaiveDate>,
    /// Editions spent waiting on a page its site withheld ([`Outcome::Waiting`]).
    #[serde(default, skip_serializing_if = "is_zero")]
    pub waits: u32,
}

fn is_zero(n: &u32) -> bool {
    *n == 0
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Added {
    New(String),
    /// Already pending under this id.
    Duplicate(String),
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(tag = "outcome", content = "reason", rename_all = "lowercase")]
pub enum Outcome {
    Printed,
    Book,
    Failed(String),
    /// Not printed, and not a failed try either: a subscriber page the site withheld. The
    /// entry stays pending with the reason, its tries untouched.
    Waiting(String),
}

enum Line {
    Entry(Box<Entry>),
    /// A line that isn't an entry (hand edits, other encodings), kept byte for byte.
    Raw(Vec<u8>),
}

#[derive(Clone, Debug)]
pub struct ReadingQueue {
    pub path: PathBuf,
}

/// The URL as a queue key: no fragment, no `utm_*` parameters.
pub fn normalize_url(url: &str) -> String {
    let Ok(mut u) = url::Url::parse(url.trim()) else {
        return url.trim().to_owned();
    };
    u.set_fragment(None);
    let kept: Vec<(String, String)> = u
        .query_pairs()
        .filter(|(k, _)| !k.starts_with("utm_"))
        .map(|(k, v)| (k.into_owned(), v.into_owned()))
        .collect();
    if kept.is_empty() {
        u.set_query(None);
    } else {
        u.query_pairs_mut().clear().extend_pairs(kept);
    }
    u.to_string()
}

pub(crate) fn fnv(s: &str) -> u64 {
    let mut h: u64 = 0xcbf2_9ce4_8422_2325;
    for b in s.bytes() {
        h ^= u64::from(b);
        h = h.wrapping_mul(0x0100_0000_01b3);
    }
    h
}

static CANONICAL: LazyLock<[Regex; 2]> = LazyLock::new(|| {
    [
        Regex::new(r#"(?i)<link[^>]*rel=["']canonical["'][^>]*href=["']([^"']+)["']"#).unwrap(),
        Regex::new(r#"(?i)<link[^>]*href=["']([^"']+)["'][^>]*rel=["']canonical["']"#).unwrap(),
    ]
});

/// Holds `<queue>.lock` while alive: one writer at a time.
struct Lock(PathBuf);

impl Lock {
    fn take(queue: &Path) -> Result<Lock, String> {
        let path = queue.with_extension("lock");
        // First save on a new machine: the data folder doesn't exist yet.
        if let Some(dir) = path.parent() {
            std::fs::create_dir_all(dir).map_err(|e| format!("{}: {e}", dir.display()))?;
        }
        let start = Instant::now();
        loop {
            match std::fs::OpenOptions::new()
                .write(true)
                .create_new(true)
                .open(&path)
            {
                Ok(_) => return Ok(Lock(path)),
                Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => {
                    if start.elapsed() > Duration::from_secs(2) {
                        return Err(format!(
                            "{} is locked (another Deckle is saving); remove it if none is",
                            path.display()
                        ));
                    }
                    std::thread::sleep(Duration::from_millis(50));
                }
                Err(e) => return Err(format!("{}: {e}", path.display())),
            }
        }
    }
}

impl Drop for Lock {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.0);
    }
}

fn copy_dir(from: &Path, to: &Path) -> std::io::Result<()> {
    std::fs::create_dir_all(to)?;
    for entry in std::fs::read_dir(from)?.flatten() {
        let dest = to.join(entry.file_name());
        if entry.file_type()?.is_dir() {
            copy_dir(&entry.path(), &dest)?;
        } else {
            std::fs::copy(entry.path(), dest)?;
        }
    }
    Ok(())
}

impl ReadingQueue {
    /// `DECKLE_READING_QUEUE`, else `reading.jsonl` beside `digest.toml`.
    pub fn default_path() -> PathBuf {
        if let Some(p) = std::env::var_os("DECKLE_READING_QUEUE").filter(|p| !p.is_empty()) {
            return PathBuf::from(p);
        }
        let config = crate::config::default_path();
        config
            .parent()
            .map(|d| d.join("reading.jsonl"))
            .unwrap_or_else(|| PathBuf::from("reading.jsonl"))
    }

    pub fn new(path: PathBuf) -> Self {
        ReadingQueue { path }
    }

    /// Where saved pages are copied: `reading/` beside the queue file.
    pub fn pages_dir(&self) -> PathBuf {
        self.path.parent().unwrap_or(Path::new(".")).join("reading")
    }

    /// The file's lines. A missing file is an empty queue; any other read error is an
    /// error, so nothing writes over a file it couldn't read.
    fn lines(&self) -> Result<Vec<Line>, String> {
        let bytes = match std::fs::read(&self.path) {
            Ok(b) => b,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
            Err(e) => return Err(format!("{}: {e}", self.path.display())),
        };
        Ok(bytes
            .split(|b| *b == b'\n')
            .filter(|l| !l.trim_ascii().is_empty())
            .map(|l| {
                match std::str::from_utf8(l)
                    .ok()
                    .and_then(|t| serde_json::from_str::<Entry>(t).ok())
                {
                    Some(e) => Line::Entry(Box::new(e)),
                    None => Line::Raw(l.to_vec()),
                }
            })
            .collect())
    }

    fn write(&self, lines: &[Line]) -> Result<(), String> {
        if let Some(dir) = self.path.parent() {
            std::fs::create_dir_all(dir).map_err(|e| e.to_string())?;
        }
        let mut out = Vec::new();
        for l in lines {
            match l {
                Line::Entry(e) => out.extend(serde_json::to_vec(e).map_err(|e| e.to_string())?),
                Line::Raw(r) => out.extend_from_slice(r),
            }
            out.push(b'\n');
        }
        let tmp = self.path.with_extension("jsonl.tmp");
        std::fs::write(&tmp, out).map_err(|e| format!("{}: {e}", tmp.display()))?;
        std::fs::rename(&tmp, &self.path).map_err(|e| format!("{}: {e}", self.path.display()))
    }

    /// Every entry that parses, in file order.
    pub fn load(&self) -> Vec<Entry> {
        self.lines()
            .unwrap_or_default()
            .into_iter()
            .filter_map(|l| match l {
                Line::Entry(e) => Some(*e),
                Line::Raw(_) => None,
            })
            .collect()
    }

    fn pending_duplicate(lines: &[Line], key: &str) -> Option<String> {
        lines.iter().find_map(|l| match l {
            Line::Entry(e)
                if e.state == State::Pending
                    && e.url.as_deref().map(normalize_url).as_deref() == Some(key) =>
            {
                Some(e.id.clone())
            }
            _ => None,
        })
    }

    fn new_id(lines: &[Line], seed: &str, now: DateTime<Utc>) -> String {
        let day = now.format("%Y%m%d");
        let mut h = fnv(seed);
        loop {
            let id = format!("r-{day}-{:04x}", h & 0xffff);
            if !lines
                .iter()
                .any(|l| matches!(l, Line::Entry(e) if e.id == id))
            {
                return id;
            }
            h = h.wrapping_add(1);
        }
    }

    fn append(&self, mut lines: Vec<Line>, entry: Entry) -> Result<(), String> {
        lines.push(Line::Entry(Box::new(entry)));
        self.write(&lines)
    }

    pub fn add_url(
        &self,
        url: &str,
        title: Option<&str>,
        source: &str,
        now: DateTime<Utc>,
    ) -> Result<Added, String> {
        let _lock = Lock::take(&self.path)?;
        let lines = self.lines()?;
        let key = normalize_url(url);
        if let Some(id) = Self::pending_duplicate(&lines, &key) {
            return Ok(Added::Duplicate(id));
        }
        let id = Self::new_id(&lines, &key, now);
        let entry = Entry {
            id: id.clone(),
            url: Some(key),
            file: None,
            title: title.map(str::to_owned).filter(|t| !t.trim().is_empty()),
            saved_at: now,
            source: source.into(),
            state: State::Pending,
            tries: 0,
            printed_in: None,
            note: None,
            tried_on: None,
            waits: 0,
        };
        self.append(lines, entry)?;
        Ok(Added::New(id))
    }

    /// Saves a page from a file: the HTML and its `<stem>_files/` folder are copied under
    /// `reading/<id>/`; `url` is the page's canonical URL when it declares one.
    pub fn add_page(&self, html: &Path, source: &str, now: DateTime<Utc>) -> Result<Added, String> {
        let text = std::fs::read(html).map_err(|e| format!("{}: {e}", html.display()))?;
        let text = String::from_utf8_lossy(&text);
        let canonical = CANONICAL
            .iter()
            .find_map(|r| r.captures(&text).map(|c| c[1].to_owned()))
            .map(|u| normalize_url(&u));
        let _lock = Lock::take(&self.path)?;
        let mut lines = self.lines()?;
        // A page for a URL already waiting is how a paywalled save gets read: the entry
        // keeps its id and starts over with the page.
        let existing = canonical
            .as_ref()
            .and_then(|key| Self::pending_duplicate(&lines, key));
        let id = existing
            .clone()
            .unwrap_or_else(|| Self::new_id(&lines, &html.to_string_lossy(), now));
        let dir = self.pages_dir().join(&id);
        let name = html.file_name().ok_or("not a file")?;
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
        std::fs::copy(html, dir.join(name)).map_err(|e| e.to_string())?;
        if let Some(stem) = html.file_stem() {
            let files = html.with_file_name(format!("{}_files", stem.to_string_lossy()));
            if files.is_dir() {
                copy_dir(&files, &dir.join(files.file_name().expect("has a name")))
                    .map_err(|e| e.to_string())?;
            }
        }
        if existing.is_some() {
            for l in &mut lines {
                if let Line::Entry(e) = l
                    && e.id == id
                {
                    e.file = Some(dir.join(name));
                    e.tries = 0;
                    e.note = None;
                    e.tried_on = None;
                }
            }
            self.write(&lines)?;
            return Ok(Added::New(id));
        }
        let entry = Entry {
            id: id.clone(),
            url: canonical,
            file: Some(dir.join(name)),
            title: None,
            saved_at: now,
            source: source.into(),
            state: State::Pending,
            tries: 0,
            printed_in: None,
            note: None,
            tried_on: None,
            waits: 0,
        };
        self.append(lines, entry)?;
        Ok(Added::New(id))
    }

    /// Removes the entry with this id (or this URL); `false` when none matched.
    pub fn remove(&self, id_or_url: &str) -> Result<bool, String> {
        let _lock = Lock::take(&self.path)?;
        let key = normalize_url(id_or_url);
        let mut lines = self.lines()?;
        let before = lines.len();
        let mut removed_dirs = Vec::new();
        lines.retain(|l| match l {
            Line::Entry(e) if e.id == id_or_url || e.url.as_deref() == Some(key.as_str()) => {
                if e.file.is_some() {
                    removed_dirs.push(self.pages_dir().join(&e.id));
                }
                false
            }
            _ => true,
        });
        if lines.len() == before {
            return Ok(false);
        }
        self.write(&lines)?;
        for d in removed_dirs {
            let _ = std::fs::remove_dir_all(d);
        }
        Ok(true)
    }

    /// What an edition for `date` carries: pending entries plus those already in an edition
    /// for `date` (a same-day rebuild reprints them), oldest first, at most `max`. Entries
    /// waiting on a withheld page only fill slots left over: they never displace a fresh save.
    pub fn for_edition(&self, date: NaiveDate, max: usize) -> Vec<Entry> {
        let (mut waiting, mut out): (Vec<Entry>, Vec<Entry>) = self
            .load()
            .into_iter()
            .filter(|e| e.state == State::Pending || e.printed_in == Some(date))
            .partition(|e| e.state == State::Pending && e.waits > 0);
        out.sort_by_key(|e| e.saved_at);
        out.truncate(max);
        waiting.sort_by_key(|e| e.saved_at);
        let left = max - out.len();
        out.extend(waiting.into_iter().take(left));
        out
    }

    /// Records what an edition for `date` did with each entry.
    pub fn commit(&self, date: NaiveDate, outcomes: &[(String, Outcome)]) -> Result<(), String> {
        let _lock = Lock::take(&self.path)?;
        let mut lines = self.lines()?;
        for line in &mut lines {
            let Line::Entry(e) = line else { continue };
            let Some((_, outcome)) = outcomes.iter().find(|(id, _)| *id == e.id) else {
                continue;
            };
            match outcome {
                Outcome::Printed => {
                    e.state = State::Printed;
                    e.printed_in = Some(date);
                }
                Outcome::Book => {
                    e.state = State::Book;
                    e.printed_in = Some(date);
                }
                Outcome::Waiting(why) if e.waits < MAX_WAITS => {
                    e.waits += 1;
                    e.note = Some(why.clone());
                }
                // Waited long enough: from now on it's a failed try like any other.
                Outcome::Waiting(why) | Outcome::Failed(why) => {
                    if e.tried_on != Some(date) {
                        e.tries += 1;
                        e.tried_on = Some(date);
                    }
                    e.note = Some(why.clone());
                    if e.tries >= MAX_TRIES {
                        e.state = State::Failed;
                        e.printed_in = Some(date);
                    }
                }
            }
        }
        self.write(&lines)
    }
}
