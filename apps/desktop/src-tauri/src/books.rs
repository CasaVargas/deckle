// SPDX-License-Identifier: GPL-3.0-or-later
//! The Books tab's library and Before you send (Books and Kindle design §8): every book Deckle
//! can see, their covers, what Deckle will do to one, its cover picker and Fix details, and the
//! watched folder.

use std::path::{Path, PathBuf};

use deckle_core::preflight::{Chapter, CoverSource, PreflightOptions, preflight};
use deckle_core::typography::TypographyReport;
use deckle_library::catalog::{Entry, catalog, cover_of};
use deckle_library::details::{Details, fix};
use deckle_library::sources::{Sources, home};
use deckle_library::upgrade::Upgrades;
use deckle_library::watch::{Watched, scan, seen_path};
use deckle_library::{Library, LibraryConfig};
use serde::Serialize;
use tauri::State;

use crate::commands::{AppState, CoverChoice, load_settings_from};
use crate::kindle::data_url;

/// A library entry, and whether it's on the Kindle picked in the footer.
#[derive(Clone, Debug, Serialize)]
pub struct EntryRow {
    #[serde(flatten)]
    pub entry: Entry,
    pub on_kindle: bool,
}

#[derive(Clone, Debug, Serialize)]
pub struct CatalogView {
    pub entries: Vec<EntryRow>,
    /// Deckle's own library is set up.
    pub library: bool,
    /// The folder whose new EPUBs are added, when one is watched.
    pub watch: Option<String>,
    pub warnings: Vec<String>,
}

/// `device` is the footer's Kindle (its profile key), when one is connected.
pub fn catalog_at(config: &Path, home: Option<&Path>, device: Option<&str>) -> CatalogView {
    let cfg = LibraryConfig::load(config);
    let library = Library::configured(config).map(|(l, _)| l);
    let sources = Sources::gather(&cfg, home);
    CatalogView {
        entries: catalog(library.as_ref(), &sources, &Upgrades::load(config))
            .into_iter()
            .map(|entry| EntryRow {
                on_kindle: device.is_some_and(|d| {
                    entry
                        .sent_to
                        .iter()
                        .any(|s| deckle_core::devices::same_device(s, d))
                }),
                entry,
            })
            .collect(),
        library: library.is_some(),
        watch: cfg.watch.map(|w| w.to_string_lossy().into_owned()),
        warnings: sources.warnings,
    }
}

#[tauri::command]
pub async fn library_catalog(device: Option<String>) -> Result<CatalogView, String> {
    tauri::async_runtime::spawn_blocking(move || {
        catalog_at(
            &deckle_library::config::default_path(),
            home().as_deref(),
            device.as_deref(),
        )
    })
    .await
    .map_err(|e| e.to_string())
}

/// Covers this tall make the grid's thumbnails.
const THUMB_PX: u32 = 360;

/// A small cover for each of `paths`, as `data:` URLs; `None` for a book without one.
pub fn covers_of(paths: &[String]) -> Vec<(String, Option<String>)> {
    paths
        .iter()
        .map(|p| {
            let thumb = cover_of(Path::new(p))
                .and_then(|b| deckle_core::thumbnail::render(&b, THUMB_PX).ok())
                .map(|j| data_url(&j));
            (p.clone(), thumb)
        })
        .collect()
}

#[tauri::command]
pub async fn library_covers(paths: Vec<String>) -> Result<Vec<(String, Option<String>)>, String> {
    tauri::async_runtime::spawn_blocking(move || covers_of(&paths))
        .await
        .map_err(|e| e.to_string())
}

/// What Deckle will do to one book, for Before you send.
#[derive(Clone, Debug, Serialize)]
pub struct PreflightView {
    pub typography: Option<TypographyReport>,
    /// Notes made into pop-up notes, when that's on.
    pub notes: Option<u32>,
    /// Links that look like notes but have no way back, left as they are.
    pub notes_skipped: u32,
    pub cover: CoverSource,
    /// The cover as it goes out.
    pub cover_url: Option<String>,
    pub images_downscaled: u32,
    pub toc_synthesized: bool,
    pub warnings: Vec<String>,
    pub before: Option<Chapter>,
    pub after: Option<Chapter>,
}

pub fn preflight_with(
    path: &Path,
    settings: &crate::commands::Settings,
    cover: Option<&CoverChoice>,
) -> Result<PreflightView, String> {
    let work = tempfile::tempdir().map_err(|e| e.to_string())?;
    let p = preflight(
        path,
        &PreflightOptions {
            typography: settings.typography,
            popup_notes: settings.popup_notes,
            generated_cover: settings.generated_cover.then_some(None),
            cover: cover.map(CoverChoice::pick).unwrap_or_default(),
        },
        work.path(),
    )?;
    let n = &p.normalize;
    Ok(PreflightView {
        typography: n.typography.clone(),
        notes: n.notes.as_ref().map(|r| r.semantic + r.heuristic),
        notes_skipped: n.notes.as_ref().map_or(0, |r| r.skipped_one_way),
        cover: p.cover,
        cover_url: p
            .cover_jpeg
            .as_deref()
            .and_then(|b| deckle_core::thumbnail::render(b, 600).ok())
            .map(|j| data_url(&j)),
        images_downscaled: n.images_downscaled,
        toc_synthesized: n.toc_synthesized,
        warnings: p.warnings,
        before: p.before,
        after: p.after,
    })
}

#[tauri::command]
pub async fn book_preflight(
    state: State<'_, AppState>,
    path: String,
    cover: Option<CoverChoice>,
) -> Result<PreflightView, String> {
    let settings = load_settings_from(&state.settings_file);
    tauri::async_runtime::spawn_blocking(move || {
        preflight_with(Path::new(&path), &settings, cover.as_ref())
    })
    .await
    .map_err(|e| e.to_string())?
}

/// An Open Library cover the picker offers.
#[derive(Clone, Debug, Serialize)]
pub struct CoverOffer {
    pub thumb: String,
    pub large: String,
}

/// Open Library's covers, when the owner has Open Library on (the metadata setting).
#[tauri::command]
pub async fn cover_candidates(
    state: State<'_, AppState>,
    title: String,
    author: String,
    isbn: Option<String>,
) -> Result<Vec<CoverOffer>, String> {
    if !load_settings_from(&state.settings_file).metadata_fill {
        return Err("Turn on Open Library in Settings → Books to look for covers there.".into());
    }
    tauri::async_runtime::spawn_blocking(move || {
        let mut fetch = deckle_core::wordwise::http_get_bytes;
        deckle_core::covers::candidates(&title, &author, isbn.as_deref(), &mut fetch).map(|c| {
            c.into_iter()
                .map(|c| CoverOffer {
                    thumb: data_url(&c.thumb),
                    large: c.large,
                })
                .collect()
        })
    })
    .await
    .map_err(|e| e.to_string())?
}

/// `covers/` in Deckle's cache, where picked covers are kept for the send.
fn covers_dir() -> PathBuf {
    deckle_core::wordwise::default_cache_dir().join("covers")
}

/// Fetches the large cover at `large` into the cache; its path is what the send uses.
#[tauri::command]
pub async fn cover_fetch(large: String) -> Result<String, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let mut fetch = deckle_core::wordwise::http_get_bytes;
        let bytes = deckle_core::covers::fetch_large(&large, &mut fetch)?;
        let name: String = large
            .rsplit('/')
            .take(2)
            .collect::<Vec<_>>()
            .join("-")
            .chars()
            .map(|c| {
                if c.is_ascii_alphanumeric() || c == '.' || c == '-' {
                    c
                } else {
                    '_'
                }
            })
            .collect();
        let dir = covers_dir();
        std::fs::create_dir_all(&dir).map_err(|e| format!("{}: {e}", dir.display()))?;
        let path = dir.join(name);
        std::fs::write(&path, bytes).map_err(|e| format!("{}: {e}", path.display()))?;
        Ok(path.to_string_lossy().into_owned())
    })
    .await
    .map_err(|e| e.to_string())?
}

/// Writes `details` into the library book in `dir` (its copy's OPF and sidecar).
pub fn fix_at(config: &Path, dir: &Path, details: &Details) -> Result<(), String> {
    let (lib, _) = Library::configured(config).ok_or("Set up a library first.")?;
    fix(&lib, dir, details).map(|_| ())
}

#[tauri::command]
pub fn library_fix_details(dir: String, details: Details) -> Result<(), String> {
    fix_at(
        &deckle_library::config::default_path(),
        Path::new(&dir),
        &details,
    )
}

pub fn watch_set_at(config: &Path, dir: Option<String>) -> Result<Option<String>, String> {
    let dir = dir.filter(|d| !d.trim().is_empty()).map(PathBuf::from);
    if let Some(d) = &dir
        && !d.is_dir()
    {
        return Err(format!("{}: not a folder", d.display()));
    }
    LibraryConfig {
        watch: dir.clone(),
        ..LibraryConfig::load(config)
    }
    .save(config)?;
    Ok(dir.map(|d| d.to_string_lossy().into_owned()))
}

#[tauri::command]
pub fn library_watch_set(dir: Option<String>) -> Result<Option<String>, String> {
    watch_set_at(&deckle_library::config::default_path(), dir)
}

/// Adds what's new in the watched folder. Nothing when no folder is watched or no library is
/// set up.
pub fn watch_scan_at(config: &Path) -> Result<Watched, String> {
    let Some((lib, cfg)) = Library::configured(config) else {
        return Ok(Watched::default());
    };
    match cfg.watch {
        Some(dir) => scan(&lib, &dir, &seen_path(config)),
        None => Ok(Watched::default()),
    }
}

#[tauri::command]
pub async fn library_watch_scan() -> Result<Watched, String> {
    tauri::async_runtime::spawn_blocking(|| watch_scan_at(&deckle_library::config::default_path()))
        .await
        .map_err(|e| e.to_string())?
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_catalog_covers_preflight_and_fix_details_work_on_a_library_book() {
        let tmp = tempfile::tempdir().unwrap();
        let config = tmp.path().join("library.json");
        crate::kindle::library_set_at(
            &config,
            Some(tmp.path().join("Library").to_string_lossy().into_owned()),
            true,
        )
        .unwrap();
        let lib = Library::new(tmp.path().join("Library"));
        let book = lib
            .add(&deckle_kfx::fixtures::path("deckle-mini.epub"))
            .unwrap()
            .book;
        let view = catalog_at(&config, None, Some("ms:Kindle"));
        assert!(view.library && view.watch.is_none());
        assert_eq!(view.entries.len(), 1);
        assert!(!view.entries[0].on_kindle);
        let path = view.entries[0].entry.path.to_string_lossy().into_owned();
        let covers = covers_of(std::slice::from_ref(&path));
        assert!(
            covers[0]
                .1
                .as_deref()
                .unwrap()
                .starts_with("data:image/jpeg")
        );

        let settings = crate::commands::Settings {
            typography: true,
            ..Default::default()
        };
        let own = preflight_with(Path::new(&path), &settings, None).unwrap();
        assert_eq!(own.cover, CoverSource::Own);
        assert!(own.typography.is_some() && own.notes.is_none());
        let generated =
            preflight_with(Path::new(&path), &settings, Some(&CoverChoice::Generated)).unwrap();
        assert_eq!(generated.cover, CoverSource::Generated);

        fix_at(
            &config,
            &book.dir,
            &Details {
                title: "Mended".into(),
                authors: vec!["A. Mender".into()],
                series: None,
            },
        )
        .unwrap();
        assert_eq!(
            catalog_at(&config, None, None).entries[0].entry.title,
            "Mended"
        );

        let downloads = tmp.path().join("Downloads");
        std::fs::create_dir_all(&downloads).unwrap();
        assert!(watch_set_at(&config, Some("/no/such/folder".into())).is_err());
        let set = watch_set_at(&config, Some(downloads.to_string_lossy().into_owned())).unwrap();
        assert!(set.is_some());
        assert!(watch_scan_at(&config).unwrap().added.is_empty());
        assert_eq!(watch_set_at(&config, None).unwrap(), None);
    }
}
