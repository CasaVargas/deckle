// SPDX-License-Identifier: GPL-3.0-or-later
//! From Your Shelf: its settings and `shelf.json` (shelf design §4).

use chrono::{Days, NaiveDate};
use deckle_digest::config::{Config, STARTER, SectionKind, section_key};
use deckle_digest::shelf::store::{BookSnap, KindleSnapshot, Shelf};

fn today() -> NaiveDate {
    NaiveDate::from_ymd_opt(2026, 10, 22).unwrap()
}

fn snap(name: &str, seen: NaiveDate) -> KindleSnapshot {
    KindleSnapshot {
        name: name.into(),
        seen: seen.format("%Y-%m-%d").to_string(),
        books: vec![BookSnap {
            file: "Book.kfx".into(),
            title: "Book".into(),
            wpm: vec![300.0],
            ..Default::default()
        }],
        ..Default::default()
    }
}

#[test]
fn starter_has_shelf_between_feeds_and_lottery() {
    let cfg = Config::parse(STARTER).unwrap();
    let at = |k| cfg.sections.iter().position(|s| *s == k).unwrap();
    assert_eq!(at(SectionKind::Shelf), at(SectionKind::Feeds) + 1);
    assert_eq!(at(SectionKind::Shelf) + 1, at(SectionKind::Lottery));
}

#[test]
fn default_sections_too() {
    let cfg = Config::default();
    let at = |k| cfg.sections.iter().position(|s| *s == k).unwrap();
    assert_eq!(at(SectionKind::Shelf), at(SectionKind::Feeds) + 1);
    assert_eq!(at(SectionKind::Shelf) + 1, at(SectionKind::Lottery));
}

#[test]
fn untouched_config_saves_no_shelf_table() {
    let text = toml::to_string(&Config::default()).unwrap();
    assert!(!text.contains("[shelf]"), "{text}");
    let cfg = Config::parse("[shelf]\nhide = [\"Draft\"]\n").unwrap();
    assert!(cfg.shelf.nightstand && cfg.shelf.highlights && cfg.shelf.words);
    assert_eq!(cfg.shelf.hide, ["Draft"]);
    assert!(toml::to_string(&cfg).unwrap().contains("[shelf]"));
}

#[test]
fn sync_shelf_defaults_on_and_follows_devices() {
    let c = Config::parse("[sync.devices.\"G0\"]\nshelf = false\n").unwrap();
    assert!(c.sync.shelf);
    assert!(!c.sync.for_device("G0").shelf);
    assert!(c.sync.for_device("other").shelf);
    assert!(
        STARTER.contains("shelf = true"),
        "the starter documents the switch"
    );
}

#[test]
fn the_shelf_section_key() {
    assert_eq!(section_key(SectionKind::Shelf, None), "shelf");
}

#[test]
fn the_shelf_store_sits_beside_digest_toml() {
    let mut cfg = Config::default();
    assert_eq!(cfg.shelf_store().file_name().unwrap(), "shelf.json");
    cfg.shelf_path = Some("/tmp/x/s.json".into());
    assert_eq!(cfg.shelf_store(), std::path::Path::new("/tmp/x/s.json"));
}

#[test]
fn round_trip_and_corrupt_file() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("shelf.json");
    let mut shelf = Shelf::default();
    shelf.put("G0", snap("Scribe", today()));
    shelf.shown.insert("h1".into(), today());
    shelf.save(&path).unwrap();
    assert_eq!(Shelf::load(&path), shelf);
    std::fs::write(&path, b"\x00not json").unwrap();
    assert_eq!(Shelf::load(&path), Shelf::default());
    assert_eq!(
        Shelf::load(&dir.path().join("missing.json")),
        Shelf::default()
    );
}

#[test]
fn forget_removes_the_file_and_a_missing_one_is_fine() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("shelf.json");
    Shelf::default().save(&path).unwrap();
    Shelf::forget(&path).unwrap();
    assert!(!path.exists());
    Shelf::forget(&path).unwrap();
}

#[test]
fn put_replaces_one_kindle_only() {
    let mut shelf = Shelf::default();
    shelf.put("A", snap("Paperwhite", today()));
    shelf.put("B", snap("Scribe", today()));
    let b = shelf.kindles["B"].clone();
    let mut again = snap("Paperwhite", today());
    again.books.clear();
    shelf.put("A", again);
    assert!(shelf.kindles["A"].books.is_empty());
    assert_eq!(shelf.kindles["B"], b);
}

#[test]
fn fresh_ignores_old_snapshots() {
    let mut shelf = Shelf::default();
    shelf.put("old", snap("Old", today() - Days::new(31)));
    shelf.put("edge", snap("Edge", today() - Days::new(30)));
    shelf.put("new", snap("New", today() - Days::new(1)));
    let names: Vec<&str> = shelf
        .fresh(today())
        .into_iter()
        .map(|(_, k)| k.name.as_str())
        .collect();
    assert_eq!(names, ["New", "Edge"], "newest first, 31 days is out");
}

#[test]
fn prune_shown_keeps_ninety_days() {
    let mut shelf = Shelf::default();
    shelf.shown.insert("old".into(), today() - Days::new(90));
    shelf.shown.insert("kept".into(), today() - Days::new(89));
    shelf.prune_shown(today());
    assert!(shelf.shown.contains_key("kept") && !shelf.shown.contains_key("old"));
}
