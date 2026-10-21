// SPDX-License-Identifier: GPL-3.0-or-later
//! `deckle library series` and `library list --series` (series design §3) on a library of
//! hand-written sidecars: two series and a standalone.

use std::path::Path;

use assert_cmd::Command;
use deckle_library::{Metadata, SendRecord, SeriesRef, Sidecar};

/// `deckle` with HOME (so the config and cache folders) under `home`.
fn deckle_at(home: &Path) -> Command {
    let mut c = Command::cargo_bin("deckle").unwrap();
    c.env("HOME", home)
        .env("XDG_CACHE_HOME", home.join(".cache"))
        .env("XDG_CONFIG_HOME", home.join(".config"))
        .env("APPDATA", home.join("AppData"));
    c
}

fn send(device: &str) -> SendRecord {
    SendRecord {
        device: format!("ms:{device}"),
        device_name: device.into(),
        file: "book.kfx".into(),
        at: "2026-10-01T12:00:00Z".into(),
        cde: Some("EBOK".into()),
        ..Default::default()
    }
}

#[allow(clippy::too_many_arguments)]
fn shelve(
    root: &Path,
    id: &str,
    author: &str,
    sort: &str,
    title: &str,
    series: Option<(&str, f32)>,
    added: &str,
    sends: Vec<SendRecord>,
) {
    let dir = root.join(sort).join(title);
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(dir.join("book.epub"), b"not read by list").unwrap();
    let sidecar = Sidecar {
        schema: 1,
        id: id.into(),
        source: "book.epub".into(),
        sha256: "0".repeat(64),
        added: added.into(),
        asin: format!("B0{id}DECK"),
        metadata: Metadata {
            title: title.into(),
            authors: vec![author.into()],
            author_sort: Some(sort.into()),
            series: series.map(|(name, i)| SeriesRef {
                name: name.into(),
                index: Some(i),
            }),
            language: "en".into(),
            ..Default::default()
        },
        sends,
        ..Default::default()
    };
    std::fs::write(
        dir.join("deckle.json"),
        serde_json::to_vec_pretty(&sidecar).unwrap(),
    )
    .unwrap();
}

/// A configured library: Hainish 1 and 4, The Expanse 1 and 2, and Piranesi.
fn library() -> (tempfile::TempDir, std::path::PathBuf) {
    let tmp = tempfile::tempdir().unwrap();
    let home = tmp.path().join("home");
    let root = tmp.path().join("Library");
    deckle_at(&home)
        .args(["library", "init"])
        .arg(&root)
        .assert()
        .success();
    let le_guin = ("Ursula K. Le Guin", "Le Guin, Ursula K.");
    let corey = ("James S. A. Corey", "Corey, James S. A.");
    shelve(
        &root,
        "hain1",
        le_guin.0,
        le_guin.1,
        "Rocannon's World",
        Some(("Hainish Cycle", 1.0)),
        "2026-01-01T00:00:00Z",
        vec![send("Kindle Scribe")],
    );
    shelve(
        &root,
        "hain4",
        le_guin.0,
        le_guin.1,
        "The Left Hand of Darkness",
        Some(("hainish  cycle", 4.0)),
        "2026-01-02T00:00:00Z",
        vec![send("Kindle Scribe"), send("Paperwhite")],
    );
    shelve(
        &root,
        "exp1",
        corey.0,
        corey.1,
        "Leviathan Wakes",
        Some(("The Expanse", 1.0)),
        "2026-02-01T00:00:00Z",
        vec![],
    );
    shelve(
        &root,
        "exp2",
        corey.0,
        corey.1,
        "Caliban's War",
        Some(("The Expanse", 2.0)),
        "2026-02-02T00:00:00Z",
        vec![],
    );
    shelve(
        &root,
        "pir",
        "Susanna Clarke",
        "Clarke, Susanna",
        "Piranesi",
        None,
        "2026-03-01T00:00:00Z",
        vec![],
    );
    (tmp, home)
}

fn stdout(home: &Path, args: &[&str]) -> String {
    let out = deckle_at(home).args(args).output().unwrap();
    assert!(
        out.status.success(),
        "{args:?}: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    String::from_utf8(out.stdout).unwrap()
}

#[test]
fn series_lists_every_series_with_its_gaps() {
    let (_tmp, home) = library();
    assert_eq!(
        stdout(&home, &["library", "series"]),
        "Hainish Cycle — 2 books, missing 2, 3\n\
         The Expanse — 2 books\n"
    );
}

#[test]
fn series_name_prints_reading_order() {
    let (_tmp, home) = library();
    let expected = "Hainish Cycle (Ursula K. Le Guin)\n\
                    \x20 1    Rocannon's World          Kindle Scribe\n\
                    \x20 2    — missing —\n\
                    \x20 3    — missing —\n\
                    \x20 4    The Left Hand of Darkness Kindle Scribe, Paperwhite\n";
    assert_eq!(
        stdout(&home, &["library", "series", "HAINISH cycle"]),
        expected
    );
    assert_eq!(
        stdout(&home, &["library", "series", "hain"]),
        expected,
        "a unique prefix"
    );
    let out = deckle_at(&home)
        .args(["library", "series", "Discworld"])
        .output()
        .unwrap();
    assert!(!out.status.success());
    assert!(String::from_utf8_lossy(&out.stderr).contains("no series"));
}

#[test]
fn series_missing_lists_only_series_with_gaps() {
    let (_tmp, home) = library();
    assert_eq!(
        stdout(&home, &["library", "series", "--missing"]),
        "Hainish Cycle — missing 2, 3\n"
    );
}

#[test]
fn series_json() {
    let (_tmp, home) = library();
    let v: serde_json::Value =
        serde_json::from_str(&stdout(&home, &["--json", "library", "series"])).unwrap();
    assert_eq!(v.as_array().unwrap().len(), 2);
    let h = &v[0];
    assert_eq!(h["name"], "Hainish Cycle");
    assert_eq!(h["key"], "hainish cycle");
    assert_eq!(h["missing"], serde_json::json!([2, 3]));
    assert_eq!(h["duplicates"], serde_json::json!([]));
    assert_eq!(h["unnumbered"], 0);
    let books = h["books"].as_array().unwrap();
    assert_eq!(books.len(), 2);
    assert_eq!(books[0]["index"], 1.0);
    assert_eq!(books[0]["id"], "hain1");
    assert_eq!(books[0]["title"], "Rocannon's World");
    assert_eq!(
        books[0]["authors"],
        serde_json::json!(["Ursula K. Le Guin"])
    );
    assert!(
        books[0]["folder"]
            .as_str()
            .unwrap()
            .ends_with("Rocannon's World")
    );
    assert_eq!(books[1]["sends"][1]["device_name"], "Paperwhite");
    assert_eq!(v[1]["name"], "The Expanse");
}

#[test]
fn list_default_output_is_unchanged() {
    let (_tmp, home) = library();
    assert_eq!(
        stdout(&home, &["library", "list"]),
        "Clarke, Susanna  Piranesi\n\
         Corey, James S. A.  Caliban's War  [The Expanse #2]\n\
         Corey, James S. A.  Leviathan Wakes  [The Expanse #1]\n\
         Le Guin, Ursula K.  Rocannon's World  [Hainish Cycle #1]\n\
         \x20     EBOK on Kindle Scribe, 2026-10-01\n\
         Le Guin, Ursula K.  The Left Hand of Darkness  [hainish  cycle #4]\n\
         \x20     EBOK on Kindle Scribe, 2026-10-01\n\
         \x20     EBOK on Paperwhite, 2026-10-01\n"
    );
}

#[test]
fn list_series_groups_under_headings() {
    let (_tmp, home) = library();
    assert_eq!(
        stdout(&home, &["library", "list", "--series"]),
        "Hainish Cycle\n\
         \x20 Le Guin, Ursula K.  Rocannon's World  [Hainish Cycle #1]\n\
         \x20       EBOK on Kindle Scribe, 2026-10-01\n\
         \x20 Le Guin, Ursula K.  The Left Hand of Darkness  [hainish  cycle #4]\n\
         \x20       EBOK on Kindle Scribe, 2026-10-01\n\
         \x20       EBOK on Paperwhite, 2026-10-01\n\
         \n\
         The Expanse\n\
         \x20 Corey, James S. A.  Leviathan Wakes  [The Expanse #1]\n\
         \x20 Corey, James S. A.  Caliban's War  [The Expanse #2]\n\
         \n\
         No series\n\
         \x20 Clarke, Susanna  Piranesi\n"
    );
    let v: serde_json::Value =
        serde_json::from_str(&stdout(&home, &["--json", "library", "list", "--series"])).unwrap();
    assert_eq!(v[0]["series"], "Hainish Cycle");
    assert_eq!(v[0]["books"][0]["book"]["id"], "hain1");
    assert_eq!(v[2]["series"], serde_json::Value::Null);
    assert_eq!(v[2]["books"][0]["book"]["id"], "pir");
}
