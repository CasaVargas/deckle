// SPDX-License-Identifier: GPL-3.0-or-later
//! `deckle digest readership` with Deckle's config folder moved into a temp dir.

use std::path::{Path, PathBuf};

use assert_cmd::Command;
use deckle_readership::layout::{Layout, SectionStart};
use deckle_readership::state::{Read, State};

fn deckle(home: &Path) -> Command {
    let mut c = Command::cargo_bin("deckle").unwrap();
    c.env("HOME", home)
        .env("XDG_CONFIG_HOME", home)
        .env("APPDATA", home);
    c
}

/// Deckle's config folder under `home`, as `default_path` finds it on this OS.
fn config_dir(home: &Path) -> PathBuf {
    if cfg!(target_os = "macos") {
        home.join("Library/Application Support/deckle")
    } else {
        home.join("deckle")
    }
}

const DIGEST: &str = "# my paper\nsections = [\"weather\", \"lottery\"] # what goes in\n";

fn setup(state: Option<State>) -> (tempfile::TempDir, PathBuf) {
    let home = tempfile::tempdir().unwrap();
    let dir = config_dir(home.path());
    std::fs::create_dir_all(&dir).unwrap();
    let digest = dir.join("digest.toml");
    std::fs::write(&digest, DIGEST).unwrap();
    if let Some(s) = state {
        s.save(&dir.join("readership.json")).unwrap();
    }
    (home, digest)
}

/// 14 editions, all opened; you get to the lottery (80% in) on 2 of them.
fn seeded() -> State {
    let mut s = State::default();
    let start: deckle_digest::chrono::NaiveDate = "2026-09-01".parse().unwrap();
    for i in 0..14u64 {
        let date = start + deckle_digest::chrono::Days::new(i);
        let sec = |key: &str, name: &str, start| SectionStart {
            key: key.into(),
            name: name.into(),
            start,
            items: None,
        };
        s.record_layout(Layout {
            date,
            title: "The Deckle Daily".into(),
            file: format!("deckle-daily-{date}"),
            end: 10_000,
            sections: vec![
                sec("front", "Front page", 0),
                sec("weather", "Weather", 1000),
                sec("lottery", "Lottery", 8000),
            ],
        });
        let f = if i < 2 { 8500 } else { 4900 };
        let r = Read {
            opened: true,
            furthest: Some(f),
            last: Some(f),
            last_read: None,
            seen: "x".into(),
        };
        s.reads.entry(date).or_default().insert("K".into(), r);
    }
    s
}

#[test]
fn readership_with_no_state_says_how_to_start() {
    let (home, _) = setup(None);
    deckle(home.path())
        .args(["digest", "readership"])
        .assert()
        .success()
        .stdout(predicates::str::contains(
            "No editions recorded yet. Build one with `deckle digest run --to device`.",
        ));
}

#[test]
fn readership_prints_the_report_and_json() {
    let (home, _) = setup(Some(seeded()));
    let out = deckle(home.path())
        .args(["digest", "readership"])
        .assert()
        .success();
    let text = String::from_utf8(out.get_output().stdout.clone()).unwrap();
    assert!(
        text.contains("Opened 14 of 14 editions · usually about 49% in"),
        "{text}"
    );
    assert!(text.contains("Lottery"), "{text}");
    assert!(text.contains("14%"), "{text}");
    assert!(
        text.contains("1. Drop Lottery: You got to it on 2 of 14 days."),
        "{text}"
    );

    let out = deckle(home.path())
        .args(["--json", "digest", "readership"])
        .assert()
        .success();
    let v: serde_json::Value = serde_json::from_slice(&out.get_output().stdout).unwrap();
    assert_eq!(v["opened"], 14);
    assert_eq!(v["suggestions"][0]["id"], "drop:lottery");
}

#[test]
fn apply_drops_the_section_from_digest_toml_keeping_comments() {
    let (home, digest) = setup(Some(seeded()));
    deckle(home.path())
        .args(["digest", "readership", "apply", "1"])
        .assert()
        .success();
    let text = std::fs::read_to_string(&digest).unwrap();
    assert!(
        text.contains("sections = [\"weather\"] # what goes in"),
        "{text}"
    );
    assert!(
        text.contains("# my paper") && text.contains("# what goes in"),
        "{text}"
    );
}

#[test]
fn dismiss_hides_the_suggestion() {
    let (home, _) = setup(Some(seeded()));
    deckle(home.path())
        .args(["digest", "readership", "dismiss", "1"])
        .assert()
        .success();
    let out = deckle(home.path())
        .args(["--json", "digest", "readership"])
        .assert()
        .success();
    let v: serde_json::Value = serde_json::from_slice(&out.get_output().stdout).unwrap();
    assert!(
        v["suggestions"]
            .as_array()
            .unwrap()
            .iter()
            .all(|s| s["id"] != "drop:lottery")
    );
}

#[test]
fn apply_out_of_range_exits_2() {
    let (home, _) = setup(Some(seeded()));
    deckle(home.path())
        .args(["digest", "readership", "apply", "9"])
        .assert()
        .code(2);
}
