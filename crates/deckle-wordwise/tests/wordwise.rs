// SPDX-License-Identifier: GPL-3.0-or-later
use std::path::Path;

use base64::Engine;
use deckle_kfx::text::Run;
use deckle_wordwise::device::{DeviceDictError, DeviceDictionary};
use deckle_wordwise::kll::{DictionaryRef, Gloss, LanguageLayer, sidecar_name, write_kll};
use deckle_wordwise::klld;
use deckle_wordwise::lexicon::{Lexicon, create, normalize_form};
use deckle_wordwise::matcher::glosses;
use rusqlite::{Connection, params};

/// id, lemma, pos, short, difficulty, forms
#[allow(clippy::type_complexity)]
const SENSES: &[(i64, &str, &str, &str, i64, &[&str])] = &[
    (1, "lugubrious", "adjective", "gloomy", 1, &["lugubrious"]),
    (
        2,
        "take off",
        "verb",
        "remove",
        4,
        &["take off", "took off", "taking off"],
    ),
    (3, "take", "verb", "grab", 5, &["take", "took", "taking"]),
    (
        4,
        "perspicacious",
        "adjective",
        "perceptive",
        1,
        &["perspicacious"],
    ),
    (5, "abominable", "adjective", "hateful", 3, &["abominable"]),
    (
        6,
        "bath",
        "noun",
        "a tub for washing",
        2,
        &["bath", "baths"],
    ),
    (7, "o'er", "adverb", "over", 2, &["o'er"]),
];

fn make_db(dir: &Path) -> std::path::PathBuf {
    let path = dir.join("ww.sqlite");
    let conn = create(&path, "2026-10-19").unwrap();
    for (id, lemma, pos, short, diff, forms) in SENSES {
        conn.execute(
            "INSERT INTO senses VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
            params![
                id,
                lemma,
                pos,
                short,
                format!("{short}, at length"),
                "An example.",
                diff
            ],
        )
        .unwrap();
        for f in *forms {
            conn.execute("INSERT INTO forms VALUES (?1, ?2)", params![f, id])
                .unwrap();
        }
    }
    path
}

fn run(pid: i64, text: &str) -> Run {
    Run {
        pid,
        text: text.into(),
    }
}

#[test]
fn kll_has_worddumbs_schema_and_rows() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join(sidecar_name("B0TESTDECK"));
    assert!(path.ends_with("LanguageLayer.en.B0TESTDECK.kll"));
    let layer = LanguageLayer {
        acr: "CR!TEST".into(),
        dictionary: DictionaryRef::default(),
        glosses: vec![
            Gloss {
                start: 10,
                end: None,
                difficulty: 1,
                sense_id: 7,
            },
            Gloss {
                start: 30,
                end: Some(38),
                difficulty: 4,
                sense_id: 9,
            },
            Gloss {
                start: 30,
                end: None,
                difficulty: 2,
                sense_id: 1,
            },
        ],
    };
    write_kll(&path, &layer).unwrap();
    let c = Connection::open(&path).unwrap();
    let md: Vec<(String, String)> = c
        .prepare("SELECT key, value FROM metadata")
        .unwrap()
        .query_map([], |r| Ok((r.get(0)?, r.get(1)?)))
        .unwrap()
        .map(Result::unwrap)
        .collect();
    let get = |k: &str| md.iter().find(|(a, _)| a == k).map(|(_, v)| v.as_str());
    assert_eq!(get("acr"), Some("CR!TEST"));
    assert_eq!(get("enDictionaryId"), Some("kll.en.en"));
    assert_eq!(get("sidecarRevision"), Some("9"));
    assert_eq!(get("sidecarFormat"), Some("1.0"));
    assert_eq!(md.len(), 9);
    let rows: Vec<(i64, Option<i64>, i64, i64, i64)> = c
        .prepare(
            "SELECT start, end, difficulty, sense_id, low_confidence FROM glosses ORDER BY start",
        )
        .unwrap()
        .query_map([], |r| {
            Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?, r.get(4)?))
        })
        .unwrap()
        .map(Result::unwrap)
        .collect();
    assert_eq!(rows, vec![(10, None, 1, 7, 0), (30, Some(38), 4, 9, 0)]);
}

#[test]
fn normalizes_forms() {
    assert_eq!(normalize_form("O\u{2019}er"), "o'er");
    assert_eq!(normalize_form("Took Off"), "took off");
}

#[test]
fn lexicon_loads_forms_and_phrase_length() {
    let dir = tempfile::tempdir().unwrap();
    let lex = Lexicon::open(&make_db(dir.path())).unwrap();
    assert_eq!(lex.max_words, 2);
    assert_eq!(lex.get("took off").unwrap().lemma, "take off");
    assert_eq!(lex.get("baths").unwrap().id, 6);
    assert!(lex.get("missing").is_none());
    assert_eq!(lex.version, "2026-10-19");
}

#[test]
fn rejects_other_databases() {
    let dir = tempfile::tempdir().unwrap();
    let p = dir.path().join("x.sqlite");
    Connection::open(&p)
        .unwrap()
        .execute_batch("CREATE TABLE metadata (key TEXT, value TEXT);")
        .unwrap();
    assert!(Lexicon::open(&p).is_err());
}

#[test]
fn matches_words_phrases_and_positions() {
    let dir = tempfile::tempdir().unwrap();
    let lex = Lexicon::open(&make_db(dir.path())).unwrap();
    // "é" before the word is one code point, so positions count characters, not bytes.
    let runs = [
        run(100, "Café: a lugubrious man took off his hat."),
        run(
            500,
            "Lugubrious, she said. He went to Bath and took a bath o\u{2019}er there.",
        ),
    ];
    let g = glosses(&runs, &lex, 5);
    let starts: Vec<(i64, Option<i64>, i64)> =
        g.iter().map(|g| (g.start, g.end, g.sense_id)).collect();
    assert_eq!(
        starts,
        vec![
            (108, None, 1),      // lugubrious
            (123, Some(131), 2), // "took off" as one phrase, not "took"
            (500, None, 1),      // sentence-initial capital is fine
            (542, None, 3),      // took (Bath, mid-sentence capital, skipped)
            (549, None, 6),      // bath
            (554, None, 7),      // o’er
        ]
    );
}

#[test]
fn names_are_not_hinted_even_at_a_sentence_start_or_after_mr() {
    let dir = tempfile::tempdir().unwrap();
    let lex = Lexicon::open(&make_db(dir.path())).unwrap();
    // "Bath" is a town here: capitalized mid-sentence more often than lowercase
    let runs = [
        run(0, "Mr. Bath arrived. Bath was lugubrious."),
        run(100, "They went to Bath. She saw Bath again."),
        run(200, "Mr. Lugubrious is a lugubrious man. A lugubrious day."),
    ];
    let ids: Vec<(i64, i64)> = glosses(&runs, &lex, 5)
        .iter()
        .map(|g| (g.start, g.sense_id))
        .collect();
    assert_eq!(ids, vec![(27, 1), (220, 1), (238, 1)]);
}

#[test]
fn level_filters_by_difficulty_and_falls_back_to_shorter_phrase() {
    let dir = tempfile::tempdir().unwrap();
    let lex = Lexicon::open(&make_db(dir.path())).unwrap();
    let runs = [run(0, "They took off. Abominable PERSPICACIOUS weather.")];
    let all: Vec<i64> = glosses(&runs, &lex, 5).iter().map(|g| g.sense_id).collect();
    assert_eq!(all, vec![2, 5]); // all-caps word skipped
    let rare: Vec<i64> = glosses(&runs, &lex, 3).iter().map(|g| g.sense_id).collect();
    assert_eq!(rare, vec![5]); // "took off" (4) and "took" (5) above the level
    assert!(glosses(&runs, &lex, 1).is_empty());
}

#[test]
fn deckle_klld_mirrors_the_kindle_layout() {
    let dir = tempfile::tempdir().unwrap();
    let db = make_db(dir.path());
    let out = dir.path().join("kll.en.zh.klld");
    klld::write_from_db(&db, &out).unwrap();
    let c = Connection::open(&out).unwrap();
    let id: String = c
        .query_row("SELECT value FROM metadata WHERE key = 'id'", [], |r| {
            r.get(0)
        })
        .unwrap();
    assert_eq!(id, "kll.en.zh");
    let (lemma, pos, short): (String, i64, String) = c
        .query_row(
            "SELECT l.lemma, s.pos_type, s.short_def FROM senses s JOIN lemmas l ON l.id = s.term_lemma_id WHERE s.id = 2",
            [],
            |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
        )
        .unwrap();
    assert_eq!(lemma, "take off");
    assert_eq!(pos, 1);
    let short = base64::engine::general_purpose::STANDARD
        .decode(short)
        .unwrap();
    assert_eq!(short, b"remove");
    let adj: i64 = c
        .query_row("SELECT pos_type FROM senses WHERE id = 1", [], |r| r.get(0))
        .unwrap();
    assert_eq!(adj, 2);
    assert_eq!(klld::deckle_version(&out).as_deref(), Some("2026-10-19"));
}

/// A `.klld` shaped like Amazon's: sense ids unrelated to Deckle's.
fn amazon_klld(dir: &Path) -> std::path::PathBuf {
    let p = dir.join("kll.en.en.klld");
    let c = Connection::open(&p).unwrap();
    c.execute_batch(
        "CREATE TABLE pos_types (id integer, label varchar(100));
         CREATE TABLE metadata (key TEXT PRIMARY KEY, value TEXT);
         CREATE TABLE lemmas (id INTEGER PRIMARY KEY, lemma TEXT);
         CREATE TABLE senses (id INTEGER PRIMARY KEY, display_lemma_id INTEGER, term_id INTEGER, term_lemma_id INTEGER, pos_type INTEGER, source_id INTEGER, sense_number REAL, synset_id INTEGER, corpus_count INTEGER, full_def TEXT, short_def TEXT, example_sentence TEXT);
         INSERT INTO pos_types VALUES (0,'noun'),(1,'verb'),(2,'adjective'),(3,'adverb');
         INSERT INTO metadata VALUES ('id','kll.en.en');
         INSERT INTO lemmas VALUES (10,'lugubrious'),(11,'take off'),(12,'take'),(13,'bath');
         INSERT INTO senses (id, display_lemma_id, pos_type, sense_number) VALUES
           (900, 10, 2, 1.0), (901, 11, 1, 2.0), (902, 11, 1, 1.0), (903, 12, 1, 1.0), (904, 13, 1, 1.0);",
    )
    .unwrap();
    p
}

#[test]
fn device_dictionary_remaps_to_amazon_sense_ids() {
    let dir = tempfile::tempdir().unwrap();
    let lex = Lexicon::open(&make_db(dir.path())).unwrap();
    let d = DeviceDictionary::open(&amazon_klld(dir.path())).unwrap();
    assert_eq!(d.len(), 4);
    assert_eq!(d.sense("take off", "verb"), Some(902)); // lowest sense_number
    assert_eq!(d.sense("bath", "noun"), Some(904)); // other POS when ours is missing
    let runs = [run(0, "A lugubrious man took off. Perspicacious!")];
    let g = d.remap(glosses(&runs, &lex, 5), &lex);
    let ids: Vec<i64> = g.iter().map(|g| g.sense_id).collect();
    assert_eq!(ids, vec![900, 902]); // perspicacious is not in Amazon's: dropped
}

#[test]
fn device_dictionary_probe_rejects_unknown_layouts_and_deckles_own() {
    let dir = tempfile::tempdir().unwrap();
    let odd = dir.path().join("odd.klld");
    Connection::open(&odd)
        .unwrap()
        .execute_batch("CREATE TABLE lemmas (id INTEGER, word TEXT);")
        .unwrap();
    assert!(matches!(
        DeviceDictionary::open(&odd),
        Err(DeviceDictError::Schema(_))
    ));
    let ours = dir.path().join("ours.klld");
    klld::write_from_db(&make_db(dir.path()), &ours).unwrap();
    assert!(matches!(
        DeviceDictionary::open(&ours),
        Err(DeviceDictError::IsDeckle)
    ));
}
