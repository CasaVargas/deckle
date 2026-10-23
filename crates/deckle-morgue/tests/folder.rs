// SPDX-License-Identifier: GPL-3.0-or-later
//! Archiving editions into the folder and keeping them a year (Morgue design §3).

use std::path::Path;

use chrono::{Days, NaiveDate};
use deckle_digest::config::{Config, MorgueConfig, STARTER};
use deckle_morgue::folder::Morgue;
use deckle_morgue::sidecar::{Files, Sidecar};

fn d(s: &str) -> NaiveDate {
    s.parse().unwrap()
}

fn sidecar(date: NaiveDate, marker: &str) -> Sidecar {
    Sidecar {
        date,
        number: 294,
        paper: marker.into(),
        place: "Linden Falls, NC".into(),
        weather: None,
        lead: None,
        sections: Vec::new(),
        files: Files {
            epub: format!("{date}.epub"),
            kfx: None,
        },
        stem: format!("deckle-daily-{date}"),
    }
}

fn morgue(dir: &Path, keep_days: u32) -> Morgue {
    Morgue {
        root: dir.join("Morgue"),
        keep_days,
    }
}

fn epub(dir: &Path, bytes: &[u8]) -> std::path::PathBuf {
    let p = dir.join("built.epub");
    std::fs::write(&p, bytes).unwrap();
    p
}

#[test]
fn archive_writes_epub_then_sidecar() {
    let tmp = tempfile::tempdir().unwrap();
    let m = morgue(tmp.path(), 365);
    let s = sidecar(d("2026-10-21"), "A");
    m.archive_edition(&epub(tmp.path(), b"EPUB"), &s).unwrap();
    let year = tmp.path().join("Morgue/2026");
    assert_eq!(
        std::fs::read(year.join("2026-10-21.epub")).unwrap(),
        b"EPUB"
    );
    let json: Sidecar =
        serde_json::from_slice(&std::fs::read(year.join("2026-10-21.json")).unwrap()).unwrap();
    assert_eq!(json, s);
    assert_eq!(m.sidecar(d("2026-10-21")), Some(s));
    let leftovers: Vec<_> = std::fs::read_dir(&year)
        .unwrap()
        .flatten()
        .map(|e| e.file_name())
        .collect();
    assert_eq!(leftovers.len(), 2, "no temp files: {leftovers:?}");
}

#[test]
fn same_date_twice_replaces() {
    let tmp = tempfile::tempdir().unwrap();
    let m = morgue(tmp.path(), 365);
    let date = d("2026-10-21");
    m.archive_edition(&epub(tmp.path(), b"FIRST"), &sidecar(date, "first"))
        .unwrap();
    m.archive_kfx(date, &epub(tmp.path(), b"OLDKFX")).unwrap();
    m.archive_edition(&epub(tmp.path(), b"SECOND"), &sidecar(date, "second"))
        .unwrap();
    assert_eq!(std::fs::read(m.paths(date).0).unwrap(), b"SECOND");
    assert_eq!(m.sidecar(date).unwrap().paper, "second");
    assert_eq!(m.editions(), [date], "listed once");
    assert!(
        !m.paths(date).2.exists(),
        "the old build's KFX is stale and goes"
    );
    assert_eq!(m.sidecar(date).unwrap().files.kfx, None);
}

#[cfg(unix)]
#[test]
fn unwritable_folder_errors_cleanly() {
    use std::os::unix::fs::PermissionsExt;
    let tmp = tempfile::tempdir().unwrap();
    let root = tmp.path().join("Morgue");
    std::fs::create_dir_all(root.join("2026")).unwrap();
    std::fs::set_permissions(root.join("2026"), std::fs::Permissions::from_mode(0o555)).unwrap();
    let m = Morgue {
        root: root.clone(),
        keep_days: 365,
    };
    let src = epub(tmp.path(), b"EPUB");
    let r = m.archive_edition(&src, &sidecar(d("2026-10-21"), "A"));
    let names: Vec<_> = std::fs::read_dir(root.join("2026"))
        .unwrap()
        .flatten()
        .map(|e| e.file_name())
        .collect();
    std::fs::set_permissions(root.join("2026"), std::fs::Permissions::from_mode(0o755)).unwrap();
    if r.is_ok() {
        // Running as a user the mode doesn't bind (root): nothing to assert about errors.
        return;
    }
    assert!(names.is_empty(), "nothing half-written: {names:?}");
}

#[test]
fn a_root_that_is_a_file_errors() {
    let tmp = tempfile::tempdir().unwrap();
    let file = tmp.path().join("Morgue");
    std::fs::write(&file, b"not a folder").unwrap();
    let m = Morgue {
        root: file,
        keep_days: 365,
    };
    assert!(
        m.archive_edition(&epub(tmp.path(), b"E"), &sidecar(d("2026-10-21"), "A"))
            .is_err()
    );
}

#[test]
fn kfx_copied_and_recorded() {
    let tmp = tempfile::tempdir().unwrap();
    let m = morgue(tmp.path(), 365);
    let date = d("2026-10-21");
    m.archive_edition(&epub(tmp.path(), b"EPUB"), &sidecar(date, "A"))
        .unwrap();
    m.archive_kfx(date, &epub(tmp.path(), b"KFX")).unwrap();
    assert_eq!(std::fs::read(m.paths(date).2).unwrap(), b"KFX");
    assert_eq!(
        m.sidecar(date).unwrap().files.kfx.as_deref(),
        Some("2026-10-21.kfx")
    );
}

#[test]
fn retention() {
    let tmp = tempfile::tempdir().unwrap();
    let m = morgue(tmp.path(), 365);
    let today = d("2026-10-21");
    let old = today - Days::new(400);
    let recent = today - Days::new(100);
    for date in [old, recent] {
        m.archive_edition(&epub(tmp.path(), b"E"), &sidecar(date, "x"))
            .unwrap();
    }
    m.archive_edition(&epub(tmp.path(), b"E"), &sidecar(today, "today"))
        .unwrap();
    assert_eq!(
        m.editions(),
        [today, recent],
        "400 days old is gone, 100 stays"
    );
    assert!(!m.paths(old).0.exists() && !m.paths(old).1.exists());

    // keep_days = 0 keeps everything
    let keep_all = morgue(&tmp.path().join("all"), 0);
    for date in [old, today] {
        keep_all
            .archive_edition(&epub(tmp.path(), b"E"), &sidecar(date, "x"))
            .unwrap();
    }
    assert_eq!(keep_all.editions().len(), 2);

    // a 20-day-old KFX goes while its EPUB and JSON stay; a 5-day-old one stays
    let twenty = today - Days::new(20);
    let five = today - Days::new(5);
    for date in [twenty, five] {
        m.archive_edition(&epub(tmp.path(), b"E"), &sidecar(date, "x"))
            .unwrap();
        m.archive_kfx(date, &epub(tmp.path(), b"K")).unwrap();
    }
    m.prune_old(today);
    assert!(
        !m.paths(twenty).2.exists() && m.paths(twenty).0.exists() && m.paths(twenty).1.exists()
    );
    assert_eq!(
        m.sidecar(twenty).unwrap().files.kfx,
        None,
        "the sidecar stops claiming it"
    );
    assert!(m.paths(five).2.exists());
}

#[test]
fn foreign_files_untouched() {
    let tmp = tempfile::tempdir().unwrap();
    let m = morgue(tmp.path(), 365);
    let root = tmp.path().join("Morgue");
    std::fs::create_dir_all(root.join("2025")).unwrap();
    std::fs::create_dir_all(root.join("2026")).unwrap();
    std::fs::write(root.join("2025/old.epub"), b"mine").unwrap();
    std::fs::write(root.join("notes.txt"), b"mine").unwrap();
    std::fs::write(root.join("2026/2026-10-21 copy.epub"), b"mine").unwrap();
    std::fs::write(root.join("2025/2025-01-01.txt"), b"mine").unwrap();
    m.archive_edition(&epub(tmp.path(), b"E"), &sidecar(d("2026-10-21"), "x"))
        .unwrap();
    m.prune_old(d("2030-01-01"));
    for kept in [
        "2025/old.epub",
        "notes.txt",
        "2026/2026-10-21 copy.epub",
        "2025/2025-01-01.txt",
    ] {
        assert!(root.join(kept).exists(), "{kept}");
    }
    assert!(
        m.editions().is_empty(),
        "only the real edition counted, and it aged out"
    );
    assert!(m.sidecar(d("2025-01-01")).is_none());
}

#[test]
fn usage_counts_editions_and_bytes() {
    let tmp = tempfile::tempdir().unwrap();
    let m = morgue(tmp.path(), 365);
    m.archive_edition(&epub(tmp.path(), b"12345"), &sidecar(d("2026-10-21"), "x"))
        .unwrap();
    m.archive_edition(&epub(tmp.path(), b"123"), &sidecar(d("2026-10-20"), "x"))
        .unwrap();
    let (n, bytes) = m.usage();
    assert_eq!(n, 2);
    assert!(bytes >= 8, "{bytes}");
}

#[test]
fn default_config_and_starter() {
    let c = MorgueConfig::default();
    assert_eq!((c.folder.as_str(), c.keep_days), ("~/Deckle/Morgue", 365));
    assert!(STARTER.contains("# [morgue]"));
    let text = toml::to_string(&Config::default()).unwrap();
    assert!(!text.contains("[morgue]"), "{text}");
    let cfg = Config::parse("[morgue]\nfolder = \"/tmp/m\"\nkeep_days = 30\n").unwrap();
    let m = Morgue::from_config(&cfg);
    assert_eq!(
        (m.root, m.keep_days),
        (std::path::PathBuf::from("/tmp/m"), 30)
    );
    assert!(toml::to_string(&cfg).unwrap().contains("[morgue]"));
}

#[test]
fn an_edition_dated_in_the_future_prunes_nothing() {
    let tmp = tempfile::tempdir().unwrap();
    let m = morgue(tmp.path(), 365);
    let today = d("2026-10-21");
    let kept = [d("2026-10-21"), d("2026-06-01"), d("2026-01-05")];
    for date in kept {
        m.archive_edition_on(&epub(tmp.path(), b"E"), &sidecar(date, "x"), today)
            .unwrap();
        m.archive_kfx(date, &epub(tmp.path(), b"K")).unwrap();
    }
    // A build for a date far ahead is still judged by today's date, not its own.
    m.archive_edition_on(
        &epub(tmp.path(), b"E"),
        &sidecar(d("2027-12-01"), "future"),
        today,
    )
    .unwrap();
    assert_eq!(m.editions().len(), 4, "{:?}", m.editions());
    assert!(m.paths(d("2026-01-05")).0.exists());
    assert!(
        m.paths(d("2026-10-21")).2.exists(),
        "today's KFX is two weeks from today, not from 2027"
    );
}
