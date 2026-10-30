// SPDX-License-Identifier: GPL-3.0-or-later
//! The Kindle tab and the library settings (library design §5): what's on a Kindle, matched
//! to the library, and removing a book. Pure logic is in plain functions for the tests.

use std::path::{Path, PathBuf};

use base64::Engine;
use deckle_core::clippings::read_on;
use deckle_core::deliver::{Conn, Target};
use deckle_core::inventory::{DeviceBook, Space, list_with_positions, remove_on};
use deckle_highlights::{Outcome, Pending, State};
use deckle_library::care::{
    self, Backup, BackupReport, Kind, Reading, RestoreReport, SpaceSplit, Spare, Weighed, kind_of,
    reading,
};
use deckle_library::freshness::Reason;
use deckle_library::shelf::{shelve, source_cover};
use deckle_library::sources::{Sources, home};
use deckle_library::upgrade::{Class, Sorted, sort};
use deckle_library::{Library, LibraryConfig};
use serde::{Deserialize, Serialize};

use crate::commands::{RunTarget, resolve_run_target};

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct LibraryStatus {
    pub root: Option<String>,
    pub auto_add: bool,
    pub books: usize,
}

/// One book on the Kindle, as the tab shows it.
#[derive(Clone, Debug, Serialize)]
pub struct DeviceBookRow {
    pub file: String,
    pub size: u64,
    pub format: String,
    pub title: Option<String>,
    pub authors: Vec<String>,
    pub cde: Option<String>,
    pub from_deckle: bool,
    pub xray: bool,
    pub word_wise: bool,
    /// `data:image/…;base64,…`: the KFX's cover, else the library copy's.
    pub cover: Option<String>,
    /// The library's source file, for Re-send and Upgrade.
    pub library_source: Option<String>,
    /// Why Deckle could build it better now (Kindle-updates design §5).
    pub updates: Vec<UpdateReason>,
    /// Another book on this Kindle is the same one.
    pub duplicate: bool,
    /// Its upgrade class (Books and Kindle design §3).
    pub class: Class,
    pub kind: Kind,
    /// How far in, 0–1: the KFX's last position over its end, else the Kindle's own count.
    pub fraction: Option<f64>,
    pub reading: Reading,
    /// When it was last read, UTC.
    pub last_read: Option<String>,
}

#[derive(Clone, Debug, Serialize)]
pub struct UpdateReason {
    /// An older build (else a missing feature).
    pub older: bool,
    /// `X-Ray` | `Word Wise` for a missing feature.
    pub feature: Option<&'static str>,
    pub text: String,
}

#[derive(Clone, Debug, Serialize)]
pub struct DeviceShelf {
    pub device: String,
    pub space: Option<Space>,
    pub books: Vec<DeviceBookRow>,
    /// The Kindle's highlights by book, as the next export would write them (highlights
    /// export design §6).
    pub highlights: Vec<Pending>,
    /// Reading or parsing `My Clippings.txt` went wrong (the rows it could read still show).
    pub highlights_error: Option<String>,
    /// This Kindle's room (design §7): the space by kind, what could make room, and the
    /// backups on this computer.
    pub split: Option<SpaceSplit>,
    pub spare: Vec<Spare>,
    pub backups: Vec<Backup>,
    /// Reading positions left unread past the per-connection cap.
    pub unread_positions: usize,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct HighlightsStatus {
    pub folder: Option<String>,
    /// `[sync] clippings` in `digest.toml`: `digest watch` exports on plug-in.
    pub clippings: bool,
}

/// `highlights.json` beside the library's `library.json`.
fn highlights_state(config: &Path) -> PathBuf {
    config.with_file_name("highlights.json")
}

pub fn data_url(bytes: &[u8]) -> String {
    let mime = if bytes.starts_with(&[0x89, b'P', b'N', b'G']) {
        "image/png"
    } else if bytes.starts_with(b"GIF8") {
        "image/gif"
    } else {
        "image/jpeg"
    };
    format!(
        "data:{mime};base64,{}",
        base64::engine::general_purpose::STANDARD.encode(bytes)
    )
}

pub fn library_status_at(config: &Path) -> LibraryStatus {
    let cfg = LibraryConfig::load(config);
    LibraryStatus {
        books: cfg
            .root
            .as_ref()
            .map(|r| Library::new(r).list().len())
            .unwrap_or(0),
        root: cfg.root.map(|r| r.to_string_lossy().into_owned()),
        auto_add: cfg.auto_add,
    }
}

pub fn library_set_at(
    config: &Path,
    root: Option<String>,
    auto_add: bool,
) -> Result<LibraryStatus, String> {
    let root = root.filter(|r| !r.trim().is_empty()).map(PathBuf::from);
    if let Some(r) = &root {
        std::fs::create_dir_all(r).map_err(|e| format!("{}: {e}", r.display()))?;
    }
    LibraryConfig {
        root,
        auto_add,
        ..LibraryConfig::load(config)
    }
    .save(config)?;
    Ok(library_status_at(config))
}

/// How far in `book` is, and when it was last read, from its position file.
fn progress(book: &DeviceBook) -> (Option<f64>, Option<String>) {
    let Some(p) = book
        .yjf
        .as_deref()
        .and_then(|b| deckle_readership::parse_yjf(b).ok())
    else {
        return (None, None);
    };
    let by_end = match (p.last, book.end) {
        (Some(last), Some(end)) if end > 0 => Some((last.max(0) as f64 / end as f64).min(1.0)),
        _ => None,
    };
    let fraction = by_end
        .or(p.kindle_fraction)
        .filter(|f| f.is_finite())
        .map(|f| f.clamp(0.0, 1.0));
    (fraction, p.last_read)
}

fn row(sorted: Sorted) -> DeviceBookRow {
    let Sorted {
        shelved: s, class, ..
    } = sorted;
    let kind = kind_of(&s, deckle_digest::EDITION_PREFIX);
    let (fraction, last_read) = progress(&s.book);
    let cover = s
        .book
        .cover
        .clone()
        .or_else(|| s.library.as_ref().and_then(source_cover))
        .map(|b| data_url(&b));
    DeviceBookRow {
        library_source: s
            .library
            .as_ref()
            .map(|b| b.source_path().to_string_lossy().into_owned()),
        file: s.book.file,
        size: s.book.size,
        format: s.book.format,
        title: s.book.title,
        authors: s.book.authors,
        cde: s.book.cde,
        from_deckle: s.book.from_deckle,
        xray: s.book.xray,
        word_wise: s.book.word_wise,
        cover,
        updates: s
            .updates
            .iter()
            .map(|r| UpdateReason {
                older: r.is_older(),
                feature: match r {
                    Reason::MissingXRay => Some("X-Ray"),
                    Reason::MissingWordWise => Some("Word Wise"),
                    _ => None,
                },
                text: r.describe(),
            })
            .collect(),
        duplicate: s.duplicate,
        class,
        kind,
        fraction,
        reading: reading(fraction),
        last_read,
    }
}

/// What's on `target`, matched to the library at `config`.
pub fn shelf_on(
    conn: &mut Conn<'_>,
    config: &Path,
    cache: Option<&Path>,
) -> Result<DeviceShelf, String> {
    let device = deckle_library::device_ref(conn.target);
    // The shelf page wants each book's position too: read in this one walk, when it's due.
    let digest = config.with_file_name("digest.toml");
    let shelf_due = deckle_digest::Config::load(&digest).is_ok_and(|c| {
        c.sections.contains(&deckle_digest::SectionKind::Shelf)
            && c.sync
                .for_device(&deckle_core::sync::device_id(conn.target))
                .shelf
    });
    // Every book's position, for the Shelf's progress (design §7).
    let (inv, skipped) = list_with_positions(conn, cache, deckle_digest::EDITION_PREFIX)
        .map_err(|e| e.to_string())?;
    let library = Library::configured(config).map(|(l, _)| l);
    let shelf_books = shelf_due.then(|| inv.books.clone());
    let current = deckle_library::freshness::Current::now(&Default::default());
    let shelved = shelve(inv.books, library.as_ref(), &device.key, &current);
    let keep = deckle_digest::Config::load(&digest).map_or(0, |c| c.keep);
    let old_papers = match keep {
        // No digest config: the paper's own prune decides nothing.
        0 => Vec::new(),
        keep => deckle_core::deliver::prunable(
            shelved
                .iter()
                .filter(|s| !s.book.file.contains('/'))
                .map(|s| s.book.file.clone()),
            deckle_digest::EDITION_PREFIX,
            keep,
        ),
    };
    let weighed: Vec<(Kind, Option<f64>)> = shelved
        .iter()
        .map(|s| {
            (
                kind_of(s, deckle_digest::EDITION_PREFIX),
                progress(&s.book).0,
            )
        })
        .collect();
    let spare = care::spare(
        &shelved
            .iter()
            .zip(&weighed)
            .map(|(s, (kind, fraction))| Weighed {
                shelved: s,
                kind: *kind,
                fraction: *fraction,
            })
            .collect::<Vec<_>>(),
        &old_papers,
    );
    let split = inv.space.map(|space| {
        care::split(
            &shelved
                .iter()
                .zip(&weighed)
                .map(|(s, (kind, _))| (*kind, s.book.size))
                .collect::<Vec<_>>(),
            space,
        )
    });
    let sources = Sources::gather(&LibraryConfig::load(config), home().as_deref());
    let mut books: Vec<DeviceBookRow> = sort(shelved, &sources, |b| {
        deckle_readership::parse_yjf(b)
            .ok()
            .and_then(|p| p.kindle_fraction)
    })
    .into_iter()
    .map(row)
    .collect();
    // Deckle's books first, then by title (else file name).
    books.sort_by_cached_key(|b| {
        (
            !b.from_deckle,
            b.title
                .clone()
                .unwrap_or_else(|| b.file.clone())
                .to_lowercase(),
        )
    });
    let clippings = read_on(conn);
    let (highlights, highlights_error) = highlights_from(&clippings, &highlights_state(config));
    // The puzzle's answers are notes: scored whenever the paper has a puzzle (puzzle design §5).
    puzzle_on(
        &clippings,
        deckle_core::clippings::notes_unreachable(conn.target),
        &config.with_file_name("digest.toml"),
        &config.with_file_name("puzzles.json"),
    );
    // Reading positions ride in the same session (readership design §3).
    crate::readership::record_on(
        conn,
        &config.with_file_name("readership.json"),
        &config.with_file_name("digest.toml"),
    );
    if let Some(books) = shelf_books {
        record_shelf(conn, config, books, skipped, library.as_ref(), &clippings);
    }
    // What the connection teaches the main paper (interests design §5), after the reads and
    // the shelf it draws on are stored.
    let _config_write = crate::config_write()?;
    if let Ok(cfg) = deckle_digest::Config::load(&digest) {
        let state = deckle_readership::State::load(&config.with_file_name("readership.json"));
        let shelf = deckle_digest::shelf::store::Shelf::load(&config.with_file_name("shelf.json"));
        let bytes = clippings.as_ref().ok().and_then(|c| c.as_deref());
        if let Err(e) = deckle_readership::learn::on_connection_at(
            &cfg,
            &config.with_file_name("interests.json"),
            &state,
            bytes,
            Some(&shelf),
            deckle_digest::chrono::Utc::now(),
        ) {
            eprintln!("learning: {e}");
        }
        // a nudge tapped on the Kindle is an accept (interests design §7)
        let learning_path = config.with_file_name("interests.json");
        let mut learning = deckle_readership::Learning::load(&learning_path);
        let mut cfg = cfg;
        let today = deckle_digest::chrono::Local::now().date_naive();
        let added = deckle_readership::learn::accept_nudges(&mut learning, &mut cfg, &state, today);
        if !added.is_empty() {
            match deckle_digest::config::save(&digest, &cfg) {
                Ok(()) => {
                    if let Err(e) = learning.save(&learning_path) {
                        eprintln!("learning: {e}");
                    }
                    for name in added {
                        eprintln!("Added {name} to your interests (you tapped it in the paper).");
                    }
                }
                Err(e) => eprintln!("learning: {e}"),
            }
        }
    }
    Ok(DeviceShelf {
        device: device.name,
        space: inv.space,
        books,
        highlights,
        highlights_error,
        split,
        spare,
        backups: care::backups(&care::backups_root(config)),
        unread_positions: skipped,
    })
}

/// Turns what this session read into the Kindle's snapshot in `shelf.json`. Errors are logged:
/// they never fail the listing.
fn record_shelf(
    conn: &mut Conn<'_>,
    config: &Path,
    books: Vec<deckle_core::inventory::DeviceBook>,
    skipped: usize,
    library: Option<&Library>,
    clippings: &ClippingsRead,
) {
    let device = deckle_library::device_ref(conn.target);
    let (vocab, vocab_failed) = match deckle_core::vocab::read_vocab_on(conn) {
        Ok(v) => (v, false),
        Err(e) => {
            eprintln!("shelf: vocabulary: {e}");
            (None, true)
        }
    };
    let id = deckle_core::sync::device_id(conn.target);
    let named = deckle_digest::Config::load(&config.with_file_name("digest.toml"))
        .ok()
        .and_then(|c| c.sync.device_name(&id));
    let store = config.with_file_name("shelf.json");
    let previous = deckle_digest::shelf::store::Shelf::load(&store)
        .kindles
        .get(&id)
        .cloned();
    let (snap, errors) = deckle_shelf::snapshot(
        deckle_shelf::Read {
            books,
            skipped,
            clippings: clippings.as_ref().ok().cloned().flatten(),
            vocab,
            clippings_failed: clippings.is_err(),
            vocab_failed,
            previous,
        },
        &named.unwrap_or(device.name),
        &device.key,
        library,
        deckle_digest::chrono::Utc::now(),
    );
    for e in errors {
        eprintln!("shelf: {e}");
    }
    if let Err(e) = deckle_shelf::record(&store, &id, snap) {
        eprintln!("shelf: {e}");
    }
}

type ClippingsRead = Result<Option<Vec<u8>>, deckle_core::deliver::DeliverError>;

/// Scores the stored puzzles from the clippings the shelf read, when the paper has a puzzle and
/// the read worked. Errors are logged: they never fail the listing.
fn puzzle_on(clippings: &ClippingsRead, unreachable: bool, digest: &Path, store_path: &Path) {
    let on = deckle_digest::Config::load(digest)
        .is_ok_and(|c| c.sections.contains(&deckle_digest::SectionKind::Puzzle));
    let Ok(bytes) = clippings else {
        return;
    };
    if !on {
        return;
    }
    use deckle_digest::puzzle::check::Notes;
    let notes = match bytes {
        Some(b) => Notes::Read(b),
        None if unreachable => Notes::Unreachable,
        None => Notes::NoFile,
    };
    let mut store = deckle_digest::puzzle::store::Store::load(store_path);
    let today = deckle_digest::chrono::Local::now().date_naive();
    deckle_digest::puzzle::check::record(
        &mut store,
        notes,
        today,
        deckle_digest::puzzle::words::Words::embedded(),
    );
    if let Err(e) = store.save(store_path) {
        eprintln!("puzzle: {}: {e}", store_path.display());
    }
}

/// The clippings in the shelf's own session: never fails the listing.
fn highlights_from(clippings: &ClippingsRead, state: &Path) -> (Vec<Pending>, Option<String>) {
    let bytes = match clippings {
        Ok(Some(b)) => b,
        Ok(None) => return (Vec::new(), None),
        Err(e) => return (Vec::new(), Some(e.to_string())),
    };
    let parsed = deckle_highlights::parse(bytes);
    let state = State::load(state);
    let rows = deckle_highlights::pending(
        &deckle_highlights::group(parsed.entries),
        state.folder.as_deref(),
        &state,
    );
    let error = parsed
        .error
        .map(|e| format!("line {}: {}", e.line, e.message));
    (rows, error)
}

pub fn highlights_status_at(state: &Path, digest: &Path) -> HighlightsStatus {
    HighlightsStatus {
        folder: State::load(state)
            .folder
            .map(|f| f.to_string_lossy().into_owned()),
        clippings: deckle_digest::Config::load(digest).is_ok_and(|c| c.sync.clippings),
    }
}

/// The folder goes in `highlights.json`; `clippings` into `digest.toml`'s `[sync]`, keeping the
/// rest of the file as written.
pub fn highlights_set_at(
    state: &Path,
    digest: &Path,
    folder: Option<String>,
    clippings: bool,
) -> Result<HighlightsStatus, String> {
    let mut st = State::load(state);
    if let Some(folder) = folder {
        let folder = PathBuf::from(folder);
        std::fs::create_dir_all(&folder).map_err(|e| format!("{}: {e}", folder.display()))?;
        st.folder = Some(folder.canonicalize().unwrap_or(folder));
    }
    if let Some(dir) = state.parent() {
        std::fs::create_dir_all(dir).map_err(|e| e.to_string())?;
    }
    st.save(state)
        .map_err(|e| format!("{}: {e}", state.display()))?;
    let _lock = crate::config_write()?;
    let mut cfg = match digest.exists() {
        true => deckle_digest::Config::load(digest)?,
        false => deckle_digest::Config::parse(deckle_digest::config::STARTER)?,
    };
    if cfg.sync.clippings != clippings {
        cfg.sync.clippings = clippings;
        deckle_digest::config::save(digest, &cfg)?;
    }
    Ok(highlights_status_at(state, digest))
}

/// Reads the clippings over `conn` and writes what's new into the remembered folder.
pub fn write_highlights_on(conn: &mut Conn<'_>, state: &Path) -> Result<Outcome, String> {
    let bytes = read_on(conn)
        .map_err(|e| e.to_string())?
        .ok_or("No highlights on this Kindle yet.")?;
    deckle_highlights::export_clippings(&bytes, state, None).map_err(|e| {
        if State::load(state).folder.is_none() {
            "Choose a folder for your highlights first.".to_owned()
        } else {
            e
        }
    })
}

/// Before an upgrade replaces anything (Books and Kindle design §6): when there's a folder for
/// highlights, what's new goes there, over its own session. A failure is logged; the old copy's
/// `.sdr` is kept either way.
pub fn highlights_first(target: &Target, config: &Path) {
    let state = highlights_state(config);
    if State::load(&state).folder.is_none() {
        return;
    }
    if let Err(e) = write_highlights_on(&mut Conn::new(target), &state) {
        eprintln!("upgrade: highlights weren't saved first: {e}");
    }
}

/// Every English word looked up on the Kindle, as a CSV Anki imports, written over `to` (#135).
/// Reads `vocab.db` over `conn`, never writes to the Kindle, and defines words only from a Word
/// Wise database already on this machine. Returns how many words went out.
pub fn anki_on(conn: &mut Conn<'_>, to: &Path) -> Result<usize, String> {
    let files = deckle_core::vocab::read_vocab_on(conn)
        .map_err(|e| e.to_string())?
        .ok_or("This Kindle has no looked-up words: its Vocabulary Builder is empty or off.")?;
    let define = deckle_shelf::words::wordwise_definer();
    deckle_shelf::anki::export(&files, to, &*define)
}

/// Removes `file` from `target` and forgets the library's record of sending it there.
pub fn remove_from(conn: &mut Conn<'_>, config: &Path, file: &str) -> Result<Vec<String>, String> {
    let device = deckle_library::device_ref(conn.target);
    let removed = remove_on(conn, file).map_err(|e| e.to_string())?;
    if let Some((lib, _)) = Library::configured(config) {
        lib.forget_send(&device.key, file)?;
    }
    Ok(removed)
}

pub fn with_target<T: Send + 'static>(
    target: RunTarget,
    f: impl FnOnce(&mut Conn<'_>, &Path) -> Result<T, String> + Send + 'static,
) -> Result<T, String> {
    let target: Target = resolve_run_target(&target)?;
    let mut conn = Conn::new(&target);
    f(&mut conn, &deckle_library::config::default_path())
}

pub fn library_status() -> LibraryStatus {
    library_status_at(&deckle_library::config::default_path())
}

pub fn library_set(root: Option<String>, auto_add: bool) -> Result<LibraryStatus, String> {
    library_set_at(&deckle_library::config::default_path(), root, auto_add)
}

/// Lists over USB/MTP; an MTP listing takes a few seconds.
pub fn device_books(target: RunTarget) -> Result<DeviceShelf, String> {
    with_target(target, |conn, config| {
        let cache = deckle_core::wordwise::default_cache_dir().join("inventory");
        shelf_on(conn, config, Some(&cache))
    })
}

pub fn highlights_status() -> HighlightsStatus {
    let config = deckle_library::config::default_path();
    highlights_status_at(
        &highlights_state(&config),
        &deckle_digest::config::default_path(),
    )
}

pub fn highlights_set(folder: Option<String>, clippings: bool) -> Result<HighlightsStatus, String> {
    let config = deckle_library::config::default_path();
    highlights_set_at(
        &highlights_state(&config),
        &deckle_digest::config::default_path(),
        folder,
        clippings,
    )
}

/// Its own session, like Remove.
pub fn highlights_write(target: RunTarget) -> Result<Outcome, String> {
    with_target(target, |conn, config| {
        write_highlights_on(conn, &highlights_state(config))
    })
}

/// Its own session, like Remove.
pub fn words_anki(target: RunTarget, to: String) -> Result<usize, String> {
    with_target(target, move |conn, _| anki_on(conn, Path::new(&to)))
}

pub fn device_remove(target: RunTarget, file: String) -> Result<Vec<String>, String> {
    with_target(target, move |conn, config| remove_from(conn, config, &file))
}

/// Removes each of `files`, over one session (This Kindle's Make room). `(file, error)` for
/// each that couldn't go.
pub fn remove_many(conn: &mut Conn<'_>, config: &Path, files: &[String]) -> Vec<(String, String)> {
    files
        .iter()
        .filter_map(|f| remove_from(conn, config, f).err().map(|e| (f.clone(), e)))
        .collect()
}

/// Backs up every DRM-free book on the Kindle and its `.sdr` into a new folder under `into`
/// (default: `kindle-backups/` beside `library.json`).
pub fn backup_to(
    conn: &mut Conn<'_>,
    config: &Path,
    cache: Option<&Path>,
    into: Option<&Path>,
) -> Result<BackupReport, String> {
    let device = deckle_library::device_ref(conn.target);
    // Every book's details, so an AZW whose record 0 says DRM is left out over MTP too.
    let (inv, _) =
        deckle_core::inventory::list_detailed(conn, cache, deckle_digest::EDITION_PREFIX)
            .map_err(|e| e.to_string())?;
    let library = Library::configured(config).map(|(l, _)| l);
    let current = deckle_library::freshness::Current::now(&Default::default());
    let shelved = shelve(inv.books, library.as_ref(), &device.key, &current);
    let root = into.map_or_else(|| care::backups_root(config), Path::to_path_buf);
    let at = deckle_digest::chrono::Utc::now()
        .format("%Y-%m-%dT%H:%M:%SZ")
        .to_string();
    care::backup_on(
        conn,
        &shelved,
        &root,
        &deckle_shelf::friendly_name(&device.name),
        deckle_digest::EDITION_PREFIX,
        &at,
    )
}

/// Puts the backup in `dir` on the Kindle, leaving the books already there alone.
pub fn restore_from(conn: &mut Conn<'_>, dir: &Path) -> Result<RestoreReport, String> {
    let present: Vec<String> = deckle_core::inventory::list_on(conn, None)
        .map_err(|e| e.to_string())?
        .books
        .into_iter()
        .map(|b| b.file)
        .collect();
    care::restore_on(conn, dir, &present)
}

pub fn device_remove_many(
    target: RunTarget,
    files: Vec<String>,
) -> Result<Vec<(String, String)>, String> {
    with_target(target, move |conn, config| {
        Ok(remove_many(conn, config, &files))
    })
}

pub fn device_backup(target: RunTarget, into: Option<String>) -> Result<BackupReport, String> {
    with_target(target, move |conn, config| {
        let cache = deckle_core::wordwise::default_cache_dir().join("inventory");
        backup_to(conn, config, Some(&cache), into.as_deref().map(Path::new))
    })
}

pub fn device_restore(target: RunTarget, dir: String) -> Result<RestoreReport, String> {
    with_target(target, move |conn, _| restore_from(conn, Path::new(&dir)))
}

#[cfg(test)]
mod rooms_tests {
    use super::*;

    #[test]
    fn the_shelf_reads_an_azw3s_place_and_this_kindle_backs_it_up_and_restores_it() {
        let tmp = tempfile::tempdir().unwrap();
        let config = tmp.path().join("library.json");
        let kindle = tmp.path().join("Kindle");
        std::fs::create_dir_all(kindle.join("Side.sdr")).unwrap();
        std::fs::write(
            kindle.join("Side.azw3"),
            deckle_mobi::fixture::build(&deckle_mobi::fixture::Book {
                title: "Side",
                ..Default::default()
            }),
        )
        .unwrap();
        // 15% in, by the Kindle's own count.
        std::fs::copy(
            Path::new(env!("CARGO_MANIFEST_DIR")).join("../../fixtures/yjf/book.yjf"),
            kindle.join("Side.sdr/Side.azw3f"),
        )
        .unwrap();
        let target = Target::Folder(kindle);
        let shelf = shelf_on(&mut Conn::new(&target), &config, None).unwrap();
        let b = &shelf.books[0];
        assert_eq!((b.kind, b.reading), (Kind::Sideloaded, Reading::Reading));
        assert!(
            (b.fraction.unwrap() - 0.15).abs() < 0.01,
            "{:?}",
            b.fraction
        );
        assert!(shelf.spare.is_empty() && shelf.backups.is_empty());

        let done = backup_to(&mut Conn::new(&target), &config, None, None).unwrap();
        assert_eq!(done.copied, ["Side.azw3"]);
        let shelf = shelf_on(&mut Conn::new(&target), &config, None).unwrap();
        assert_eq!(shelf.backups.len(), 1);

        let other = Target::Folder(tmp.path().join("Other"));
        std::fs::create_dir_all(tmp.path().join("Other")).unwrap();
        let back = restore_from(&mut Conn::new(&other), &shelf.backups[0].dir).unwrap();
        assert_eq!(back.restored, ["Side.azw3"]);
        assert!(
            remove_many(
                &mut Conn::new(&other),
                &config,
                &["Side.azw3".into(), "Gone.azw3".into()]
            )
            .len()
                == 1
        );
        assert!(!tmp.path().join("Other/Side.azw3").exists());
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_folder_shelf_matches_the_library_and_remove_forgets_the_send() {
        let tmp = tempfile::tempdir().unwrap();
        let config = tmp.path().join("library.json");
        let status = library_set_at(
            &config,
            Some(tmp.path().join("Library").to_string_lossy().into_owned()),
            true,
        )
        .unwrap();
        assert!(status.root.is_some() && status.auto_add);
        let out = tmp.path().join("Kindle");
        std::fs::create_dir_all(&out).unwrap();
        let file = "Testwright, Jane - Deckle Mini Fixture.kfx";
        std::fs::write(
            out.join(file),
            std::fs::read(deckle_kfx::fixtures::path("deckle-mini-pdoc.golden.kfx")).unwrap(),
        )
        .unwrap();
        std::fs::write(out.join("zz.pdf"), b"%PDF").unwrap();
        let target = Target::Folder(out.clone());
        let lib = Library::new(tmp.path().join("Library"));
        let mut book = lib
            .add(&deckle_kfx::fixtures::path("deckle-mini.epub"))
            .unwrap()
            .book;
        lib.record_send(
            &mut book,
            deckle_library::SendRecord {
                device: deckle_library::device_ref(&target).key,
                file: file.into(),
                ..Default::default()
            },
        )
        .unwrap();
        assert_eq!(library_status_at(&config).books, 1);

        let mut conn = Conn::new(&target);
        let shelf = shelf_on(&mut conn, &config, None).unwrap();
        assert_eq!(shelf.books.len(), 2);
        let b = &shelf.books[0];
        assert_eq!(b.file, file, "Deckle's first");
        assert_eq!(b.cde.as_deref(), Some("PDOC"));
        assert!(b.cover.as_deref().unwrap().starts_with("data:image/jpeg"));
        assert_eq!(
            b.library_source.as_deref(),
            Some(book.source_path().to_string_lossy().as_ref())
        );
        assert!(shelf.books[1].library_source.is_none());

        assert_eq!(b.kind, Kind::Deckle);
        // The golden KFX carries no build stamp, so it reads as an older build.
        assert_eq!(b.class, Class::Older);
        assert_eq!(shelf.books[1].class, Class::Pdf);
        assert!(shelf.split.is_some());

        remove_from(&mut conn, &config, file).unwrap();
        assert!(!out.join(file).exists());
        assert!(lib.list()[0].sidecar.sends.is_empty());
        assert!(remove_from(&mut conn, &config, "../x.kfx").is_err());
    }
    #[test]
    fn anki_export_reads_the_kindle_and_writes_nothing_to_it() {
        let tmp = tempfile::tempdir().unwrap();
        let kindle = tmp.path().join("Kindle");
        let vocab = kindle.join("system/vocabulary");
        std::fs::create_dir_all(&vocab).unwrap();
        let c = rusqlite::Connection::open(vocab.join("vocab.db")).unwrap();
        c.execute_batch(
            "CREATE TABLE WORDS (id TEXT PRIMARY KEY, word TEXT, stem TEXT, lang TEXT, category INTEGER, timestamp INTEGER, profileid TEXT);
             CREATE TABLE LOOKUPS (id TEXT PRIMARY KEY, word_key TEXT, book_key TEXT, dict_key TEXT, pos TEXT, usage TEXT, timestamp INTEGER);
             CREATE TABLE BOOK_INFO (id TEXT PRIMARY KEY, asin TEXT, guid TEXT, lang TEXT, title TEXT, authors TEXT);
             INSERT INTO BOOK_INFO VALUES ('b1','B0','g','en','Walden','Thoreau');
             INSERT INTO WORDS VALUES ('w1','sedulous','sedulous','en',0,1761000000000,'p');
             INSERT INTO LOOKUPS VALUES ('l1','w1','b1','d','n','a sedulous reader',1761000000000);",
        )
        .unwrap();
        drop(c);
        let before = std::fs::read(vocab.join("vocab.db")).unwrap();
        let target = Target::MassStorage(deckle_core::deliver::Device {
            root: kindle.clone(),
            model_hint: "Kindle".into(),
            thumbnails_writable: false,
            firmware: None,
            serial: None,
        });
        let out = tmp.path().join("words.csv");
        let n = anki_on(&mut Conn::new(&target), &out).unwrap();
        assert_eq!(n, 1);
        let csv = std::fs::read_to_string(&out).unwrap();
        assert!(csv.contains("\"sedulous\""), "{csv}");
        assert_eq!(
            std::fs::read(vocab.join("vocab.db")).unwrap(),
            before,
            "the Kindle's copy is untouched"
        );

        let empty = tmp.path().join("NoVocab");
        std::fs::create_dir_all(&empty).unwrap();
        let none = Target::MassStorage(deckle_core::deliver::Device {
            root: empty,
            model_hint: "Kindle".into(),
            thumbnails_writable: false,
            firmware: None,
            serial: None,
        });
        let e = anki_on(&mut Conn::new(&none), &out).unwrap_err();
        assert!(e.contains("no looked-up words"), "{e}");
    }

    fn clippings_fixture() -> Vec<u8> {
        std::fs::read(
            Path::new(env!("CARGO_MANIFEST_DIR")).join("../../fixtures/clippings/variants.txt"),
        )
        .unwrap()
    }

    #[test]
    fn a_shelf_scores_the_puzzle_when_the_paper_has_one() {
        use deckle_digest::puzzle::store::{Day, Store};
        let tmp = tempfile::tempdir().unwrap();
        let config = tmp.path().join("library.json");
        let store_path = tmp.path().join("puzzles.json");
        let out = tmp.path().join("Kindle");
        std::fs::create_dir_all(&out).unwrap();
        let target = Target::Folder(out.clone());
        let yesterday =
            deckle_digest::chrono::Local::now().date_naive() - deckle_digest::chrono::Days::new(1);
        let mut store = Store::default();
        let puzzle = deckle_digest::puzzle::make(
            yesterday,
            &Default::default(),
            deckle_digest::puzzle::words::Words::embedded(),
            &store,
        );
        let c = puzzle.clues[0].clone();
        store.days.insert(
            yesterday,
            Day {
                puzzle,
                headlines: Vec::new(),
                solve: None,
            },
        );
        store.save(&store_path).unwrap();
        let title = format!(
            "The Deckle Daily — {} (Deckle)",
            deckle_digest::long_date(yesterday)
        );
        std::fs::write(
            out.join("My Clippings.txt"),
            format!(
                "{title}\r\n- Your Highlight on Location 10-11 | Added on Monday, October 19, \
                 2026 8:00:00 AM\r\n\r\n{} {}\r\n==========\r\n{title}\r\n- Your Note on \
                 Location 11 | Added on Monday, October 19, 2026 8:01:00 AM\r\n\r\n{}\r\n\
                 ==========\r\n",
                c.number, c.clue, c.answer
            ),
        )
        .unwrap();

        // no puzzle in the paper: left alone
        std::fs::write(tmp.path().join("digest.toml"), "sections = [\"weather\"]\n").unwrap();
        shelf_on(&mut Conn::new(&target), &config, None).unwrap();
        assert_eq!(Store::load(&store_path), store);

        std::fs::write(tmp.path().join("digest.toml"), "sections = [\"puzzle\"]\n").unwrap();
        shelf_on(&mut Conn::new(&target), &config, None).unwrap();
        let after = Store::load(&store_path);
        assert_eq!(
            after.days[&yesterday].solve.as_ref().map(|s| s.solved),
            Some(1)
        );
        assert_eq!(after.notes_readable, Some(true));

        // a Kindle with no file yet (no highlights) keeps the hint
        std::fs::remove_file(out.join("My Clippings.txt")).unwrap();
        shelf_on(&mut Conn::new(&target), &config, None).unwrap();
        assert_eq!(Store::load(&store_path).notes_readable, Some(true));
    }

    #[test]
    fn a_shelf_carries_the_kindles_highlights() {
        let tmp = tempfile::tempdir().unwrap();
        let config = tmp.path().join("library.json");
        let out = tmp.path().join("Kindle");
        std::fs::create_dir_all(&out).unwrap();
        let target = Target::Folder(out.clone());

        let shelf = shelf_on(&mut Conn::new(&target), &config, None).unwrap();
        assert!(shelf.highlights.is_empty());
        assert!(shelf.highlights_error.is_none());

        std::fs::write(out.join("My Clippings.txt"), clippings_fixture()).unwrap();
        let shelf = shelf_on(&mut Conn::new(&target), &config, None).unwrap();
        let walden = shelf
            .highlights
            .iter()
            .find(|h| h.title == "Walden")
            .unwrap();
        assert_eq!((walden.total, walden.new), (3, 3));
        assert!(walden.markdown.starts_with("---\ntitle: Walden\n"));
        assert!(shelf.highlights_error.is_none());

        let mut bytes = clippings_fixture();
        bytes.extend_from_slice(b"Broken (X)\r\nnot a meta line\r\n==========\r\n");
        std::fs::write(out.join("My Clippings.txt"), bytes).unwrap();
        let shelf = shelf_on(&mut Conn::new(&target), &config, None).unwrap();
        assert_eq!(shelf.highlights.len(), 6);
        assert!(
            shelf
                .highlights_error
                .as_deref()
                .unwrap()
                .starts_with("line 54: ")
        );
    }

    #[test]
    fn a_shelf_records_the_kindles_shelf_when_the_paper_has_one() {
        use deckle_digest::shelf::store::Shelf;
        let tmp = tempfile::tempdir().unwrap();
        let config = tmp.path().join("library.json");
        let out = tmp.path().join("Kindle");
        std::fs::create_dir_all(out.join("Walden.sdr")).unwrap();
        std::fs::write(out.join("Walden.kfx"), b"not a real kfx").unwrap();
        std::fs::write(
            out.join("Walden.sdr/Walden9d6e.yjf"),
            std::fs::read(
                Path::new(env!("CARGO_MANIFEST_DIR")).join("../../fixtures/yjf/book.yjf"),
            )
            .unwrap(),
        )
        .unwrap();
        std::fs::write(out.join("My Clippings.txt"), clippings_fixture()).unwrap();
        let target = Target::Folder(out.clone());
        let store = tmp.path().join("shelf.json");

        // no shelf in the paper: nothing recorded
        std::fs::write(tmp.path().join("digest.toml"), "sections = [\"weather\"]\n").unwrap();
        shelf_on(&mut Conn::new(&target), &config, None).unwrap();
        assert!(!store.exists());

        std::fs::write(tmp.path().join("digest.toml"), "sections = [\"shelf\"]\n").unwrap();
        let listed = shelf_on(&mut Conn::new(&target), &config, None).unwrap();
        assert!(
            listed.books.iter().any(|b| b.file == "Walden.kfx"),
            "the tab still lists the books"
        );
        let snap = Shelf::load(&store);
        let k = snap.kindles.values().next().expect("a snapshot");
        assert_eq!(k.books.len(), 1);
        assert_eq!(k.books[0].title, "Walden");
        assert!(k.highlights_shared);

        // left out for this Kindle: not read again
        std::fs::remove_file(&store).unwrap();
        let id = deckle_core::sync::device_id(&target);
        std::fs::write(
            tmp.path().join("digest.toml"),
            format!("sections = [\"shelf\"]\n\n[sync.devices.'{id}']\nshelf = false\n"),
        )
        .unwrap();
        shelf_on(&mut Conn::new(&target), &config, None).unwrap();
        assert!(!store.exists());
    }

    #[test]
    fn highlights_settings_write_the_folder_and_the_sync_key() {
        let tmp = tempfile::tempdir().unwrap();
        let state = tmp.path().join("highlights.json");
        let digest = tmp.path().join("digest.toml");
        assert_eq!(
            highlights_status_at(&state, &digest),
            HighlightsStatus {
                folder: None,
                clippings: false
            }
        );
        let notes = tmp.path().join("notes");
        let s = highlights_set_at(
            &state,
            &digest,
            Some(notes.to_string_lossy().into_owned()),
            true,
        )
        .unwrap();
        assert!(s.folder.as_deref().unwrap().ends_with("notes"));
        assert!(s.clippings);
        let text = std::fs::read_to_string(&digest).unwrap();
        assert!(text.contains("clippings = true"), "{text}");
        assert!(
            text.contains("# What `deckle digest watch` does"),
            "comments kept"
        );
        let s = highlights_set_at(
            &state,
            &digest,
            Some(notes.to_string_lossy().into_owned()),
            false,
        )
        .unwrap();
        assert!(!s.clippings);
    }

    #[test]
    fn writing_highlights_needs_a_folder_then_exports() {
        let tmp = tempfile::tempdir().unwrap();
        let state = tmp.path().join("highlights.json");
        let out = tmp.path().join("Kindle");
        std::fs::create_dir_all(&out).unwrap();
        std::fs::write(out.join("My Clippings.txt"), clippings_fixture()).unwrap();
        let target = Target::Folder(out);
        assert!(write_highlights_on(&mut Conn::new(&target), &state).is_err());
        let notes = tmp.path().join("notes");
        let digest = tmp.path().join("digest.toml");
        highlights_set_at(
            &state,
            &digest,
            Some(notes.to_string_lossy().into_owned()),
            false,
        )
        .unwrap();
        let o = write_highlights_on(&mut Conn::new(&target), &state).unwrap();
        assert_eq!(o.report.written.len(), 6);
        assert!(notes.join("Walden.md").exists());
    }
}
