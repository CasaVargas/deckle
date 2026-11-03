// SPDX-License-Identifier: GPL-3.0-or-later
//! Puzzle book returns (puzzle book bundle design §4, §6): the MIT schema takes the fixture and
//! refuses a bad one; the reader merges readers by the latest per puzzle, ignores a book Deckle
//! doesn't have, and skips what it can't read.

use std::path::{Path, PathBuf};

use deckle_digest::returns::{self, DIR, PUZZLES_DIR, PuzzleBookReturns, read_books, solved};
use serde_json::Value;
use uuid::Uuid;

const SCHEMA: &str = include_str!("../../../../docs/formats/deckle-puzzle-returns.schema.json");

fn fixtures() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../fixtures/puzzles/returns")
}

fn sampler() -> Uuid {
    "6c1e3b0a-2f4d-5a8b-9c1e-0d2f3a4b5c6d".parse().unwrap()
}

fn fixture() -> String {
    std::fs::read_to_string(fixtures().join(format!("{}.json", sampler()))).unwrap()
}

fn validate(v: &Value) -> Vec<String> {
    let schema: Value = serde_json::from_str(SCHEMA).unwrap();
    let validator = jsonschema::validator_for(&schema).unwrap();
    validator.iter_errors(v).map(|e| e.to_string()).collect()
}

fn write(root: &Path, reader: &str, name: &str, text: &str) {
    let dir = root.join(DIR).join(reader).join(PUZZLES_DIR);
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(dir.join(name), text).unwrap();
}

#[test]
fn the_fixture_validates_and_the_bad_ones_dont() {
    let v: Value = serde_json::from_str(&fixture()).unwrap();
    assert_eq!(validate(&v), Vec::<String>::new());
    let r = returns::parse_book(fixture().as_bytes()).unwrap();
    assert_eq!(r.book, sampler());
    assert_eq!(r.puzzles.len(), 7);
    assert_eq!(
        r.puzzles[1].found.as_deref(),
        Some(&["COURT".to_owned(), "MILL".to_owned()][..])
    );
    assert_eq!(r.puzzles[6].grid, None);
    // what a reader wrote survives a round trip
    let again: PuzzleBookReturns =
        serde_json::from_str(&serde_json::to_string(&r).unwrap()).unwrap();
    assert_eq!(again, r);
    for bad in ["invalid-key-too-short.json", "invalid-missing-done.json"] {
        let v: Value =
            serde_json::from_str(&std::fs::read_to_string(fixtures().join(bad)).unwrap()).unwrap();
        assert!(!validate(&v).is_empty(), "{bad}");
    }
    let mut other = v.clone();
    other["format"] = "deckle-returns".into();
    assert!(!validate(&other).is_empty());
    assert!(
        returns::parse_book(other.to_string().as_bytes())
            .unwrap_err()
            .contains("format")
    );
    let mut newer = v;
    newer["version"] = "2.0".into();
    assert!(
        returns::parse_book(newer.to_string().as_bytes())
            .unwrap_err()
            .contains("version 2.0")
    );
}

#[test]
fn two_readers_merge_by_the_latest_and_an_unknown_book_is_ignored() {
    let tmp = tempfile::tempdir().unwrap();
    assert!(read_books(tmp.path()).is_empty());
    write(tmp.path(), "a", &format!("{}.json", sampler()), &fixture());
    // a second reader, later on the word search (done) and earlier on the crossword (undone)
    let mut later: Value = serde_json::from_str(&fixture()).unwrap();
    later["reader"]["id"] = "ipad-other".into();
    later["updated_at"] = "2026-11-04T08:00:00Z".into();
    later["puzzles"] = serde_json::json!([
        { "n": 1, "done": false, "seconds": 10, "last": "2026-11-01T08:00:00Z", "grid": ["#   #", "     ", "     ", "     ", "#   #"] },
        { "n": 2, "done": true, "seconds": 200, "last": "2026-11-04T08:00:00Z", "found": ["COURT", "MILL", "RIVER"] }
    ]);
    write(
        tmp.path(),
        "b",
        &format!("{}.json", sampler()),
        &later.to_string(),
    );
    // a book Deckle doesn't have, and a file that isn't returns
    let mut unknown: Value = serde_json::from_str(&fixture()).unwrap();
    unknown["book"] = "11111111-2222-3333-4444-555555555555".into();
    write(
        tmp.path(),
        "b",
        "11111111-2222-3333-4444-555555555555.json",
        &unknown.to_string(),
    );
    write(tmp.path(), "c", "garbage.json", "{ not json");
    let all = read_books(tmp.path());
    assert_eq!(all.len(), 3, "the malformed one is skipped");
    let merged = returns::merge_book(&all, sampler());
    assert_eq!(merged.len(), 7);
    assert!(merged[&1].done, "done is sticky: reader a checked it");
    assert_eq!(merged[&1].seconds, 412.0, "reader a's later state wins");
    assert!(merged[&2].done);
    assert_eq!(
        merged[&2].found.as_ref().unwrap().len(),
        3,
        "reader b's later search wins"
    );
    let s = solved(&all, sampler(), 7).unwrap();
    assert_eq!((s.done, s.of), (4, 7));
    assert_eq!(s.seconds, 412.0 + 200.0 + 30.0 + 12.0 + 300.0);
    assert_eq!(
        solved(&all, Uuid::nil(), 7),
        None,
        "no reader wrote about it"
    );
}
