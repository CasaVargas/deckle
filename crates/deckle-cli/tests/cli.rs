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
#[cfg(unix)] // the fake Previewer is a shell script
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

#[test]
fn run_checks_previewer_before_creating_the_output_folder() {
    let tmp = tempfile::tempdir().unwrap();
    let target = tmp.path().join("new-folder");
    let res = Command::cargo_bin("deckle")
        .unwrap()
        .args(["run"])
        .arg(deckle_kfx::fixtures::path("deckle-mini.epub"))
        .args(["--to"])
        .arg(&target)
        .args(["--previewer", "/nonexistent/KindlePreviewer4CLI"])
        .output()
        .unwrap();
    assert!(!res.status.success());
    let text = format!(
        "{}{}",
        String::from_utf8_lossy(&res.stdout),
        String::from_utf8_lossy(&res.stderr)
    );
    assert!(text.contains("Kindle Previewer is not installed"), "{text}");
    assert!(
        !target.exists(),
        "output folder must not be created before the Previewer check"
    );
}

fn save(queue: &std::path::Path, args: &[&str]) -> (bool, String) {
    let out = Command::cargo_bin("deckle")
        .unwrap()
        .env("DECKLE_READING_QUEUE", queue)
        .arg("save")
        .args(args)
        .output()
        .unwrap();
    (
        out.status.success(),
        String::from_utf8_lossy(&out.stdout).into_owned() + &String::from_utf8_lossy(&out.stderr),
    )
}

#[test]
fn save_list_remove_roundtrip() {
    let dir = tempfile::tempdir().unwrap();
    let q = dir.path().join("reading.jsonl");
    let (ok, text) = save(&q, &["https://ex.com/a"]);
    assert!(ok && text.contains("saved r-"), "{text}");
    let id = text
        .split_whitespace()
        .find(|w| w.starts_with("r-"))
        .unwrap()
        .to_owned();
    let (_, list) = save(&q, &["--list"]);
    assert!(
        list.contains(&id) && list.contains("pending") && list.contains("https://ex.com/a"),
        "{list}"
    );
    let (ok, _) = save(&q, &["--remove", &id]);
    assert!(ok);
    let (_, list) = save(&q, &["--list"]);
    assert!(!list.contains(&id), "{list}");
}

#[test]
fn save_duplicate_says_so() {
    let dir = tempfile::tempdir().unwrap();
    let q = dir.path().join("reading.jsonl");
    save(&q, &["https://ex.com/a"]);
    let (ok, text) = save(&q, &["https://ex.com/a#again"]);
    assert!(ok && text.contains("already saved"), "{text}");
}

#[test]
fn bookmarklet_prints_javascript_with_token() {
    let dir = tempfile::tempdir().unwrap();
    let q = dir.path().join("reading.jsonl");
    let (ok, text) = save(&q, &["--bookmarklet"]);
    assert!(ok, "{text}");
    let js = text
        .lines()
        .find(|l| l.starts_with("javascript:"))
        .unwrap_or_default();
    assert!(js.contains("127.0.0.1:47219/save?token="), "{text}");
    // The token is stable across calls.
    let (_, again) = save(&q, &["--bookmarklet"]);
    assert!(again.contains(js));
}
