// SPDX-License-Identifier: GPL-3.0-or-later
//! The Morgue folder: `YYYY/YYYY-MM-DD.{epub,json,kfx}` (design §3). Only files named exactly
//! so, inside a `YYYY` folder, are ever listed or deleted; whatever else you put there is yours.

use std::io;
use std::path::{Path, PathBuf};

use chrono::NaiveDate;
use deckle_digest::Config;

use crate::sidecar::Sidecar;

/// KFX copies are kept this many days: they only make a re-send fast.
const KFX_DAYS: i64 = 14;

#[derive(Clone, Debug)]
pub struct Morgue {
    pub root: PathBuf,
    /// Editions older than this are removed on archive; `0` keeps them all.
    pub keep_days: u32,
}

/// A temp name beside `path` no other process or call shares, so two writers of one date never
/// trade bytes mid-write.
pub fn unique_tmp(path: &Path) -> PathBuf {
    use std::sync::atomic::{AtomicU64, Ordering};
    static NEXT: AtomicU64 = AtomicU64::new(0);
    let mut name = path.file_name().unwrap_or_default().to_os_string();
    name.push(format!(
        ".{}-{}.tmp",
        std::process::id(),
        NEXT.fetch_add(1, Ordering::Relaxed)
    ));
    path.with_file_name(name)
}

/// A file written beside its destination, not yet in place.
struct Staged {
    tmp: PathBuf,
    to: PathBuf,
}

impl Staged {
    fn write(to: &Path, bytes: &[u8]) -> io::Result<Staged> {
        let tmp = unique_tmp(to);
        std::fs::write(&tmp, bytes).inspect_err(|_| {
            let _ = std::fs::remove_file(&tmp);
        })?;
        Ok(Staged {
            tmp,
            to: to.to_owned(),
        })
    }

    fn copy(from: &Path, to: &Path) -> io::Result<Staged> {
        let tmp = unique_tmp(to);
        std::fs::copy(from, &tmp).inspect_err(|_| {
            let _ = std::fs::remove_file(&tmp);
        })?;
        Ok(Staged {
            tmp,
            to: to.to_owned(),
        })
    }

    fn commit(self) -> io::Result<()> {
        std::fs::rename(&self.tmp, &self.to).inspect_err(|_| {
            let _ = std::fs::remove_file(&self.tmp);
        })
    }
}

impl Drop for Staged {
    /// A staged file that was never committed (or already renamed away) leaves nothing behind.
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.tmp);
    }
}

fn write_atomic(path: &Path, bytes: &[u8]) -> io::Result<()> {
    Staged::write(path, bytes)?.commit()
}

fn copy_atomic(from: &Path, to: &Path) -> io::Result<()> {
    Staged::copy(from, to)?.commit()
}

/// `YYYY-MM-DD` as a date, only when that is the whole name.
fn strict_date(name: &str) -> Option<NaiveDate> {
    (name.len() == 10)
        .then(|| NaiveDate::parse_from_str(name, "%Y-%m-%d").ok())
        .flatten()
}

impl Morgue {
    pub fn from_config(cfg: &Config) -> Morgue {
        Morgue {
            root: deckle_digest::inbox::expand_home(&cfg.morgue.folder),
            keep_days: cfg.morgue.keep_days,
        }
    }

    /// The EPUB, JSON and KFX paths of an edition.
    pub fn paths(&self, date: NaiveDate) -> (PathBuf, PathBuf, PathBuf) {
        let dir = self.root.join(date.format("%Y").to_string());
        let name = date.to_string();
        (
            dir.join(format!("{name}.epub")),
            dir.join(format!("{name}.json")),
            dir.join(format!("{name}.kfx")),
        )
    }

    /// Keeps `epub` and its sidecar, EPUB first and JSON last, so a JSON never points at a
    /// missing EPUB. A second build of the same date replaces the first, KFX copy included:
    /// that was made from the older build.
    ///
    /// Old editions are pruned against today's date, never the edition's own: a build for a
    /// date far ahead must not age the whole archive out.
    pub fn archive_edition(&self, epub: &Path, sidecar: &Sidecar) -> io::Result<()> {
        self.archive_edition_on(epub, sidecar, chrono::Local::now().date_naive())
    }

    /// [`Morgue::archive_edition`] pruning as of `today`.
    pub fn archive_edition_on(
        &self,
        epub: &Path,
        sidecar: &Sidecar,
        today: NaiveDate,
    ) -> io::Result<()> {
        let (epub_to, json_to, kfx_to) = self.paths(sidecar.date);
        std::fs::create_dir_all(epub_to.parent().expect("a year folder"))?;
        // Stage both files before replacing either, so a failure copying the EPUB or writing the
        // JSON leaves the previous EPUB and sidecar as they were. Only a failed rename (the two
        // are in one folder, so it is rare) can still leave the new EPUB beside the old JSON.
        let staged_epub = Staged::copy(epub, &epub_to)?;
        let mut sidecar = sidecar.clone();
        sidecar.files.kfx = None;
        let json = serde_json::to_vec_pretty(&sidecar).map_err(io::Error::other)?;
        let staged_json = Staged::write(&json_to, &json)?;
        staged_epub.commit()?;
        staged_json.commit()?;
        let _ = std::fs::remove_file(kfx_to);
        self.prune_old(today);
        Ok(())
    }

    /// Keeps the converted KFX beside the edition and says so in its sidecar.
    pub fn archive_kfx(&self, date: NaiveDate, kfx: &Path) -> io::Result<()> {
        let (_, json_to, kfx_to) = self.paths(date);
        let mut sidecar = self.sidecar(date).ok_or_else(|| {
            if json_to.is_file() {
                io::Error::new(
                    io::ErrorKind::InvalidData,
                    format!("{} can't be read", json_to.display()),
                )
            } else {
                io::Error::new(io::ErrorKind::NotFound, "that edition isn't archived")
            }
        })?;
        copy_atomic(kfx, &kfx_to)?;
        sidecar.files.kfx = Some(format!("{date}.kfx"));
        let json = serde_json::to_vec_pretty(&sidecar).map_err(io::Error::other)?;
        write_atomic(&json_to, &json)
    }

    /// Every archived edition, newest first.
    pub fn editions(&self) -> Vec<NaiveDate> {
        let mut out = Vec::new();
        let Ok(years) = std::fs::read_dir(&self.root) else {
            return out;
        };
        for year in years.flatten() {
            let Some(y) = year.file_name().to_str().map(str::to_owned) else {
                continue;
            };
            if y.len() != 4 || !y.bytes().all(|b| b.is_ascii_digit()) || !year.path().is_dir() {
                continue;
            }
            let Ok(files) = std::fs::read_dir(year.path()) else {
                continue;
            };
            for f in files.flatten() {
                let name = f.file_name().to_string_lossy().into_owned();
                let Some(date) = name.strip_suffix(".json").and_then(strict_date) else {
                    continue;
                };
                if date.format("%Y").to_string() == y {
                    out.push(date);
                }
            }
        }
        out.sort_by(|a, b| b.cmp(a));
        out
    }

    pub fn sidecar(&self, date: NaiveDate) -> Option<Sidecar> {
        let bytes = std::fs::read(self.paths(date).1).ok()?;
        serde_json::from_slice(&bytes).ok()
    }

    /// Removes editions past `keep_days` (when it isn't 0) and KFX copies past two weeks,
    /// relative to `today`. Returns what it removed.
    pub fn prune_old(&self, today: NaiveDate) -> Vec<PathBuf> {
        let mut removed = Vec::new();
        for date in self.editions() {
            let age = (today - date).num_days();
            let (epub, json, kfx) = self.paths(date);
            if self.keep_days > 0 && age > i64::from(self.keep_days) {
                for p in [epub, json, kfx] {
                    if std::fs::remove_file(&p).is_ok() {
                        removed.push(p);
                    }
                }
            } else if age > KFX_DAYS && kfx.exists() && std::fs::remove_file(&kfx).is_ok() {
                removed.push(kfx);
                if let Some(mut s) = self.sidecar(date) {
                    s.files.kfx = None;
                    if let Ok(json_bytes) = serde_json::to_vec_pretty(&s) {
                        let _ = write_atomic(&json, &json_bytes);
                    }
                }
            }
        }
        removed
    }

    /// The editions kept and the bytes they take.
    pub fn usage(&self) -> (usize, u64) {
        let dates = self.editions();
        let bytes = dates
            .iter()
            .flat_map(|d| {
                let (a, b, c) = self.paths(*d);
                [a, b, c]
            })
            .filter_map(|p| std::fs::metadata(p).ok())
            .map(|m| m.len())
            .sum();
        (dates.len(), bytes)
    }
}
