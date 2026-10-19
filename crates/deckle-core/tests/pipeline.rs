// SPDX-License-Identifier: GPL-3.0-or-later
use std::path::PathBuf;

use deckle_core::deliver::Target;
use deckle_core::pipeline::{RunOptions, RunReport, run};
use deckle_kfx::convert::CdeType;

fn fake(name: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fake_previewer")
        .join(name)
}

#[test]
fn run_with_fake_previewer_delivers_a_kfx_to_a_folder() {
    let out = tempfile::tempdir().unwrap();
    let opts = RunOptions {
        cde_type: CdeType::Pdoc,
        asin: None,
        pages: Some(None),
        target: Target::Folder(out.path().to_path_buf()),
        previewer: Some(fake("ok.sh")),
        keep_work_dir: false,
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
fn enhanced_typesetting_refusal_is_reported_not_downgraded() {
    let out = tempfile::tempdir().unwrap();
    let opts = RunOptions {
        cde_type: CdeType::Pdoc,
        asin: None,
        pages: None,
        target: Target::Folder(out.path().to_path_buf()),
        previewer: Some(fake("unsupported.sh")),
        keep_work_dir: false,
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
    };
    let report = run(&deckle_kfx::fixtures::path("deckle-mini.epub"), &opts);
    assert_eq!(report.error, None, "{report:?}");
    assert!(report.delivered.unwrap().exists());
}
