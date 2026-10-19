// SPDX-License-Identifier: GPL-3.0-or-later
use assert_cmd::Command;

#[test]
fn help_lists_all_subcommands() {
    let out = Command::cargo_bin("deckle")
        .unwrap()
        .arg("--help")
        .output()
        .unwrap();
    let s = String::from_utf8(out.stdout).unwrap();
    for sub in ["inspect", "normalize", "convert", "send", "devices", "run"] {
        assert!(s.contains(sub), "missing subcommand {sub} in:\n{s}");
    }
}

#[test]
fn devices_without_a_kindle_succeeds_with_a_message() {
    let out = Command::cargo_bin("deckle")
        .unwrap()
        .args(["devices"])
        .output()
        .unwrap();
    assert!(out.status.success());
    let text = String::from_utf8(out.stdout).unwrap();
    assert!(
        text.contains("Kindle") || text.contains("thumbnails"),
        "{text}"
    );
}

#[test]
fn convert_from_kpf_matches_the_golden() {
    let golden = std::fs::read(deckle_kfx::fixtures::path(
        "deckle-mini-pdoc-nopages.golden.kfx",
    ))
    .unwrap();
    let doc = deckle_kfx::container::read_kfx(&golden).unwrap();
    let out = tempfile::tempdir().unwrap();
    let kfx = out.path().join("out.kfx");
    Command::cargo_bin("deckle")
        .unwrap()
        .args(["convert"])
        .arg(deckle_kfx::fixtures::path("deckle-mini.kpf"))
        .args(["-o"])
        .arg(&kfx)
        .args([
            "--cde",
            "pdoc",
            "--asin",
            "B0DECK000002",
            "--no-authors",
            "--container-id",
            &doc.container_id,
            "--creator",
            &doc.creator,
        ])
        .assert()
        .success();
    assert_eq!(std::fs::read(&kfx).unwrap(), golden);
}

#[test]
fn inspect_json_reports_the_title() {
    let out = Command::cargo_bin("deckle")
        .unwrap()
        .args(["--json", "inspect"])
        .arg(deckle_kfx::fixtures::path("deckle-mini.epub"))
        .output()
        .unwrap();
    assert!(out.status.success());
    let v: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(v["title"], "Deckle Mini Fixture");
}

#[test]
fn run_with_fake_previewer_from_the_cli() {
    let fake = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../deckle-core/tests/fake_previewer/ok.sh");
    let out = tempfile::tempdir().unwrap();
    let res = Command::cargo_bin("deckle")
        .unwrap()
        .args(["--json", "run"])
        .arg(deckle_kfx::fixtures::path("deckle-mini.epub"))
        .args(["--to"])
        .arg(out.path())
        .args(["--previewer"])
        .arg(&fake)
        .output()
        .unwrap();
    assert!(
        res.status.success(),
        "{}",
        String::from_utf8_lossy(&res.stderr)
    );
    let v: serde_json::Value = serde_json::from_slice(&res.stdout).unwrap();
    assert!(
        v[0]["delivered"]
            .as_str()
            .unwrap()
            .ends_with("Deckle Mini Fixture.kfx")
    );
}
