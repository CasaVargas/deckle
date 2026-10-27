// SPDX-License-Identifier: GPL-3.0-or-later
use assert_cmd::Command;
use std::io::{Read, Write};

#[test]
fn inspect_and_normalize_expose_accessibility_results() {
    let temp = tempfile::tempdir().unwrap();
    let source = temp.path().join("input.epub");
    let output = temp.path().join("fixed.epub");
    let mut original = zip::ZipArchive::new(
        std::fs::File::open(deckle_kfx::fixtures::path("deckle-mini.epub")).unwrap(),
    )
    .unwrap();
    let mut zip = zip::ZipWriter::new(std::fs::File::create(&source).unwrap());
    for i in 0..original.len() {
        let mut entry = original.by_index(i).unwrap();
        let name = entry.name().unwrap().into_owned();
        let mut bytes = Vec::new();
        entry.read_to_end(&mut bytes).unwrap();
        if name == "OEBPS/content.opf" {
            bytes = String::from_utf8(bytes)
                .unwrap()
                .replace(
                    "<dc:language>en</dc:language>",
                    "<dc:language>en_US</dc:language>",
                )
                .into_bytes();
        } else if name == "OEBPS/c0.xhtml" {
            bytes = br#"<html xmlns="http://www.w3.org/1999/xhtml"><body><h1>One</h1><img src="cover.png"/><img src="cover.png" alt=""/></body></html>"#.to_vec();
        }
        zip.start_file(name, zip::write::SimpleFileOptions::default())
            .unwrap();
        zip.write_all(&bytes).unwrap();
    }
    zip.finish().unwrap();
    let report = Command::cargo_bin("deckle")
        .unwrap()
        .args(["inspect", "--json"])
        .arg(&source)
        .output()
        .unwrap();
    assert!(
        report.status.success(),
        "{}",
        String::from_utf8_lossy(&report.stderr)
    );
    let json: serde_json::Value = serde_json::from_slice(&report.stdout).unwrap();
    assert_eq!(json["language"], "en-US");
    assert_eq!(json["images_missing_alt"], 1);
    Command::cargo_bin("deckle")
        .unwrap()
        .arg("inspect")
        .arg(&source)
        .assert()
        .success()
        .stdout(predicates::str::contains("1 image(s) missing alt text"));
    let report = Command::cargo_bin("deckle")
        .unwrap()
        .args(["normalize", "--json"])
        .arg(&source)
        .arg("-o")
        .arg(&output)
        .output()
        .unwrap();
    assert!(
        report.status.success(),
        "{}",
        String::from_utf8_lossy(&report.stderr)
    );
    let json: serde_json::Value = serde_json::from_slice(&report.stdout).unwrap();
    assert!(json["language_tags_fixed"].as_u64().unwrap() >= 3);
    let info = deckle_core::inspect::inspect(&output).unwrap();
    assert_eq!(info.images_missing_alt, 1); // Reporting does not fabricate descriptions.
}
