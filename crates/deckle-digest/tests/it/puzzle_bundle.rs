// SPDX-License-Identifier: GPL-3.0-or-later
//! The puzzle book bundle (puzzle book bundle design §6): every kind round-trips through
//! `book.json`, validates against the MIT schema, and is written as safely as an edition.

use std::path::{Path, PathBuf};

use deckle_digest::chrono::{NaiveDate, TimeZone, Utc};
use deckle_digest::puzzle::book::bundle::{
    self, BookJson, Extras, PuzzleJson, bundle_name, read, to_book, verify, write, write_with,
};
use deckle_digest::puzzle::book::plan::Theme;
use deckle_digest::puzzle::book::theme::Source;
use deckle_digest::puzzle::book::{Book, BookOptions, Difficulty, Kind, Puzzles, make};
use deckle_digest::puzzle::words::Words;
use serde_json::Value;

const SCHEMA: &str = include_str!("../../../../docs/formats/puzzle-book.schema.json");

fn date() -> NaiveDate {
    NaiveDate::from_ymd_opt(2026, 11, 3).unwrap()
}

fn fixture() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../fixtures/puzzles/deckle-puzzles-sampler-2026-11-03-9f3a.decklepuzzles")
}

/// A small book of `kind`, seeded, so a run is the same book every time.
fn book(kind: Kind, difficulty: Difficulty) -> (Book, BookOptions) {
    let size = match kind {
        Kind::Crossword => 7,
        Kind::Search => 10,
        Kind::Nonogram => 10,
        Kind::Maze => 8,
        other => other.default_size(),
    };
    let o = BookOptions {
        count: 2,
        size,
        difficulty,
        seed: Some(0x9f3a_0000_0000_0007),
        ..BookOptions::new(kind, date())
    };
    (make(&o, Words::embedded()).unwrap(), o)
}

fn validate(v: &Value) -> Vec<String> {
    let schema: Value = serde_json::from_str(SCHEMA).unwrap();
    let validator = jsonschema::validator_for(&schema).unwrap();
    validator.iter_errors(v).map(|e| e.to_string()).collect()
}

fn book_json(dir: &Path) -> Value {
    serde_json::from_slice(&std::fs::read(dir.join("book.json")).unwrap()).unwrap()
}

/// `b` with what the bundle can't carry masked: a cryptogram's key for letters its quote
/// never uses (the reader needs only the letters on the page).
fn comparable(b: &Book) -> Book {
    let mut b = b.clone();
    if let Puzzles::Cryptogram(cs) = &mut b.puzzles {
        for c in cs {
            for p in 0..26u8 {
                if !c.plain.contains((b'A' + p) as char) {
                    c.key[p as usize] = 0;
                }
            }
        }
    }
    b
}

#[test]
fn every_kind_round_trips_through_book_json_and_validates() {
    let out = tempfile::tempdir().unwrap();
    for kind in Kind::ALL {
        let difficulty = if kind == Kind::Search {
            Difficulty::Hard // all eight directions
        } else {
            Difficulty::Medium
        };
        let (b, o) = book(kind, difficulty);
        let dir = write(&b, &o, out.path()).unwrap();
        assert_eq!(dir, out.path().join(bundle_name(&b)));
        let name = dir.file_name().unwrap().to_str().unwrap();
        assert_eq!(
            name,
            format!(
                "deckle-puzzles-{}-2026-11-03-9f3a.decklepuzzles",
                bundle::slug(kind.title())
            ),
            "{kind:?}"
        );
        assert!(!out.path().join(format!(".{name}.tmp")).exists());

        // the schema, with the fixture's validator
        let v = book_json(&dir);
        let errors = validate(&v);
        assert!(errors.is_empty(), "{kind:?}: {errors:#?}");
        assert_eq!(v["format"], "deckle-puzzles");
        assert_eq!(v["version"], "1.0");
        assert_eq!(v["book"]["kind"], kind.key());
        assert_eq!(v["book"]["seed"], "9f3a000000000007");
        assert_eq!(v["book"]["title"], kind.title());
        assert_eq!(v["book"]["subtitle"], b.subtitle.as_str());
        assert_eq!(v["book"]["theme"]["kind"], "none");
        assert_eq!(v["book"]["paper_id"], Value::Null);
        assert_eq!(v["book"]["sunday_of"], Value::Null);
        assert_eq!(
            v["book"].get("clue_style").is_some(),
            kind == Kind::Crossword,
            "{kind:?}"
        );
        assert_eq!(v["book"].get("size").is_some(), kind.sized(), "{kind:?}");

        // the package: every file listed with its hash, the cover a 600×800 JPEG
        verify(&dir).unwrap();
        let files = v["files"].as_array().unwrap();
        assert_eq!(files.len(), 1);
        assert_eq!(files[0]["path"], "cover.jpg");
        let cover =
            image::load_from_memory(&std::fs::read(dir.join("cover.jpg")).unwrap()).unwrap();
        assert_eq!((cover.width(), cover.height()), (600, 800));

        // the same book comes back
        let json = read(&dir).unwrap();
        assert_eq!(json.book.id.get_version_num(), 5);
        let ns: Vec<usize> = json.puzzles.iter().map(PuzzleJson::n).collect();
        let expect = match kind {
            Kind::Ladder => 6, // three a page, one entry a ladder
            _ => 2,
        };
        assert_eq!(ns, (1..=expect).collect::<Vec<_>>(), "{kind:?}");
        let again = to_book(&json).unwrap();
        assert_eq!(comparable(&again), comparable(&b), "{kind:?}");
    }
    // a second write of a book that's there is refused, and the first is kept
    let (b, o) = book(Kind::Maze, Difficulty::Medium);
    let err = write(&b, &o, out.path()).unwrap_err();
    assert!(
        err.contains("already there") && err.contains("--replace"),
        "{err}"
    );
    verify(&out.path().join(bundle_name(&b))).unwrap();
}

#[test]
fn the_puzzles_say_what_the_pages_print() {
    let out = tempfile::tempdir().unwrap();
    // a hard word search hides words every way, and each reads forward from its square
    let (b, o) = book(Kind::Search, Difficulty::Hard);
    let v = book_json(&write(&b, &o, out.path()).unwrap());
    let p = &v["puzzles"][0];
    let grid: Vec<&str> = p["grid"]
        .as_array()
        .unwrap()
        .iter()
        .map(|r| r.as_str().unwrap())
        .collect();
    assert_eq!(grid.len(), 10);
    for w in p["words"].as_array().unwrap() {
        let word = w["word"].as_str().unwrap();
        let (dr, dc) = match w["dir"].as_str().unwrap() {
            "across" => (0, 1),
            "down" => (1, 0),
            "down-right" => (1, 1),
            "up-right" => (-1, 1),
            "left" => (0, -1),
            "up" => (-1, 0),
            "up-left" => (-1, -1),
            "down-left" => (1, -1),
            other => panic!("dir {other}"),
        };
        let (r, c) = (w["row"].as_i64().unwrap(), w["col"].as_i64().unwrap());
        let read: String = (0..word.len() as i64)
            .map(|k| grid[(r + dr * k) as usize].as_bytes()[(c + dc * k) as usize] as char)
            .collect();
        assert_eq!(read, word);
    }

    // a crossword's numbers are the mini's rule, and its clues' answers are on the grid
    let (b, o) = book(Kind::Crossword, Difficulty::Medium);
    let v = book_json(&write(&b, &o, out.path()).unwrap());
    let p = &v["puzzles"][0];
    assert_eq!(p["size"], 7);
    let numbers = p["numbers"].as_array().unwrap();
    assert_eq!(numbers[0][2], 1);
    for c in p["clues"].as_array().unwrap() {
        let n = c["number"].as_u64().unwrap();
        assert!(numbers.iter().any(|x| x[2] == n), "clue {n} has a square");
    }
    assert_eq!(v["book"]["clue_style"], "plain");

    // sudoku: nine strings, the solution fills the givens
    let (b, o) = book(Kind::Sudoku, Difficulty::Easy);
    let v = book_json(&write(&b, &o, out.path()).unwrap());
    let p = &v["puzzles"][1];
    assert_eq!(p["grade"], "easy");
    let givens = p["givens"].as_array().unwrap();
    let solution = p["solution"].as_array().unwrap();
    assert_eq!((givens.len(), solution.len()), (9, 9));
    for (g, s) in givens.iter().zip(solution) {
        for (gc, sc) in g.as_str().unwrap().chars().zip(s.as_str().unwrap().chars()) {
            assert!(gc == '.' || gc == sc);
        }
    }

    // ladders: one entry a ladder, steps between the ends
    let (b, o) = book(Kind::Ladder, Difficulty::Easy);
    let v = book_json(&write(&b, &o, out.path()).unwrap());
    let ladders = v["puzzles"].as_array().unwrap();
    assert_eq!(ladders.len(), 6);
    for l in ladders {
        let answer = l["answer"].as_array().unwrap();
        assert_eq!(answer.len() as u64, l["steps"].as_u64().unwrap() + 2);
        assert_eq!(answer[0], l["from"]);
        assert_eq!(answer[answer.len() - 1], l["to"]);
    }

    // a cryptogram's given letters are plaintext, and they're in the quote
    let (b, o) = book(Kind::Cryptogram, Difficulty::Easy);
    let v = book_json(&write(&b, &o, out.path()).unwrap());
    let p = &v["puzzles"][0];
    let plain = p["plain"].as_str().unwrap();
    assert_eq!(plain.len(), p["cipher"].as_str().unwrap().len());
    let given = p["given"].as_array().unwrap();
    assert_eq!(given.len(), 3);
    for g in given {
        assert!(plain.contains(g.as_str().unwrap()), "{g} in {plain}");
    }

    // a nonogram's runs are the picture's, and a blank line is []
    let (b, o) = book(Kind::Nonogram, Difficulty::Medium);
    let v = book_json(&write(&b, &o, out.path()).unwrap());
    let p = &v["puzzles"][0];
    let solution: Vec<&str> = p["solution"]
        .as_array()
        .unwrap()
        .iter()
        .map(|r| r.as_str().unwrap())
        .collect();
    let runs = |line: &str| -> Vec<u64> {
        line.split('.')
            .filter(|r| !r.is_empty())
            .map(|r| r.len() as u64)
            .collect()
    };
    let rows: Vec<Vec<u64>> = serde_json::from_value(p["rows"].clone()).unwrap();
    assert_eq!(rows, solution.iter().map(|r| runs(r)).collect::<Vec<_>>());
    let cols: Vec<Vec<u64>> = serde_json::from_value(p["cols"].clone()).unwrap();
    for (c, col) in cols.iter().enumerate() {
        let line: String = solution.iter().map(|r| r.as_bytes()[c] as char).collect();
        assert_eq!(*col, runs(&line));
    }
    assert_eq!(p["picture"], Value::Null);

    // a maze runs from the top left to the bottom right along its solution
    let (b, o) = book(Kind::Maze, Difficulty::Medium);
    let v = book_json(&write(&b, &o, out.path()).unwrap());
    let p = &v["puzzles"][0];
    assert_eq!(p["start"], serde_json::json!([0, 0]));
    assert_eq!(p["end"], serde_json::json!([7, 7]));
    assert_eq!(p["walls"].as_array().unwrap().len(), 64);
    let path = p["solution"].as_array().unwrap();
    assert_eq!(path[0], p["start"]);
    assert_eq!(path[path.len() - 1], p["end"]);
    for pair in path.windows(2) {
        let d = (pair[0][0].as_i64().unwrap() - pair[1][0].as_i64().unwrap()).abs()
            + (pair[0][1].as_i64().unwrap() - pair[1][1].as_i64().unwrap()).abs();
        assert_eq!(d, 1, "one square a step");
    }
}

#[test]
fn extras_name_the_theme_the_clues_and_the_sunday() {
    let out = tempfile::tempdir().unwrap();
    let (b, o) = book(Kind::Crossword, Difficulty::Medium);
    let paper = uuid::Uuid::new_v4();
    let extras = Extras {
        theme: Theme::Source(Source::Week { days: 7 }),
        style: deckle_digest::puzzle::ai::Style::Trivia,
        paper_id: Some(paper),
        sunday_of: Some(date()),
        built_at: Utc.with_ymd_and_hms(2026, 11, 3, 7, 12, 4).unwrap(),
        replace: false,
    };
    let dir = write_with(&b, &o, &extras, out.path()).unwrap();
    let v = book_json(&dir);
    assert!(validate(&v).is_empty(), "{:#?}", validate(&v));
    assert_eq!(v["book"]["theme"]["kind"], "week");
    assert_eq!(v["book"]["theme"]["label"], "This week's paper");
    assert_eq!(v["book"]["clue_style"], "trivia");
    assert_eq!(v["book"]["paper_id"], paper.to_string().as_str());
    assert_eq!(v["book"]["sunday_of"], "2026-11-03");
    assert_eq!(v["book"]["built_at"], "2026-11-03T07:12:04Z");
    // a subject carries its text; the same book has the same id whoever writes it
    let subject = Extras {
        theme: Theme::Subject {
            text: "Rome".into(),
        },
        replace: true,
        ..extras.clone()
    };
    let v2 = book_json(&write_with(&b, &o, &subject, out.path()).unwrap());
    assert_eq!(v2["book"]["theme"]["text"], "Rome");
    assert_eq!(v2["book"]["theme"]["label"], "Rome");
    assert_eq!(v2["book"]["id"], v["book"]["id"]);
}

#[test]
fn a_bundle_is_written_whole_or_not_at_all() {
    let out = tempfile::tempdir().unwrap();
    let (b, o) = book(Kind::Maze, Difficulty::Medium);
    let name = bundle_name(&b);
    // the staging folder's place is taken by a file: the write fails and leaves no bundle
    std::fs::write(out.path().join(format!(".{name}.tmp")), b"in the way").unwrap();
    let err = write(&b, &o, out.path()).unwrap_err();
    assert!(err.contains(".tmp"), "{err}");
    assert!(!out.path().join(&name).exists());
    std::fs::remove_file(out.path().join(format!(".{name}.tmp"))).unwrap();

    let dir = write(&b, &o, out.path()).unwrap();
    verify(&dir).unwrap();
    // a file the manifest doesn't list, or one that changed, fails the writer's own check
    std::fs::write(dir.join("notes.txt"), b"x").unwrap();
    let err = verify(&dir).unwrap_err();
    assert!(err.contains("notes.txt"), "{err}");
    std::fs::remove_file(dir.join("notes.txt")).unwrap();
    std::fs::write(dir.join("cover.jpg"), b"not the cover").unwrap();
    assert!(verify(&dir).is_err());
    // --replace writes it afresh
    let extras = Extras {
        replace: true,
        ..Extras::new(&o, Utc::now())
    };
    write_with(&b, &o, &extras, out.path()).unwrap();
    verify(&dir).unwrap();
}

#[test]
fn the_sampler_fixture_reads_with_every_kind_and_validates() {
    let json: BookJson = read(&fixture()).unwrap();
    assert_eq!(json.version, "1.0");
    assert_eq!(json.book.title, "Sampler");
    assert_eq!(json.book.seed, "0000000000009f3a");
    let kinds: Vec<Kind> = json.puzzles.iter().map(PuzzleJson::kind).collect();
    assert_eq!(kinds, Kind::ALL);
    let v: Value =
        serde_json::from_slice(&std::fs::read(fixture().join("book.json")).unwrap()).unwrap();
    assert!(validate(&v).is_empty(), "{:#?}", validate(&v));
    // what the reader-side types read is what the file says
    let again: Value = serde_json::to_value(&json).unwrap();
    assert_eq!(again, v);
    verify(&fixture()).unwrap();
    // a mixed book isn't one Deckle makes
    let err = to_book(&json).unwrap_err();
    assert!(err.contains("puzzle 2 is a word search"), "{err}");
    // the one-kind books the fixture holds come back as books
    for kind in Kind::ALL {
        let mut one = json.clone();
        one.book.kind = kind;
        one.puzzles.retain(|p| p.kind() == kind);
        let b = to_book(&one).unwrap();
        assert_eq!(b.puzzles.kind(), kind);
        assert_eq!(b.puzzles.len(), 1);
    }
    // not a book
    let tmp = tempfile::tempdir().unwrap();
    std::fs::write(
        tmp.path().join("book.json"),
        br#"{"format":"deckle-edition","version":"1.3","generator":"","book":{},"puzzles":[],"files":[]}"#,
    )
    .unwrap();
    assert!(read(tmp.path()).is_err());
}
