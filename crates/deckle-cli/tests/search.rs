// SPDX-License-Identifier: GPL-3.0-or-later
use assert_cmd::Command;
use std::path::Path;

fn command(home: &Path, mount: &Path) -> Command {
    let mut c = Command::cargo_bin("deckle").unwrap();
    c.env("HOME", home)
        .env("XDG_CONFIG_HOME", home.join(".config"))
        .env("XDG_CACHE_HOME", home.join(".cache"))
        .env("APPDATA", home.join("AppData"))
        .env("DECKLE_MOUNTS", mount);
    c
}

#[test]
fn search_without_library_finds_device_books_and_has_stable_json() {
    let temp = tempfile::tempdir().unwrap();
    let mount = temp.path().join("Fixture Kindle");
    std::fs::create_dir_all(mount.join("documents")).unwrap();
    std::fs::create_dir_all(mount.join("system")).unwrap();
    std::fs::write(
        mount.join("documents/Deckle-search-fixture Éarthsea.txt"),
        b"notes",
    )
    .unwrap();
    let out = command(temp.path(), &mount)
        .args([
            "library",
            "search",
            "DECKLE-SEARCH-FIXTURE",
            "earthsea",
            "--json",
        ])
        .output()
        .unwrap();
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    let report: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    assert!(report["warnings"].is_array());
    let hits = report["results"].as_array().unwrap();
    assert_eq!(hits.len(), 1);
    assert_eq!(hits[0]["title"], "Deckle-search-fixture Éarthsea.txt");
    assert_eq!(hits[0]["authors"], serde_json::json!([]));
    assert!(hits[0]["series"].is_null());
    assert_eq!(hits[0]["locations"][0]["kind"], "connected");
    assert_eq!(hits[0]["locations"][0]["name"], "Fixture Kindle");
    assert_eq!(
        hits[0]["locations"][0]["path"],
        "Deckle-search-fixture Éarthsea.txt"
    );
}

#[test]
fn no_matches_is_success_and_query_is_required() {
    let temp = tempfile::tempdir().unwrap();
    command(temp.path(), temp.path())
        .args(["library", "search", "deckle-fixture-no-such-book-1904"])
        .assert()
        .success()
        .stdout(predicates::str::contains("No books match."));
    command(temp.path(), temp.path())
        .args(["library", "search"])
        .assert()
        .failure();
}
