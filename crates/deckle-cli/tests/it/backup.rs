// SPDX-License-Identifier: GPL-3.0-or-later
//! `deckle kindle backup` and `restore` against folders standing in for two Kindles'
//! documents/ (Books and Kindle design §7).

use std::path::Path;

use assert_cmd::Command;

fn deckle_at(home: &Path) -> Command {
    let mut c = Command::cargo_bin("deckle").unwrap();
    c.env("HOME", home)
        .env("XDG_CACHE_HOME", home.join(".cache"))
        .env("XDG_CONFIG_HOME", home.join(".config"))
        .env("APPDATA", home.join("AppData"));
    c
}

#[test]
fn a_backup_skips_store_books_and_restores_onto_another_kindle() {
    let tmp = tempfile::tempdir().unwrap();
    let home = tmp.path().join("home");
    let one = tmp.path().join("One");
    std::fs::create_dir_all(one.join("Mini.sdr")).unwrap();
    std::fs::write(
        one.join("Mini.azw3"),
        deckle_mobi::fixture::build(&deckle_mobi::fixture::Book {
            title: "Mini",
            ..Default::default()
        }),
    )
    .unwrap();
    std::fs::write(one.join("Mini.sdr/Mini.azw3f"), b"place").unwrap();
    std::fs::write(
        one.join("Store.azw"),
        deckle_mobi::fixture::build(&deckle_mobi::fixture::Book {
            title: "Store",
            drm: true,
            cde: Some("EBOK"),
            ..Default::default()
        }),
    )
    .unwrap();
    let backups = tmp.path().join("Backups");
    let out = deckle_at(&home)
        .args(["kindle", "backup", "--json", "--into"])
        .arg(&backups)
        .arg("--to")
        .arg(&one)
        .output()
        .unwrap();
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    let report: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(report["copied"], serde_json::json!(["Mini.azw3"]));
    assert_eq!(report["skipped"][0][0], "Store.azw");
    let dir = Path::new(report["dir"].as_str().unwrap());
    assert!(dir.join("deckle-backup.json").is_file());

    let two = tmp.path().join("Two");
    std::fs::create_dir_all(&two).unwrap();
    let out = deckle_at(&home)
        .args(["kindle", "restore"])
        .arg(dir)
        .arg("--to")
        .arg(&two)
        .output()
        .unwrap();
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    assert!(String::from_utf8_lossy(&out.stdout).contains("restored Mini.azw3"));
    assert_eq!(
        std::fs::read(two.join("Mini.sdr/Mini.azw3f")).unwrap(),
        b"place"
    );
    assert!(!two.join("Store.azw").exists());
}
