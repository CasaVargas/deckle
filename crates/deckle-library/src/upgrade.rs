// SPDX-License-Identifier: GPL-3.0-or-later
//! Upgrading books Deckle didn't send (Books and Kindle design §3, §6): what each book on a
//! Kindle is, the record of every upgrade, and putting an upgrade back.

use std::path::{Path, PathBuf};

use deckle_core::deliver::{Conn, DeliverError, sanitize};
use deckle_core::inventory::DeviceBook;
use serde::{Deserialize, Serialize};

use crate::shelf::Shelved;
use crate::sources::{Origin, Source};

/// What a book on a Kindle is, for upgrading (design §3).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Class {
    /// Sent by Deckle, nothing newer to give.
    Current,
    /// Sent by Deckle; the freshness rule says it can be built better.
    Older,
    /// Not Deckle's, and its original is on this computer.
    HasSource,
    /// A DRM-free AZW3 or MOBI with no original here: rebuilt from the Kindle's own copy.
    KindleCopy,
    /// A DRM-free KFX with no original here.
    NoSource,
    /// A Kindle Store book: never touched.
    AmazonOwn,
    /// DRM-protected, not a store book (a loan, another shop): never opened.
    Locked,
    FixedLayout,
    Pdf,
    /// A text file, an EPUB, anything else.
    Other,
    /// Over MTP, before its details were read.
    Unread,
}

impl Class {
    /// Deckle can upgrade it now (a rebuild from the Kindle's copy is [`Class::KindleCopy`]).
    pub fn upgradable(self) -> bool {
        matches!(self, Class::HasSource | Class::Older)
    }

    /// A short label for listings.
    pub fn label(self) -> &'static str {
        match self {
            Class::Current => "Deckle, current",
            Class::Older => "Deckle, older build",
            Class::HasSource => "original on this computer",
            Class::KindleCopy => "rebuild from the Kindle's copy",
            Class::NoSource => "no original on this computer",
            Class::AmazonOwn => "Amazon's own",
            Class::Locked => "locked (DRM)",
            Class::FixedLayout => "fixed layout",
            Class::Pdf => "PDF",
            Class::Other => "other",
            Class::Unread => "not read yet",
        }
    }
}

/// Sorts a shelved book. `source` is its original from [`crate::sources::Sources::find`].
pub fn classify(s: &Shelved, source: Option<&Source>) -> Class {
    let b = &s.book;
    if s.library.is_some() || b.from_deckle {
        return if s.updates.is_empty() {
            Class::Current
        } else {
            Class::Older
        };
    }
    match b.format.as_str() {
        "pdf" => return Class::Pdf,
        "kfx" | "azw3" | "azw" | "mobi" | "prc" => {}
        _ => return Class::Other,
    }
    if !b.details && !b.drm {
        return Class::Unread;
    }
    if b.fixed_layout {
        return Class::FixedLayout;
    }
    let personal = b.cde.as_deref() == Some("PDOC");
    if b.drm && !personal {
        // A store KFX shows nothing but its DRM; an AZW says EBOK.
        return Class::AmazonOwn;
    }
    if source.is_some() {
        // A personal document Send to Kindle locked can still be upgraded from its original.
        return Class::HasSource;
    }
    if b.drm {
        return Class::Locked;
    }
    if b.format == "kfx" {
        Class::NoSource
    } else {
        Class::KindleCopy
    }
}

/// One upgrade, as `upgrades.json` keeps it: enough to put it back.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct UpgradeRecord {
    /// The Kindle's profile key (`device_ref`).
    pub device: String,
    /// The file Deckle replaced, under `documents/`.
    pub old_file: String,
    /// The copy Deckle wrote, when the run said.
    pub new_file: Option<String>,
    pub title: String,
    /// The original it was built from.
    pub source: PathBuf,
    pub origin: Option<Origin>,
    /// Where the old file and its `.sdr` are kept.
    pub kept: PathBuf,
    /// RFC 3339.
    pub at: String,
}

/// `upgrades.json` beside `library.json`.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct Upgrades {
    pub version: u32,
    pub upgrades: Vec<UpgradeRecord>,
}

impl Upgrades {
    pub fn path(config: &Path) -> PathBuf {
        config.with_file_name("upgrades.json")
    }

    pub fn load(config: &Path) -> Upgrades {
        std::fs::read(Self::path(config))
            .ok()
            .and_then(|b| serde_json::from_slice(&b).ok())
            .unwrap_or_default()
    }

    pub fn save(&self, config: &Path) -> Result<(), String> {
        let mut out = self.clone();
        out.version = 1;
        let json = serde_json::to_vec_pretty(&out).map_err(|e| e.to_string())?;
        crate::write_atomic(&Self::path(config), &json)
    }

    /// The device's upgrades, newest first.
    pub fn for_device<'a>(&'a self, device: &'a str) -> impl Iterator<Item = &'a UpgradeRecord> {
        self.upgrades
            .iter()
            .rev()
            .filter(move |u| deckle_core::devices::same_device(&u.device, device))
    }
}

/// A fresh folder for the old copy of `old_file`, under `kindle-originals/` beside
/// `library.json`: `<device>/<when> <file stem>/`.
pub fn originals_dir(config: &Path, device: &str, old_file: &str, stamp: &str) -> PathBuf {
    let stem = Path::new(old_file.rsplit('/').next().unwrap_or(old_file))
        .file_stem()
        .map(|s| s.to_string_lossy().into_owned())
        .unwrap_or_default();
    let stamp: String = stamp.chars().filter(char::is_ascii_alphanumeric).collect();
    config
        .with_file_name("kindle-originals")
        .join(sanitize(device))
        .join(format!("{stamp} {}", sanitize(&stem)))
}

/// What a Put back did.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize)]
pub struct PutBack {
    pub restored: String,
    pub removed: Vec<String>,
}

/// Puts the old copy of `old_file` back on the connection's Kindle and removes Deckle's copy,
/// then forgets the upgrade and its kept folder.
pub fn put_back(conn: &mut Conn<'_>, config: &Path, old_file: &str) -> Result<PutBack, String> {
    let device = crate::device_ref(conn.target).key;
    let mut all = Upgrades::load(config);
    let i = all
        .upgrades
        .iter()
        .rposition(|u| {
            deckle_core::devices::same_device(&u.device, &device) && u.old_file == old_file
        })
        .ok_or_else(|| format!("{old_file}: no upgrade of it on this Kindle"))?;
    let record = all.upgrades[i].clone();
    let mut out = PutBack {
        restored: format!("documents/{old_file}"),
        ..Default::default()
    };
    if let Some(new) = record.new_file.as_deref().filter(|n| *n != old_file) {
        match deckle_core::inventory::remove_on(conn, new) {
            Ok(removed) => out.removed = removed,
            Err(DeliverError::Refused(_)) => {}
            Err(e) => return Err(format!("removing {new}: {e}")),
        }
    }
    deckle_core::inventory::copy_in(conn, &record.kept, old_file).map_err(|e| e.to_string())?;
    all.upgrades.remove(i);
    all.save(config)?;
    let _ = std::fs::remove_dir_all(&record.kept);
    Ok(out)
}

/// A book's fraction read, from the Kindle's own count in its `.yjf`; `None` when it hasn't
/// been opened (or the file says nothing).
pub fn fraction_read(book: &DeviceBook, parse: impl Fn(&[u8]) -> Option<f64>) -> Option<f64> {
    book.yjf
        .as_deref()
        .and_then(parse)
        .filter(|f| f.is_finite())
}

/// Partway through: opened, past the first pages and not finished. Such a book isn't ticked in
/// Upgrade all (design §12, ruling 1).
pub fn partway(fraction: Option<f64>) -> bool {
    fraction.is_some_and(|f| (0.02..0.97).contains(&f))
}

/// A shelved book with its class, its original when one was found, and how far in it is.
#[derive(Clone, Debug)]
pub struct Sorted {
    pub shelved: Shelved,
    pub class: Class,
    pub source: Option<Source>,
    pub fraction: Option<f64>,
}

impl Sorted {
    /// Partway through (see [`partway`]): not ticked in Upgrade all.
    pub fn partway(&self) -> bool {
        partway(self.fraction)
    }
}

/// Sorts a shelf. `parse` reads a `.yjf` into the Kindle's fraction read.
pub fn sort(
    shelf: Vec<Shelved>,
    sources: &crate::sources::Sources,
    parse: impl Fn(&[u8]) -> Option<f64>,
) -> Vec<Sorted> {
    shelf
        .into_iter()
        .map(|s| {
            let source = if s.library.is_none() && !s.book.from_deckle {
                sources.find(&s.book).cloned()
            } else {
                None
            };
            Sorted {
                class: classify(&s, source.as_ref()),
                fraction: fraction_read(&s.book, &parse),
                source,
                shelved: s,
            }
        })
        .collect()
}

/// Records an upgrade once its run replaced `old_file`; a run that didn't (it failed, or the
/// old copy couldn't be kept) leaves the kept folder removed and returns false.
#[allow(clippy::too_many_arguments)]
pub fn record(
    config: &Path,
    device: &str,
    old_file: &str,
    title: &str,
    source: &Path,
    origin: Option<Origin>,
    kept: &Path,
    report: &deckle_core::pipeline::RunReport,
    at: &str,
) -> Result<bool, String> {
    // A failed run replaces nothing.
    if !report.replaced.iter().any(|f| f == old_file) {
        let _ = std::fs::remove_dir_all(kept);
        return Ok(false);
    }
    let mut all = Upgrades::load(config);
    all.upgrades.push(UpgradeRecord {
        device: device.to_owned(),
        old_file: old_file.to_owned(),
        new_file: report
            .delivered
            .as_ref()
            .and_then(|p| p.file_name())
            .map(|n| n.to_string_lossy().into_owned()),
        title: title.to_owned(),
        source: source.to_path_buf(),
        origin,
        kept: kept.to_path_buf(),
        at: at.to_owned(),
    });
    all.save(config)?;
    Ok(true)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn shelved(b: DeviceBook) -> Shelved {
        Shelved {
            book: b,
            library: None,
            updates: Vec::new(),
            duplicate: false,
        }
    }

    fn book(format: &str) -> DeviceBook {
        DeviceBook {
            file: format!("A Book.{format}"),
            format: format.into(),
            details: true,
            title: Some("A Book".into()),
            ..Default::default()
        }
    }

    fn source() -> Source {
        Source {
            path: "/c/A Book.epub".into(),
            origin: Origin::Calibre,
            title: "A Book".into(),
            authors: Vec::new(),
            isbn: None,
            asin: None,
            calibre_id: None,
        }
    }

    #[test]
    fn every_kind_of_book_lands_in_its_class() {
        let s = source();
        assert_eq!(classify(&shelved(book("azw3")), Some(&s)), Class::HasSource);
        assert_eq!(classify(&shelved(book("azw3")), None), Class::KindleCopy);
        assert_eq!(classify(&shelved(book("kfx")), None), Class::NoSource);
        assert_eq!(classify(&shelved(book("pdf")), Some(&s)), Class::Pdf);
        assert_eq!(classify(&shelved(book("txt")), None), Class::Other);
        let mut deckle = book("kfx");
        deckle.from_deckle = true;
        assert_eq!(classify(&shelved(deckle.clone()), None), Class::Current);
        let mut older = shelved(deckle);
        older.updates = vec![crate::freshness::Reason::Unstamped];
        assert_eq!(classify(&older, None), Class::Older);
        let mut fixed = book("azw3");
        fixed.fixed_layout = true;
        assert_eq!(classify(&shelved(fixed), Some(&s)), Class::FixedLayout);
        let unread = DeviceBook {
            details: false,
            ..book("azw3")
        };
        assert_eq!(classify(&shelved(unread), Some(&s)), Class::Unread);
    }

    #[test]
    fn drm_is_never_upgraded_except_a_personal_document_with_its_original() {
        let s = source();
        let store = DeviceBook {
            drm: true,
            cde: Some("EBOK".into()),
            ..book("azw")
        };
        assert_eq!(classify(&shelved(store), Some(&s)), Class::AmazonOwn);
        let store_kfx = DeviceBook {
            drm: true,
            details: false,
            title: None,
            ..book("kfx")
        };
        assert_eq!(classify(&shelved(store_kfx), Some(&s)), Class::AmazonOwn);
        let doc = DeviceBook {
            drm: true,
            cde: Some("PDOC".into()),
            ..book("azw3")
        };
        assert_eq!(classify(&shelved(doc.clone()), None), Class::Locked);
        assert_eq!(classify(&shelved(doc), Some(&s)), Class::HasSource);
        assert!(!Class::AmazonOwn.upgradable() && !Class::Locked.upgradable());
        assert!(Class::HasSource.upgradable());
    }

    #[test]
    fn partway_is_past_the_start_and_short_of_the_end() {
        assert!(!partway(None));
        assert!(!partway(Some(0.0)) && !partway(Some(0.01)));
        assert!(partway(Some(0.48)));
        assert!(!partway(Some(0.99)));
    }

    #[test]
    fn records_round_trip_and_originals_get_their_own_folder() {
        let tmp = tempfile::tempdir().unwrap();
        let config = tmp.path().join("library.json");
        let dir = originals_dir(
            &config,
            "ms:Kindle Scribe",
            "Books/Middlemarch.azw3",
            "2026-10-23T21:40:00Z",
        );
        assert_eq!(
            dir,
            tmp.path()
                .join("kindle-originals")
                .join(sanitize("ms:Kindle Scribe"))
                .join("20261023T214000Z Middlemarch")
        );
        let mut all = Upgrades::default();
        all.upgrades.push(UpgradeRecord {
            device: "ms:Kindle Scribe".into(),
            old_file: "Middlemarch.azw3".into(),
            new_file: Some("Eliot, George - Middlemarch.kfx".into()),
            title: "Middlemarch".into(),
            source: "/c/Middlemarch.epub".into(),
            origin: Some(Origin::Calibre),
            kept: dir,
            at: "2026-10-23T21:40:00Z".into(),
        });
        all.save(&config).unwrap();
        let back = Upgrades::load(&config);
        assert_eq!(back.version, 1);
        assert_eq!(back.upgrades, all.upgrades);
        assert_eq!(back.for_device("ms:Kindle Scribe").count(), 1);
        assert_eq!(back.for_device("ms:Other").count(), 0);
    }
}
