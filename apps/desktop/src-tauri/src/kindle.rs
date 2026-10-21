// SPDX-License-Identifier: GPL-3.0-or-later
//! The Kindle tab and the library settings (library design §5): what's on a Kindle, matched
//! to the library, and removing a book. Pure logic is in plain functions for the tests.

use std::path::{Path, PathBuf};

use base64::Engine;
use deckle_core::deliver::{Conn, Target};
use deckle_core::inventory::{Space, list_on, remove_on};
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
    Ok(DeviceShelf {
        device: device.name,
        space: inv.space,
        books,
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
}
