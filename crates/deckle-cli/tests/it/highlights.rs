// SPDX-License-Identifier: GPL-3.0-or-later
//! `deckle highlights` against a folder standing in for a Kindle's `documents/`, with Deckle's
//! config folder moved into a temp dir.

use std::path::Path;

use assert_cmd::Command;

fn deckle(config: &Path) -> Command {
    let mut c = Command::cargo_bin("deckle").unwrap();
    c.env("HOME", config)
        .env("XDG_CONFIG_HOME", config)
        .env("APPDATA", config);
    c
}

fn kindle_with_clippings(dir: &Path, extra: &str) -> std::path::PathBuf {
    let kindle = dir.join("kindle");
    std::fs::create_dir_all(&kindle).unwrap();
    let mut bytes = std::fs::read(
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../../fixtures/clippings/variants.txt"),
    )
    .unwrap();
    bytes.extend_from_slice(extra.as_bytes());
    std::fs::write(kindle.join("My Clippings.txt"), bytes).unwrap();
    kindle
}

#[test]
fn export_writes_files_and_remembers_the_folder() {
    let tmp = tempfile::tempdir().unwrap();
    let kindle = kindle_with_clippings(
        tmp.path(),
        "Broken (X)\r\nnot a meta line\r\n==========\r\n",
    );
    let out = tmp.path().join("notes");
    let o = deckle(tmp.path())
        .args(["--json", "highlights", "--from"])
        .arg(&kindle)
        .args(["export", "--to"])
        .arg(&out)
        .output()
        .unwrap();
    assert!(o.status.success(), "{}", String::from_utf8_lossy(&o.stderr));
    let v: serde_json::Value = serde_json::from_slice(&o.stdout).unwrap();
    let written = v["written"].as_array().unwrap();
    assert_eq!(written.len(), 6, "{v}");
    assert!(
        written
            .iter()
            .any(|w| w["file"] == "Walden.md" && w["created"] == true)
    );
    assert_eq!(v["failed"].as_array().unwrap().len(), 0);
    assert_eq!(v["parse_error"]["line"], 54, "{v}");
    assert!(String::from_utf8_lossy(&o.stderr).contains("line 54"));
    assert!(out.join("Walden.md").exists());

    let o = deckle(tmp.path())
        .args(["--json", "highlights", "--from"])
        .arg(&kindle)
        .arg("export")
        .output()
        .unwrap();
    assert!(o.status.success(), "{}", String::from_utf8_lossy(&o.stderr));
    let v: serde_json::Value = serde_json::from_slice(&o.stdout).unwrap();
    assert_eq!(v["written"].as_array().unwrap().len(), 0, "{v}");
}

#[test]
fn list_shows_counts() {
    let tmp = tempfile::tempdir().unwrap();
    let kindle = kindle_with_clippings(tmp.path(), "");
    let o = deckle(tmp.path())
        .args(["--json", "highlights", "--from"])
        .arg(&kindle)
        .arg("list")
        .output()
        .unwrap();
    assert!(o.status.success(), "{}", String::from_utf8_lossy(&o.stderr));
    let v: serde_json::Value = serde_json::from_slice(&o.stdout).unwrap();
    let walden = v
        .as_array()
        .unwrap()
        .iter()
        .find(|b| b["title"] == "Walden")
        .unwrap();
    assert_eq!(
        (walden["total"].as_u64(), walden["new"].as_u64()),
        (Some(3), Some(3))
    );
    assert!(walden.get("markdown").is_none(), "list keeps it short");

    let o = deckle(tmp.path())
        .args(["highlights", "--from"])
        .arg(&kindle)
        .arg("list")
        .output()
        .unwrap();
    assert!(String::from_utf8_lossy(&o.stdout).contains("Walden"));
}

#[test]
fn no_clippings_is_not_an_error() {
    let tmp = tempfile::tempdir().unwrap();
    let kindle = tmp.path().join("kindle");
    std::fs::create_dir_all(&kindle).unwrap();
    for action in ["list", "export"] {
        let o = deckle(tmp.path())
            .args(["highlights", "--from"])
            .arg(&kindle)
            .arg(action)
            .output()
            .unwrap();
        assert!(o.status.success(), "{action}");
        assert!(String::from_utf8_lossy(&o.stdout).contains("no highlights on this Kindle yet"));
    }
}

#[test]
fn export_without_a_folder_says_how() {
    let tmp = tempfile::tempdir().unwrap();
    let kindle = kindle_with_clippings(tmp.path(), "");
    let o = deckle(tmp.path())
        .args(["highlights", "--from"])
        .arg(&kindle)
        .arg("export")
        .output()
        .unwrap();
    assert_eq!(o.status.code(), Some(2));
    assert!(String::from_utf8_lossy(&o.stderr).contains("--to"));
}
