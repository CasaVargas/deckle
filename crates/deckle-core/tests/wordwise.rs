// SPDX-License-Identifier: GPL-3.0-or-later
use std::io::Write;
use std::path::{Path, PathBuf};

use deckle_core::deliver::{DeliverOptions, Target, detect_in};
use deckle_core::pipeline::{RunReport, attach_word_wise};
use deckle_core::wordwise::{
    DeviceDicts, Slot, WordWiseOptions, build, ensure_database, ensure_database_pinned, probe,
};
use deckle_kfx::text::Run;
use rusqlite::{Connection, params};

/// A two-word Deckle database (the mini fixture's "universally acknowledged").
fn make_db(dir: &Path, version: &str) -> PathBuf {
    let path = dir.join(format!("ww-{version}.sqlite"));
    let conn = deckle_wordwise::lexicon::create(&path, version).unwrap();
    for (id, lemma, pos, short, diff) in [
        (1, "universally", "adverb", "everywhere", 4),
        (2, "acknowledged", "adjective", "accepted", 5),
    ] {
        conn.execute(
            "INSERT INTO senses VALUES (?1, ?2, ?3, ?4, ?4, NULL, ?5)",
            params![id, lemma, pos, short, diff],
        )
        .unwrap();
        conn.execute("INSERT INTO forms VALUES (?1, ?2)", params![lemma, id])
            .unwrap();
    }
    path
}

fn runs() -> Vec<Run> {
    vec![Run {
        pid: 1000,
        text: "It is a truth universally acknowledged.".into(),
    }]
}

fn opts(db: &Path, cache: &Path, level: u8) -> WordWiseOptions {
    WordWiseOptions {
        level,
        database: Some(db.to_path_buf()),
        cache_dir: cache.to_path_buf(),
    }
}

fn gloss_rows(kll: &[u8]) -> Vec<(i64, i64)> {
    let dir = tempfile::tempdir().unwrap();
    let p = dir.path().join("x.kll");
    std::fs::write(&p, kll).unwrap();
    let c = Connection::open(&p).unwrap();
    c.prepare("SELECT start, sense_id FROM glosses ORDER BY start")
        .unwrap()
        .query_map([], |r| Ok((r.get(0)?, r.get(1)?)))
        .unwrap()
        .map(Result::unwrap)
        .collect()
}

fn amazon_klld(path: &Path) {
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    Connection::open(path)
        .unwrap()
        .execute_batch(
            "CREATE TABLE pos_types (id integer, label varchar(100));
             CREATE TABLE metadata (key TEXT PRIMARY KEY, value TEXT);
             CREATE TABLE lemmas (id INTEGER PRIMARY KEY, lemma TEXT);
             CREATE TABLE senses (id INTEGER PRIMARY KEY, display_lemma_id INTEGER, pos_type INTEGER, sense_number REAL);
             INSERT INTO pos_types VALUES (2,'adjective'),(3,'adverb');
             INSERT INTO lemmas VALUES (7,'universally'),(8,'acknowledged');
             INSERT INTO senses VALUES (70001, 7, 3, 1.0), (80001, 8, 2, 1.0);",
        )
        .unwrap();
}

fn kindle(root: &Path) -> Target {
    std::fs::create_dir_all(root.join("documents")).unwrap();
    std::fs::create_dir_all(root.join("system")).unwrap();
    Target::MassStorage(detect_in(&[root.to_path_buf()]).remove(0))
}

#[test]
fn deckle_dictionary_mode_installs_the_klld_once() {
    let dir = tempfile::tempdir().unwrap();
    let db = make_db(dir.path(), "2026-10-19");
    let cache = dir.path().join("cache");
    let empty = DeviceDicts {
        kindle_en: None,
        zh_slot: Slot::Empty,
    };
    let out = build(
        "B0TESTDECK",
        "CR!X",
        &runs(),
        &db,
        false,
        &opts(&db, &cache, 5),
        &empty,
    )
    .unwrap()
    .unwrap();
    assert_eq!(out.file_name, "LanguageLayer.en.B0TESTDECK.kll");
    assert_eq!(out.summary.dictionary, "deckle");
    assert_eq!(out.summary.hints, 2);
    assert_eq!(gloss_rows(&out.bytes), vec![(1014, 1), (1026, 2)]);
    let (path, klld) = out.dictionary_file.expect("dictionary to install");
    assert_eq!(path, ["system", "kll", "kll.en.zh.klld"]);
    assert!(!klld.is_empty());
    // the slot already holds this version: nothing to install
    let same = DeviceDicts {
        kindle_en: None,
        zh_slot: Slot::Deckle("2026-10-19".into()),
    };
    let out = build(
        "B0TESTDECK",
        "CR!X",
        &runs(),
        &db,
        false,
        &opts(&db, &cache, 5),
        &same,
    )
    .unwrap()
    .unwrap();
    assert!(out.dictionary_file.is_none());
    // an older Deckle dictionary is replaced
    let old = DeviceDicts {
        kindle_en: None,
        zh_slot: Slot::Deckle("2025-01-01".into()),
    };
    let out = build(
        "B0TESTDECK",
        "CR!X",
        &runs(),
        &db,
        false,
        &opts(&db, &cache, 5),
        &old,
    )
    .unwrap()
    .unwrap();
    assert!(out.dictionary_file.is_some());
}

#[test]
fn amazons_chinese_dictionary_is_never_replaced() {
    let dir = tempfile::tempdir().unwrap();
    let db = make_db(dir.path(), "v");
    let other = DeviceDicts {
        kindle_en: None,
        zh_slot: Slot::Other,
    };
    let err = build(
        "B0X",
        "a",
        &runs(),
        &db,
        false,
        &opts(&db, dir.path(), 5),
        &other,
    )
    .unwrap_err();
    assert!(err.contains("will not replace"), "{err}");
}

#[test]
fn level_and_empty_results() {
    let dir = tempfile::tempdir().unwrap();
    let db = make_db(dir.path(), "v");
    let empty = DeviceDicts {
        kindle_en: None,
        zh_slot: Slot::Empty,
    };
    let out = build(
        "B0X",
        "a",
        &runs(),
        &db,
        false,
        &opts(&db, dir.path(), 4),
        &empty,
    )
    .unwrap()
    .unwrap();
    assert_eq!(out.summary.hints, 1);
    assert_eq!(out.summary.level, 4);
    assert!(
        build(
            "B0X",
            "a",
            &runs(),
            &db,
            false,
            &opts(&db, dir.path(), 3),
            &empty
        )
        .unwrap()
        .is_none()
    );
}

#[test]
fn a_kindle_with_its_own_dictionary_gets_amazon_sense_ids_and_no_install() {
    let dir = tempfile::tempdir().unwrap();
    let db = make_db(dir.path(), "v");
    let root = dir.path().join("kindle");
    let target = kindle(&root);
    amazon_klld(&root.join("system/kll/kll.en.en.klld"));
    let dicts = probe(&target).unwrap();
    assert!(dicts.kindle_en.is_some());
    assert_eq!(dicts.zh_slot, Slot::Empty);
    let out = build(
        "B0X",
        "a",
        &runs(),
        &db,
        false,
        &opts(&db, dir.path(), 5),
        &dicts,
    )
    .unwrap()
    .unwrap();
    assert_eq!(out.summary.dictionary, "device");
    assert_eq!(gloss_rows(&out.bytes), vec![(1014, 70001), (1026, 80001)]);
    assert!(out.dictionary_file.is_none());
}

#[test]
fn an_unreadable_kindle_dictionary_falls_back_to_deckles_with_a_warning() {
    let dir = tempfile::tempdir().unwrap();
    let db = make_db(dir.path(), "v");
    let dicts = DeviceDicts {
        kindle_en: Some(b"not sqlite at all".to_vec()),
        zh_slot: Slot::Empty,
    };
    let out = build(
        "B0X",
        "a",
        &runs(),
        &db,
        false,
        &opts(&db, dir.path(), 5),
        &dicts,
    )
    .unwrap()
    .unwrap();
    assert_eq!(out.summary.dictionary, "deckle");
    assert!(
        out.warnings
            .iter()
            .any(|w| w.contains("not using the Kindle's dictionary"))
    );
}

#[test]
fn probe_reads_the_zh_slot_of_a_folder_or_kindle() {
    let dir = tempfile::tempdir().unwrap();
    let db = make_db(dir.path(), "2026-10-19");
    let out = dir.path().join("export");
    std::fs::create_dir_all(out.join("system/kll")).unwrap();
    assert_eq!(
        probe(&Target::Folder(out.clone())).unwrap().zh_slot,
        Slot::Empty
    );
    deckle_wordwise::klld::write_from_db(&db, &out.join("system/kll/kll.en.zh.klld")).unwrap();
    assert_eq!(
        probe(&Target::Folder(out.clone())).unwrap().zh_slot,
        Slot::Deckle("2026-10-19".into())
    );
    let root = dir.path().join("kindle");
    let target = kindle(&root);
    amazon_klld(&root.join("system/kll/kll.en.zh.klld"));
    assert_eq!(probe(&target).unwrap().zh_slot, Slot::Other);
}

#[test]
fn the_database_is_downloaded_once_verified_and_cached() {
    let dir = tempfile::tempdir().unwrap();
    let db = make_db(dir.path(), "v");
    let mut gz = flate2::write::GzEncoder::new(Vec::new(), flate2::Compression::fast());
    gz.write_all(&std::fs::read(&db).unwrap()).unwrap();
    let gz = gz.finish().unwrap();
    use sha2::Digest;
    let sha: String = sha2::Sha256::digest(&gz)
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect();
    let cache = dir.path().join("cache");
    let o = WordWiseOptions {
        level: 5,
        database: None,
        cache_dir: cache.clone(),
    };
    let mut calls = 0;
    let mut fetch = |url: &str| {
        calls += 1;
        assert!(url.ends_with(".sqlite.gz"), "{url}");
        Ok(gz.clone())
    };
    let (p, downloaded) = ensure_database_pinned(&o, &mut fetch, &sha).unwrap();
    assert!(downloaded);
    assert_eq!(std::fs::read(&p).unwrap(), std::fs::read(&db).unwrap());
    let (p2, downloaded) = ensure_database_pinned(&o, &mut fetch, &sha).unwrap();
    assert_eq!((p2, downloaded), (p, false));
    assert_eq!(calls, 1);
    // a corrupt download is refused and nothing is cached
    let o2 = WordWiseOptions {
        cache_dir: dir.path().join("cache2"),
        ..o
    };
    let err = ensure_database(&o2, &mut |_| Ok(b"garbage".to_vec())).unwrap_err();
    assert!(err.contains("corrupt"), "{err}");
    assert!(!o2.cache_dir.join(deckle_core::wordwise::DB_FILE).exists());
    let err = ensure_database(&o2, &mut |_| Err("offline".into())).unwrap_err();
    assert!(err.contains("offline"), "{err}");
}

#[test]
fn a_failed_word_wise_is_a_warning_and_deckle_mode_says_how_to_switch() {
    let mut report = RunReport::default();
    let mut d = DeliverOptions::default();
    attach_word_wise(&mut report, &mut d, Err("could not download".into()), 5);
    assert_eq!(report.error, None);
    assert!(report.warnings[0].starts_with("Word Wise skipped: could not download"));
    assert!(d.sidecars.is_empty() && d.system_files.is_empty());

    let dir = tempfile::tempdir().unwrap();
    let db = make_db(dir.path(), "v");
    let built = build(
        "B0X",
        "a",
        &runs(),
        &db,
        true,
        &opts(&db, dir.path(), 5),
        &DeviceDicts {
            kindle_en: None,
            zh_slot: Slot::Empty,
        },
    );
    let mut report = RunReport::default();
    attach_word_wise(&mut report, &mut d, built, 5);
    let s = report.word_wise.unwrap();
    assert!(s.downloaded);
    assert_eq!(d.sidecars.len(), 1);
    assert_eq!(d.system_files.len(), 1);
    assert!(report.warnings.iter().any(|w| w.contains("中文")));
}
