// SPDX-License-Identifier: GPL-3.0-or-later
//! `deckle words anki` against a copy of a Kindle's drive.

use std::path::Path;

use assert_cmd::Command;

fn deckle(home: &Path) -> Command {
    let mut c = Command::cargo_bin("deckle").unwrap();
    c.env("HOME", home)
        .env("XDG_CONFIG_HOME", home)
        .env("APPDATA", home)
        .env("LOCALAPPDATA", home)
        // No Word Wise database: definitions stay empty, nothing is downloaded.
        .env("DECKLE_WORDWISE_DB", home.join("none.db"));
    c
}

#[test]
fn anki_writes_a_csv_from_a_drive_copy() {
    let tmp = tempfile::tempdir().unwrap();
    let kindle = tmp.path().join("kindle");
    let vocab = kindle.join("system/vocabulary");
    std::fs::create_dir_all(&vocab).unwrap();
    let c = rusqlite::Connection::open(vocab.join("vocab.db")).unwrap();
    c.execute_batch(
        "CREATE TABLE WORDS (id TEXT PRIMARY KEY, word TEXT, stem TEXT, lang TEXT, category INTEGER, timestamp INTEGER, profileid TEXT);
         CREATE TABLE LOOKUPS (id TEXT PRIMARY KEY, word_key TEXT, book_key TEXT, dict_key TEXT, pos TEXT, usage TEXT, timestamp INTEGER);
         CREATE TABLE BOOK_INFO (id TEXT PRIMARY KEY, asin TEXT, guid TEXT, lang TEXT, title TEXT, authors TEXT);
         INSERT INTO BOOK_INFO VALUES ('b1','B0','g','en','Walden','Thoreau');
         INSERT INTO WORDS VALUES ('w1','sedulous','sedulous','en',0,1761000000000,'p');
         INSERT INTO LOOKUPS VALUES ('l1','w1','b1','d','n','a sedulous reader',1761000000000);",
    )
    .unwrap();
    drop(c);
    let out = tmp.path().join("words.csv");
    let o = deckle(tmp.path())
        .args(["--json", "words", "--from"])
        .arg(&kindle)
        .args(["anki", "--to"])
        .arg(&out)
        .output()
        .unwrap();
    assert!(o.status.success(), "{}", String::from_utf8_lossy(&o.stderr));
    let v: serde_json::Value = serde_json::from_slice(&o.stdout).unwrap();
    assert_eq!(v["words"], 1);
    let text = std::fs::read_to_string(&out).unwrap();
    assert!(
        text.ends_with("\"sedulous\",\"\",\"a sedulous reader\",\"Walden\",\"2025-10-20\"\n"),
        "{text}"
    );
}

#[test]
fn a_folder_without_a_vocabulary_builder_is_an_error() {
    let tmp = tempfile::tempdir().unwrap();
    let o = deckle(tmp.path())
        .args(["words", "--from"])
        .arg(tmp.path().join("empty"))
        .args(["anki", "--to"])
        .arg(tmp.path().join("w.csv"))
        .output()
        .unwrap();
    assert_eq!(o.status.code(), Some(1));
    assert!(String::from_utf8_lossy(&o.stderr).contains("Vocabulary Builder"));
    assert!(!tmp.path().join("w.csv").exists());
}
