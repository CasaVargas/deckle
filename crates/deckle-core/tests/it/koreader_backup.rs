// SPDX-License-Identifier: GPL-3.0-or-later
//! KOReader backup and restore (docs/superpowers/specs/2026-10-30-koreader-hub-design.md §D).
use std::path::Path;
use std::time::{Duration, SystemTime};

use deckle_core::koreader::partial_md5;
use deckle_core::koreader_backup::{Options, Outcome, backup, load, restore};

fn write(path: &Path, bytes: impl AsRef<[u8]>) {
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(path, bytes).unwrap();
}

fn age(path: &Path, secs: u64) {
    let f = std::fs::File::options().write(true).open(path).unwrap();
    f.set_modified(SystemTime::now() - Duration::from_secs(secs))
        .unwrap();
}

fn sidecar(md5: &str, percent: f64) -> String {
    format!(
        "-- ./Walden.sdr/metadata.epub.lua\nreturn {{\n    [\"doc_props\"] = {{ [\"title\"] = \"Walden\" }},\n    [\"partial_md5_checksum\"] = \"{md5}\",\n    [\"percent_finished\"] = {percent},\n}}\n"
    )
}

/// A Kobo running KOReader: Walden with a sidecar beside it, Emma with its sidecar in
/// `docsettings/`, and KOReader's settings. Returns Walden's partial MD5.
fn kobo(root: &Path) -> String {
    let walden: Vec<u8> = (0..9000u32).map(|i| (i % 253) as u8).collect();
    write(&root.join("Books/Walden.epub"), &walden);
    let md5 = partial_md5(&root.join("Books/Walden.epub")).unwrap();
    let meta = root.join("Books/Walden.sdr/metadata.epub.lua");
    write(&meta, sidecar(&md5, 0.42));
    age(&meta, 3600);
    write(&root.join("Books/Walden.sdr/metadata.epub.lua.old"), "old");
    write(&root.join("Books/Emma.pdf"), b"%PDF emma");
    write(
        &root.join(".adds/koreader/docsettings/mnt/onboard/Books/Emma.sdr/metadata.pdf.lua"),
        "return { [\"percent_finished\"] = 0.1 }",
    );
    write(
        &root.join(".adds/koreader/settings.reader.lua"),
        "return {}",
    );
    write(&root.join(".adds/koreader/history.lua"), "return {}");
    write(
        &root.join(".adds/koreader/settings/statistics.sqlite3"),
        b"SQLite stats",
    );
    write(&root.join(".adds/koreader/cache/big.bin"), b"not backed up");
    md5
}

fn settings(on: bool, force: bool) -> Options {
    Options {
        settings: on,
        force,
    }
}

#[test]
fn a_kobos_reading_life_moves_to_a_pocketbook() {
    let tmp = tempfile::tempdir().unwrap();
    let (src, dst) = (tmp.path().join("KOBOeReader"), tmp.path().join("PB632"));
    let md5 = kobo(&src);
    let dir = tmp.path().join("backups/Kobo/2026-10-30 101500");
    let done = backup(&src, &dir, "Kobo", "2026-10-30T10:15:00Z").unwrap();
    assert!(done.failed.is_empty(), "{:?}", done.failed);

    let m = load(&dir).unwrap();
    assert_eq!(m.device, "Kobo");
    assert_eq!(m.koreader_dir.as_deref(), Some(".adds/koreader"));
    let paths: Vec<&str> = m.files.iter().map(|f| f.path.as_str()).collect();
    assert_eq!(
        paths,
        [
            ".adds/koreader/docsettings/mnt/onboard/Books/Emma.sdr/metadata.pdf.lua",
            ".adds/koreader/history.lua",
            ".adds/koreader/settings.reader.lua",
            ".adds/koreader/settings/statistics.sqlite3",
            "Books/Walden.sdr/metadata.epub.lua",
            "Books/Walden.sdr/metadata.epub.lua.old",
        ]
    );
    let walden = m
        .sidecars
        .iter()
        .find(|s| s.book == "Books/Walden.epub")
        .unwrap();
    assert_eq!(walden.partial_md5.as_deref(), Some(md5.as_str()));
    // Emma's sidecar has no checksum: it's worked out from the book.
    let emma = m
        .sidecars
        .iter()
        .find(|s| s.book == "Books/Emma.pdf")
        .unwrap();
    assert_eq!(emma.partial_md5, partial_md5(&src.join("Books/Emma.pdf")));
    assert!(dir.join("Books/Walden.sdr/metadata.epub.lua").is_file());

    // The PocketBook keeps Walden in another folder, has no Emma, and runs KOReader too.
    std::fs::create_dir_all(dst.join("applications/koreader")).unwrap();
    std::fs::create_dir_all(dst.join("Library/Classics")).unwrap();
    std::fs::copy(
        src.join("Books/Walden.epub"),
        dst.join("Library/Classics/Walden.epub"),
    )
    .unwrap();
    let r = restore(&dir, &dst, Options::default()).unwrap();
    let w = r
        .sidecars
        .iter()
        .find(|i| i.book == "Books/Walden.epub")
        .unwrap();
    assert_eq!(w.outcome, Outcome::Restored);
    assert_eq!(w.by, Some("md5"));
    assert_eq!(w.to.as_deref(), Some("Library/Classics/Walden.epub"));
    let put = dst.join("Library/Classics/Walden.sdr/metadata.epub.lua");
    assert_eq!(std::fs::read_to_string(&put).unwrap(), sidecar(&md5, 0.42));
    assert!(
        dst.join("Library/Classics/Walden.sdr/metadata.epub.lua.old")
            .is_file()
    );
    // The backup's time comes back with it.
    let secs = |p: &Path| {
        std::fs::metadata(p)
            .unwrap()
            .modified()
            .unwrap()
            .duration_since(SystemTime::UNIX_EPOCH)
            .unwrap()
            .as_secs()
    };
    assert_eq!(
        secs(&put),
        secs(&src.join("Books/Walden.sdr/metadata.epub.lua"))
    );
    let e = r
        .sidecars
        .iter()
        .find(|i| i.book == "Books/Emma.pdf")
        .unwrap();
    assert_eq!(e.outcome, Outcome::NoBook);
    // Settings stay put unless asked for, and no temporary file is left behind.
    assert!(r.settings.is_empty());
    assert!(
        !dst.join("applications/koreader/settings.reader.lua")
            .exists()
    );
    let hidden = std::fs::read_dir(dst.join("Library/Classics/Walden.sdr"))
        .unwrap()
        .flatten()
        .filter(|e| e.file_name().to_string_lossy().starts_with('.'))
        .count();
    assert_eq!(hidden, 0);
}

#[test]
fn a_newer_sidecar_on_the_target_is_kept_unless_forced() {
    let tmp = tempfile::tempdir().unwrap();
    let src = tmp.path().join("KOBOeReader");
    kobo(&src);
    let dir = tmp.path().join("backup");
    backup(&src, &dir, "Kobo", "2026-10-30T10:15:00Z").unwrap();
    // The same drive, read further since: its sidecar is newer than the backup's.
    let mine = src.join("Books/Walden.sdr/metadata.epub.lua");
    std::fs::write(&mine, sidecar("x", 0.9)).unwrap();
    let r = restore(&dir, &src, settings(false, false)).unwrap();
    let w = r
        .sidecars
        .iter()
        .find(|i| i.book == "Books/Walden.epub")
        .unwrap();
    assert_eq!((w.outcome, w.by), (Outcome::Kept, Some("path")));
    assert!(std::fs::read_to_string(&mine).unwrap().contains("0.9"));

    let r = restore(&dir, &src, settings(false, true)).unwrap();
    let w = r
        .sidecars
        .iter()
        .find(|i| i.book == "Books/Walden.epub")
        .unwrap();
    assert_eq!(w.outcome, Outcome::Restored);
    assert!(std::fs::read_to_string(&mine).unwrap().contains("0.42"));
}

#[test]
fn settings_go_back_only_when_asked() {
    let tmp = tempfile::tempdir().unwrap();
    let (src, dst) = (tmp.path().join("KOBOeReader"), tmp.path().join("Kindle"));
    kobo(&src);
    let dir = tmp.path().join("backup");
    backup(&src, &dir, "Kobo", "2026-10-30T10:15:00Z").unwrap();
    std::fs::create_dir_all(dst.join("koreader")).unwrap();
    let r = restore(&dir, &dst, settings(true, false)).unwrap();
    let to: Vec<_> = r
        .settings
        .iter()
        .map(|i| (i.to.as_deref().unwrap(), i.outcome))
        .collect();
    assert_eq!(
        to,
        [
            ("koreader/settings.reader.lua", Outcome::Restored),
            ("koreader/history.lua", Outcome::Restored),
            ("koreader/settings/statistics.sqlite3", Outcome::Restored),
        ]
    );
    assert_eq!(
        std::fs::read(dst.join("koreader/settings/statistics.sqlite3")).unwrap(),
        b"SQLite stats"
    );

    // A target with no KOReader folder: each is reported, and nothing is written.
    let bare = tmp.path().join("bare");
    std::fs::create_dir_all(&bare).unwrap();
    let r = restore(&dir, &bare, settings(true, false)).unwrap();
    assert_eq!(r.settings.len(), 3);
    assert!(r.settings.iter().all(|i| i.outcome == Outcome::Failed));
    assert_eq!(std::fs::read_dir(&bare).unwrap().count(), 0);
}

#[test]
fn a_backup_never_lands_on_an_existing_folder() {
    let tmp = tempfile::tempdir().unwrap();
    let src = tmp.path().join("src");
    kobo(&src);
    assert!(backup(&src, tmp.path(), "Kobo", "now").is_err());
}
