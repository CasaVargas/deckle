// SPDX-License-Identifier: GPL-3.0-or-later
//! `readership.json` (readership design §5): each edition's layout, and what each Kindle
//! reported about it.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use chrono::NaiveDate;
use serde::{Deserialize, Serialize};

use crate::krds::parse_yjf;
use crate::layout::Layout;

/// Editions kept in the file.
pub const KEEP: usize = 60;

/// An edition as found on a Kindle.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum OnDevice {
    /// Its file isn't on this Kindle: nothing to say.
    Absent,
    /// On the Kindle, never opened (no `.sdr` or no `.yjf`).
    Unopened,
    /// The `.yjf`'s bytes.
    Opened(Vec<u8>),
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct Read {
    pub opened: bool,
    pub furthest: Option<i64>,
    pub last: Option<i64>,
    pub last_read: Option<String>,
    /// When Deckle read it.
    pub seen: String,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize)]
pub struct Recorded {
    pub opened: usize,
    pub unopened: usize,
    /// `(stem, why)` for each `.yjf` that couldn't be read.
    pub errors: Vec<(String, String)>,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct State {
    pub editions: BTreeMap<NaiveDate, Layout>,
    /// Per date, per Kindle (its sync id).
    pub reads: BTreeMap<NaiveDate, BTreeMap<String, Read>>,
    /// Suggestion id → the newest edition date when it was dismissed.
    pub dismissed: BTreeMap<String, NaiveDate>,
}

/// `readership.json`, next to `library.json`.
pub fn state_path() -> PathBuf {
    deckle_library::config::default_path().with_file_name("readership.json")
}

impl State {
    /// A missing or unreadable file is an empty state.
    pub fn load(path: &Path) -> State {
        std::fs::read(path)
            .ok()
            .and_then(|b| serde_json::from_slice(&b).ok())
            .unwrap_or_default()
    }

    pub fn save(&self, path: &Path) -> Result<(), String> {
        let err = |e: &dyn std::fmt::Display| format!("{}: {e}", path.display());
        if let Some(dir) = path.parent() {
            std::fs::create_dir_all(dir).map_err(|e| err(&e))?;
        }
        let tmp = path.with_extension("json.tmp");
        let bytes = serde_json::to_vec_pretty(self).map_err(|e| err(&e))?;
        std::fs::write(&tmp, bytes).map_err(|e| err(&e))?;
        std::fs::rename(&tmp, path).map_err(|e| err(&e))
    }

    /// Adds or replaces an edition's layout. A rebuilt date whose sections moved loses its
    /// reads: their positions belong to the old build.
    pub fn record_layout(&mut self, layout: Layout) {
        let date = layout.date;
        let starts = |l: &Layout| l.sections.iter().map(|s| s.start).collect::<Vec<_>>();
        if self
            .editions
            .get(&date)
            .is_some_and(|old| starts(old) != starts(&layout))
        {
            self.reads.remove(&date);
        }
        self.editions.insert(date, layout);
        while self.editions.len() > KEEP {
            let oldest = *self.editions.keys().next().unwrap();
            self.editions.remove(&oldest);
            self.reads.remove(&oldest);
        }
    }

    /// Every recorded edition's stem, newest first.
    pub fn stems(&self) -> Vec<String> {
        self.editions
            .values()
            .rev()
            .map(|l| l.file.clone())
            .collect()
    }

    /// What `device` reported for each stem; stems Deckle has no layout for are ignored.
    pub fn record_reads(
        &mut self,
        device: &str,
        found: &[(String, OnDevice)],
        now: &str,
    ) -> Recorded {
        let mut done = Recorded::default();
        for (stem, on) in found {
            let Some(date) = self
                .editions
                .values()
                .find(|l| &l.file == stem)
                .map(|l| l.date)
            else {
                continue;
            };
            let read = match on {
                OnDevice::Absent => continue,
                OnDevice::Unopened => {
                    let had = self.reads.get(&date).and_then(|m| m.get(device));
                    if had.is_some_and(|r| r.opened) {
                        continue;
                    }
                    done.unopened += 1;
                    Read {
                        seen: now.to_owned(),
                        ..Read::default()
                    }
                }
                OnDevice::Opened(bytes) => match parse_yjf(bytes) {
                    Ok(p) => {
                        done.opened += 1;
                        Read {
                            opened: true,
                            furthest: p.furthest,
                            last: p.last,
                            last_read: p.last_read,
                            seen: now.to_owned(),
                        }
                    }
                    Err(e) => {
                        done.errors.push((stem.clone(), e.to_string()));
                        continue;
                    }
                },
            };
            self.reads
                .entry(date)
                .or_default()
                .insert(device.to_owned(), read);
        }
        done
    }
}
