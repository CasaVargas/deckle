// SPDX-License-Identifier: GPL-3.0-or-later
use std::path::PathBuf;

use deckle_core::deliver::Target;
use deckle_core::pipeline::{RunOptions, RunReport, XRayMode, run};
use deckle_kfx::convert::CdeType;

#[cfg(unix)]
fn fake(name: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fake_previewer")
        .join(name)
}

#[test]
#[cfg(unix)] // the fake Previewer is a shell script
fn run_with_fake_previewer_delivers_a_kfx_to_a_folder() {
    let out = tempfile::tempdir().unwrap();
    let opts = RunOptions {
        cde_type: CdeType::Pdoc,
        asin: None,
        pages: Some(None),
        target: Target::Folder(out.path().to_path_buf()),
        previewer: Some(fake("ok.sh")),
        keep_work_dir: false,
        cde_override: None,
        amazon_match: false,
        verified_real_asin: false,
        xray: XRayMode::Auto,
        llm: None,
        wikipedia: false,
        word_wise: None,
    };
    let report = run(&deckle_kfx::fixtures::path("deckle-mini.epub"), &opts);
    assert_eq!(report.error, None, "{report:?}");
    assert_eq!(report.info.title, "Deckle Mini Fixture");
    assert!(report.normalize.cover_converted_to_jpeg);
    let delivered = report.delivered.clone().expect("delivered");
    assert_eq!(
        delivered,
        out.path()
            .join("Testwright, Jane - Deckle Mini Fixture.kfx")
    );
    let bytes = std::fs::read(&delivered).unwrap();
    assert!(bytes.starts_with(b"CONT"));
    let doc = deckle_kfx::container::read_kfx(&bytes).unwrap();
    // a generated, recognizable ASIN was stamped and the book is PDOC
    let md = doc
        .fragments
        .get_single(&deckle_kfx::ion::Sym::new("$490"))
        .unwrap();
    let s = format!("{:?}", md.value);
    assert!(s.contains("B0") && s.contains("DECK"), "{s}");
    assert!(s.contains("PDOC"));
    assert_eq!(report.pages, Some(45));
    assert!(report.warnings.is_empty(), "{:?}", report.warnings);
    for stage in ["inspect", "normalize", "convert", "repackage", "deliver"] {
        assert!(
            report.stage_ms.contains_key(stage),
            "missing timing for {stage}"
        );
    }
    // the JSON report round-trips
    let json = serde_json::to_string(&report).unwrap();
    assert!(json.contains("\"title\":\"Deckle Mini Fixture\""));
}

#[test]
fn missing_previewer_fails_before_any_work() {
    let out = tempfile::tempdir().unwrap();
    let opts = RunOptions {
        cde_type: CdeType::Pdoc,
        asin: None,
        pages: None,
        target: Target::Folder(out.path().to_path_buf()),
        previewer: Some(PathBuf::from("/nonexistent/previewer")),
        keep_work_dir: false,
        cde_override: None,
        amazon_match: false,
        verified_real_asin: false,
        xray: XRayMode::Auto,
        llm: None,
        wikipedia: false,
        word_wise: None,
    };
    let report = run(&deckle_kfx::fixtures::path("deckle-mini.epub"), &opts);
    let err = report.error.expect("error");
    assert!(err.contains("Kindle Previewer is not installed"), "{err}");
    assert!(err.contains("G202131170"), "{err}");
    assert!(
        report.stage_ms.is_empty(),
        "no stage should have run: {:?}",
        report.stage_ms
    );
    assert_eq!(std::fs::read_dir(out.path()).unwrap().count(), 0);
}

#[test]
#[cfg(unix)] // the fake Previewer is a shell script
fn enhanced_typesetting_refusal_is_reported_not_downgraded() {
    let out = tempfile::tempdir().unwrap();
    let opts = RunOptions {
        cde_type: CdeType::Pdoc,
        asin: None,
        pages: None,
        target: Target::Folder(out.path().to_path_buf()),
        previewer: Some(fake("unsupported.sh")),
        keep_work_dir: false,
        cde_override: None,
        amazon_match: false,
        verified_real_asin: false,
        xray: XRayMode::Auto,
        llm: None,
        wikipedia: false,
        word_wise: None,
    };
    let report = run(&deckle_kfx::fixtures::path("deckle-mini.epub"), &opts);
    let err = report.error.expect("error");
    assert!(err.starts_with("convert: "), "{err}");
    assert!(
        err.contains("Fixed layout books are not supported"),
        "{err}"
    );
    assert!(report.delivered.is_none());
}

#[cfg(unix)]
fn docx_fixture() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../fixtures/docx/deckle-mini.docx")
}

#[test]
#[cfg(unix)] // the fake Previewer is a shell script
fn a_word_document_runs_through_the_whole_pipeline() {
    let out = tempfile::tempdir().unwrap();
    let opts = RunOptions {
        cde_type: CdeType::Pdoc,
        asin: None,
        pages: None,
        target: Target::Folder(out.path().to_path_buf()),
        previewer: Some(fake("ok.sh")),
        keep_work_dir: true,
        cde_override: None,
        amazon_match: false,
        verified_real_asin: false,
        xray: XRayMode::Off,
        llm: None,
        wikipedia: false,
        word_wise: None,
    };
    let report = run(&docx_fixture(), &opts);
    assert_eq!(report.error, None, "{report:?}");
    assert_eq!(report.input, docx_fixture());
    // Metadata comes from the document, through the imported EPUB.
    assert_eq!(report.info.title, "Deckle Mini Document");
    assert_eq!(report.info.creators[0].name, "Jane Testwright");
    assert_eq!(report.info.toc.len(), 4);
    let delivered = report.delivered.clone().expect("delivered");
    // (the fake Previewer always returns the mini fixture's KPF, so the name is not checked)
    assert_eq!(delivered.parent(), Some(out.path()));
    assert!(std::fs::read(&delivered).unwrap().starts_with(b"CONT"));
    for stage in [
        "import",
        "inspect",
        "normalize",
        "convert",
        "repackage",
        "deliver",
    ] {
        assert!(report.stage_ms.contains_key(stage), "missing {stage}");
    }
    let work = report.work_dir.clone().expect("kept work dir");
    assert!(work.join("imported.epub").is_file());
    std::fs::remove_dir_all(work).ok();
}

#[test]
#[cfg(unix)] // the fake Previewer is a shell script
fn a_broken_word_document_fails_at_import() {
    let dir = tempfile::tempdir().unwrap();
    let bad = dir.path().join("notes.docx");
    std::fs::write(&bad, b"not really a document").unwrap();
    let opts = RunOptions {
        cde_type: CdeType::Pdoc,
        asin: None,
        pages: None,
        target: Target::Folder(dir.path().to_path_buf()),
        previewer: Some(fake("ok.sh")),
        keep_work_dir: false,
        cde_override: None,
        amazon_match: false,
        verified_real_asin: false,
        xray: XRayMode::Off,
        llm: None,
        wikipedia: false,
        word_wise: None,
    };
    let report = run(&bad, &opts);
    let err = report.error.expect("error");
    assert_eq!(
        err,
        "import: not a Word document: the file is not a zip archive"
    );
    assert!(!report.stage_ms.contains_key("inspect"));
}

#[test]
fn size_warning_text() {
    let mut r = RunReport::default();
    r.push_size_warning(25_467_398);
    assert_eq!(
        r.warnings,
        vec!["KFX is 25.5 MB; some devices lock up above 16 MB".to_string()]
    );
    let mut small = RunReport::default();
    small.push_size_warning(10 * 1024 * 1024);
    assert!(small.warnings.is_empty());
}

/// Real Previewer end to end, opt-in.
#[test]
fn real_previewer_end_to_end() {
    if std::env::var("DECKLE_PREVIEWER_REAL").ok().as_deref() != Some("1") {
        return;
    }
    let out = tempfile::tempdir().unwrap();
    let opts = RunOptions {
        cde_type: CdeType::Pdoc,
        asin: None,
        pages: Some(None),
        target: Target::Folder(out.path().to_path_buf()),
        previewer: None,
        keep_work_dir: false,
        cde_override: None,
        amazon_match: false,
        verified_real_asin: false,
        xray: XRayMode::Auto,
        llm: None,
        wikipedia: false,
        word_wise: None,
    };
    let report = run(&deckle_kfx::fixtures::path("deckle-mini.epub"), &opts);
    assert_eq!(report.error, None, "{report:?}");
    assert!(report.delivered.unwrap().exists());
}

#[test]
#[cfg(unix)] // the fake Previewer is a shell script
fn missing_media_notice_from_previewer_reaches_the_report() {
    let out = tempfile::tempdir().unwrap();
    let opts = RunOptions {
        cde_type: CdeType::Pdoc,
        asin: None,
        pages: None,
        target: Target::Folder(out.path().to_path_buf()),
        previewer: Some(fake("notice.sh")),
        keep_work_dir: false,
        cde_override: None,
        amazon_match: false,
        verified_real_asin: false,
        xray: XRayMode::Auto,
        llm: None,
        wikipedia: false,
        word_wise: None,
    };
    let report = run(&deckle_kfx::fixtures::path("deckle-mini.epub"), &opts);
    assert_eq!(report.error, None);
    assert!(
        report.warnings.iter().any(|w| w.contains("W14010")),
        "{:?}",
        report.warnings
    );
}

#[test]
#[cfg(unix)] // the fake Previewer is a shell script
fn run_with_progress_reports_each_stage_in_order() {
    use deckle_core::pipeline::run_with_progress;
    let out = tempfile::tempdir().unwrap();
    let opts = RunOptions {
        cde_type: CdeType::Pdoc,
        asin: None,
        pages: None,
        target: Target::Folder(out.path().to_path_buf()),
        previewer: Some(fake("ok.sh")),
        keep_work_dir: false,
        cde_override: None,
        amazon_match: false,
        verified_real_asin: false,
        xray: XRayMode::Auto,
        llm: None,
        wikipedia: false,
        word_wise: None,
    };
    let mut seen: Vec<&'static str> = Vec::new();
    let report = run_with_progress(
        &deckle_kfx::fixtures::path("deckle-mini.epub"),
        &opts,
        &mut |stage| seen.push(stage),
    );
    assert_eq!(report.error, None);
    assert_eq!(
        seen,
        ["inspect", "normalize", "convert", "repackage", "deliver"],
        "PDOC books get no enrich stage"
    );
}

#[test]
#[cfg(unix)]
fn ebok_runs_enrich_and_ships_the_xray_sidecar() {
    use deckle_core::pipeline::run_with_progress;
    let out = tempfile::tempdir().unwrap();
    let opts = RunOptions {
        cde_type: CdeType::Ebok,
        cde_override: Some(CdeType::Ebok),
        asin: None,
        pages: None,
        target: Target::Folder(out.path().to_path_buf()),
        previewer: Some(fake("ok.sh")),
        keep_work_dir: false,
        amazon_match: false,
        verified_real_asin: false,
        xray: XRayMode::Auto,
        llm: None,
        wikipedia: false,
        word_wise: None,
    };
    let mut seen: Vec<&'static str> = Vec::new();
    let report = run_with_progress(
        &deckle_kfx::fixtures::path("deckle-mini.epub"),
        &opts,
        &mut |stage| seen.push(stage),
    );
    assert_eq!(report.error, None, "{report:?}");
    assert_eq!(
        seen,
        [
            "inspect",
            "normalize",
            "convert",
            "repackage",
            "enrich",
            "deliver"
        ]
    );
    // the mini fixture is chapter one of Pride and Prejudice: three recurring names
    let x = report.xray.as_ref().expect("xray summary");
    assert_eq!((x.people, x.terms, x.source.as_str()), (3, 0, "basic"));
    let asin = report.asin.clone().unwrap();
    let sdr = report.delivered.clone().unwrap().with_extension("sdr");
    let asc = sdr.join(format!("XRAY.entities.{asin}.asc"));
    assert!(asc.exists(), "{asc:?}");
    let db = rusqlite::Connection::open(&asc).unwrap();
    let labels: Vec<String> = db
        .prepare("SELECT label FROM entity WHERE id > 0 ORDER BY label")
        .unwrap()
        .query_map([], |r| r.get(0))
        .unwrap()
        .map(Result::unwrap)
        .collect();
    assert_eq!(labels, ["Mr. Bennet", "Mr. Bingley", "Mrs. Bennet"]);
    let mut off = opts.clone();
    off.xray = XRayMode::Off;
    let mut seen: Vec<&'static str> = Vec::new();
    run_with_progress(
        &deckle_kfx::fixtures::path("deckle-mini.epub"),
        &off,
        &mut |s| seen.push(s),
    );
    assert!(!seen.contains(&"enrich"));
}

#[test]
fn run_with_progress_stops_at_the_failing_stage() {
    use deckle_core::pipeline::run_with_progress;
    let out = tempfile::tempdir().unwrap();
    let opts = RunOptions {
        cde_type: CdeType::Pdoc,
        asin: None,
        pages: None,
        target: Target::Folder(out.path().to_path_buf()),
        previewer: Some(PathBuf::from("/nonexistent/previewer")),
        keep_work_dir: false,
        cde_override: None,
        amazon_match: false,
        verified_real_asin: false,
        xray: XRayMode::Auto,
        llm: None,
        wikipedia: false,
        word_wise: None,
    };
    let mut seen: Vec<&'static str> = Vec::new();
    let report = run_with_progress(
        &deckle_kfx::fixtures::path("deckle-mini.epub"),
        &opts,
        &mut |stage| seen.push(stage),
    );
    assert!(report.error.is_some());
    assert!(seen.is_empty());
}

#[test]
#[cfg(unix)]
fn mass_storage_target_gets_ebok_and_a_thumbnail_by_the_cover_rule() {
    use deckle_core::cover_rule::AsinChoice;
    let root = tempfile::tempdir().unwrap();
    std::fs::create_dir_all(root.path().join("documents")).unwrap();
    std::fs::create_dir_all(root.path().join("system/thumbnails")).unwrap();
    std::fs::write(
        root.path().join("system/version.txt"),
        "Kindle 5.16.2.1.1 (4184940002)\n",
    )
    .unwrap();
    let device = deckle_core::deliver::detect_in(&[root.path().to_path_buf()]).remove(0);
    let opts = RunOptions {
        cde_type: CdeType::Pdoc, // ignored: the rule decides unless cde_override is set
        cde_override: None,
        asin: None,
        pages: None,
        target: Target::MassStorage(device),
        previewer: Some(fake("ok.sh")),
        keep_work_dir: false,
        amazon_match: false,
        verified_real_asin: false,
        xray: XRayMode::Auto,
        llm: None,
        wikipedia: false,
        word_wise: None,
    };
    let report = run(&deckle_kfx::fixtures::path("deckle-mini.epub"), &opts);
    assert_eq!(report.error, None, "{report:?}");
    let decision = report.decision.as_ref().expect("decision");
    assert_eq!(decision.cde, CdeType::Ebok);
    assert!(decision.write_thumbnail);
    assert_eq!(decision.asin, AsinChoice::Generated);
    assert_eq!(report.cde_type.as_deref(), Some("EBOK"));
    assert_eq!(report.firmware.as_deref(), Some("5.16.2.1.1"));
    assert!(
        !report.warnings.iter().any(|w| w.contains("over MTP")),
        "no MTP rows on a mass-storage Kindle: {:?}",
        report.warnings
    );
    let asin = report.asin.clone().unwrap();
    let thumb = root.path().join(format!(
        "system/thumbnails/thumbnail_{asin}_EBOK_portrait.jpg"
    ));
    assert!(thumb.exists(), "{thumb:?}");
    let img = image::load_from_memory(&std::fs::read(&thumb).unwrap()).unwrap();
    assert_eq!(img.height(), 500);
}

#[test]
#[cfg(unix)]
fn folder_target_is_pdoc_and_an_override_wins() {
    let out = tempfile::tempdir().unwrap();
    let mut opts = RunOptions {
        cde_type: CdeType::Pdoc,
        cde_override: None,
        asin: None,
        pages: None,
        target: Target::Folder(out.path().to_path_buf()),
        previewer: Some(fake("ok.sh")),
        keep_work_dir: false,
        amazon_match: false,
        verified_real_asin: false,
        xray: XRayMode::Auto,
        llm: None,
        wikipedia: false,
        word_wise: None,
    };
    let r = run(&deckle_kfx::fixtures::path("deckle-mini.epub"), &opts);
    assert_eq!(r.decision.unwrap().cde, CdeType::Pdoc);
    opts.cde_override = Some(CdeType::Ebok);
    let r = run(&deckle_kfx::fixtures::path("deckle-mini.epub"), &opts);
    let d = r.decision.unwrap();
    assert_eq!(d.cde, CdeType::Ebok);
    assert!(d.reason.contains("override"));
}

/// A long read in `deckle digest run` is built into a folder and queued: the device is
/// unknown, so it ships as PDOC and X-Ray and Word Wise are never built (no `.sdr` to lose).
#[test]
#[cfg(unix)]
fn a_folder_build_asking_for_xray_and_word_wise_ships_no_sidecars() {
    let out = tempfile::tempdir().unwrap();
    let opts = RunOptions {
        cde_type: CdeType::Pdoc,
        cde_override: None,
        asin: None,
        pages: None,
        target: Target::Folder(out.path().to_path_buf()),
        previewer: Some(fake("ok.sh")),
        keep_work_dir: false,
        amazon_match: false,
        verified_real_asin: false,
        xray: XRayMode::Auto,
        llm: None,
        wikipedia: false,
        word_wise: Some(Default::default()),
    };
    let r = run(&deckle_kfx::fixtures::path("deckle-mini.epub"), &opts);
    assert_eq!(r.error, None, "{r:?}");
    assert_eq!(r.decision.as_ref().unwrap().cde, CdeType::Pdoc);
    assert!(r.xray.is_none() && r.word_wise.is_none(), "{r:?}");
    let kfx = r.delivered.unwrap();
    assert!(!kfx.with_extension("sdr").exists());
}

#[test]
fn a_failed_xray_is_a_warning_not_a_failed_run() {
    use deckle_core::deliver::DeliverOptions;
    use deckle_core::pipeline::attach_xray;
    let mut report = RunReport::default();
    let mut opts = DeliverOptions::default();
    attach_xray(&mut report, &mut opts, Err("no position map".into()));
    assert_eq!(report.error, None);
    assert!(
        report
            .warnings
            .iter()
            .any(|w| w.starts_with("X-Ray skipped: no position map")),
        "{:?}",
        report.warnings
    );
    assert!(opts.sidecars.is_empty());
    assert!(report.xray.is_none());
}

#[test]
#[cfg(unix)]
fn an_ebok_book_ships_a_word_wise_sidecar_and_deckles_dictionary() {
    use deckle_core::wordwise::WordWiseOptions;
    let tmp = tempfile::tempdir().unwrap();
    let db = tmp.path().join("ww.sqlite");
    let conn = deckle_wordwise::lexicon::create(&db, "test").unwrap();
    conn.execute_batch(
        "INSERT INTO senses VALUES (1, 'universally', 'adverb', 'everywhere', 'everywhere', NULL, 4);
         INSERT INTO forms VALUES ('universally', 1);",
    )
    .unwrap();
    drop(conn);
    let out = tmp.path().join("out");
    let opts = RunOptions {
        cde_type: CdeType::Pdoc,
        cde_override: Some(CdeType::Ebok),
        asin: Some("B0WWTESTDK".into()),
        pages: None,
        target: Target::Folder(out.clone()),
        previewer: Some(fake("ok.sh")),
        keep_work_dir: false,
        amazon_match: false,
        verified_real_asin: false,
        xray: XRayMode::Off,
        llm: None,
        wikipedia: false,
        word_wise: Some(WordWiseOptions {
            level: 5,
            database: Some(db),
            cache_dir: tmp.path().join("cache"),
        }),
    };
    let mut seen = Vec::new();
    let r = deckle_core::pipeline::run_with_progress(
        &deckle_kfx::fixtures::path("deckle-mini.epub"),
        &opts,
        &mut |s| seen.push(s),
    );
    assert_eq!(r.error, None, "{r:?}");
    assert!(seen.contains(&"enrich"));
    let ww = r.word_wise.expect("word wise summary");
    assert_eq!(ww.dictionary, "deckle");
    assert!(ww.hints >= 10, "{ww:?}"); // the fixture repeats the sentence
    let sdr = r.delivered.unwrap().with_extension("sdr");
    let kll = sdr.join("LanguageLayer.en.B0WWTESTDK.kll");
    let c = rusqlite::Connection::open(&kll).unwrap();
    let acr: String = c
        .query_row("SELECT value FROM metadata WHERE key = 'acr'", [], |r| {
            r.get(0)
        })
        .unwrap();
    assert!(!acr.is_empty());
    assert!(out.join("system/kll/kll.en.zh.klld").exists());
    assert!(r.xray.is_none());
}
