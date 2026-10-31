// SPDX-License-Identifier: GPL-3.0-or-later
//! The write-back against calibre's own `calibredb` and a library calibre made (calibre bridge
//! design §8). Runs when `calibredb` is on `PATH`; passes with a note otherwise, unless
//! `DECKLE_REQUIRE_CALIBRE` is set, as CI's calibre job sets it.

use std::path::Path;
use std::process::Command;

use deckle_core::reading::Record;
use deckle_library::calibre;
use deckle_library::writeback::{self, Paths, ReadLog, Writeback};

fn calibredb() -> Option<std::path::PathBuf> {
    let found = writeback::find_calibredb(None);
    if found.is_none() {
        assert!(
            std::env::var_os("DECKLE_REQUIRE_CALIBRE").is_none(),
            "calibredb isn't installed"
        );
        eprintln!("skipped: no calibredb");
    }
    found
}

fn calibredb_ok(exe: &Path, args: &[&str]) -> String {
    let out = Command::new(exe).args(args).output().unwrap();
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    String::from_utf8_lossy(&out.stdout).into_owned()
}

#[test]
fn real_calibredb_takes_the_columns_and_the_values() {
    let Some(exe) = calibredb() else { return };
    let tmp = tempfile::tempdir().unwrap();
    let lib = tmp.path().join("Calibre Library");
    std::fs::create_dir_all(&lib).unwrap();
    let l = lib.to_string_lossy().into_owned();
    calibredb_ok(
        &exe,
        &[
            "add",
            "--empty",
            "--title",
            "Dune",
            "--authors",
            "Frank Herbert",
            "--with-library",
            &l,
        ],
    );
    calibredb_ok(
        &exe,
        &[
            "add",
            "--empty",
            "--title",
            "Emma",
            "--authors",
            "Jane Austen",
            "--with-library",
            &l,
        ],
    );

    // The usual columns, through calibredb, then read from the database calibre wrote.
    let mut wb = Writeback::default();
    let added =
        writeback::create_usual(&mut wb, &exe, &lib, &mut writeback::system_runner).unwrap();
    assert_eq!(added.len(), 5);
    let cols = calibre::columns(&lib).unwrap();
    let types: Vec<(&str, &str)> = cols
        .iter()
        .map(|c| (c.label.as_str(), c.datatype.as_str()))
        .collect();
    assert_eq!(
        types,
        [
            ("deckle_highlights", "comments"),
            ("deckle_last_read", "datetime"),
            ("deckle_progress", "int"),
            ("deckle_rating", "rating"),
            ("deckle_read", "bool")
        ]
    );
    wb.enabled = true;
    let books = calibre::books(&lib).unwrap();
    let dune = books.iter().find(|b| b.title == "Dune").unwrap().clone();
    let emma = books.iter().find(|b| b.title == "Emma").unwrap().clone();

    // A reader's records: Dune by its calibre id, Emma by title (confirmed).
    let config = tmp.path().join("library.json");
    std::fs::write(&config, format!(r#"{{"version":1,"calibre":{:?}}}"#, l)).unwrap();
    let p = Paths::new(&config, None);
    assert_eq!(p.library.as_deref(), Some(lib.as_path()));
    let mut log = ReadLog::default();
    log.record(
        "kobo/1",
        &[
            Record {
                title: "Dune".into(),
                authors: vec!["Frank Herbert".into()],
                calibre_id: Some(dune.uuid.clone()),
                percent: Some(62.4),
                last_read: Some("2026-10-30T09:15:00Z".into()),
                rating: Some(4),
                highlights: vec![deckle_core::reading::Highlight {
                    text: "The spice must flow.".into(),
                    note: "Twice now".into(),
                    chapter: Some("Book One".into()),
                    datetime: "2026-10-30 09:00:00".into(),
                    spine: Some(1),
                    ..Default::default()
                }],
                from: "koreader".into(),
                ..Default::default()
            },
            Record {
                title: "Emma".into(),
                authors: vec!["Jane Austen".into()],
                path: Some("Jane Austen/Emma.epub".into()),
                percent: Some(100.0),
                finished: true,
                last_read: Some("2026-10-29T20:00:00Z".into()),
                from: "kobo".into(),
                ..Default::default()
            },
        ],
    );
    log.save(&p.log).unwrap();
    let s = writeback::status(&{
        wb.save(&p.config).unwrap();
        Paths::new(&config, None)
    });
    let plan = s.plan.unwrap();
    assert_eq!(plan.to_confirm.len(), 1);
    wb.confirmed
        .insert(plan.to_confirm[0].key.clone(), emma.uuid.clone());
    wb.save(&p.config).unwrap();

    let report = writeback::write_now(&p, None, &mut writeback::system_runner)
        .unwrap()
        .unwrap();
    assert_eq!(
        (
            report.written,
            report.failed.clone(),
            report.stopped.clone()
        ),
        (8, vec![], None),
        "{report:?}"
    );

    // What calibre now holds, read back both ways.
    let col = |l: &str| cols.iter().find(|c| c.label == l).unwrap().clone();
    assert_eq!(
        calibre::value(&lib, &col("deckle_progress"), dune.id)
            .unwrap()
            .as_deref(),
        Some("62")
    );
    assert_eq!(
        calibre::value(&lib, &col("deckle_read"), dune.id)
            .unwrap()
            .as_deref(),
        Some("false")
    );
    assert_eq!(
        calibre::value(&lib, &col("deckle_rating"), dune.id)
            .unwrap()
            .as_deref(),
        Some("8")
    );
    assert_eq!(
        calibre::value(&lib, &col("deckle_read"), emma.id)
            .unwrap()
            .as_deref(),
        Some("true")
    );
    let notes = calibre::value(&lib, &col("deckle_highlights"), dune.id)
        .unwrap()
        .unwrap();
    assert!(
        notes.contains("> The spice must flow.") && notes.contains("Twice now"),
        "{notes}"
    );
    let listed = calibredb_ok(
        &exe,
        &[
            "list",
            "--fields",
            "title,*deckle_progress,*deckle_read,*deckle_rating,*deckle_last_read",
            "--for-machine",
            "--with-library",
            &l,
        ],
    );
    let v: serde_json::Value = serde_json::from_str(&listed).unwrap();
    let dune_row = v
        .as_array()
        .unwrap()
        .iter()
        .find(|r| r["title"] == "Dune")
        .unwrap();
    assert_eq!(dune_row["*deckle_progress"], 62);
    assert_eq!(dune_row["*deckle_rating"], 8);
    assert!(
        dune_row["*deckle_last_read"]
            .as_str()
            .unwrap()
            .starts_with("2026-10-30"),
        "{dune_row}"
    );

    // Written once: the next plan is empty, and a value the owner changes is left alone.
    let again = writeback::status(&Paths::new(&config, None)).plan.unwrap();
    assert!(again.changes.is_empty(), "{:?}", again.changes);
    calibredb_ok(
        &exe,
        &[
            "set_custom",
            "--with-library",
            &l,
            "deckle_progress",
            &dune.id.to_string(),
            "70",
        ],
    );
    let again = writeback::status(&Paths::new(&config, None)).plan.unwrap();
    assert!(again.changes.is_empty());
    assert!(
        again
            .conflicts
            .iter()
            .any(|c| c.column == "deckle_progress" && c.calibre == "70")
    );

    // calibre's own annotations, when this calibredb can import them (after calibre 9.17).
    if writeback::supports_annotations(&exe, &mut writeback::system_runner) {
        let epub = tmp.path().join("dune.epub");
        tiny_epub(&epub);
        calibredb_ok(
            &exe,
            &[
                "add_format",
                "--with-library",
                &l,
                &dune.id.to_string(),
                &epub.to_string_lossy(),
            ],
        );
        let mut wb = Writeback::load(&p.config);
        wb.annotations = true;
        wb.save(&p.config).unwrap();
        let r = writeback::write_now(&p, None, &mut writeback::system_runner)
            .unwrap()
            .unwrap();
        assert_eq!((r.annotations, r.failed.clone()), (1, vec![]), "{r:?}");
        let listed = calibredb_ok(&exe, &["annotations", "--with-library", &l, "list"]);
        assert!(listed.contains("The spice must flow."), "{listed}");
        // Once: imported again, nothing new.
        assert!(
            writeback::status(&Paths::new(&config, None))
                .plan
                .unwrap()
                .annotations
                .is_empty()
        );
    } else {
        eprintln!("this calibredb has no annotations import: that part waits for calibre 9.18");
    }
}

fn tiny_epub(path: &Path) {
    use std::io::Write;
    let mut z = zip::ZipWriter::new(std::fs::File::create(path).unwrap());
    let o = zip::write::SimpleFileOptions::default();
    z.start_file("mimetype", o).unwrap();
    z.write_all(b"application/epub+zip").unwrap();
    z.start_file("META-INF/container.xml", o).unwrap();
    z.write_all(br#"<?xml version="1.0"?><container version="1.0" xmlns="urn:oasis:names:tc:opendocument:xmlns:container"><rootfiles><rootfile full-path="c.opf" media-type="application/oebps-package+xml"/></rootfiles></container>"#).unwrap();
    z.start_file("c.opf", o).unwrap();
    z.write_all(br#"<?xml version="1.0"?><package xmlns="http://www.idpf.org/2007/opf" version="2.0" unique-identifier="i"><metadata xmlns:dc="http://purl.org/dc/elements/1.1/"><dc:title>Dune</dc:title><dc:language>en</dc:language><dc:identifier id="i">x</dc:identifier></metadata><manifest><item id="a" href="a.xhtml" media-type="application/xhtml+xml"/><item id="b" href="b.xhtml" media-type="application/xhtml+xml"/></manifest><spine><itemref idref="a"/><itemref idref="b"/></spine></package>"#).unwrap();
    for f in ["a.xhtml", "b.xhtml"] {
        z.start_file(f, o).unwrap();
        z.write_all(b"<html xmlns=\"http://www.w3.org/1999/xhtml\"><body><p>The spice must flow.</p></body></html>").unwrap();
    }
    z.finish().unwrap();
}
