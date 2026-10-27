// SPDX-License-Identifier: GPL-3.0-or-later
use deckle_core::deliver::{Device, Target};
use deckle_core::inventory::DeviceBook;
use deckle_library::search::{DeviceInventory, LocationKind, collect, search};
use deckle_library::{Book, Library, LibraryConfig, Metadata, SendRecord, SeriesRef, Sidecar};

fn book() -> Book {
    Book {
        dir: "/library/Le Guin/Earthsea".into(),
        sidecar: Sidecar {
            schema: 1,
            id: "earthsea".into(),
            source: "book.epub".into(),
            asin: "B01234DECK".into(),
            metadata: Metadata {
                title: "A Wizard of Éarthsea".into(),
                authors: vec!["Ursula K. Le Guin".into()],
                series: Some(SeriesRef {
                    name: "Earthsea Cycle".into(),
                    index: Some(1.0),
                }),
                ..Default::default()
            },
            sends: vec![SendRecord {
                device: "mtp:123".into(),
                device_name: "Scribe".into(),
                file: "wizard.kfx".into(),
                ..Default::default()
            }],
            ..Default::default()
        },
    }
}
fn device(books: Vec<DeviceBook>) -> DeviceInventory {
    DeviceInventory {
        key: "mtp:123".into(),
        name: "Scribe".into(),
        books,
    }
}

#[test]
fn fixture_sidecars_match_terms_across_fields_and_normalize_accents() {
    let root = tempfile::tempdir().unwrap();
    let b = book();
    let dir = root.path().join("author/title");
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(dir.join("book.epub"), b"metadata comes from sidecar").unwrap();
    std::fs::write(
        dir.join("deckle.json"),
        serde_json::to_vec(&b.sidecar).unwrap(),
    )
    .unwrap();
    let books = Library::new(root.path()).list();
    assert_eq!(search(&books, &[], "GUIN earthsea cycle").len(), 1);
    assert_eq!(search(&books, &[], "e\u{301}arthsea ursula").len(), 1);
    assert!(search(&books, &[], "earthsea missing").is_empty());
    assert_eq!(search(&books, &[], "  ").len(), 1);
}

#[test]
fn live_inventory_replaces_history_and_merges_by_send_path_or_asin() {
    let b = book();
    let historical = search(std::slice::from_ref(&b), &[], "wizard");
    assert_eq!(historical[0].locations[1].kind, LocationKind::RecordedSend);
    let gone = search(std::slice::from_ref(&b), &[device(vec![])], "wizard");
    assert_eq!(gone[0].locations.len(), 1);
    for row in [
        DeviceBook {
            file: "wizard.kfx".into(),
            ..Default::default()
        },
        DeviceBook {
            file: "renamed.kfx".into(),
            asin: Some(b.sidecar.asin.clone()),
            ..Default::default()
        },
    ] {
        let hits = search(std::slice::from_ref(&b), &[device(vec![row])], "guin");
        assert_eq!(hits.len(), 1);
        assert_eq!(hits[0].locations.len(), 2);
        assert_eq!(hits[0].locations[1].kind, LocationKind::Connected);
    }
}

#[test]
fn title_alone_does_not_merge_different_editions_and_folder_exports_are_not_kindles() {
    let mut b = book();
    b.sidecar.sends.push(SendRecord {
        device: "folder:/export".into(),
        ..Default::default()
    });
    let hits = search(
        &[b],
        &[device(vec![DeviceBook {
            file: "other.kfx".into(),
            title: Some("A Wizard of Éarthsea".into()),
            ..Default::default()
        }])],
        "earthsea",
    );
    assert_eq!(hits.len(), 2);
    assert!(hits.iter().all(|h| h.locations.len() == 1));
}

#[test]
fn device_only_books_work_without_a_configured_library() {
    let root = tempfile::tempdir().unwrap();
    std::fs::create_dir(root.path().join("documents")).unwrap();
    std::fs::write(root.path().join("documents/Éarthsea notes.txt"), b"notes").unwrap();
    let target = Target::MassStorage(Device {
        root: root.path().into(),
        model_hint: "Paperwhite".into(),
        thumbnails_writable: false,
        firmware: None,
        serial: Some("test".into()),
    });
    let report = collect(&root.path().join("absent.json"), &[target], "earthsea");
    assert!(report.warnings.is_empty());
    assert_eq!(report.results.len(), 1);
    assert_eq!(report.results[0].locations[0].name, "Paperwhite");
    let json = serde_json::to_value(&report).unwrap();
    assert_eq!(json["results"][0]["locations"][0]["kind"], "connected");
    assert!(json["results"][0]["authors"].is_array());
}

#[test]
fn failed_inventory_retains_history_with_a_warning() {
    let root = tempfile::tempdir().unwrap();
    let dir = root.path().join("library/book");
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(dir.join("book.epub"), b"source").unwrap();
    std::fs::write(
        dir.join("deckle.json"),
        serde_json::to_vec(&book().sidecar).unwrap(),
    )
    .unwrap();
    let config = root.path().join("library.json");
    LibraryConfig {
        root: Some(root.path().join("library")),
        ..Default::default()
    }
    .save(&config)
    .unwrap();
    let target = Target::MassStorage(Device {
        root: root.path().join("missing"),
        model_hint: "Missing Kindle".into(),
        thumbnails_writable: false,
        firmware: None,
        serial: None,
    });
    let report = collect(&config, &[target], "wizard");
    assert_eq!(report.results.len(), 1);
    assert_eq!(
        report.results[0].locations[1].kind,
        LocationKind::RecordedSend
    );
    assert_eq!(report.warnings.len(), 1);
}
