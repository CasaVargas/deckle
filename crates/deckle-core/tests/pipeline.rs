// SPDX-License-Identifier: GPL-3.0-or-later
use std::path::PathBuf;

use deckle_core::deliver::Target;
use deckle_core::pipeline::{RunOptions, RunReport, run};
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
        ["inspect", "normalize", "convert", "repackage", "deliver"]
    );
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
    };
    let report = run(&deckle_kfx::fixtures::path("deckle-mini.epub"), &opts);
    assert_eq!(report.error, None, "{report:?}");
    let decision = report.decision.as_ref().expect("decision");
    assert_eq!(decision.cde, CdeType::Ebok);
    assert!(decision.write_thumbnail);
    assert_eq!(decision.asin, AsinChoice::Generated);
    assert_eq!(report.cde_type.as_deref(), Some("EBOK"));
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
    };
    let r = run(&deckle_kfx::fixtures::path("deckle-mini.epub"), &opts);
    assert_eq!(r.decision.unwrap().cde, CdeType::Pdoc);
    opts.cde_override = Some(CdeType::Ebok);
    let r = run(&deckle_kfx::fixtures::path("deckle-mini.epub"), &opts);
    let d = r.decision.unwrap();
    assert_eq!(d.cde, CdeType::Ebok);
    assert!(d.reason.contains("override"));
}
