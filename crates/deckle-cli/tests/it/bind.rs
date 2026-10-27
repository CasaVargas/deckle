// SPDX-License-Identifier: GPL-3.0-or-later
//! `deckle bind` (batch-bind design §4): mixed inputs into one EPUB.

use std::path::{Path, PathBuf};

use assert_cmd::Command;

fn deckle(home: &Path) -> Command {
    let mut c = Command::cargo_bin("deckle").unwrap();
    c.env("HOME", home)
        .env("XDG_CONFIG_HOME", home)
        .env("APPDATA", home)
        .env("LOCALAPPDATA", home);
    c
}

fn fixture(rel: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../fixtures")
        .join(rel)
}

#[test]
fn binds_an_epub_a_word_document_a_text_file_and_a_pdf() {
    // The PDF part needs PDFium: tools/fetch-pdfium.sh, or DECKLE_PDFIUM.
    let tmp = tempfile::tempdir().unwrap();
    let out = tmp.path().join("packet.epub");
    let o = deckle(tmp.path())
        .args(["--json", "bind"])
        .arg(fixture("mini/deckle-mini.epub"))
        .arg(fixture("docx/deckle-mini.docx"))
        .arg(fixture("txt/gutenberg-mini.txt"))
        .arg(fixture("pdf/book.pdf"))
        .args([
            "--title",
            "Course Packet",
            "--author",
            "A. Teacher",
            "--out",
        ])
        .arg(&out)
        .output()
        .unwrap();
    assert!(o.status.success(), "{}", String::from_utf8_lossy(&o.stderr));
    let v: serde_json::Value = serde_json::from_slice(&o.stdout).unwrap();
    assert_eq!(
        v["parts"],
        serde_json::json!([
            "Deckle Mini Fixture",
            "Deckle Mini Document",
            "The Deckle Mini Novel",
            "A Small Book"
        ])
    );
    let notes = v["warnings"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|w| w.as_str().unwrap().contains("reflowed from a PDF"))
        .count();
    assert_eq!(notes, 1, "the PDF's quality note, once: {v}");

    let o = deckle(tmp.path())
        .args(["--json", "inspect"])
        .arg(&out)
        .output()
        .unwrap();
    assert!(o.status.success(), "{}", String::from_utf8_lossy(&o.stderr));
    let info: serde_json::Value = serde_json::from_slice(&o.stdout).unwrap();
    assert_eq!(info["title"], "Course Packet");
    assert_eq!(info["creators"][0]["name"], "A. Teacher");
}

#[test]
fn a_kfx_or_a_single_file_is_refused() {
    let tmp = tempfile::tempdir().unwrap();
    let kfx = tmp.path().join("done.kfx");
    std::fs::write(&kfx, b"x").unwrap();
    let o = deckle(tmp.path())
        .arg("bind")
        .arg(fixture("mini/deckle-mini.epub"))
        .arg(&kfx)
        .args(["--title", "X"])
        .output()
        .unwrap();
    assert!(!o.status.success());
    assert!(String::from_utf8_lossy(&o.stderr).contains("finished Kindle book"));

    let o = deckle(tmp.path())
        .arg("bind")
        .arg(fixture("mini/deckle-mini.epub"))
        .args(["--title", "X"])
        .output()
        .unwrap();
    assert!(!o.status.success());
    assert!(String::from_utf8_lossy(&o.stderr).contains("at least two"));
}
