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
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Added {
    New(String),
    /// Already pending under this id.
    Duplicate(String),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Outcome {
    Printed,
    Book,
    Failed(String),
}

enum Line {
    Entry(Box<Entry>),
    Raw(String),
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

fn fnv(s: &str) -> u64 {
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

    fn lines(&self) -> Vec<Line> {
        let Ok(text) = std::fs::read_to_string(&self.path) else {
            return Vec::new();
        };
        text.lines()
            .filter(|l| !l.trim().is_empty())
            .map(|l| match serde_json::from_str::<Entry>(l) {
                Ok(e) => Line::Entry(Box::new(e)),
                Err(_) => Line::Raw(l.to_owned()),
            })
            .collect()
    }

    fn write(&self, lines: &[Line]) -> Result<(), String> {
        if let Some(dir) = self.path.parent() {
            std::fs::create_dir_all(dir).map_err(|e| e.to_string())?;
        }
        let mut text = String::new();
        for l in lines {
            match l {
                Line::Entry(e) => {
                    text.push_str(&serde_json::to_string(e).map_err(|e| e.to_string())?)
                }
                Line::Raw(r) => text.push_str(r),
            }
            text.push('\n');
        }
        let tmp = self.path.with_extension("jsonl.tmp");
        std::fs::write(&tmp, text).map_err(|e| format!("{}: {e}", tmp.display()))?;
        std::fs::rename(&tmp, &self.path).map_err(|e| format!("{}: {e}", self.path.display()))
    }

    /// Every entry that parses, in file order.
    pub fn load(&self) -> Vec<Entry> {
        self.lines()
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
        let lines = self.lines();
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
        let lines = self.lines();
        if let Some(key) = &canonical
            && let Some(id) = Self::pending_duplicate(&lines, key)
        {
            return Ok(Added::Duplicate(id));
        }
        let id = Self::new_id(&lines, &html.to_string_lossy(), now);
        let dir = self.pages_dir().join(&id);
        let name = html.file_name().ok_or("not a file")?;
        std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
        std::fs::copy(html, dir.join(name)).map_err(|e| e.to_string())?;
        if let Some(stem) = html.file_stem() {
            let files = html.with_file_name(format!("{}_files", stem.to_string_lossy()));
            if files.is_dir() {
                copy_dir(&files, &dir.join(files.file_name().expect("has a name")))
                    .map_err(|e| e.to_string())?;
            }
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
        };
        self.append(lines, entry)?;
        Ok(Added::New(id))
    }

    /// Removes the entry with this id (or this URL); `false` when none matched.
    pub fn remove(&self, id_or_url: &str) -> Result<bool, String> {
        let _lock = Lock::take(&self.path)?;
        let key = normalize_url(id_or_url);
        let mut lines = self.lines();
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
    /// for `date` (a same-day rebuild reprints them), oldest first, at most `max`.
    pub fn for_edition(&self, date: NaiveDate, max: usize) -> Vec<Entry> {
        let mut out: Vec<Entry> = self
            .load()
            .into_iter()
            .filter(|e| e.state == State::Pending || e.printed_in == Some(date))
            .collect();
        out.sort_by_key(|e| e.saved_at);
        out.truncate(max);
        out
    }

    /// Records what an edition for `date` did with each entry.
    pub fn commit(&self, date: NaiveDate, outcomes: &[(String, Outcome)]) -> Result<(), String> {
        let _lock = Lock::take(&self.path)?;
        let mut lines = self.lines();
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
                Outcome::Failed(why) => {
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
