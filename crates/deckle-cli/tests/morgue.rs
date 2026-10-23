// SPDX-License-Identifier: GPL-3.0-or-later
//! The Morgue from the CLI (Morgue design §5), with Deckle's config and cache folders moved
//! into a temp dir.

use std::path::{Path, PathBuf};

use assert_cmd::Command;

fn deckle(home: &Path) -> Command {
    let mut c = Command::cargo_bin("deckle").unwrap();
    c.env("HOME", home)
        .env("XDG_CONFIG_HOME", home)
        .env("XDG_CACHE_HOME", home.join("cache"))
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

/// Deckle's cache folder under `home`, as `default_cache_dir` finds it on this OS.
fn cache_dir(home: &Path) -> PathBuf {
    if cfg!(target_os = "macos") {
        home.join("Library/Caches/deckle")
    } else if cfg!(target_os = "windows") {
        home.join("deckle")
    } else {
        home.join("cache/deckle")
    }
}

/// A home whose paper is the horoscope alone (no network) for Leo.
fn setup(extra: &str) -> tempfile::TempDir {
    let home = tempfile::tempdir().unwrap();
    let dir = config_dir(home.path());
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(
        dir.join("digest.toml"),
        format!("sections = [\"horoscope\"]\n\n[horoscope]\nsigns = [\"leo\"]\n\n{extra}"),
    )
    .unwrap();
    home
}

fn build(home: &Path, date: &str) -> assert_cmd::assert::Assert {
    let out = home.join("out");
    deckle(home)
        .args(["digest", "build", "--date", date, "-o"])
        .arg(&out)
        .assert()
        .success()
}

fn morgue(home: &Path) -> PathBuf {
    home.join("Deckle/Morgue")
}

#[test]
fn build_archives() {
    let home = setup("");
    build(home.path(), "2026-10-21");
    let year = morgue(home.path()).join("2026");
    assert!(year.join("2026-10-21.epub").exists());
    let json: serde_json::Value =
        serde_json::from_slice(&std::fs::read(year.join("2026-10-21.json")).unwrap()).unwrap();
    assert_eq!(json["date"], "2026-10-21");
    assert!(json["sections"][0]["stories"].is_array());
}

#[test]
fn archive_failure_is_a_note() {
    let home = setup("[morgue]\nfolder = \"~/not-a-folder\"\n");
    std::fs::write(home.path().join("not-a-folder"), b"a file").unwrap();
    let out = deckle(home.path())
        .args(["--json", "digest", "build", "--date", "2026-10-21", "-o"])
        .arg(home.path().join("out"))
        .assert()
        .success();
    let v: serde_json::Value = serde_json::from_slice(&out.get_output().stdout).unwrap();
    let notes = v["notes"].as_array().unwrap();
    assert!(
        notes.iter().any(|n| n
            .as_str()
            .unwrap()
            .starts_with("Morgue: couldn't save the edition")),
        "{notes:?}"
    );
    assert!(
        home.path()
            .join("out/deckle-daily-2026-10-21.epub")
            .exists(),
        "the build itself stands"
    );
}

#[test]
fn list_search_show() {
    let home = setup("");
    build(home.path(), "2026-10-21");
    build(home.path(), "2026-10-20");
    let list = deckle(home.path())
        .args(["digest", "morgue", "list", "--month", "2026-10"])
        .assert()
        .success();
    let text = String::from_utf8(list.get_output().stdout.clone()).unwrap();
    assert!(
        text.contains("2026-10-21") && text.contains("2026-10-20"),
        "{text}"
    );

    let found = deckle(home.path())
        .args(["digest", "morgue", "search", "leo"])
        .assert()
        .success();
    let text = String::from_utf8(found.get_output().stdout.clone()).unwrap();
    assert!(
        text.contains("2026-10-21") && text.contains("Horoscope"),
        "{text}"
    );
    let none = deckle(home.path())
        .args(["digest", "morgue", "search", "zzzzqx"])
        .assert()
        .success();
    assert!(
        String::from_utf8(none.get_output().stdout.clone())
            .unwrap()
            .contains("Nothing")
    );

    let show = deckle(home.path())
        .args(["--json", "digest", "morgue", "show", "2026-10-21"])
        .assert()
        .success();
    let v: serde_json::Value = serde_json::from_slice(&show.get_output().stdout).unwrap();
    assert_eq!(v["heading"], "Wednesday, October 21 · No. 294");
}

#[test]
fn show_of_a_missing_edition_says_so() {
    let home = setup("");
    deckle(home.path())
        .args(["digest", "morgue", "show", "2026-01-01"])
        .assert()
        .failure()
        .stderr(predicates::str::contains("no edition"));
}

#[test]
fn send_queues_kept_kfx() {
    let home = setup("");
    build(home.path(), "2026-10-21");
    // Keep a KFX for that date, as `digest run` would.
    let kfx = home.path().join("paper.kfx");
    std::fs::write(&kfx, b"KFX BYTES").unwrap();
    let year = morgue(home.path()).join("2026");
    std::fs::copy(&kfx, year.join("2026-10-21.kfx")).unwrap();
    let out = deckle(home.path())
        .args(["--json", "digest", "morgue", "send", "2026-10-21"])
        .assert()
        .success();
    let v: serde_json::Value = serde_json::from_slice(&out.get_output().stdout).unwrap();
    assert_eq!(v["queued"], "deckle-daily-2026-10-21.kfx");
    let queued = cache_dir(home.path()).join("outbox/deckle-daily-2026-10-21.kfx");
    assert_eq!(std::fs::read(queued).unwrap(), b"KFX BYTES");
}
