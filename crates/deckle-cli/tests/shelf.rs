// SPDX-License-Identifier: GPL-3.0-or-later
//! The shelf snapshot taken on connect (shelf design §6), against a folder standing in for a
//! Kindle's `documents/`, with Deckle's config folder in a temp dir.

use std::path::{Path, PathBuf};

use assert_cmd::Command;
use deckle_digest::shelf::store::Shelf;

fn deckle(home: &Path) -> Command {
    let mut c = Command::cargo_bin("deckle").unwrap();
    c.env("HOME", home)
        .env("XDG_CONFIG_HOME", home)
        .env("APPDATA", home)
        .env("LOCALAPPDATA", home);
    c
}

fn config_dir(home: &Path) -> PathBuf {
    if cfg!(target_os = "macos") {
        home.join("Library/Application Support/deckle")
    } else {
        home.join("deckle")
    }
}

fn fixture(rel: &str) -> Vec<u8> {
    std::fs::read(
        Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../fixtures")
            .join(rel),
    )
    .unwrap()
}

/// `digest` as digest.toml, and a Kindle folder with one book (with its position) and a
/// `My Clippings.txt`.
fn setup(digest: &str) -> (tempfile::TempDir, PathBuf) {
    let home = tempfile::tempdir().unwrap();
    let dir = config_dir(home.path());
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(dir.join("digest.toml"), digest).unwrap();
    let k = home.path().join("kindle");
    std::fs::create_dir_all(k.join("Walden.sdr")).unwrap();
    std::fs::write(k.join("Walden.kfx"), b"not a real kfx").unwrap();
    std::fs::write(k.join("Walden.sdr/Walden9d6e.yjf"), fixture("yjf/book.yjf")).unwrap();
    std::fs::write(
        k.join("My Clippings.txt"),
        fixture("clippings/variants.txt"),
    )
    .unwrap();
    (home, k)
}

fn shelf_json(home: &Path) -> PathBuf {
    config_dir(home).join("shelf.json")
}

#[test]
fn push_records_the_shelf() {
    let (home, k) = setup("sections = [\"shelf\"]\n\n[sync]\nclippings = false\n");
    let out = deckle(home.path())
        .args(["digest", "push", "--to"])
        .arg(&k)
        .assert()
        .success();
    let text = String::from_utf8(out.get_output().stdout.clone()).unwrap();
    assert!(text.contains("Shelf: "), "{text}");
    let shelf = Shelf::load(&shelf_json(home.path()));
    assert_eq!(shelf.kindles.len(), 1);
    let snap = shelf.kindles.values().next().unwrap();
    assert_eq!(snap.books.len(), 1, "{snap:?}");
    assert_eq!(snap.books[0].title, "Walden");
    assert_eq!(snap.books[0].last, Some(54797));
    assert!(snap.highlights_shared && !snap.highlights.is_empty());
    // clippings are off: the shelf read them, but no highlights were exported
    assert!(!config_dir(home.path()).join("highlights.json").exists());
}

#[test]
fn shelf_off_for_device_reads_nothing() {
    let (home, k) = setup("");
    let digest = format!(
        "sections = [\"shelf\"]\n\n[sync.devices.\"{}\"]\nshelf = false\n",
        k.display()
    );
    std::fs::write(config_dir(home.path()).join("digest.toml"), digest).unwrap();
    deckle(home.path())
        .args(["digest", "push", "--to"])
        .arg(&k)
        .assert()
        .success();
    assert!(!shelf_json(home.path()).exists());
}

#[test]
fn shelf_not_in_sections_reads_nothing() {
    let (home, k) = setup("sections = [\"weather\"]\n");
    deckle(home.path())
        .args(["digest", "push", "--to"])
        .arg(&k)
        .assert()
        .success();
    assert!(!shelf_json(home.path()).exists());
}

#[test]
fn a_second_kindle_keeps_the_first_ones_snapshot() {
    let (home, k) = setup("sections = [\"shelf\"]\n");
    deckle(home.path())
        .args(["digest", "push", "--to"])
        .arg(&k)
        .assert()
        .success();
    // Another Kindle with no notes: the first one's highlights stay.
    let k2 = home.path().join("kindle2");
    std::fs::create_dir_all(&k2).unwrap();
    deckle(home.path())
        .args(["digest", "push", "--to"])
        .arg(&k2)
        .assert()
        .success();
    let shelf = Shelf::load(&shelf_json(home.path()));
    assert_eq!(shelf.kindles.len(), 2);
    assert!(shelf.kindles.values().any(|k| !k.highlights.is_empty()));
}
