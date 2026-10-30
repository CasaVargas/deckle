// SPDX-License-Identifier: GPL-3.0-or-later
//! The Kindle tab's Upgrades (Books and Kindle design §7): books Deckle didn't send, sorted
//! into classes, with their originals on this computer; the history of upgrades with Put back;
//! and where originals are looked for.

use std::collections::BTreeMap;
use std::path::Path;

use deckle_core::deliver::Conn;
use deckle_library::sources::{Origin, Sources, calibre_root, home};
use deckle_library::upgrade::{Class, PutBack, Sorted, Upgrades, put_back, sort};
use deckle_library::{Library, LibraryConfig};
use serde::{Deserialize, Serialize};

use crate::commands::RunTarget;

/// A book the Kindle tab can upgrade.
#[derive(Clone, Debug, Serialize)]
pub struct UpgradeRow {
    pub file: String,
    pub title: String,
    pub authors: Vec<String>,
    pub format: String,
    pub class: Class,
    /// The original's path; `None` for a book rebuilt from the Kindle's copy when it's
    /// upgraded ([`device_rebuild`]).
    pub source: Option<String>,
    pub origin: Origin,
    /// How far in, 0–1, from the Kindle's own count.
    pub fraction: Option<f64>,
    /// Partway through: not ticked in Upgrade all.
    pub partway: bool,
    /// What it gains: "Enhanced Typesetting", "Page numbers", "X-Ray", "Word Wise".
    pub gains: Vec<&'static str>,
    pub cover: Option<String>,
}

#[derive(Clone, Debug, Serialize)]
pub struct HistoryRow {
    pub old_file: String,
    pub new_file: Option<String>,
    pub title: String,
    pub origin: Option<Origin>,
    pub at: String,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct SourcesStatus {
    pub find_in_calibre: bool,
    /// The calibre library in use (the configured one, else `~/Calibre Library`), when found.
    pub calibre: Option<String>,
    pub folders: Vec<String>,
    /// The folder whose new EPUBs are added to the library, when one is watched.
    #[serde(default)]
    pub watch: Option<String>,
}

#[derive(Clone, Debug, Serialize)]
pub struct UpgradesView {
    /// The Kindle's profile key, for the Books tab's sends.
    pub device: String,
    pub rows: Vec<UpgradeRow>,
    /// How many of the Kindle's books are in each class (Deckle's own included).
    pub counts: BTreeMap<String, usize>,
    pub history: Vec<HistoryRow>,
    pub sources: SourcesStatus,
    pub warnings: Vec<String>,
}

/// What the original gives that the Kindle's copy lacks.
fn gains(s: &Sorted) -> Vec<&'static str> {
    let b = &s.shelved.book;
    let mut g = Vec::new();
    if b.format != "kfx" {
        g.extend(["Enhanced Typesetting", "Page numbers"]);
    }
    if !b.xray {
        g.push("X-Ray");
    }
    if !b.word_wise {
        g.push("Word Wise");
    }
    g
}

fn class_key(c: Class) -> String {
    serde_json::to_value(c)
        .ok()
        .and_then(|v| v.as_str().map(str::to_owned))
        .unwrap_or_default()
}

pub fn sources_status_at(config: &Path) -> SourcesStatus {
    let cfg = LibraryConfig::load(config);
    SourcesStatus {
        find_in_calibre: cfg.find_in_calibre,
        calibre: calibre_root(&cfg, home().as_deref()).map(|p| p.to_string_lossy().into_owned()),
        folders: cfg
            .find_in
            .iter()
            .map(|p| p.to_string_lossy().into_owned())
            .collect(),
        watch: cfg.watch.map(|w| w.to_string_lossy().into_owned()),
    }
}

pub fn sources_set_at(
    config: &Path,
    find_in_calibre: bool,
    calibre: Option<String>,
    folders: Vec<String>,
) -> Result<SourcesStatus, String> {
    LibraryConfig {
        find_in_calibre,
        calibre: calibre.filter(|c| !c.trim().is_empty()).map(Into::into),
        find_in: folders
            .into_iter()
            .filter(|f| !f.trim().is_empty())
            .map(Into::into)
            .collect(),
        ..LibraryConfig::load(config)
    }
    .save(config)?;
    Ok(sources_status_at(config))
}

/// The upgrades for the connection's Kindle. Reads every book's details, once (over MTP each
/// book not seen before is read whole), and its reading position.
pub fn upgrades_on(
    conn: &mut Conn<'_>,
    config: &Path,
    cache: Option<&Path>,
    home: Option<&Path>,
) -> Result<UpgradesView, String> {
    let device = deckle_library::device_ref(conn.target);
    let (inv, _) =
        deckle_core::inventory::list_detailed(conn, cache, deckle_digest::EDITION_PREFIX)
            .map_err(|e| e.to_string())?;
    let library = Library::configured(config).map(|(l, _)| l);
    let current = deckle_library::freshness::Current::now(&Default::default());
    let shelf = deckle_library::shelf::shelve(inv.books, library.as_ref(), &device.key, &current);
    let cfg = LibraryConfig::load(config);
    let sources = Sources::gather(&cfg, home);
    let sorted = sort(shelf, &sources, |b| {
        deckle_readership::parse_yjf(b)
            .ok()
            .and_then(|p| p.kindle_fraction)
    });
    let mut counts = BTreeMap::new();
    for s in &sorted {
        *counts.entry(class_key(s.class)).or_insert(0) += 1;
    }
    let mut rows: Vec<UpgradeRow> = sorted
        .iter()
        .filter(|s| matches!(s.class, Class::HasSource | Class::KindleCopy))
        .map(|s| {
            let src = s.source.as_ref();
            let b = &s.shelved.book;
            UpgradeRow {
                file: b.file.clone(),
                title: b
                    .title
                    .clone()
                    .or_else(|| src.map(|x| x.title.clone()))
                    .unwrap_or_else(|| b.file.clone()),
                authors: match (b.authors.is_empty(), src) {
                    (true, Some(x)) => x.authors.clone(),
                    _ => b.authors.clone(),
                },
                format: b.format.clone(),
                class: s.class,
                source: src.map(|x| x.path.to_string_lossy().into_owned()),
                origin: src.map_or(Origin::KindleCopy, |x| x.origin),
                fraction: s.fraction,
                partway: s.partway(),
                gains: gains(s),
                cover: b.cover.as_deref().map(crate::kindle::data_url),
            }
        })
        .collect();
    rows.sort_by_cached_key(|r| r.title.to_lowercase());
    let history = Upgrades::load(config)
        .for_device(&device.key)
        .map(|u| HistoryRow {
            old_file: u.old_file.clone(),
            new_file: u.new_file.clone(),
            title: u.title.clone(),
            origin: u.origin,
            at: u.at.clone(),
        })
        .collect();
    Ok(UpgradesView {
        device: device.key,
        rows,
        counts,
        history,
        sources: sources_status_at(config),
        warnings: sources.warnings,
    })
}

/// Lists over USB/MTP.
pub fn device_upgrades(target: RunTarget) -> Result<UpgradesView, String> {
    crate::kindle::with_target(target, |conn, config| {
        let cache = deckle_core::wordwise::default_cache_dir().join("inventory");
        upgrades_on(conn, config, Some(&cache), home().as_deref())
    })
}

/// One book [`device_rebuild`] tried: the rebuilt EPUB, or why it couldn't be rebuilt.
#[derive(Clone, Debug, Serialize)]
pub struct Rebuild {
    pub file: String,
    pub source: Option<String>,
    pub error: Option<String>,
}

/// Rebuilds the Kindle's copies of `files` as EPUBs, over one session (Books and Kindle §5).
pub fn rebuild_on(conn: &mut Conn<'_>, config: &Path, files: &[String]) -> Vec<Rebuild> {
    files
        .iter()
        .map(
            |f| match deckle_library::upgrade::rebuild_from_kindle(conn, config, f) {
                Ok(p) => Rebuild {
                    file: f.clone(),
                    source: Some(p.to_string_lossy().into_owned()),
                    error: None,
                },
                Err(e) => Rebuild {
                    file: f.clone(),
                    source: None,
                    error: Some(e),
                },
            },
        )
        .collect()
}

pub fn device_rebuild(target: RunTarget, files: Vec<String>) -> Result<Vec<Rebuild>, String> {
    crate::kindle::with_target(target, move |conn, config| {
        Ok(rebuild_on(conn, config, &files))
    })
}

pub fn device_put_back(target: RunTarget, file: String) -> Result<PutBack, String> {
    crate::kindle::with_target(target, move |conn, config| put_back(conn, config, &file))
}

pub fn library_sources() -> SourcesStatus {
    sources_status_at(&deckle_library::config::default_path())
}

pub fn library_sources_set(
    find_in_calibre: bool,
    calibre: Option<String>,
    folders: Vec<String>,
) -> Result<SourcesStatus, String> {
    sources_set_at(
        &deckle_library::config::default_path(),
        find_in_calibre,
        calibre,
        folders,
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use deckle_core::deliver::Target;

    #[test]
    fn a_folder_kindle_lists_the_book_its_original_is_in_a_folder_for() {
        let tmp = tempfile::tempdir().unwrap();
        let config = tmp.path().join("library.json");
        let originals = tmp.path().join("Books");
        std::fs::create_dir_all(&originals).unwrap();
        let epub = deckle_kfx::fixtures::path("deckle-mini.epub");
        std::fs::copy(&epub, originals.join("mini.epub")).unwrap();
        let info = deckle_core::inspect::inspect(&epub).unwrap();
        let status = sources_set_at(
            &config,
            false,
            None,
            vec![originals.to_string_lossy().into_owned()],
        )
        .unwrap();
        assert_eq!(status.folders.len(), 1);
        assert!(status.calibre.is_none() && !status.find_in_calibre);

        let kindle = tmp.path().join("Kindle");
        std::fs::create_dir_all(&kindle).unwrap();
        let author = info.creators[0].name.clone();
        std::fs::write(
            kindle.join("Mini.azw3"),
            deckle_mobi::fixture::build(&deckle_mobi::fixture::Book {
                title: &info.title,
                authors: &[&author],
                ..Default::default()
            }),
        )
        .unwrap();
        std::fs::write(
            kindle.join("Store.azw"),
            deckle_mobi::fixture::build(&deckle_mobi::fixture::Book {
                title: "Store",
                drm: true,
                cde: Some("EBOK"),
                ..Default::default()
            }),
        )
        .unwrap();
        let target = Target::Folder(kindle);
        let view = upgrades_on(&mut Conn::new(&target), &config, None, None).unwrap();
        assert_eq!(view.rows.len(), 1);
        let row = &view.rows[0];
        assert!(row.source.is_some());
        assert_eq!(row.file, "Mini.azw3");
        assert_eq!(row.origin, Origin::Folder);
        assert_eq!(
            row.gains,
            ["Enhanced Typesetting", "Page numbers", "X-Ray", "Word Wise"]
        );
        assert!(!row.partway);
        assert_eq!(view.counts.get("has-source"), Some(&1));
        assert_eq!(view.counts.get("amazon-own"), Some(&1));
        assert!(view.history.is_empty());
    }
}

#[cfg(test)]
mod rebuild_tests {
    use super::*;
    use deckle_core::deliver::Target;

    #[test]
    fn a_kindle_copy_is_listed_without_a_source_and_rebuilt_on_demand() {
        let tmp = tempfile::tempdir().unwrap();
        let config = tmp.path().join("library.json");
        sources_set_at(&config, false, None, Vec::new()).unwrap();
        let kindle = tmp.path().join("Kindle");
        std::fs::create_dir_all(&kindle).unwrap();
        let part = format!(
            "<?xml version=\"1.0\"?><html xmlns=\"http://www.w3.org/1999/xhtml\"><head><title>A</title></head><body><h2 id=\"a\">A</h2>{}</body></html>",
            "<p>Words enough to make a page of a book here.</p>".repeat(40)
        );
        std::fs::write(
            kindle.join("Voyage.azw3"),
            deckle_mobi::fixture::build_kf8(&deckle_mobi::fixture::Kf8 {
                title: "Voyage",
                parts: &[part.as_str()],
                ..Default::default()
            }),
        )
        .unwrap();
        let target = Target::Folder(kindle);
        let view = upgrades_on(&mut Conn::new(&target), &config, None, None).unwrap();
        let row = &view.rows[0];
        assert_eq!(
            (row.class, row.origin, row.source.as_deref()),
            (Class::KindleCopy, Origin::KindleCopy, None)
        );
        let done = rebuild_on(
            &mut Conn::new(&target),
            &config,
            &["Voyage.azw3".into(), "Gone.azw3".into()],
        );
        assert!(done[0].error.is_none(), "{:?}", done[0].error);
        assert!(std::path::Path::new(done[0].source.as_deref().unwrap()).is_file());
        assert!(done[1].source.is_none() && done[1].error.is_some());
    }
}
