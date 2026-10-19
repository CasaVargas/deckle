// SPDX-License-Identifier: GPL-3.0-or-later
use std::path::PathBuf;

use deckle_core::previewer::{ConvertError, PREVIEWER_DOWNLOAD_URL, convert, locate, locate_in};

fn fake(name: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fake_previewer")
        .join(name)
}

#[test]
fn fake_previewer_converts_to_kpf_with_enhanced_typesetting() {
    let p = locate(Some(&fake("ok.sh"))).unwrap();
    assert_eq!(p.version, "4.0.3");
    assert_eq!(p.major, 4);
    let out = tempfile::tempdir().unwrap();
    let work = out.path().join("work"); // does not exist yet: convert must create it
    let r = convert(&p, &deckle_kfx::fixtures::path("deckle-mini.epub"), &work).unwrap();
    assert!(r.et_supported);
    assert!(r.kpf.ends_with("deckle-mini_epub.kpf"), "{:?}", r.kpf);
    assert!(r.kpf.exists());
    assert!(r.log_errors.is_empty());
}

#[test]
fn enhanced_typesetting_refusal_reports_the_first_error_row() {
    let p = locate(Some(&fake("unsupported.sh"))).unwrap();
    let out = tempfile::tempdir().unwrap();
    match convert(
        &p,
        &deckle_kfx::fixtures::path("deckle-mini.epub"),
        out.path(),
    ) {
        Err(ConvertError::EnhancedTypesettingUnsupported { first_error }) => {
            assert!(
                first_error.contains("Fixed layout books are not supported"),
                "{first_error}"
            );
        }
        other => panic!("expected EnhancedTypesettingUnsupported, got {other:?}"),
    }
}

#[test]
fn missing_previewer_names_the_download_url() {
    let empty = tempfile::tempdir().unwrap();
    match locate_in(&[empty.path().to_path_buf()], None) {
        Err(ConvertError::PreviewerMissing { download_url }) => {
            assert_eq!(download_url, PREVIEWER_DOWNLOAD_URL);
            assert_eq!(
                download_url,
                "https://kdp.amazon.com/en_US/help/topic/G202131170"
            );
        }
        other => panic!("expected PreviewerMissing, got {other:?}"),
    }
}

#[test]
fn override_path_that_does_not_exist_is_missing_too() {
    assert!(matches!(
        locate(Some(&PathBuf::from("/nonexistent/KindlePreviewer4CLI"))),
        Err(ConvertError::PreviewerMissing { .. })
    ));
}

/// Real Kindle Previewer, when installed and opted in with DECKLE_PREVIEWER_REAL=1.
#[test]
fn real_previewer_converts_the_mini_epub() {
    if std::env::var("DECKLE_PREVIEWER_REAL").ok().as_deref() != Some("1") {
        eprintln!(
            "skipped: set DECKLE_PREVIEWER_REAL=1 to run against the installed Kindle Previewer"
        );
        return;
    }
    let p = locate(None).expect("Kindle Previewer installed");
    let out = tempfile::tempdir().unwrap();
    let r = convert(
        &p,
        &deckle_kfx::fixtures::path("deckle-mini.epub"),
        out.path(),
    )
    .unwrap();
    assert!(r.et_supported);
    assert!(r.kpf.exists());
}
