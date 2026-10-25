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
        preset: None,
        cover: Default::default(),
        cde_type: CdeType::Pdoc,
        asin: None,
        pages: Some(None),
        target: Target::Folder(out.path().to_path_buf()),
        previewer: Some(fake("ok.sh")),
        keep_work_dir: false,
        cde_override: None,
        amazon_match: false,
        generated_cover: Some(None),
        metadata_fill: false,
        typography: false,
        popup_notes: false,
        verified_real_asin: false,
        xray: XRayMode::Auto,
        llm: None,
        wikipedia: false,
        word_wise: None,
        replaces: Vec::new(),
        originals: None,
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
    assert_eq!(
        report.end_position,
        deckle_kfx::text::content_runs(&bytes).ok().map(|(_, e)| e),
        "pipeline_reports_end"
    );
    assert!(report.end_position.is_some());
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
        preset: None,
        cover: Default::default(),
        cde_type: CdeType::Pdoc,
        asin: None,
        pages: None,
        target: Target::Folder(out.path().to_path_buf()),
        previewer: Some(PathBuf::from("/nonexistent/previewer")),
        keep_work_dir: false,
        cde_override: None,
        amazon_match: false,
        generated_cover: Some(None),
        metadata_fill: false,
        typography: false,
        popup_notes: false,
        verified_real_asin: false,
        xray: XRayMode::Auto,
        llm: None,
        wikipedia: false,
        word_wise: None,
        replaces: Vec::new(),
        originals: None,
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
        preset: None,
        cover: Default::default(),
        cde_type: CdeType::Pdoc,
        asin: None,
        pages: None,
        target: Target::Folder(out.path().to_path_buf()),
        previewer: Some(fake("unsupported.sh")),
        keep_work_dir: false,
        cde_override: None,
        amazon_match: false,
        generated_cover: Some(None),
        metadata_fill: false,
        typography: false,
        popup_notes: false,
        verified_real_asin: false,
        xray: XRayMode::Auto,
        llm: None,
        wikipedia: false,
        word_wise: None,
        replaces: Vec::new(),
        originals: None,
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
        preset: None,
        cover: Default::default(),
        cde_type: CdeType::Pdoc,
        asin: None,
        pages: None,
        target: Target::Folder(out.path().to_path_buf()),
        previewer: Some(fake("ok.sh")),
        keep_work_dir: true,
        cde_override: None,
        amazon_match: false,
        generated_cover: Some(None),
        metadata_fill: false,
        typography: false,
        popup_notes: false,
        verified_real_asin: false,
        xray: XRayMode::Off,
        llm: None,
        wikipedia: false,
        word_wise: None,
        replaces: Vec::new(),
        originals: None,
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
        preset: None,
        cover: Default::default(),
        cde_type: CdeType::Pdoc,
        asin: None,
        pages: None,
        target: Target::Folder(dir.path().to_path_buf()),
        previewer: Some(fake("ok.sh")),
        keep_work_dir: false,
        cde_override: None,
        amazon_match: false,
        generated_cover: Some(None),
        metadata_fill: false,
        typography: false,
        popup_notes: false,
        verified_real_asin: false,
        xray: XRayMode::Off,
        llm: None,
        wikipedia: false,
        word_wise: None,
        replaces: Vec::new(),
        originals: None,
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
#[cfg(unix)] // the fake Previewer is a shell script
fn a_plain_text_file_runs_through_the_whole_pipeline() {
    let txt =
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../fixtures/txt/gutenberg-mini.txt");
    let out = tempfile::tempdir().unwrap();
    let opts = RunOptions {
        preset: None,
        cover: Default::default(),
        cde_type: CdeType::Pdoc,
        asin: None,
        pages: None,
        target: Target::Folder(out.path().to_path_buf()),
        previewer: Some(fake("ok.sh")),
        keep_work_dir: true,
        cde_override: None,
        amazon_match: false,
        generated_cover: Some(None),
        metadata_fill: false,
        typography: false,
        popup_notes: false,
        verified_real_asin: false,
        xray: XRayMode::Off,
        llm: None,
        wikipedia: false,
        word_wise: None,
        replaces: Vec::new(),
        originals: None,
    };
    let report = run(&txt, &opts);
    assert_eq!(report.error, None, "{report:?}");
    // Metadata comes from the Gutenberg header, through the imported EPUB.
    assert_eq!(report.info.title, "The Deckle Mini Novel");
    assert_eq!(report.info.creators[0].name, "Ada Testwright");
    assert_eq!(report.info.toc.len(), 6);
    let delivered = report.delivered.clone().expect("delivered");
    assert!(std::fs::read(&delivered).unwrap().starts_with(b"CONT"));
    assert!(report.stage_ms.contains_key("import"));
    let work = report.work_dir.clone().expect("kept work dir");
    assert!(work.join("imported.epub").is_file());
    std::fs::remove_dir_all(work).ok();
}

#[test]
#[cfg(unix)] // the fake Previewer is a shell script
fn a_text_pdf_runs_through_the_whole_pipeline_with_a_quality_note() {
    // Needs PDFium: tools/fetch-pdfium.sh, or DECKLE_PDFIUM.
    let pdf = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../fixtures/pdf/book.pdf");
    let out = tempfile::tempdir().unwrap();
    let opts = RunOptions {
        preset: None,
        cover: Default::default(),
        cde_type: CdeType::Pdoc,
        asin: None,
        pages: None,
        target: Target::Folder(out.path().to_path_buf()),
        previewer: Some(fake("ok.sh")),
        keep_work_dir: true,
        cde_override: None,
        amazon_match: false,
        generated_cover: Some(None),
        metadata_fill: false,
        typography: false,
        popup_notes: false,
        verified_real_asin: false,
        xray: XRayMode::Off,
        llm: None,
        wikipedia: false,
        word_wise: None,
        replaces: Vec::new(),
        originals: None,
    };
    let report = run(&pdf, &opts);
    assert_eq!(report.error, None, "{report:?}");
    // The imported EPUB passed inspect: metadata and contents came through it.
    assert_eq!(report.info.title, "A Small Book");
    assert_eq!(report.info.creators[0].name, "B. Writer");
    assert_eq!(report.info.toc.len(), 2);
    assert!(
        report
            .warnings
            .iter()
            .any(|w| w.contains("reflowed from a PDF")),
        "{:?}",
        report.warnings
    );
    assert!(report.delivered.is_some());
    let work = report.work_dir.clone().expect("kept work dir");
    assert!(work.join("imported.epub").is_file());
    std::fs::remove_dir_all(work).ok();
}

#[test]
#[cfg(unix)] // the fake Previewer is a shell script
fn an_empty_text_file_fails_at_import() {
    let dir = tempfile::tempdir().unwrap();
    let empty = dir.path().join("empty.txt");
    std::fs::write(&empty, b"\n\n").unwrap();
    let opts = RunOptions {
        preset: None,
        cover: Default::default(),
        cde_type: CdeType::Pdoc,
        asin: None,
        pages: None,
        target: Target::Folder(dir.path().to_path_buf()),
        previewer: Some(fake("ok.sh")),
        keep_work_dir: false,
        cde_override: None,
        amazon_match: false,
        generated_cover: Some(None),
        metadata_fill: false,
        typography: false,
        popup_notes: false,
        verified_real_asin: false,
        xray: XRayMode::Off,
        llm: None,
        wikipedia: false,
        word_wise: None,
        replaces: Vec::new(),
        originals: None,
    };
    let report = run(&empty, &opts);
    assert_eq!(
        report.error.as_deref(),
        Some("import: the text file is empty")
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
        preset: None,
        cover: Default::default(),
        cde_type: CdeType::Pdoc,
        asin: None,
        pages: Some(None),
        target: Target::Folder(out.path().to_path_buf()),
        previewer: None,
        keep_work_dir: false,
        cde_override: None,
        amazon_match: false,
        generated_cover: Some(None),
        metadata_fill: false,
        typography: false,
        popup_notes: false,
        verified_real_asin: false,
        xray: XRayMode::Auto,
        llm: None,
        wikipedia: false,
        word_wise: None,
        replaces: Vec::new(),
        originals: None,
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
        preset: None,
        cover: Default::default(),
        cde_type: CdeType::Pdoc,
        asin: None,
        pages: None,
        target: Target::Folder(out.path().to_path_buf()),
        previewer: Some(fake("notice.sh")),
        keep_work_dir: false,
        cde_override: None,
        amazon_match: false,
        generated_cover: Some(None),
        metadata_fill: false,
        typography: false,
        popup_notes: false,
        verified_real_asin: false,
        xray: XRayMode::Auto,
        llm: None,
        wikipedia: false,
        word_wise: None,
        replaces: Vec::new(),
        originals: None,
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
        preset: None,
        cover: Default::default(),
        cde_type: CdeType::Pdoc,
        asin: None,
        pages: None,
        target: Target::Folder(out.path().to_path_buf()),
        previewer: Some(fake("ok.sh")),
        keep_work_dir: false,
        cde_override: None,
        amazon_match: false,
        generated_cover: Some(None),
        metadata_fill: false,
        typography: false,
        popup_notes: false,
        verified_real_asin: false,
        xray: XRayMode::Auto,
        llm: None,
        wikipedia: false,
        word_wise: None,
        replaces: Vec::new(),
        originals: None,
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
        preset: None,
        cover: Default::default(),
        cde_type: CdeType::Ebok,
        cde_override: Some(CdeType::Ebok),
        asin: None,
        pages: None,
        target: Target::Folder(out.path().to_path_buf()),
        previewer: Some(fake("ok.sh")),
        keep_work_dir: false,
        amazon_match: false,
        generated_cover: Some(None),
        metadata_fill: false,
        typography: false,
        popup_notes: false,
        verified_real_asin: false,
        xray: XRayMode::Auto,
        llm: None,
        wikipedia: false,
        word_wise: None,
        replaces: Vec::new(),
        originals: None,
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
        preset: None,
        cover: Default::default(),
        cde_type: CdeType::Pdoc,
        asin: None,
        pages: None,
        target: Target::Folder(out.path().to_path_buf()),
        previewer: Some(PathBuf::from("/nonexistent/previewer")),
        keep_work_dir: false,
        cde_override: None,
        amazon_match: false,
        generated_cover: Some(None),
        metadata_fill: false,
        typography: false,
        popup_notes: false,
        verified_real_asin: false,
        xray: XRayMode::Auto,
        llm: None,
        wikipedia: false,
        word_wise: None,
        replaces: Vec::new(),
        originals: None,
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
        preset: None,
        cover: Default::default(),
        cde_type: CdeType::Pdoc, // ignored: the rule decides unless cde_override is set
        cde_override: None,
        asin: None,
        pages: None,
        target: Target::MassStorage(device),
        previewer: Some(fake("ok.sh")),
        keep_work_dir: false,
        amazon_match: false,
        generated_cover: Some(None),
        metadata_fill: false,
        typography: false,
        popup_notes: false,
        verified_real_asin: false,
        xray: XRayMode::Auto,
        llm: None,
        wikipedia: false,
        word_wise: None,
        replaces: Vec::new(),
        originals: None,
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
        preset: None,
        cover: Default::default(),
        cde_type: CdeType::Pdoc,
        cde_override: None,
        asin: None,
        pages: None,
        target: Target::Folder(out.path().to_path_buf()),
        previewer: Some(fake("ok.sh")),
        keep_work_dir: false,
        amazon_match: false,
        generated_cover: Some(None),
        metadata_fill: false,
        typography: false,
        popup_notes: false,
        verified_real_asin: false,
        xray: XRayMode::Auto,
        llm: None,
        wikipedia: false,
        word_wise: None,
        replaces: Vec::new(),
        originals: None,
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
        preset: None,
        cover: Default::default(),
        cde_type: CdeType::Pdoc,
        cde_override: None,
        asin: None,
        pages: None,
        target: Target::Folder(out.path().to_path_buf()),
        previewer: Some(fake("ok.sh")),
        keep_work_dir: false,
        amazon_match: false,
        generated_cover: Some(None),
        metadata_fill: false,
        typography: false,
        popup_notes: false,
        verified_real_asin: false,
        xray: XRayMode::Auto,
        llm: None,
        wikipedia: false,
        word_wise: Some(Default::default()),
        replaces: Vec::new(),
        originals: None,
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
        preset: None,
        cover: Default::default(),
        cde_type: CdeType::Pdoc,
        cde_override: Some(CdeType::Ebok),
        asin: Some("B0WWTESTDK".into()),
        pages: None,
        target: Target::Folder(out.clone()),
        previewer: Some(fake("ok.sh")),
        keep_work_dir: false,
        amazon_match: false,
        generated_cover: Some(None),
        metadata_fill: false,
        typography: false,
        popup_notes: false,
        verified_real_asin: false,
        xray: XRayMode::Off,
        llm: None,
        wikipedia: false,
        word_wise: Some(WordWiseOptions {
            level: 5,
            database: Some(db),
            cache_dir: tmp.path().join("cache"),
        }),
        replaces: Vec::new(),
        originals: None,
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

/// Enrichment is source-agnostic (spec 2026-10-21-source-agnostic-enrichment): a Word document
/// built as EBOK gets X-Ray and Word Wise exactly like an EPUB, because `enrich` reads the KFX.
#[test]
#[cfg(unix)]
fn a_word_document_built_as_ebok_ships_xray_and_word_wise_sidecars() {
    use deckle_core::pipeline::run_with_progress;
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
        preset: None,
        cover: Default::default(),
        cde_type: CdeType::Ebok,
        cde_override: Some(CdeType::Ebok),
        asin: Some("B0DOCXTEST".into()),
        pages: None,
        target: Target::Folder(out.clone()),
        previewer: Some(fake("ok.sh")),
        keep_work_dir: false,
        amazon_match: false,
        generated_cover: Some(None),
        metadata_fill: false,
        typography: false,
        popup_notes: false,
        verified_real_asin: false,
        xray: XRayMode::Auto,
        llm: None,
        wikipedia: false,
        word_wise: Some(WordWiseOptions {
            level: 5,
            database: Some(db),
            cache_dir: tmp.path().join("cache"),
        }),
        replaces: Vec::new(),
        originals: None,
    };
    let mut seen: Vec<&'static str> = Vec::new();
    let r = run_with_progress(&docx_fixture(), &opts, &mut |s| seen.push(s));
    assert_eq!(r.error, None, "{r:?}");
    assert_eq!(
        seen,
        [
            "import",
            "inspect",
            "normalize",
            "convert",
            "repackage",
            "enrich",
            "deliver"
        ]
    );
    // metadata is the document's, through the imported EPUB
    assert_eq!(r.info.title, "Deckle Mini Document");
    assert_eq!(r.decision.as_ref().unwrap().cde, CdeType::Ebok);
    // (the fake Previewer always returns the mini fixture's KPF: three recurring names)
    let x = r.xray.as_ref().expect("xray summary");
    assert_eq!((x.people, x.source.as_str()), (3, "basic"));
    let ww = r.word_wise.as_ref().expect("word wise summary");
    assert_eq!(ww.dictionary, "deckle");
    assert!(ww.hints >= 1, "{ww:?}");
    let sdr = r.delivered.clone().unwrap().with_extension("sdr");
    let asc = sdr.join("XRAY.entities.B0DOCXTEST.asc");
    assert!(asc.exists(), "{asc:?}");
    let kll = sdr.join("LanguageLayer.en.B0DOCXTEST.kll");
    assert!(kll.exists(), "{kll:?}");
    assert!(out.join("system/kll/kll.en.zh.klld").exists());
}

/// `kfx` with its container id and `kfxgen_payload_sha1` value zeroed.
#[cfg(unix)]
fn without_container_id(mut kfx: Vec<u8>) -> Vec<u8> {
    let id = deckle_kfx::container::read_kfx(&kfx).unwrap().container_id;
    let sha_key = b"kfxgen_payload_sha1\",value:\"";
    let mut blank = |needle: &[u8], skip: usize, len: usize| {
        let mut i = 0;
        while let Some(at) = kfx[i..].windows(needle.len()).position(|w| w == needle) {
            let from = i + at + skip;
            kfx[from..from + len].fill(b'0');
            i = from + len;
        }
    };
    blank(id.as_bytes(), 0, id.len());
    blank(sha_key, sha_key.len(), 40);
    kfx
}

/// Device-aware build §4.1: `run` is `finish(prepare(..))`, so splitting at the KPF changes
/// nothing a folder (or a Kindle) receives.
#[test]
#[cfg(unix)]
fn prepare_then_finish_to_a_folder_is_byte_identical_to_run() {
    use deckle_core::deliver::Conn;
    use deckle_core::pipeline::{finish, prepare};
    let tmp = tempfile::tempdir().unwrap();
    let (a, b) = (tmp.path().join("run"), tmp.path().join("split"));
    let opts = |dir: &std::path::Path| RunOptions {
        preset: None,
        cover: Default::default(),
        cde_type: CdeType::Pdoc,
        cde_override: None,
        asin: None,
        pages: Some(None),
        target: Target::Folder(dir.to_path_buf()),
        previewer: Some(fake("ok.sh")),
        keep_work_dir: false,
        amazon_match: false,
        generated_cover: Some(None),
        metadata_fill: false,
        typography: false,
        popup_notes: false,
        verified_real_asin: false,
        xray: XRayMode::Auto,
        llm: None,
        wikipedia: false,
        word_wise: None,
        replaces: Vec::new(),
        originals: None,
    };
    let epub = deckle_kfx::fixtures::path("deckle-mini.epub");
    let whole = run(&epub, &opts(&a));
    assert_eq!(whole.error, None, "{whole:?}");

    let split_opts = opts(&b);
    let mut stages = Vec::new();
    let prepared = prepare(&epub, &split_opts, &mut |s| stages.push(s)).expect("prepared");
    assert_eq!(stages, ["inspect", "normalize", "convert"]);
    assert!(prepared.kpf.exists());
    let report = finish(
        &prepared,
        &mut Conn::new(&split_opts.target),
        &split_opts,
        &mut |s| stages.push(s),
    );
    assert_eq!(report.error, None, "{report:?}");
    assert_eq!(
        stages,
        ["inspect", "normalize", "convert", "repackage", "deliver"]
    );
    let (x, y) = (whole.delivered.unwrap(), report.delivered.unwrap());
    assert_eq!(x.file_name(), y.file_name());
    // Every build draws a fresh random container id, and the payload digest covers it (two
    // `run`s differ there too): blank both, then every other byte must match.
    let (x, y) = (std::fs::read(&x).unwrap(), std::fs::read(&y).unwrap());
    assert_eq!(x.len(), y.len());
    assert!(
        without_container_id(x) == without_container_id(y),
        "the split build differs from run's"
    );
    assert_eq!(whole.asin, report.asin);
    assert_eq!(whole.pages, report.pages);
    assert_eq!(whole.warnings, report.warnings);
    assert_eq!(whole.info, report.info);
}

#[test]
#[cfg(unix)] // the fake Previewer is a shell script
fn a_send_replaces_deckles_older_copy_under_another_name() {
    use deckle_kfx::convert::{ConvertOptions, kpf_to_kfx};
    let out = tempfile::tempdir().unwrap();
    let epub = deckle_kfx::fixtures::path("deckle-mini.epub");
    let info = deckle_core::inspect::inspect(&epub).unwrap();
    let asin = deckle_kfx::metadata::generate_asin(&deckle_core::pipeline::asin_seed(&info));
    // Deckle's copy from before a title fix, with its sidecars; and a store book.
    let kpf = deckle_kfx::fixtures::path("deckle-mini.kpf");
    let old = kpf_to_kfx(
        &kpf,
        &ConvertOptions {
            asin: Some(asin.clone()),
            ..Default::default()
        },
    )
    .unwrap();
    std::fs::write(out.path().join("Old Title.kfx"), &old.bytes).unwrap();
    std::fs::create_dir_all(out.path().join("Old Title.sdr")).unwrap();
    std::fs::write(out.path().join("Old Title.sdr/x.asc"), b"x").unwrap();
    let store = std::fs::read(deckle_kfx::fixtures::path("deckle-mini-pdoc.golden.kfx")).unwrap();
    std::fs::write(out.path().join("Store.kfx"), &store).unwrap();
    // The library's record names one more (an MTP-style hint); one that's gone is ignored.
    std::fs::write(out.path().join("Renamed by hand.kfx"), &store).unwrap();
    let opts = RunOptions {
        preset: None,
        cover: Default::default(),
        cde_type: CdeType::Pdoc,
        asin: None,
        pages: None,
        target: Target::Folder(out.path().to_path_buf()),
        previewer: Some(fake("ok.sh")),
        keep_work_dir: false,
        cde_override: None,
        amazon_match: false,
        generated_cover: Some(None),
        metadata_fill: false,
        typography: false,
        popup_notes: false,
        verified_real_asin: false,
        xray: XRayMode::Off,
        llm: None,
        wikipedia: false,
        word_wise: None,
        replaces: vec!["Renamed by hand.kfx".into(), "Long gone.kfx".into()],
        originals: None,
    };
    let report = run(&epub, &opts);
    assert_eq!(report.error, None, "{report:?}");
    let mut replaced = report.replaced.clone();
    replaced.sort();
    assert_eq!(replaced, ["Old Title.kfx", "Renamed by hand.kfx"]);
    let mut left: Vec<String> = std::fs::read_dir(out.path())
        .unwrap()
        .flatten()
        .map(|e| e.file_name().to_string_lossy().into_owned())
        .collect();
    left.sort();
    assert_eq!(
        left,
        ["Store.kfx", "Testwright, Jane - Deckle Mini Fixture.kfx"]
    );
}

/// Options for the mini fixture through the fake Previewer to a folder, with the metadata fill
/// on or off.
#[cfg(unix)]
fn fill_opts(out: &std::path::Path, metadata_fill: bool) -> RunOptions {
    RunOptions {
        preset: None,
        cover: Default::default(),
        cde_type: CdeType::Pdoc,
        asin: None,
        pages: None,
        target: Target::Folder(out.to_path_buf()),
        previewer: Some(fake("ok.sh")),
        keep_work_dir: true,
        cde_override: None,
        amazon_match: false,
        generated_cover: Some(None),
        metadata_fill,
        typography: false,
        popup_notes: false,
        verified_real_asin: false,
        xray: XRayMode::Off,
        llm: None,
        wikipedia: false,
        word_wise: None,
        replaces: Vec::new(),
        originals: None,
    }
}

/// prepare (with `fetch` for the metadata fill) and finish to the folder.
#[cfg(unix)]
fn run_filling(
    opts: &RunOptions,
    fetch: &mut deckle_core::matcher::Fetcher,
) -> deckle_core::pipeline::RunReport {
    let prepared = deckle_core::pipeline::prepare_with(
        &deckle_kfx::fixtures::path("deckle-mini.epub"),
        opts,
        &mut |_| {},
        fetch,
    )
    .expect("prepared");
    deckle_core::pipeline::finish(
        &prepared,
        &mut deckle_core::deliver::Conn::new(&opts.target),
        opts,
        &mut |_| {},
    )
}

#[test]
#[cfg(unix)] // the fake Previewer is a shell script
fn with_the_metadata_fill_off_the_pipeline_makes_no_request() {
    let out = tempfile::tempdir().unwrap();
    let opts = fill_opts(out.path(), false);
    let report = run_filling(&opts, &mut |url: &str| -> Result<String, String> {
        panic!("the metadata fill is off but the pipeline fetched {url}")
    });
    assert_eq!(report.error, None, "{report:?}");
    assert!(report.delivered.is_some());
    assert_eq!(report.metadata_fill, None);
    assert!(!report.stage_ms.contains_key("fill"));
    assert_eq!(report.info.description, None);
}

#[test]
#[cfg(unix)] // the fake Previewer is a shell script
fn with_the_metadata_fill_on_empty_fields_reach_the_epub_previewer_converts() {
    let out = tempfile::tempdir().unwrap();
    let opts = fill_opts(out.path(), true);
    let mut urls = Vec::new();
    let report = run_filling(&opts, &mut |url: &str| {
        urls.push(url.to_owned());
        if url.starts_with("https://openlibrary.org/search.json?") {
            Ok(r#"{"docs":[{"key":"/works/OL1W","title":"Deckle Mini Fixture","author_name":["Jane Testwright"]}]}"#.into())
        } else if url == "https://openlibrary.org/works/OL1W.json" {
            Ok(r#"{"description":{"type":"/type/text","value":"A tiny book."},"subjects":["Testing"]}"#.into())
        } else {
            Err(format!("unexpected {url}"))
        }
    });
    assert_eq!(report.error, None, "{report:?}");
    assert_eq!(urls.len(), 2, "{urls:?}");
    assert_eq!(
        report.metadata_fill,
        Some(deckle_core::metadata_fill::FillOutcome::Filled {
            fields: vec!["description", "subjects"],
            source: "search",
        })
    );
    assert_eq!(report.info.description.as_deref(), Some("A tiny book."));
    // The book's own title, author and series are kept.
    assert_eq!(report.info.title, "Deckle Mini Fixture");
    assert_eq!(report.info.creators[0].name, "Jane Testwright");
    assert_eq!(report.info.series.as_ref().unwrap().name, "Fixture Series");
    let work = report.work_dir.clone().expect("kept work dir");
    let normalized = deckle_core::inspect::inspect(&work.join("normalized.epub")).unwrap();
    assert_eq!(normalized.description.as_deref(), Some("A tiny book."));
    assert_eq!(normalized.subjects, vec!["Testing".to_owned()]);
    assert!(report.stage_ms.contains_key("fill"));
    let _ = std::fs::remove_dir_all(work);
}

#[test]
#[cfg(unix)] // the fake Previewer is a shell script
fn a_failed_metadata_lookup_is_a_warning_and_the_book_still_converts() {
    for body in [
        Err::<String, String>("Connection refused".into()),
        Ok("not json".into()),
    ] {
        let out = tempfile::tempdir().unwrap();
        let opts = fill_opts(out.path(), true);
        let report = run_filling(&opts, &mut |_: &str| body.clone());
        assert_eq!(report.error, None, "{report:?}");
        assert!(report.delivered.as_ref().is_some_and(|d| d.is_file()));
        assert!(
            matches!(
                report.metadata_fill,
                Some(deckle_core::metadata_fill::FillOutcome::Error { .. })
            ),
            "{:?}",
            report.metadata_fill
        );
        assert!(
            report
                .warnings
                .iter()
                .any(|w| w.starts_with("metadata lookup failed: ")),
            "{:?}",
            report.warnings
        );
        assert_eq!(report.info.description, None);
        if let Some(w) = report.work_dir {
            let _ = std::fs::remove_dir_all(w);
        }
    }
}

/// The DOCX fixture through the pipeline with the work dir kept; returns the report and the
/// normalized EPUB's bytes.
#[cfg(unix)] // the fake Previewer is a shell script
fn docx_normalized(
    generated_cover: Option<Option<deckle_core::pipeline::CoverTemplate>>,
) -> (RunReport, Vec<u8>) {
    let out = tempfile::tempdir().unwrap();
    let opts = RunOptions {
        cover: Default::default(),
        generated_cover,
        keep_work_dir: true,
        ..fill_opts(out.path(), false)
    };
    let report = run(&docx_fixture(), &opts);
    assert_eq!(report.error, None, "{report:?}");
    let work = report.work_dir.clone().expect("kept work dir");
    let bytes = std::fs::read(work.join("normalized.epub")).unwrap();
    std::fs::remove_dir_all(work).ok();
    (report, bytes)
}

#[test]
#[cfg(unix)] // the fake Previewer is a shell script
fn a_word_document_gets_a_generated_cover() {
    let (report, epub) = docx_normalized(Some(None));
    let style = report.normalize.cover_generated.clone().expect("generated");
    assert!(
        style
            .parse::<deckle_core::pipeline::CoverTemplate>()
            .is_ok(),
        "{style}"
    );
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("n.epub");
    std::fs::write(&path, &epub).unwrap();
    let info = deckle_core::inspect::inspect(&path).unwrap();
    assert_eq!(info.cover_href.as_deref(), Some("deckle-cover.jpg"));
    // The same book always gets the same cover.
    assert_eq!(docx_normalized(Some(None)).1, epub);
    // An explicit style wins over the hash.
    let (r, _) = docx_normalized(Some(Some(deckle_core::pipeline::CoverTemplate::Rule)));
    assert_eq!(r.normalize.cover_generated.as_deref(), Some("rule"));
}

#[test]
#[cfg(unix)] // the fake Previewer is a shell script
fn with_generated_covers_off_a_word_document_normalizes_as_before() {
    let (report, epub) = docx_normalized(None);
    assert_eq!(report.normalize.cover_generated, None);
    // Today's output: the imported EPUB normalized with the default options.
    let dir = tempfile::tempdir().unwrap();
    let (imported, before) = (dir.path().join("i.epub"), dir.path().join("b.epub"));
    deckle_docx::convert(&docx_fixture(), &imported).unwrap();
    deckle_core::normalize::normalize(&imported, &before, &Default::default()).unwrap();
    assert_eq!(epub, std::fs::read(before).unwrap());
}

#[test]
#[cfg(unix)] // the fake Previewer is a shell script
fn an_epub_with_a_cover_normalizes_the_same_with_generated_covers_on_or_off() {
    let normalized = |generated_cover| {
        let out = tempfile::tempdir().unwrap();
        let opts = RunOptions {
            cover: Default::default(),
            generated_cover,
            keep_work_dir: true,
            ..fill_opts(out.path(), false)
        };
        let report = run(&deckle_kfx::fixtures::path("deckle-mini.epub"), &opts);
        assert_eq!(report.normalize.cover_generated, None);
        let work = report.work_dir.clone().expect("kept work dir");
        let bytes = std::fs::read(work.join("normalized.epub")).unwrap();
        std::fs::remove_dir_all(work).ok();
        bytes
    };
    assert_eq!(normalized(Some(None)), normalized(None));
}

#[test]
#[cfg(unix)] // the fake Previewer is a shell script
fn a_bound_book_runs_through_the_whole_pipeline() {
    use deckle_core::bind::{BindOptions, Part, bind};
    let tmp = tempfile::tempdir().unwrap();
    let txt = deckle_core::import::import(
        &PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../fixtures/txt/gutenberg-mini.txt"),
        tmp.path(),
        "txt.epub",
    )
    .unwrap()
    .unwrap();
    let bound = tmp.path().join("bound.epub");
    bind(
        &[
            Part {
                epub: deckle_kfx::fixtures::path("deckle-mini.epub"),
                title: None,
            },
            Part {
                epub: txt.epub,
                title: None,
            },
        ],
        &BindOptions {
            title: "Two in One".into(),
            author: None,
        },
        &bound,
    )
    .unwrap();
    let out = tempfile::tempdir().unwrap();
    let opts = RunOptions {
        preset: None,
        cover: Default::default(),
        cde_type: CdeType::Pdoc,
        asin: None,
        pages: None,
        target: Target::Folder(out.path().to_path_buf()),
        previewer: Some(fake("ok.sh")),
        keep_work_dir: false,
        cde_override: None,
        amazon_match: false,
        generated_cover: Some(None),
        metadata_fill: false,
        typography: false,
        popup_notes: false,
        verified_real_asin: false,
        xray: XRayMode::Off,
        llm: None,
        wikipedia: false,
        word_wise: None,
        replaces: Vec::new(),
        originals: None,
    };
    let report = run(&bound, &opts);
    assert_eq!(report.error, None, "{report:?}");
    assert_eq!(report.info.title, "Two in One");
    assert!(report.delivered.is_some());
}

#[test]
#[cfg(unix)]
fn markdown_file_and_folder_run_through_import_and_delivery() {
    let fixtures = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../fixtures/markdown");
    for (source, title) in [("mini.md", "Deckle Markdown Mini"), ("notes", "notes")] {
        let out = tempfile::tempdir().unwrap();
        let opts = fill_opts(out.path(), false);
        let mut stages = Vec::new();
        let r = deckle_core::pipeline::run_with_progress(&fixtures.join(source), &opts, &mut |s| {
            stages.push(s)
        });
        assert_eq!(r.error, None, "{r:?}");
        assert_eq!(r.info.title, title);
        assert_eq!(stages.first(), Some(&"import"));
        assert!(r.delivered.as_ref().unwrap().exists());
        assert!(r.work_dir.as_ref().unwrap().join("imported.epub").exists());
        if source == "notes" {
            assert!(r.warnings.iter().any(|w| w.contains("Missing")));
        }
        std::fs::remove_dir_all(r.work_dir.unwrap()).unwrap();
    }
}

#[test]
#[cfg(unix)]
#[ignore = "requires installed Kindle Previewer; run explicitly for import validation"]
fn markdown_real_previewer_smoke() {
    let fixtures = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../fixtures/markdown");
    for source in ["mini.md", "notes"] {
        let out = tempfile::tempdir().unwrap();
        let mut opts = fill_opts(out.path(), false);
        opts.previewer = None;
        opts.keep_work_dir = false;
        let report = run(&fixtures.join(source), &opts);
        assert_eq!(report.error, None, "{source}: {report:?}");
        let bytes = std::fs::read(report.delivered.unwrap()).unwrap();
        assert!(deckle_kfx::container::read_kfx(&bytes).is_ok());
    }
}

#[test]
#[cfg(unix)]
fn built_in_presets_reach_normalization_and_select_stage_defaults() {
    use deckle_core::preset::Preset;
    let out = tempfile::tempdir().unwrap();
    for preset in [Preset::Academic, Preset::Novel, Preset::Manual] {
        let mut opts = RunOptions {
            preset: None,
            cover: Default::default(),
            cde_type: CdeType::Pdoc,
            asin: None,
            pages: Some(None),
            target: Target::Folder(out.path().to_path_buf()),
            previewer: Some(fake("ok.sh")),
            keep_work_dir: false,
            cde_override: None,
            amazon_match: false,
            generated_cover: Some(None),
            metadata_fill: false,
            typography: false,
            popup_notes: false,
            verified_real_asin: false,
            xray: XRayMode::Auto,
            llm: None,
            wikipedia: false,
            word_wise: None,
            replaces: Vec::new(),
            originals: None,
        };
        preset.apply(&mut opts);
        assert_eq!(opts.pages, Some(None));
        assert_eq!(
            opts.xray,
            if preset == Preset::Novel {
                XRayMode::Auto
            } else {
                XRayMode::Off
            }
        );
        assert!(opts.word_wise.is_none());
        assert!(!opts.amazon_match && !opts.metadata_fill && !opts.wikipedia && opts.llm.is_none());
        assert!(opts.cde_override.is_none());
        let report = run(&deckle_kfx::fixtures::path("deckle-mini.epub"), &opts);
        assert_eq!(report.error, None, "{report:?}");
        assert_eq!(report.normalize.style_preset, Some(preset));
        assert_eq!(
            report.normalize.typography.is_some(),
            preset != Preset::Manual
        );
        assert_eq!(report.normalize.notes.is_some(), preset != Preset::Manual);
        assert!(report.pages.is_some());
    }
}

#[test]
#[cfg(unix)]
fn fb2_and_zipped_fb2_run_through_import_and_delivery() {
    let fixtures = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../fixtures/fb2");
    for source in ["mini.fb2", "mini.fb2.zip"] {
        let out = tempfile::tempdir().unwrap();
        let opts = fill_opts(out.path(), false);
        let mut stages = Vec::new();
        let r = deckle_core::pipeline::run_with_progress(&fixtures.join(source), &opts, &mut |s| {
            stages.push(s)
        });
        assert_eq!(r.error, None, "{r:?}");
        assert_eq!(r.info.title, "The Deckle FB2 Mini");
        let creators: Vec<&str> = r.info.creators.iter().map(|c| c.name.as_str()).collect();
        assert_eq!(creators, ["Ada K. Testwright", "Typesetter"]);
        let series = r.info.series.as_ref().unwrap();
        assert_eq!(
            (series.name.as_str(), series.index),
            ("Deckle Fixtures", Some(2.0))
        );
        assert!(r.info.cover_href.is_some());
        assert_eq!(stages.first(), Some(&"import"));
        assert!(r.delivered.as_ref().unwrap().exists());
        std::fs::remove_dir_all(r.work_dir.unwrap()).unwrap();
    }
}

#[test]
#[cfg(unix)]
#[ignore = "requires installed Kindle Previewer; run explicitly for import validation"]
fn fb2_real_previewer_smoke() {
    let fixtures = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../fixtures/fb2");
    for source in ["mini.fb2", "cp1251.fb2"] {
        let out = tempfile::tempdir().unwrap();
        let mut opts = fill_opts(out.path(), false);
        opts.previewer = None;
        opts.keep_work_dir = false;
        let report = run(&fixtures.join(source), &opts);
        assert_eq!(report.error, None, "{source}: {report:?}");
        let bytes = std::fs::read(report.delivered.unwrap()).unwrap();
        assert!(deckle_kfx::container::read_kfx(&bytes).is_ok());
    }
}

#[test]
#[cfg(unix)]
fn odt_and_flat_odt_run_through_import_and_delivery() {
    let fixtures = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../fixtures/odt");
    for source in ["mini.odt", "mini.fodt"] {
        let out = tempfile::tempdir().unwrap();
        let opts = fill_opts(out.path(), false);
        let mut stages = Vec::new();
        let r = deckle_core::pipeline::run_with_progress(&fixtures.join(source), &opts, &mut |s| {
            stages.push(s)
        });
        assert_eq!(r.error, None, "{r:?}");
        assert_eq!(r.info.title, "The Deckle ODT Mini");
        let creators: Vec<&str> = r.info.creators.iter().map(|c| c.name.as_str()).collect();
        assert_eq!(creators, ["Ada Testwright"]);
        assert_eq!(r.info.language, "en-US");
        assert_eq!(stages.first(), Some(&"import"));
        assert!(r.delivered.as_ref().unwrap().exists());
        std::fs::remove_dir_all(r.work_dir.unwrap()).unwrap();
    }
}

#[test]
#[cfg(unix)]
#[ignore = "requires installed Kindle Previewer; run explicitly for import validation"]
fn odt_real_previewer_smoke() {
    let fixtures = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../fixtures/odt");
    for source in ["mini.odt", "mini.fodt"] {
        let out = tempfile::tempdir().unwrap();
        let mut opts = fill_opts(out.path(), false);
        opts.previewer = None;
        opts.keep_work_dir = false;
        let report = run(&fixtures.join(source), &opts);
        assert_eq!(report.error, None, "{source}: {report:?}");
        let bytes = std::fs::read(report.delivered.unwrap()).unwrap();
        assert!(deckle_kfx::container::read_kfx(&bytes).is_ok());
    }
}

#[test]
#[cfg(unix)]
fn rtf_runs_through_import_and_delivery() {
    let fixtures = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../fixtures/rtf");
    let out = tempfile::tempdir().unwrap();
    let opts = fill_opts(out.path(), false);
    let mut stages = Vec::new();
    let r = deckle_core::pipeline::run_with_progress(&fixtures.join("mini.rtf"), &opts, &mut |s| {
        stages.push(s)
    });
    assert_eq!(r.error, None, "{r:?}");
    assert_eq!(r.info.title, "The Deckle RTF Mini");
    assert_eq!(r.info.creators[0].name, "Ada Testwright");
    assert_eq!(r.info.language, "en-GB");
    assert_eq!(stages.first(), Some(&"import"));
    assert!(r.delivered.as_ref().unwrap().exists());
    std::fs::remove_dir_all(r.work_dir.unwrap()).unwrap();
}

#[test]
#[cfg(unix)]
#[ignore = "requires installed Kindle Previewer; run explicitly for import validation"]
fn rtf_real_previewer_smoke() {
    let fixtures = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../fixtures/rtf");
    for source in ["mini.rtf", "cyrillic.rtf", "japanese.rtf"] {
        let out = tempfile::tempdir().unwrap();
        let mut opts = fill_opts(out.path(), false);
        opts.previewer = None;
        opts.keep_work_dir = false;
        let report = run(&fixtures.join(source), &opts);
        assert_eq!(report.error, None, "{source}: {report:?}");
        let bytes = std::fs::read(report.delivered.unwrap()).unwrap();
        assert!(deckle_kfx::container::read_kfx(&bytes).is_ok());
    }
}

#[test]
#[cfg(unix)]
fn kindle_books_run_through_import_and_delivery() {
    let fixtures = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../fixtures/mobi");
    for (source, title) in [
        ("mini6.mobi", "The Deckle MOBI Mini"),
        ("huff.mobi", "The Deckle MOBI Mini"),
        ("mini.azw3", "The Deckle AZW3 Mini"),
    ] {
        let out = tempfile::tempdir().unwrap();
        let opts = fill_opts(out.path(), false);
        let mut stages = Vec::new();
        let r = deckle_core::pipeline::run_with_progress(&fixtures.join(source), &opts, &mut |s| {
            stages.push(s)
        });
        assert_eq!(r.error, None, "{source}: {r:?}");
        assert_eq!(r.info.title, title);
        assert_eq!(r.info.creators[0].name, "Ada Testwright");
        assert_eq!(stages.first(), Some(&"import"));
        assert!(r.delivered.as_ref().unwrap().exists());
        std::fs::remove_dir_all(r.work_dir.unwrap()).unwrap();
    }
}

#[test]
#[cfg(unix)]
fn a_drm_protected_kindle_book_stops_at_import() {
    let out = tempfile::tempdir().unwrap();
    let src = out.path().join("store.azw3");
    std::fs::write(
        &src,
        deckle_mobi::fixture::build(&deckle_mobi::fixture::Book {
            title: "Store",
            drm: true,
            kf8: true,
            ..Default::default()
        }),
    )
    .unwrap();
    let opts = fill_opts(out.path(), false);
    let r = run(&src, &opts);
    let err = r.error.expect("refused");
    assert!(err.contains("never removes DRM"), "{err}");
    assert!(r.delivered.is_none());
}

#[test]
#[cfg(unix)]
#[ignore = "requires installed Kindle Previewer; run explicitly for import validation"]
fn mobi_real_previewer_smoke() {
    let fixtures = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../fixtures/mobi");
    for source in ["mini6.mobi", "huff.mobi", "mini.azw3"] {
        let out = tempfile::tempdir().unwrap();
        let mut opts = fill_opts(out.path(), false);
        opts.previewer = None;
        opts.keep_work_dir = false;
        let report = run(&fixtures.join(source), &opts);
        assert_eq!(report.error, None, "{source}: {report:?}");
        let bytes = std::fs::read(report.delivered.unwrap()).unwrap();
        assert!(deckle_kfx::container::read_kfx(&bytes).is_ok());
    }
}
