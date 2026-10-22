// SPDX-License-Identifier: GPL-3.0-or-later
//! Taking a snapshot of a Kindle's shelf (shelf design §4).

use std::path::Path;

use chrono::{DateTime, Duration, TimeZone, Utc};
use deckle_core::inventory::DeviceBook;
use deckle_core::vocab::VocabFiles;
use deckle_digest::shelf::store::{KindleSnapshot, Shelf};
use deckle_library::{Library, Metadata, SendRecord, Sidecar};
use deckle_shelf::{Read, books, record, snapshot, words};
use rusqlite::Connection;

fn now() -> DateTime<Utc> {
    Utc.with_ymd_and_hms(2026, 10, 22, 12, 0, 0).unwrap()
}

fn fixture(rel: &str) -> Vec<u8> {
    std::fs::read(
        Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../fixtures")
            .join(rel),
    )
    .unwrap()
}

fn book(file: &str, title: Option<&str>, yjf: Option<Vec<u8>>) -> DeviceBook {
    DeviceBook {
        file: file.into(),
        format: "kfx".into(),
        title: title.map(str::to_owned),
        yjf,
        ..Default::default()
    }
}

fn read_of(books: Vec<DeviceBook>) -> Read {
    Read {
        books,
        skipped: 0,
        clippings: None,
        vocab: None,
        clippings_failed: false,
        vocab_failed: false,
        previous: None,
    }
}

/// A library holding one book whose send to `mtp:K` named `Odd.kfx` and ended at 5000.
fn library_with_send(root: &Path) -> Library {
    let dir = root.join("Author - Library Title");
    std::fs::create_dir_all(&dir).unwrap();
    let sidecar = Sidecar {
        schema: 1,
        asin: "B0LIBRDECK".into(),
        metadata: Metadata {
            title: "Library Title".into(),
            authors: vec!["Ada Author".into()],
            ..Default::default()
        },
        sends: vec![SendRecord {
            device: "mtp:K".into(),
            file: "Odd.kfx".into(),
            end: Some(5000),
            ..Default::default()
        }],
        ..Default::default()
    };
    std::fs::write(
        dir.join("deckle.json"),
        serde_json::to_vec(&sidecar).unwrap(),
    )
    .unwrap();
    Library::new(root)
}

#[test]
fn books_get_titles_and_ends() {
    let tmp = tempfile::tempdir().unwrap();
    let lib = library_with_send(tmp.path());
    let yjf = fixture("yjf/book.yjf");
    let books = vec![
        book("Kept.kfx", Some("From The KFX"), Some(yjf.clone())),
        book("Odd.kfx", None, Some(yjf.clone())),
        book("Walden_B0ABCDEFGH.kfx", None, Some(yjf.clone())),
        book("NoPosition.kfx", Some("Unread"), None),
        book(
            "Corrupt.kfx",
            Some("Corrupt Book"),
            Some(b"nonsense".to_vec()),
        ),
    ];
    let (snap, errors) = snapshot(read_of(books), "Scribe", "mtp:K", Some(&lib), now());
    let by = |t: &str| snap.books.iter().find(|b| b.title == t);
    assert_eq!(by("From The KFX").unwrap().last, Some(54797));
    let odd = by("Library Title").expect("a library match supplies the title");
    assert_eq!(odd.end, Some(5000), "and the send's end");
    assert_eq!(by("Walden").unwrap().file, "Walden_B0ABCDEFGH.kfx");
    assert_eq!(
        snap.books.len(),
        3,
        "no position and a corrupt one are left out"
    );
    assert_eq!(errors.len(), 1, "{errors:?}");
    assert!(errors[0].contains("Corrupt"), "{errors:?}");
    assert_eq!(snap.name, "Scribe");
    assert!(snap.seen.starts_with("2026-10-22"));
    let b = by("From The KFX").unwrap();
    assert_eq!((b.words_read, b.wpm.len()), (Some(9519), 4));
}

#[test]
fn titles_from_file_names() {
    for (file, want) in [
        ("Walden_B0ABCDEFGH.kfx", Some("Walden")),
        ("Walden-B0ABCDEFGH.kfx", Some("Walden")),
        ("Walden_EBOK.kfx", Some("Walden")),
        ("Walden_PDOC.kfx", Some("Walden")),
        ("Sub/Plain Name.kfx", Some("Plain Name")),
        ("B0ABCDEFGH_EBOK.kfx", None),
        ("B0ABCDEFGH.kfx", None),
        (".kfx", None),
        ("_EBOK.kfx", None),
    ] {
        assert_eq!(books::title_from_file(file).as_deref(), want, "{file}");
    }
}

#[test]
fn highlights_snapshot() {
    let mut clippings = fixture("clippings/variants.txt");
    clippings.extend_from_slice(
        "\nThe Morning Deckle \u{2014} Tuesday, October 20, 2026 (Deckle)\n- Your Highlight on Location 5-6 | Added on Tuesday, October 20, 2026 7:00:00 AM\n\nAn edition passage.\n==========\n"
            .as_bytes(),
    );
    let mut read = read_of(vec![]);
    read.clippings = Some(clippings);
    let (snap, _) = snapshot(read, "Paperwhite", "ms:P", None, now());
    assert!(snap.highlights_shared);
    assert!(
        snap.highlights.iter().all(|h| h.author != "Deckle"),
        "editions are dropped"
    );
    let noted: Vec<_> = snap
        .highlights
        .iter()
        .filter(|h| !h.notes.is_empty())
        .collect();
    assert_eq!(noted.len(), 1, "the note attaches to its highlight");
    let walden = noted[0];
    assert_eq!(
        (walden.title.as_str(), walden.author.as_str()),
        ("Walden", "Henry David Thoreau")
    );
    assert!(
        walden.text.ends_with("teach."),
        "the extended highlight replaced the first"
    );
    assert_eq!(walden.notes, ["read this again before the cabin trip"]);
    assert_eq!(walden.page.as_deref(), Some("88"));
    assert_eq!(
        walden.added.map(|d| d.to_string()).as_deref(),
        Some("2025-03-04")
    );

    let (none, _) = snapshot(read_of(vec![]), "Scribe", "mtp:S", None, now());
    assert!(!none.highlights_shared && none.highlights.is_empty());
}

#[test]
fn scribe_shaped_read() {
    let yjf = fixture("yjf/book.yjf");
    let (snap, errors) = snapshot(
        read_of(vec![book("A.kfx", Some("A"), Some(yjf))]),
        "Scribe",
        "mtp:S",
        None,
        now(),
    );
    assert!(errors.is_empty());
    assert!(!snap.highlights_shared && !snap.words_shared);
    assert!(!snap.books.is_empty());
}

fn make_vocab(dir: &Path) -> Connection {
    let c = Connection::open(dir.join("vocab.db")).unwrap();
    c.execute_batch(
        "PRAGMA journal_mode=WAL; PRAGMA wal_autocheckpoint=0;
         CREATE TABLE WORDS (id TEXT PRIMARY KEY, word TEXT, stem TEXT, lang TEXT, category INTEGER, timestamp INTEGER, profileid TEXT);
         CREATE TABLE LOOKUPS (id TEXT PRIMARY KEY, word_key TEXT, book_key TEXT, dict_key TEXT, pos TEXT, usage TEXT, timestamp INTEGER);
         CREATE TABLE BOOK_INFO (id TEXT PRIMARY KEY, asin TEXT, guid TEXT, lang TEXT, title TEXT, authors TEXT);
         INSERT INTO BOOK_INFO VALUES ('b1','B0','g','en','Walden','Thoreau');",
    )
    .unwrap();
    c
}

fn lookup(c: &Connection, id: &str, word: &str, lang: &str, category: i64, age: Duration) {
    let ms = (now() - age).timestamp_millis();
    c.execute(
        "INSERT INTO WORDS VALUES (?1, ?2, ?2, ?3, ?4, ?5, 'p')",
        rusqlite::params![format!("w-{id}"), word, lang, category, ms],
    )
    .unwrap();
    c.execute(
        "INSERT INTO LOOKUPS VALUES (?1, ?2, 'b1', 'd', 'n', ?3, ?4)",
        rusqlite::params![id, format!("w-{id}"), format!("a sentence with {word}"), ms],
    )
    .unwrap();
}

#[test]
fn vocab_read_filters_and_uses_wal() {
    let dir = tempfile::tempdir().unwrap();
    let c = make_vocab(dir.path());
    lookup(&c, "1", "ephemeral", "en", 0, Duration::days(2));
    lookup(&c, "2", "hund", "de", 0, Duration::days(2));
    lookup(&c, "3", "skipped", "en", 100, Duration::days(2));
    lookup(&c, "4", "ancient", "en", 0, Duration::days(15));
    c.execute_batch("PRAGMA wal_checkpoint(FULL);").unwrap();
    // This lookup lives only in the write-ahead log while the connection stays open.
    c.execute_batch("PRAGMA wal_autocheckpoint=0;").unwrap();
    lookup(&c, "5", "newest", "en", 0, Duration::hours(3));
    let files = VocabFiles {
        db: std::fs::read(dir.path().join("vocab.db")).unwrap(),
        wal: std::fs::read(dir.path().join("vocab.db-wal")).ok(),
    };
    assert!(
        files.wal.as_ref().is_some_and(|w| !w.is_empty()),
        "the lookup is in the WAL"
    );
    let got = words::read(&files, now() - Duration::days(14), &|w| {
        (w == "ephemeral").then(|| "hopeful".to_owned())
    })
    .unwrap();
    let names: Vec<&str> = got.iter().map(|w| w.word.as_str()).collect();
    assert_eq!(
        names,
        ["newest", "ephemeral"],
        "newest first; de, category 100 and 15 days out"
    );
    assert_eq!(got[1].definition.as_deref(), Some("hopeful"));
    assert_eq!(got[1].book, "Walden");
    assert_eq!(got[1].usage, "a sentence with ephemeral");
    assert!(
        got[1].at.ends_with('Z') && got[1].at.starts_with("2026-10-20"),
        "{}",
        got[1].at
    );
    drop(c);
}

#[test]
fn a_bad_vocab_db_is_an_error_not_a_panic() {
    let files = VocabFiles {
        db: b"not sqlite".to_vec(),
        wal: None,
    };
    assert!(words::read(&files, now() - Duration::days(14), &|_| None).is_err());
}

fn snap_with_highlight() -> KindleSnapshot {
    KindleSnapshot {
        name: "Paperwhite".into(),
        seen: "2026-10-20".into(),
        highlights_shared: true,
        highlights: vec![Default::default()],
        ..Default::default()
    }
}

#[test]
fn record_keeps_other_kindles() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("shelf.json");
    let first = record(&path, "P", snap_with_highlight()).unwrap();
    assert_eq!(
        first.describe(),
        "Shelf: 0 books in progress, 1 highlight source, 0 words"
    );
    let scribe = KindleSnapshot {
        name: "Scribe".into(),
        seen: "2026-10-22".into(),
        ..Default::default()
    };
    record(&path, "S", scribe).unwrap();
    let shelf = Shelf::load(&path);
    assert_eq!(
        shelf.kindles["P"].highlights.len(),
        1,
        "the Paperwhite's highlights survive"
    );
    assert!(shelf.kindles["S"].highlights.is_empty());
}

#[test]
fn a_long_usage_is_trimmed_at_a_word_boundary() {
    let long = "word ".repeat(80);
    let dir = tempfile::tempdir().unwrap();
    let c = make_vocab(dir.path());
    let ms = (now() - Duration::days(1)).timestamp_millis();
    c.execute(
        "INSERT INTO WORDS VALUES ('w1','verbose','verbose','en',0,?1,'p')",
        [ms],
    )
    .unwrap();
    c.execute(
        "INSERT INTO LOOKUPS VALUES ('l1','w1','b1','d','n',?1,?2)",
        rusqlite::params![long, ms],
    )
    .unwrap();
    c.execute_batch("PRAGMA wal_checkpoint(FULL);").unwrap();
    let files = VocabFiles {
        db: std::fs::read(dir.path().join("vocab.db")).unwrap(),
        wal: None,
    };
    let got = words::read(&files, now() - Duration::days(14), &|_| None).unwrap();
    let u = &got[0].usage;
    assert!(u.chars().count() <= 161, "{}", u.chars().count());
    assert!(u.ends_with('\u{2026}'), "{u}");
    assert!(!u.trim_end_matches('\u{2026}').ends_with(' '));
    assert!(u.starts_with("word word"));
}

#[test]
fn a_failed_read_keeps_what_the_last_plug_in_found() {
    let previous = KindleSnapshot {
        name: "Paperwhite".into(),
        seen: "2026-10-20".into(),
        highlights_shared: true,
        highlights: vec![Default::default()],
        words_shared: true,
        words: vec![Default::default()],
        ..Default::default()
    };
    let mut read = read_of(vec![]);
    read.clippings_failed = true;
    read.vocab_failed = true;
    read.previous = Some(previous);
    let (snap, _) = snapshot(read, "Paperwhite", "ms:P", None, now());
    assert!(snap.highlights_shared && snap.highlights.len() == 1);
    assert!(snap.words_shared && snap.words.len() == 1);
    // With no failure, a missing file really means nothing shared.
    let mut read = read_of(vec![]);
    read.previous = Some(KindleSnapshot {
        highlights_shared: true,
        highlights: vec![Default::default()],
        ..Default::default()
    });
    let (snap, _) = snapshot(read, "Paperwhite", "ms:P", None, now());
    assert!(!snap.highlights_shared && snap.highlights.is_empty());
}
