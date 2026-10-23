// SPDX-License-Identifier: GPL-3.0-or-later
//! The Kindle tab's This Kindle room (Books and Kindle design §7): where the space goes, books
//! that could make room, and backing a Kindle's DRM-free books up and restoring them.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use deckle_core::deliver::{Conn, sanitize};
use deckle_core::inventory::{DeviceBook, Space, copy_in, copy_out};
use serde::{Deserialize, Serialize};

use crate::shelf::{Shelved, identity};

/// What a book on a Kindle is, for the space it takes and the Shelf's filters.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Kind {
    /// Locked by DRM: from the Kindle Store, or Send to Kindle. Amazon keeps a copy.
    AmazonOwn,
    /// A book Deckle built.
    Deckle,
    /// The Deckle Daily.
    Paper,
    /// Anything else put on by USB.
    Sideloaded,
}

impl Kind {
    pub fn label(self) -> &'static str {
        match self {
            Kind::AmazonOwn => "Amazon's own",
            Kind::Deckle => "Deckle's books",
            Kind::Paper => "Deckle papers",
            Kind::Sideloaded => "Sideloaded",
        }
    }
}

/// A file name the paper goes by: date-named, or sent by hand as "Deckle - …".
pub fn is_paper(file: &str, prefix: &str) -> bool {
    let name = file.rsplit('/').next().unwrap_or(file);
    name.starts_with(prefix) || name.starts_with("Deckle - ")
}

/// `prefix` is the paper's file-name prefix (`deckle_digest::EDITION_PREFIX`).
pub fn kind_of(s: &Shelved, prefix: &str) -> Kind {
    let b = &s.book;
    if b.drm {
        Kind::AmazonOwn
    } else if b.from_deckle && is_paper(&b.file, prefix) {
        Kind::Paper
    } else if b.from_deckle || s.library.is_some() {
        Kind::Deckle
    } else {
        Kind::Sideloaded
    }
}

/// How far through a book is.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Reading {
    NotStarted,
    Reading,
    Finished,
}

/// Under this fraction a book counts as not started; at or over [`FINISHED`], finished.
pub const STARTED: f64 = 0.02;
pub const FINISHED: f64 = 0.97;

pub fn reading(fraction: Option<f64>) -> Reading {
    match fraction {
        Some(f) if f >= FINISHED => Reading::Finished,
        Some(f) if f >= STARTED => Reading::Reading,
        _ => Reading::NotStarted,
    }
}

/// The space on a Kindle, split by kind. `other` is what isn't a book in `documents/`: the
/// system, dictionaries, fonts, and books in folders the listing doesn't look into.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize)]
pub struct SpaceSplit {
    pub total: u64,
    pub free: u64,
    /// `(kind, bytes, books)`, largest first.
    pub kinds: Vec<(Kind, u64, usize)>,
    pub other: u64,
}

pub fn split(books: &[(Kind, u64)], space: Space) -> SpaceSplit {
    let mut by: BTreeMap<Kind, (u64, usize)> = BTreeMap::new();
    for (k, size) in books {
        let e = by.entry(*k).or_default();
        e.0 += size;
        e.1 += 1;
    }
    let mut kinds: Vec<(Kind, u64, usize)> = by.into_iter().map(|(k, (b, n))| (k, b, n)).collect();
    kinds.sort_by(|a, b| b.1.cmp(&a.1).then(a.0.cmp(&b.0)));
    let books: u64 = kinds.iter().map(|k| k.1).sum();
    SpaceSplit {
        total: space.total,
        free: space.free,
        other: space.total.saturating_sub(space.free).saturating_sub(books),
        kinds,
    }
}

/// Finished books smaller than this aren't worth the bother of removing.
pub const BIG: u64 = 10_000_000;

/// Why a book could go to make room.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(tag = "why", rename_all = "kebab-case")]
pub enum Why {
    /// Read to the end, and over [`BIG`].
    Finished,
    /// Another copy of the same book stays.
    Duplicate { keeping: String },
    /// An edition past the paper's `keep`.
    OldPaper,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct Spare {
    pub file: String,
    pub title: String,
    pub size: u64,
    #[serde(flatten)]
    pub why: Why,
    /// It has a reading position, highlights or notes in its `.sdr`, which go with it.
    pub has_sdr: bool,
}

/// One book as the Make room list weighs it.
pub struct Weighed<'a> {
    pub shelved: &'a Shelved,
    pub kind: Kind,
    pub fraction: Option<f64>,
}

fn title(b: &DeviceBook) -> String {
    b.title.clone().unwrap_or_else(|| b.file.clone())
}

/// The copy of a duplicated book to keep: Deckle's, then a KFX, then the one being read, then
/// the largest.
fn keeper<'a>(group: &[&'a Weighed<'a>]) -> &'a Weighed<'a> {
    group
        .iter()
        .copied()
        .max_by_key(|w| {
            let b = &w.shelved.book;
            (
                matches!(w.kind, Kind::Deckle | Kind::AmazonOwn),
                b.format == "kfx",
                w.fraction.is_some(),
                b.size,
                std::cmp::Reverse(b.file.clone()),
            )
        })
        .expect("a group has a book")
}

/// What could go to make room, largest first: finished books over [`BIG`], the spare copies of
/// duplicated books, and the papers in `old_papers` (what the paper's `keep` would prune).
pub fn spare(books: &[Weighed<'_>], old_papers: &[String]) -> Vec<Spare> {
    let mut out = Vec::new();
    let mut groups: BTreeMap<String, Vec<&Weighed<'_>>> = BTreeMap::new();
    for w in books {
        if w.shelved.duplicate
            && w.kind != Kind::Paper
            && let Some(id) = identity(&w.shelved.book)
        {
            groups.entry(id).or_default().push(w);
        }
    }
    let mut duplicate: BTreeMap<&str, String> = BTreeMap::new();
    for group in groups.values().filter(|g| g.len() > 1) {
        let keep = keeper(group);
        for w in group {
            if !std::ptr::eq(*w, keep) {
                duplicate.insert(&w.shelved.book.file, keep.shelved.book.file.clone());
            }
        }
    }
    for w in books {
        let b = &w.shelved.book;
        let why = if w.kind == Kind::Paper {
            old_papers.contains(&b.file).then_some(Why::OldPaper)
        } else if let Some(keeping) = duplicate.get(b.file.as_str()) {
            Some(Why::Duplicate {
                keeping: keeping.clone(),
            })
        } else if reading(w.fraction) == Reading::Finished && b.size > BIG {
            Some(Why::Finished)
        } else {
            None
        };
        if let Some(why) = why {
            out.push(Spare {
                file: b.file.clone(),
                title: title(b),
                size: b.size,
                why,
                has_sdr: b.yjf.is_some(),
            });
        }
    }
    out.sort_by(|a, b| b.size.cmp(&a.size).then(a.file.cmp(&b.file)));
    out
}

/// `deckle-backup.json` in a backup's folder.
pub const MANIFEST: &str = "deckle-backup.json";

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct BackedUp {
    /// The path under `documents/`.
    pub file: String,
    pub title: String,
    pub authors: Vec<String>,
    pub size: u64,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct Manifest {
    /// The Kindle's name.
    pub device: String,
    /// RFC 3339.
    pub at: String,
    pub books: Vec<BackedUp>,
}

/// A backup as the room lists it.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct Backup {
    pub dir: PathBuf,
    pub device: String,
    pub at: String,
    pub books: usize,
    pub size: u64,
}

/// `kindle-backups/` beside `library.json`, where backups go unless a folder is given.
pub fn backups_root(config: &Path) -> PathBuf {
    config.with_file_name("kindle-backups")
}

/// The backups in `root`, newest first.
pub fn backups(root: &Path) -> Vec<Backup> {
    let mut out: Vec<Backup> = std::fs::read_dir(root)
        .into_iter()
        .flatten()
        .flatten()
        .filter_map(|e| {
            let dir = e.path();
            let m: Manifest =
                serde_json::from_slice(&std::fs::read(dir.join(MANIFEST)).ok()?).ok()?;
            Some(Backup {
                size: m.books.iter().map(|b| b.size).sum(),
                books: m.books.len(),
                device: m.device,
                at: m.at,
                dir,
            })
        })
        .collect();
    out.sort_by(|a, b| b.at.cmp(&a.at).then(a.dir.cmp(&b.dir)));
    out
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize)]
pub struct BackupReport {
    pub dir: PathBuf,
    pub copied: Vec<String>,
    /// `(file, why)`: Amazon's own, locked, or the paper.
    pub skipped: Vec<(String, String)>,
    /// `(file, error)`
    pub failed: Vec<(String, String)>,
}

/// Why a book isn't backed up, or `None` when it is.
pub fn left_out(s: &Shelved, prefix: &str) -> Option<&'static str> {
    match kind_of(s, prefix) {
        Kind::AmazonOwn => Some("Amazon's own: Amazon downloads it again"),
        Kind::Paper => Some("the paper: Deckle makes a new one each day"),
        Kind::Deckle | Kind::Sideloaded => None,
    }
}

/// A folder name from a Kindle's name and a timestamp.
fn backup_name(device: &str, stamp: &str) -> String {
    let stamp: String = stamp
        .chars()
        .take(16)
        .map(|c| if c == ':' { '-' } else { c })
        .filter(|c| c.is_ascii_alphanumeric() || *c == '-')
        .collect();
    format!("{} {stamp}", sanitize(device))
}

/// Copies every book on the Kindle that isn't Amazon's own or the paper, with its `.sdr`, into
/// a new folder under `root`, over the caller's connection. `at` is the time, RFC 3339. A book
/// that won't copy costs that book, not the backup.
pub fn backup_on(
    conn: &mut Conn<'_>,
    books: &[Shelved],
    root: &Path,
    device: &str,
    prefix: &str,
    at: &str,
) -> Result<BackupReport, String> {
    let mut dir = root.join(backup_name(device, at));
    let mut n = 2;
    while dir.exists() {
        dir = root.join(format!("{} {n}", backup_name(device, at)));
        n += 1;
    }
    std::fs::create_dir_all(&dir).map_err(|e| format!("{}: {e}", dir.display()))?;
    let mut report = BackupReport {
        dir: dir.clone(),
        ..Default::default()
    };
    let mut manifest = Manifest {
        device: device.to_owned(),
        at: at.to_owned(),
        books: Vec::new(),
    };
    for s in books {
        let b = &s.book;
        if let Some(why) = left_out(s, prefix) {
            report.skipped.push((b.file.clone(), why.to_owned()));
            continue;
        }
        // Copying in and out reaches one folder below documents/, no deeper.
        if b.file.matches('/').count() > 1 {
            report
                .skipped
                .push((b.file.clone(), "in a folder within a folder".to_owned()));
            continue;
        }
        let dest = match b.file.rsplit_once('/') {
            Some((folder, _)) => dir.join(folder),
            None => dir.clone(),
        };
        match copy_out(conn, &b.file, &dest) {
            Ok(()) => {
                report.copied.push(b.file.clone());
                manifest.books.push(BackedUp {
                    file: b.file.clone(),
                    title: title(b),
                    authors: b.authors.clone(),
                    size: b.size,
                });
            }
            Err(e) => report.failed.push((b.file.clone(), e.to_string())),
        }
    }
    let json = serde_json::to_vec_pretty(&manifest).map_err(|e| e.to_string())?;
    let path = dir.join(MANIFEST);
    std::fs::write(&path, json).map_err(|e| format!("{}: {e}", path.display()))?;
    Ok(report)
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize)]
pub struct RestoreReport {
    pub restored: Vec<String>,
    /// Already on this Kindle under the same name: left as it is.
    pub already: Vec<String>,
    /// `(file, error)`
    pub failed: Vec<(String, String)>,
}

/// Puts the books of the backup in `dir` on the connection's Kindle, with their `.sdr`. A book
/// already there (`present`, as the listing names files) is left alone.
pub fn restore_on(
    conn: &mut Conn<'_>,
    dir: &Path,
    present: &[String],
) -> Result<RestoreReport, String> {
    let path = dir.join(MANIFEST);
    let manifest: Manifest = serde_json::from_slice(
        &std::fs::read(&path).map_err(|e| format!("{}: {e}", path.display()))?,
    )
    .map_err(|e| format!("{}: {e}", path.display()))?;
    let mut report = RestoreReport::default();
    for b in manifest.books {
        if present.contains(&b.file) {
            report.already.push(b.file);
            continue;
        }
        let src = match b.file.rsplit_once('/') {
            Some((folder, _)) => dir.join(folder),
            None => dir.to_path_buf(),
        };
        match copy_in(conn, &src, &b.file) {
            Ok(()) => report.restored.push(b.file),
            Err(e) => report.failed.push((b.file, e.to_string())),
        }
    }
    Ok(report)
}

#[cfg(test)]
mod tests {
    use super::*;
    use deckle_core::deliver::Target;

    fn shelved(file: &str, size: u64) -> Shelved {
        Shelved {
            book: DeviceBook {
                file: file.into(),
                size,
                format: file.rsplit('.').next().unwrap().into(),
                ..Default::default()
            },
            library: None,
            updates: Vec::new(),
            duplicate: false,
        }
    }

    #[test]
    fn kinds_follow_drm_then_deckle_then_the_paper() {
        let mut store = shelved("Store.kfx", 1);
        store.book.drm = true;
        let mut deckle = shelved("Mine.kfx", 1);
        deckle.book.from_deckle = true;
        let mut paper = shelved("deckle-daily-2026-10-23.kfx", 1);
        paper.book.from_deckle = true;
        let side = shelved("Side.azw3", 1);
        let p = "deckle-daily-";
        assert_eq!(
            [&store, &deckle, &paper, &side].map(|s| kind_of(s, p)),
            [Kind::AmazonOwn, Kind::Deckle, Kind::Paper, Kind::Sideloaded]
        );
    }

    #[test]
    fn the_split_puts_what_isnt_a_book_in_other() {
        let s = split(
            &[
                (Kind::Sideloaded, 10),
                (Kind::AmazonOwn, 30),
                (Kind::Sideloaded, 5),
            ],
            Space {
                total: 100,
                free: 40,
            },
        );
        assert_eq!(
            s.kinds,
            [(Kind::AmazonOwn, 30, 1), (Kind::Sideloaded, 15, 2)]
        );
        assert_eq!(s.other, 15);
    }

    #[test]
    fn make_room_offers_big_finished_books_spare_copies_and_old_papers() {
        let big = shelved("Big.azw3", 20_000_000);
        let small = shelved("Small.azw3", 1_000_000);
        let reading = shelved("Half.azw3", 30_000_000);
        let mut a = shelved("Twin.azw3", 2_000_000);
        a.book.title = Some("Twin".into());
        a.duplicate = true;
        let mut b = shelved("Twin.kfx", 1_000_000);
        b.book.title = Some("Twin".into());
        b.duplicate = true;
        let mut paper = shelved("deckle-daily-2026-10-01.kfx", 3_000_000);
        paper.book.from_deckle = true;
        let w = |s, kind, fraction| Weighed {
            shelved: s,
            kind,
            fraction,
        };
        let books = [
            w(&big, Kind::Sideloaded, Some(1.0)),
            w(&small, Kind::Sideloaded, Some(1.0)),
            w(&reading, Kind::Sideloaded, Some(0.5)),
            w(&a, Kind::Sideloaded, None),
            w(&b, Kind::Sideloaded, None),
            w(&paper, Kind::Paper, None),
        ];
        let got = spare(&books, &["deckle-daily-2026-10-01.kfx".into()]);
        let files: Vec<(&str, &Why)> = got.iter().map(|s| (s.file.as_str(), &s.why)).collect();
        assert_eq!(
            files,
            [
                ("Big.azw3", &Why::Finished),
                ("deckle-daily-2026-10-01.kfx", &Why::OldPaper),
                (
                    "Twin.azw3",
                    &Why::Duplicate {
                        keeping: "Twin.kfx".into()
                    }
                ),
            ]
        );
    }

    #[test]
    fn a_backup_leaves_out_amazons_own_and_restores_onto_another_kindle() {
        let tmp = tempfile::tempdir().unwrap();
        let one = tmp.path().join("one");
        std::fs::create_dir_all(one.join("Side.sdr")).unwrap();
        std::fs::create_dir_all(one.join("Shelf")).unwrap();
        std::fs::write(one.join("Side.azw3"), b"side").unwrap();
        std::fs::write(one.join("Side.sdr/Side.azw3f"), b"position").unwrap();
        std::fs::write(one.join("Shelf/Deep.epub"), b"deep").unwrap();
        std::fs::write(one.join("Store.kfx"), b"store").unwrap();
        let mut store = shelved("Store.kfx", 5);
        store.book.drm = true;
        let books = [
            shelved("Side.azw3", 4),
            shelved("Shelf/Deep.epub", 4),
            store,
        ];
        let root = tmp.path().join("backups");
        let target = Target::Folder(one);
        let done = backup_on(
            &mut Conn::new(&target),
            &books,
            &root,
            "Kindle Scribe",
            "deckle-daily-",
            "2026-10-23T22:30:00Z",
        )
        .unwrap();
        assert_eq!(done.copied, ["Side.azw3", "Shelf/Deep.epub"]);
        assert_eq!(done.skipped.len(), 1);
        assert!(done.failed.is_empty());
        assert!(done.dir.ends_with("Kindle Scribe 2026-10-23T22-30"));
        assert_eq!(
            std::fs::read(done.dir.join("Side.sdr/Side.azw3f")).unwrap(),
            b"position"
        );
        let listed = backups(&root);
        assert_eq!(listed.len(), 1);
        assert_eq!((listed[0].books, listed[0].size), (2, 8));

        let two = tmp.path().join("two");
        std::fs::create_dir_all(&two).unwrap();
        std::fs::write(two.join("Side.azw3"), b"theirs").unwrap();
        let target = Target::Folder(two.clone());
        let back = restore_on(&mut Conn::new(&target), &done.dir, &["Side.azw3".into()]).unwrap();
        assert_eq!(back.restored, ["Shelf/Deep.epub"]);
        assert_eq!(back.already, ["Side.azw3"]);
        assert_eq!(std::fs::read(two.join("Shelf/Deep.epub")).unwrap(), b"deep");
        assert_eq!(std::fs::read(two.join("Side.azw3")).unwrap(), b"theirs");
    }
}
