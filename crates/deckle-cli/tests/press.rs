// SPDX-License-Identifier: GPL-3.0-or-later
//! Stop the presses (tomorrow's paper design §4.2) with Deckle's config folder in a temp dir.

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

fn config_dir(home: &Path) -> PathBuf {
    if cfg!(target_os = "macos") {
        home.join("Library/Application Support/deckle")
    } else {
        home.join("deckle")
    }
}

/// A config folder with a minimal paper and a reading queue holding one saved link.
fn setup() -> (tempfile::TempDir, PathBuf) {
    let home = tempfile::tempdir().unwrap();
    let dir = config_dir(home.path());
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(dir.join("digest.toml"), "sections = [\"horoscope\"]\n").unwrap();
    std::fs::write(
        dir.join("reading.jsonl"),
        "{\"id\":\"a\",\"url\":\"https://example.com/x\",\"file\":null,\"title\":\"X\",\
         \"saved_at\":\"2026-10-22T13:14:00Z\",\"source\":\"cli\",\"state\":\"pending\",\
         \"tries\":0,\"printed_in\":null,\"note\":null}\n",
    )
    .unwrap();
    let out = home.path().join("out");
    std::fs::create_dir_all(&out).unwrap();
    (home, out)
}

fn queue(home: &Path) -> Vec<u8> {
    std::fs::read(config_dir(home).join("reading.jsonl")).unwrap()
}

#[test]
fn stop_then_run_builds_nothing() {
    let (home, out) = setup();
    deckle(home.path())
        .args(["digest", "stop", "--date", &tomorrow()])
        .assert()
        .success();
    let before = queue(home.path());
    let o = deckle(home.path())
        .args(["digest", "run", "--date", &tomorrow(), "--to"])
        .arg(&out)
        .env("PATH", "/nonexistent")
        .output()
        .unwrap();
    assert!(o.status.success(), "{}", String::from_utf8_lossy(&o.stderr));
    let text = String::from_utf8(o.stdout).unwrap();
    assert!(
        text.contains(&format!("Presses stopped for {}", long_tomorrow())),
        "{text}"
    );
    assert!(text.contains("deckle digest stop --undo"), "{text}");
    assert_eq!(std::fs::read_dir(&out).unwrap().count(), 0);
    assert_eq!(queue(home.path()), before);
    // the JSON form
    let o = deckle(home.path())
        .args(["--json", "digest", "run", "--date", &tomorrow(), "--to"])
        .arg(&out)
        .env("PATH", "/nonexistent")
        .output()
        .unwrap();
    let v: serde_json::Value = serde_json::from_slice(&o.stdout).unwrap();
    assert_eq!(v["stopped"], tomorrow());
}

#[test]
fn force_runs_anyway() {
    let (home, out) = setup();
    deckle(home.path())
        .args(["digest", "stop", "--date", &tomorrow()])
        .assert()
        .success();
    let o = deckle(home.path())
        .args([
            "digest",
            "run",
            "--force",
            "--date",
            &tomorrow(),
            "--previewer",
        ])
        .arg(home.path().join("missing"))
        .arg("--to")
        .arg(&out)
        .output()
        .unwrap();
    // past the stop: it fails on the missing Previewer instead
    assert!(!o.status.success());
    let err = String::from_utf8_lossy(&o.stderr);
    assert!(err.contains("Previewer"), "{err}");
}

#[test]
fn asleep_through_press() {
    // the Mac slept through 05:30; launchd runs the job at 09:00 on wake, with no --date
    let (home, out) = setup();
    let today = chrono_today();
    deckle(home.path())
        .args(["digest", "stop", "--date", &today])
        .assert()
        .success();
    let o = deckle(home.path())
        .args(["digest", "run", "--to"])
        .arg(&out)
        .output()
        .unwrap();
    assert!(o.status.success());
    assert!(String::from_utf8_lossy(&o.stdout).contains("Presses stopped for"));
}

#[test]
fn undo_clears() {
    let (home, _) = setup();
    deckle(home.path())
        .args(["digest", "stop", "--date", &tomorrow()])
        .assert()
        .success();
    assert!(config_dir(home.path()).join("press.json").exists());
    let o = deckle(home.path())
        .args(["digest", "stop", "--undo"])
        .output()
        .unwrap();
    assert!(o.status.success());
    assert!(String::from_utf8_lossy(&o.stdout).contains("Presses restarted."));
    assert!(!config_dir(home.path()).join("press.json").exists());
}

#[test]
fn stop_without_schedule_errors() {
    let (home, _) = setup();
    let o = deckle(home.path())
        .args(["digest", "stop"])
        .output()
        .unwrap();
    assert_eq!(o.status.code(), Some(1));
    assert!(
        String::from_utf8_lossy(&o.stderr)
            .contains("nothing is scheduled; there's no press to stop")
    );
}

fn chrono_today() -> String {
    deckle_digest::chrono::Local::now()
        .date_naive()
        .format("%Y-%m-%d")
        .to_string()
}

// --- deckle digest next (Task 7) ---

const PAPER: &str = "title = \"The Morning Deckle\"\nplace = \"Linden Falls, NC\"\n\
sections = [\"reading\", \"weather\", \"feeds\", \"horoscope\"]\n\n\
[[feeds]]\nname = \"Science\"\nurl = \"http://10.255.255.1/feed\"   # unroutable: never asked without --fetch\nmax_items = 4\n";

fn paper_setup() -> tempfile::TempDir {
    let (home, _) = setup();
    std::fs::write(config_dir(home.path()).join("digest.toml"), PAPER).unwrap();
    home
}

#[test]
fn next_json_has_the_view() {
    let home = paper_setup();
    let o = deckle(home.path())
        .args(["--json", "digest", "next"])
        .output()
        .unwrap();
    assert!(o.status.success(), "{}", String::from_utf8_lossy(&o.stderr));
    let v: serde_json::Value = serde_json::from_slice(&o.stdout).unwrap();
    assert!(
        v["lead"]["kicker"]
            .as_str()
            .unwrap()
            .starts_with("YOUR READING"),
        "{v}"
    );
    assert_eq!(v["plan"][0]["key"], "reading");
    assert!(v["pages"].as_u64().unwrap() >= 1);
    assert_eq!(v["week"].as_array().unwrap().len(), 6);
}

#[test]
fn next_text_lines() {
    let home = paper_setup();
    let o = deckle(home.path())
        .args(["digest", "next"])
        .output()
        .unwrap();
    let text = String::from_utf8(o.stdout).unwrap();
    for want in [
        "The Morning Deckle · No. ",
        "Lead: YOUR READING · EXAMPLE.COM — X",
        "Weather · fills in at press time",
        "Page plan (about ",
        " Kindle pages):",
        "Science — 10.255.255.1 [4 stories]",
        "This week: ",
        "No press time set",
    ] {
        assert!(text.contains(want), "missing {want:?} in:\n{text}");
    }
}

#[test]
fn next_without_fetch_makes_no_request() {
    let home = paper_setup();
    let start = std::time::Instant::now();
    let o = deckle(home.path())
        .args(["digest", "next"])
        .output()
        .unwrap();
    assert!(start.elapsed() < std::time::Duration::from_secs(5));
    assert!(String::from_utf8_lossy(&o.stdout).contains("Science · 4 slots"));
    // nothing was cached either
    assert!(
        !home
            .path()
            .join("Library/Caches/deckle/digest/next.json")
            .exists()
    );
}

/// The stop tests use tomorrow: a stop for a day already past is ignored.
fn tomorrow() -> String {
    (deckle_digest::chrono::Local::now().date_naive() + deckle_digest::chrono::Days::new(1))
        .format("%Y-%m-%d")
        .to_string()
}

fn long_tomorrow() -> String {
    (deckle_digest::chrono::Local::now().date_naive() + deckle_digest::chrono::Days::new(1))
        .format("%A, %B %-d")
        .to_string()
}
