// SPDX-License-Identifier: GPL-3.0-or-later
use std::path::PathBuf;

use deckle_core::previewer::{
    ConvertError, PREVIEWER_DOWNLOAD_URL, convert, locate, locate_in, windows_apps_candidates,
};

#[cfg(unix)]
fn fake(name: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fake_previewer")
        .join(name)
}

#[test]
#[cfg(unix)] // the fake Previewer is a shell script
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
#[cfg(unix)] // the fake Previewer is a shell script
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

#[test]
fn windows_apps_aliases_are_candidates_cli_first() {
    let dir = tempfile::tempdir().unwrap();
    for name in [
        "KindlePreviewer4.exe",
        "KindlePreviewer4CLI.exe",
        "python.exe",
        "kindle-notes.exe",
    ] {
        std::fs::write(dir.path().join(name), b"").unwrap();
    }
    let found = windows_apps_candidates(dir.path());
    let names: Vec<_> = found
        .iter()
        .map(|p| p.file_name().unwrap().to_string_lossy().into_owned())
        .collect();
    assert_eq!(names, ["KindlePreviewer4CLI.exe", "KindlePreviewer4.exe"]);
    assert!(windows_apps_candidates(&dir.path().join("absent")).is_empty());
}

#[test]
#[cfg(unix)]
fn override_that_exists_but_cannot_run_names_the_path() {
    let dir = tempfile::tempdir().unwrap();
    let p = dir.path().join("KindlePreviewer4CLI"); // not executable
    std::fs::write(&p, b"").unwrap();
    match locate_in(&[], Some(&p)) {
        Err(ConvertError::PreviewerUnlaunchable { path, .. }) => assert_eq!(path, p),
        other => panic!("expected PreviewerUnlaunchable, got {other:?}"),
    }
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

#[test]
#[cfg(unix)] // the fake Previewer is a shell script
fn outright_conversion_failure_is_distinct_and_carries_the_error_row() {
    let p = locate(Some(&fake("failed.sh"))).unwrap();
    let out = tempfile::tempdir().unwrap();
    match convert(
        &p,
        &deckle_kfx::fixtures::path("deckle-mini.epub"),
        out.path(),
    ) {
        Err(ConvertError::ConversionFailed { message }) => {
            assert!(message.contains("E23001"), "{message}");
        }
        other => panic!("expected ConversionFailed, got {other:?}"),
    }
}

#[test]
fn log_issues_keep_warnings_and_missing_media_notices() {
    use deckle_core::previewer::{IssueKind, read_log_issues};
    let dir = tempfile::tempdir().unwrap();
    let log = dir.path().join("x_log.csv");
    std::fs::write(&log, "\u{feff}\"Error: legend\"\n\"Notice: legend\"\n\"Type\",\"Description\",\"Source File\",\"Line Number\",\"Text Preceding Warning or Error\",\"Recommended Fix\"\n\"Error\",\"E1 bad\",\"a.xhtml\",\"3\",\"\",\"\"\n\"Warning\",\"W2 odd\",\"b.xhtml\",\"\",\"\",\"\"\n\"Notice\",\"W14010: media file not found Images/cover.png\",\"c.xhtml\",\"\",\"\",\"\"\n\"Notice\",\"I3 harmless\",\"\",\"\",\"\",\"\"\n").unwrap();
    let issues = read_log_issues(&log);
    let kinds: Vec<IssueKind> = issues.iter().map(|i| i.kind).collect();
    assert_eq!(
        kinds,
        [
            IssueKind::Error,
            IssueKind::Warning,
            IssueKind::Notice,
            IssueKind::Notice
        ]
    );
    assert_eq!(issues[0].message, "E1 bad (a.xhtml:3)");
    let surfaced: Vec<&str> = issues
        .iter()
        .filter(|i| i.should_surface())
        .map(|i| i.message.as_str())
        .collect();
    assert_eq!(
        surfaced,
        [
            "E1 bad (a.xhtml:3)",
            "W2 odd (b.xhtml)",
            "W14010: media file not found Images/cover.png (c.xhtml)"
        ]
    );
}
