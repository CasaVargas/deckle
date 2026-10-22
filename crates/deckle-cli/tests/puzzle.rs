// SPDX-License-Identifier: GPL-3.0-or-later
//! The puzzle's answers read from a Kindle's notes on connect (puzzle design §5), against a
//! folder standing in for its `documents/`, with Deckle's config folder in a temp dir.

use std::path::{Path, PathBuf};

use assert_cmd::Command;
use deckle_digest::chrono::{Days, Local, NaiveDate};
use deckle_digest::config::PuzzleConfig;
use deckle_digest::puzzle::store::{Day, Store};
use deckle_digest::puzzle::words::Words;

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

fn yesterday() -> NaiveDate {
    Local::now().date_naive() - Days::new(1)
}

/// A config folder with `digest` as digest.toml and yesterday's puzzle stored.
fn setup(digest: &str) -> (tempfile::TempDir, Store) {
    let home = tempfile::tempdir().unwrap();
    let dir = config_dir(home.path());
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(dir.join("digest.toml"), digest).unwrap();
    let mut store = Store::default();
    let puzzle = deckle_digest::puzzle::make(
        yesterday(),
        &PuzzleConfig::default(),
        Words::embedded(),
        &store,
    );
    store.days.insert(
        yesterday(),
        Day {
            puzzle,
            headlines: Vec::new(),
            solve: None,
        },
    );
    store.save(&dir.join("puzzles.json")).unwrap();
    (home, store)
}

/// A Kindle folder whose clippings answer yesterday's first clue correctly.
fn kindle(home: &Path, store: &Store, with_file: bool) -> PathBuf {
    let k = home.join("kindle");
    std::fs::create_dir_all(&k).unwrap();
    if with_file {
        let c = &store.days[&yesterday()].puzzle.clues[0];
        let title = format!(
            "The Deckle Daily — {} (Deckle)",
            deckle_digest::long_date(yesterday())
        );
        let text = format!(
            "{title}\r\n- Your Highlight on Location 10-11 | Added on Monday, October 19, 2026 \
             8:00:00 AM\r\n\r\n{} {}\r\n==========\r\n{title}\r\n- Your Note on Location 11 | \
             Added on Monday, October 19, 2026 8:01:00 AM\r\n\r\n{}\r\n==========\r\n",
            c.number, c.clue, c.answer
        );
        std::fs::write(k.join("My Clippings.txt"), text).unwrap();
    }
    k
}

fn stored(home: &Path) -> Store {
    Store::load(&config_dir(home).join("puzzles.json"))
}

#[test]
fn push_scores_yesterday() {
    let (home, store) = setup("sections = [\"puzzle\"]\n\n[sync]\nclippings = false\n");
    let k = kindle(home.path(), &store, true);
    deckle(home.path())
        .args(["digest", "push", "--to"])
        .arg(&k)
        .assert()
        .success();
    let after = stored(home.path());
    let s = after.days[&yesterday()].solve.clone().expect("scored");
    assert_eq!(s.solved, 1);
    assert_eq!(after.notes_readable, Some(true));
    // clippings are off: no highlights were exported
    assert!(!config_dir(home.path()).join("highlights.json").exists());
}

#[test]
fn puzzle_off_reads_no_clippings() {
    let (home, store) = setup("sections = [\"weather\"]\n\n[sync]\nclippings = false\n");
    let k = kindle(home.path(), &store, true);
    deckle(home.path())
        .args(["digest", "push", "--to"])
        .arg(&k)
        .assert()
        .success();
    assert_eq!(stored(home.path()), store);
}

#[test]
fn no_clippings_file_marks_unreadable() {
    let (home, store) = setup("sections = [\"puzzle\"]\n");
    let k = kindle(home.path(), &store, false);
    deckle(home.path())
        .args(["digest", "push", "--to"])
        .arg(&k)
        .assert()
        .success();
    let after = stored(home.path());
    assert_eq!(after.notes_readable, Some(false));
    assert_eq!(after.days[&yesterday()].solve, None);
}

#[test]
fn digest_puzzle_prints_the_mini() {
    let (home, _) = setup("sections = [\"puzzle\"]\n");
    let out = deckle(home.path())
        .args(["digest", "puzzle"])
        .output()
        .unwrap();
    assert!(out.status.success());
    let text = String::from_utf8(out.stdout).unwrap();
    assert!(text.contains("The Mini · No. 2"), "{text}");
    assert!(text.contains("Across"), "{text}");
    assert!(text.contains("Yesterday:"), "{text}");
    // today's is kept: the paper prints the same one
    let today = Local::now().date_naive();
    let kept = stored(home.path()).days[&today].puzzle.clone();
    let out = deckle(home.path())
        .args(["--json", "digest", "puzzle"])
        .output()
        .unwrap();
    let v: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    assert!(v["clues"].is_array());
    assert_eq!(v["number"], 2);
    assert_eq!(v["clues"].as_array().unwrap().len(), kept.clues.len());
    assert_eq!(v["yesterday"]["puzzle"]["number"], 1);
}
