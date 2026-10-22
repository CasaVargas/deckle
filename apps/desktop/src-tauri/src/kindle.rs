// SPDX-License-Identifier: GPL-3.0-or-later
//! The Kindle tab and the library settings (library design §5): what's on a Kindle, matched
//! to the library, and removing a book. Pure logic is in plain functions for the tests.

use std::path::{Path, PathBuf};

use base64::Engine;
use deckle_core::clippings::read_on;
use deckle_core::deliver::{Conn, Target};
use deckle_core::inventory::{Space, list_on, remove_on};
use deckle_highlights::{Outcome, Pending, State};
use deckle_library::freshness::Reason;
use deckle_library::shelf::{Shelved, shelve, source_cover};
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

fn data_url(bytes: &[u8]) -> String {
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

fn row(s: Shelved) -> DeviceBookRow {
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
    }
}

/// What's on `target`, matched to the library at `config`.
pub fn shelf_on(
    conn: &mut Conn<'_>,
    config: &Path,
    cache: Option<&Path>,
) -> Result<DeviceShelf, String> {
    let device = deckle_library::device_ref(conn.target);
    let inv = list_on(conn, cache).map_err(|e| e.to_string())?;
    let library = Library::configured(config).map(|(l, _)| l);
    let current = deckle_library::freshness::Current::now(&Default::default());
    let mut books: Vec<DeviceBookRow> = shelve(inv.books, library.as_ref(), &device.key, &current)
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
        &config.with_file_name("digest.toml"),
        &config.with_file_name("puzzles.json"),
    );
    // Reading positions ride in the same session (readership design §3).
    crate::readership::record_on(
        conn,
        &config.with_file_name("readership.json"),
        &config.with_file_name("digest.toml"),
    );
    Ok(DeviceShelf {
        device: device.name,
        space: inv.space,
        books,
        highlights,
        highlights_error,
    })
}

type ClippingsRead = Result<Option<Vec<u8>>, deckle_core::deliver::DeliverError>;

/// Scores the stored puzzles from the clippings the shelf read, when the paper has a puzzle and
/// the read worked. Errors are logged: they never fail the listing.
fn puzzle_on(clippings: &ClippingsRead, digest: &Path, store_path: &Path) {
    let on = deckle_digest::Config::load(digest)
        .is_ok_and(|c| c.sections.contains(&deckle_digest::SectionKind::Puzzle));
    let Ok(bytes) = clippings else {
        return;
    };
    if !on {
        return;
    }
    let mut store = deckle_digest::puzzle::store::Store::load(store_path);
    let today = deckle_digest::chrono::Local::now().date_naive();
    deckle_digest::puzzle::check::record(&mut store, bytes.as_deref(), today);
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

/// Removes `file` from `target` and forgets the library's record of sending it there.
pub fn remove_from(conn: &mut Conn<'_>, config: &Path, file: &str) -> Result<Vec<String>, String> {
    let device = deckle_library::device_ref(conn.target);
    let removed = remove_on(conn, file).map_err(|e| e.to_string())?;
    if let Some((lib, _)) = Library::configured(config) {
        lib.forget_send(&device.key, file)?;
    }
    Ok(removed)
}

fn with_target<T: Send + 'static>(
    target: RunTarget,
    f: impl FnOnce(&mut Conn<'_>, &Path) -> Result<T, String> + Send + 'static,
) -> Result<T, String> {
    let target: Target = resolve_run_target(&target)?;
    let mut conn = Conn::new(&target);
    f(&mut conn, &deckle_library::config::default_path())
}

#[tauri::command]
pub fn library_status() -> LibraryStatus {
    library_status_at(&deckle_library::config::default_path())
}

#[tauri::command]
pub fn library_set(root: Option<String>, auto_add: bool) -> Result<LibraryStatus, String> {
    library_set_at(&deckle_library::config::default_path(), root, auto_add)
}

/// Lists over USB/MTP off the main thread (an MTP listing takes a few seconds).
#[tauri::command]
pub async fn device_books(target: RunTarget) -> Result<DeviceShelf, String> {
    tauri::async_runtime::spawn_blocking(move || {
        with_target(target, |conn, config| {
            let cache = deckle_core::wordwise::default_cache_dir().join("inventory");
            shelf_on(conn, config, Some(&cache))
        })
    })
    .await
    .map_err(|e| e.to_string())?
}

#[tauri::command]
pub fn highlights_status() -> HighlightsStatus {
    let config = deckle_library::config::default_path();
    highlights_status_at(
        &highlights_state(&config),
        &deckle_digest::config::default_path(),
    )
}

#[tauri::command]
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
#[tauri::command]
pub async fn highlights_write(target: RunTarget) -> Result<Outcome, String> {
    tauri::async_runtime::spawn_blocking(move || {
        with_target(target, |conn, config| {
            write_highlights_on(conn, &highlights_state(config))
        })
    })
    .await
    .map_err(|e| e.to_string())?
}

#[tauri::command]
pub async fn device_remove(target: RunTarget, file: String) -> Result<Vec<String>, String> {
    tauri::async_runtime::spawn_blocking(move || {
        with_target(target, move |conn, config| remove_from(conn, config, &file))
    })
    .await
    .map_err(|e| e.to_string())?
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

        remove_from(&mut conn, &config, file).unwrap();
        assert!(!out.join(file).exists());
        assert!(lib.list()[0].sidecar.sends.is_empty());
        assert!(remove_from(&mut conn, &config, "../x.kfx").is_err());
    }
    fn clippings_fixture() -> Vec<u8> {
        std::fs::read(
            Path::new(env!("CARGO_MANIFEST_DIR")).join("../../../fixtures/clippings/variants.txt"),
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

        std::fs::remove_file(out.join("My Clippings.txt")).unwrap();
        shelf_on(&mut Conn::new(&target), &config, None).unwrap();
        assert_eq!(Store::load(&store_path).notes_readable, Some(false));
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
