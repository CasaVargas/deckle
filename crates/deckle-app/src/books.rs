// SPDX-License-Identifier: GPL-3.0-or-later
//! The Books tab's library and Before you send (Books and Kindle design §8): every book Deckle
//! can see, their covers, what Deckle will do to one, its cover picker and Fix details, and the
//! watched folder.

use std::path::{Path, PathBuf};

use deckle_core::metadata_fill::{Metadata, Missing, lookup};
use deckle_core::preflight::{Chapter, CoverSource, PreflightOptions, preflight};
use deckle_core::typography::TypographyReport;
use deckle_library::catalog::{Entry, catalog, cover_of};
use deckle_library::details::{Details, fix, read_book};
use deckle_library::sources::{Sources, home};
use deckle_library::upgrade::Upgrades;
use deckle_library::watch::{Watched, scan, seen_path};
use deckle_library::{Library, LibraryConfig, SeriesRef};
use serde::Serialize;

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

pub fn library_catalog(device: Option<String>) -> CatalogView {
    catalog_at(
        &deckle_library::config::default_path(),
        home().as_deref(),
        device.as_deref(),
    )
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

pub fn library_covers(paths: Vec<String>) -> Vec<(String, Option<String>)> {
    covers_of(&paths)
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
    let preset = settings
        .book_presets
        .get(&path.to_string_lossy().into_owned())
        .copied();
    let p = preflight(
        path,
        &PreflightOptions {
            preset,
            typography: preset.map_or(settings.typography, |p| p.typography()),
            popup_notes: preset.map_or(settings.popup_notes, |p| p.popup_notes()),
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

pub fn book_preflight(
    state: &AppState,
    path: String,
    cover: Option<CoverChoice>,
) -> Result<PreflightView, String> {
    let settings = load_settings_from(&state.settings_file);
    preflight_with(Path::new(&path), &settings, cover.as_ref())
}

/// An Open Library cover the picker offers.
#[derive(Clone, Debug, Serialize)]
pub struct CoverOffer {
    pub thumb: String,
    pub large: String,
}

/// Open Library's covers, when the owner has Open Library on (the metadata setting).
pub fn cover_candidates(
    state: &AppState,
    title: String,
    author: String,
    isbn: Option<String>,
) -> Result<Vec<CoverOffer>, String> {
    if !load_settings_from(&state.settings_file).metadata_fill {
        return Err("Turn on Open Library in Settings → Books to look for covers there.".into());
    }
    let mut fetch = deckle_core::wordwise::http_get_bytes;
    deckle_core::covers::candidates(&title, &author, isbn.as_deref(), &mut fetch).map(|c| {
        c.into_iter()
            .map(|c| CoverOffer {
                thumb: data_url(&c.thumb),
                large: c.large,
            })
            .collect()
    })
}

/// `covers/` in Deckle's cache, where picked covers are kept for the send.
fn covers_dir() -> PathBuf {
    deckle_core::wordwise::default_cache_dir().join("covers")
}

/// Fetches the large cover at `large` into the cache; its path is what the send uses.
pub fn cover_fetch(large: String) -> Result<String, String> {
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
}

/// Writes `details` into the library book in `dir` (its copy's OPF and sidecar).
pub fn fix_at(config: &Path, dir: &Path, details: &Details) -> Result<(), String> {
    let (lib, _) = Library::configured(config).ok_or("Set up a library first.")?;
    fix(&lib, dir, details).map(|_| ())
}

pub fn library_fix_details(dir: String, details: Details) -> Result<(), String> {
    fix_at(
        &deckle_library::config::default_path(),
        Path::new(&dir),
        &details,
    )
}

/// The details of the library book in `dir`, as its copy's OPF has them.
pub fn details_at(config: &Path, dir: &Path) -> Result<Details, String> {
    let (lib, _) = Library::configured(config).ok_or("Set up a library first.")?;
    let book = lib
        .list()
        .into_iter()
        .find(|b| b.dir == dir)
        .ok_or_else(|| format!("{}: not a book in the library", dir.display()))?;
    read_book(&book.source_path())
}

pub fn library_read_details(dir: String) -> Result<Details, String> {
    details_at(&deckle_library::config::default_path(), Path::new(&dir))
}

/// What Open Library has for the fields a book leaves empty (Books tab redesign §6). Empty
/// fields here are ones it has nothing for, or the book already fills.
#[derive(Clone, Debug, Default, PartialEq, Serialize)]
pub struct Suggested {
    pub authors: Vec<String>,
    pub series: Option<SeriesRef>,
    pub description: Option<String>,
    pub tags: Vec<String>,
    pub publisher: Option<String>,
    pub published: Option<String>,
    pub language: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct Suggestions {
    /// "isbn" (the edition) or "search" (the work, by title and author).
    pub source: &'static str,
    pub suggested: Suggested,
}

/// Only what `own` leaves empty: the book's own details always win.
pub fn suggest(own: &Details, found: &Metadata) -> Suggested {
    let empty = |v: &Option<String>| v.as_deref().is_none_or(|s| s.trim().is_empty());
    let or_none = |want: bool, v: &Option<String>| v.clone().filter(|_| want);
    Suggested {
        authors: if own.authors.is_empty() {
            found.authors.clone()
        } else {
            vec![]
        },
        series: if own.series.is_none() {
            found.series.as_ref().map(|s| SeriesRef {
                name: s.name.clone(),
                index: s.index,
            })
        } else {
            None
        },
        description: or_none(empty(&own.description), &found.description),
        tags: if own.tags.as_ref().is_none_or(Vec::is_empty) {
            found.subjects.clone()
        } else {
            vec![]
        },
        publisher: or_none(empty(&own.publisher), &found.publisher),
        published: or_none(empty(&own.published), &found.published),
        language: or_none(empty(&own.language), &found.language),
    }
}

/// Looks the library book in `dir` up on Open Library. `None`: it has no record there.
pub fn suggest_at(
    config: &Path,
    dir: &Path,
    fetch: &mut deckle_core::matcher::Fetcher,
) -> Result<Option<Suggestions>, String> {
    let (lib, _) = Library::configured(config).ok_or("Set up a library first.")?;
    let book = lib
        .list()
        .into_iter()
        .find(|b| b.dir == dir)
        .ok_or_else(|| format!("{}: not a book in the library", dir.display()))?;
    let path = book.source_path();
    let own = read_book(&path)?;
    let info = deckle_core::inspect::inspect(&path).map_err(|e| e.to_string())?;
    Ok(
        lookup(&info, Missing::of(&info), fetch)?.map(|(md, source)| Suggestions {
            source,
            suggested: suggest(&own, &md),
        }),
    )
}

/// Open Library's suggestions for a library book. Online, so only with the metadata setting on
/// or when the person `asked` (the editor's Look up on Open Library).
pub fn library_suggest_details(
    state: &AppState,
    dir: String,
    asked: bool,
) -> Result<Option<Suggestions>, String> {
    if !asked && !load_settings_from(&state.settings_file).metadata_fill {
        return Err("Fill missing details is off in Settings → Books.".into());
    }
    let mut fetch = |url: &str| {
        deckle_core::wordwise::http_get_bytes(url)
            .and_then(|b| String::from_utf8(b).map_err(|e| e.to_string()))
    };
    suggest_at(
        &deckle_library::config::default_path(),
        Path::new(&dir),
        &mut fetch,
    )
}

/// A book Deckle sent to a Kindle, for the Kindle tab when that Kindle isn't plugged in
/// (Kindle tab redesign §7).
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct SentBook {
    /// The library copy, for its cover.
    pub path: String,
    pub title: String,
    pub authors: Vec<String>,
    /// When it was last sent there, RFC 3339.
    pub at: String,
    pub cde: Option<String>,
}

/// A Kindle the library has sent books to, with those books, newest first.
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct SentKindle {
    /// Its profile key (`ms:…`, `mtp:…`), as a connected Kindle's `DeviceRow.key`.
    pub key: String,
    pub name: String,
    /// The last send to it.
    pub last: String,
    pub books: Vec<SentBook>,
}

/// The Kindles in the library's send records, most recently sent to first. Exports to folders
/// are left out; a book sent twice counts once, at its latest send.
pub fn sent_kindles(lib: &Library) -> Vec<SentKindle> {
    let mut out: Vec<SentKindle> = Vec::new();
    for book in lib.list() {
        let m = &book.sidecar.metadata;
        for s in book
            .sidecar
            .sends
            .iter()
            .filter(|s| !s.device.is_empty() && !s.device.starts_with("folder:"))
        {
            let k = match out.iter_mut().position(|k| k.key == s.device) {
                Some(i) => &mut out[i],
                None => {
                    out.push(SentKindle {
                        key: s.device.clone(),
                        name: s.device_name.clone(),
                        last: String::new(),
                        books: Vec::new(),
                    });
                    out.last_mut().unwrap()
                }
            };
            if s.at > k.last {
                k.last = s.at.clone();
                if !s.device_name.is_empty() {
                    k.name = s.device_name.clone();
                }
            }
            let path = book.source_path().to_string_lossy().into_owned();
            match k.books.iter_mut().find(|b| b.path == path) {
                Some(b) if s.at > b.at => {
                    b.at = s.at.clone();
                    b.cde = s.cde.clone();
                }
                Some(_) => {}
                None => k.books.push(SentBook {
                    path,
                    title: m.title.clone(),
                    authors: m.authors.clone(),
                    at: s.at.clone(),
                    cde: s.cde.clone(),
                }),
            }
        }
    }
    for k in &mut out {
        k.books.sort_by(|a, b| b.at.cmp(&a.at));
    }
    out.sort_by(|a, b| b.last.cmp(&a.last));
    out
}

pub fn library_sent_kindles() -> Vec<SentKindle> {
    Library::configured(&deckle_library::config::default_path())
        .map(|(lib, _)| sent_kindles(&lib))
        .unwrap_or_default()
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

pub fn library_watch_scan() -> Result<Watched, String> {
    watch_scan_at(&deckle_library::config::default_path())
}

/// Explicit searches scan devices once, rather than opening an MTP session on each keystroke.
pub fn library_search(query: String) -> deckle_library::search::Report {
    use deckle_core::deliver::{Target, detect_mass_storage};
    let mut targets: Vec<_> = detect_mass_storage()
        .into_iter()
        .map(Target::MassStorage)
        .collect();
    targets.extend(deckle_transport::usb::detect().into_iter().map(Target::Mtp));
    deckle_library::search::collect(&deckle_library::config::default_path(), &targets, &query)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_saved_preset_is_used_after_reloading_settings_and_in_preflight() {
        let tmp = tempfile::tempdir().unwrap();
        let file = tmp.path().join("settings.json");
        let source = deckle_kfx::fixtures::path("deckle-mini.epub");
        let mut settings = crate::commands::Settings::default();
        settings.book_presets.insert(
            source.to_string_lossy().into_owned(),
            deckle_core::preset::Preset::Academic,
        );
        crate::commands::save_settings_to(&file, &settings).unwrap();
        let loaded = crate::commands::load_settings_from(&file);
        let view = preflight_with(&source, &loaded, None).unwrap();
        assert!(view.typography.is_some() && view.notes.is_some());
        assert!(view.after.unwrap().css.contains("line-height: 1.45"));
        assert!(!view.before.unwrap().css.contains("line-height: 1.45"));
    }

    #[test]
    fn sent_kindles_list_each_kindles_books_newest_first_without_exports() {
        let tmp = tempfile::tempdir().unwrap();
        let lib = Library::new(tmp.path().join("Library"));
        let mut book = lib
            .add(&deckle_kfx::fixtures::path("deckle-mini.epub"))
            .unwrap()
            .book;
        let send = |device: &str, name: &str, at: &str, cde: &str| deckle_library::SendRecord {
            device: device.into(),
            device_name: name.into(),
            file: "x.kfx".into(),
            at: at.into(),
            cde: Some(cde.into()),
            ..Default::default()
        };
        book.sidecar.sends = vec![
            send("ms:a", "Kindle", "2026-10-01T10:00:00Z", "PDOC"),
            send("ms:a", "Paperwhite", "2026-10-12T10:00:00Z", "EBOK"),
            send("mtp:b", "Oasis", "2026-09-01T10:00:00Z", "PDOC"),
            send("folder:/tmp/out", "", "2026-10-20T10:00:00Z", "PDOC"),
        ];
        lib.save(&book).unwrap();
        let k = sent_kindles(&lib);
        assert_eq!(
            k.iter()
                .map(|k| (k.key.as_str(), k.name.as_str(), k.last.as_str()))
                .collect::<Vec<_>>(),
            vec![
                ("ms:a", "Paperwhite", "2026-10-12T10:00:00Z"),
                ("mtp:b", "Oasis", "2026-09-01T10:00:00Z")
            ]
        );
        assert_eq!(k[0].books.len(), 1, "sent twice, counted once");
        assert_eq!(
            k[0].books[0].cde.as_deref(),
            Some("EBOK"),
            "its latest send"
        );
        assert!(sent_kindles(&Library::new(tmp.path().join("Empty"))).is_empty());
    }

    #[test]
    fn suggestions_cover_only_what_the_book_leaves_empty() {
        let found = Metadata {
            authors: vec!["Ada Field".into()],
            description: Some("A blurb".into()),
            subjects: vec!["SF".into()],
            publisher: Some("Orbit".into()),
            published: Some("2011".into()),
            language: Some("en".into()),
            ..Default::default()
        };
        let own = Details {
            title: "T".into(),
            authors: vec!["Me".into()],
            description: Some(String::new()),
            tags: Some(vec![]),
            publisher: Some("Mine".into()),
            published: Some(" ".into()),
            language: Some("fr".into()),
            ..Default::default()
        };
        assert_eq!(
            suggest(&own, &found),
            Suggested {
                description: Some("A blurb".into()),
                tags: vec!["SF".into()],
                published: Some("2011".into()),
                ..Default::default()
            }
        );
    }

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
                ..Details::default()
            },
        )
        .unwrap();
        assert_eq!(
            catalog_at(&config, None, None).entries[0].entry.title,
            "Mended"
        );
        let read = details_at(&config, &book.dir).unwrap();
        assert_eq!(
            (read.title.as_str(), read.authors.clone()),
            ("Mended", vec!["A. Mender".to_owned()])
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
