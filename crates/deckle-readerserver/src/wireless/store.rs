// SPDX-License-Identifier: GPL-3.0-or-later
//! What the wireless server keeps (calibre bridge design §4.3, §4.4), in a folder: the queue, a
//! file a job, so the CLI and the app add and the server takes without a lock; and
//! `readers.json`, the readers seen and what they hold, written only by the server.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use super::session::{Device, OnReader};

/// Events kept in `readers.json`.
const LOG_KEEP: usize = 50;

/// One thing to do on a reader.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Job {
    pub id: String,
    /// The reader it's for: its `store_uuid`, its name (any case), or `any` for the next to
    /// connect.
    pub reader: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub send: Option<PathBuf>,
    /// An `lpath` to take off the reader.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub remove: Option<String>,
    pub added: String,
}

impl Job {
    /// Whether this job is for `d`.
    pub fn is_for(&self, d: &Device) -> bool {
        let r = self.reader.trim();
        r.eq_ignore_ascii_case("any")
            || r == d.store_uuid
            || r.eq_ignore_ascii_case(d.name.trim())
            || r.eq_ignore_ascii_case(d.kind.trim())
    }
}

/// A reader as last seen.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Seen {
    pub name: String,
    pub kind: String,
    pub app: String,
    pub connected: bool,
    pub last_seen: String,
    pub free: Option<u64>,
    pub books: Vec<OnReader>,
}

/// `readers.json`.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Readers {
    /// By `store_uuid`.
    pub readers: BTreeMap<String, Seen>,
    /// Whether broadcasts are answered this run.
    pub discovery: bool,
    /// What happened lately, newest last: "sent Dune to KOReader (Kobo_frost)".
    pub log: Vec<String>,
}

#[derive(Clone, Debug)]
pub struct Store {
    pub dir: PathBuf,
}

impl Store {
    pub fn new(dir: impl Into<PathBuf>) -> Store {
        Store { dir: dir.into() }
    }

    fn queue_dir(&self) -> PathBuf {
        self.dir.join("queue")
    }

    fn readers_path(&self) -> PathBuf {
        self.dir.join("readers.json")
    }

    /// Adds a job; returns it.
    pub fn add(
        &self,
        reader: &str,
        send: Option<PathBuf>,
        remove: Option<String>,
    ) -> std::io::Result<Job> {
        let now = chrono::Utc::now();
        let job = Job {
            id: format!(
                "{}-{}",
                now.format("%Y%m%d%H%M%S%3f"),
                &uuid::Uuid::new_v4().simple().to_string()[..8]
            ),
            reader: reader.trim().to_owned(),
            send,
            remove,
            added: now.to_rfc3339(),
        };
        let dir = self.queue_dir();
        std::fs::create_dir_all(&dir)?;
        let tmp = dir.join(format!(".{}.tmp", job.id));
        std::fs::write(&tmp, serde_json::to_vec_pretty(&job)?)?;
        std::fs::rename(tmp, dir.join(format!("{}.json", job.id)))?;
        Ok(job)
    }

    /// Every job waiting, oldest first. A file that doesn't parse is skipped.
    pub fn jobs(&self) -> Vec<Job> {
        let mut jobs: Vec<Job> = std::fs::read_dir(self.queue_dir())
            .into_iter()
            .flatten()
            .flatten()
            .filter(|e| e.file_name().to_string_lossy().ends_with(".json"))
            .filter_map(|e| std::fs::read(e.path()).ok())
            .filter_map(|b| serde_json::from_slice(&b).ok())
            .collect();
        jobs.sort_by(|a, b| a.id.cmp(&b.id));
        jobs
    }

    /// Takes a job off the queue (done, or given up on).
    pub fn finish(&self, job: &Job) {
        let _ = std::fs::remove_file(self.queue_dir().join(format!("{}.json", job.id)));
    }

    pub fn readers(&self) -> Readers {
        std::fs::read(self.readers_path())
            .ok()
            .and_then(|b| serde_json::from_slice(&b).ok())
            .unwrap_or_default()
    }

    /// Changes `readers.json` (the server is its only writer).
    pub fn update(&self, f: impl FnOnce(&mut Readers)) {
        let mut r = self.readers();
        f(&mut r);
        if r.log.len() > LOG_KEEP {
            let cut = r.log.len() - LOG_KEEP;
            r.log.drain(..cut);
        }
        if let Err(e) = write(&self.readers_path(), &r) {
            eprintln!("wireless: {}: {e}", self.readers_path().display());
        }
    }

    /// Adds a line to the log, and says it.
    pub fn log(&self, line: impl Into<String>) {
        let line = line.into();
        eprintln!("wireless: {line}");
        self.update(|r| r.log.push(line));
    }
}

fn write(path: &Path, r: &Readers) -> std::io::Result<()> {
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir)?;
    }
    let tmp = path.with_extension("json.tmp");
    std::fs::write(&tmp, serde_json::to_vec_pretty(r)?)?;
    std::fs::rename(tmp, path)
}
