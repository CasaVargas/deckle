// SPDX-License-Identifier: GPL-3.0-or-later
//! `digest daemon` (Linux Docker design): one tick at a fixed clock prints a due paper through
//! `digest run`, once, and skips a missed one after noon. Offline: the paper prints only the
//! horoscope, to an export folder, so no Previewer is needed.

use std::path::{Path, PathBuf};

use assert_cmd::Command;

fn config_dir(home: &Path) -> PathBuf {
    if cfg!(target_os = "macos") {
        home.join("Library/Application Support/deckle")
    } else {
        home.join("deckle")
    }
}

/// Today in UTC (the zone the tests run the daemon and its children in), as `digest run`
/// builds for the real date and the Morgue keeps only today's edition.
fn today() -> String {
    deckle_digest::chrono::Utc::now().date_naive().to_string()
}

/// One tick of the daemon at `hh:mm` today, UTC.
fn daemon(home: &Path, hhmm: &str) -> std::process::Output {
    let mounts = home.join("mounts");
    std::fs::create_dir_all(&mounts).unwrap();
    Command::cargo_bin("deckle")
        .unwrap()
        .env("HOME", home)
        .env("USERPROFILE", home)
        .env("XDG_CONFIG_HOME", home)
        .env("APPDATA", home)
        .env("LOCALAPPDATA", home)
        .env("XDG_CACHE_HOME", home.join("cache"))
        .env("DECKLE_MOUNTS", &mounts)
        .env("DECKLE_PREVIEWER", home.join("no-previewer"))
        .env("PATH", "/nonexistent")
        .env("TZ", "UTC")
        .env("DECKLE_DAEMON_ONCE", "1")
        .env(
            "DECKLE_DAEMON_FAKE_NOW",
            format!("{}T{hhmm}:00+00:00", today()),
        )
        .args(["digest", "daemon"])
        .output()
        .unwrap()
}

/// A horoscope-only main paper exporting to `<home>/iPad`, with `extra` appended.
fn setup(extra: &str) -> (tempfile::TempDir, PathBuf) {
    let tmp = tempfile::tempdir().unwrap();
    let home = tmp.path().join("home");
    let dir = config_dir(&home);
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(dir.join("digest.toml"), digest_toml(&home, extra)).unwrap();
    (tmp, home)
}

fn digest_toml(home: &Path, extra: &str) -> String {
    format!(
        "sections = [\"horoscope\"]\n{extra}\n[[export]]\nname = \"iPad\"\ndir = {:?}\n",
        ipad(home).display().to_string()
    )
}

fn ipad(home: &Path) -> PathBuf {
    home.join("iPad")
}

fn edition(home: &Path) -> PathBuf {
    ipad(home).join(format!("deckle-daily-{}.epub", today()))
}

fn state(home: &Path) -> serde_json::Value {
    let s = std::fs::read_to_string(config_dir(home).join("daemon.json")).unwrap_or_default();
    serde_json::from_str(&s).unwrap_or(serde_json::Value::Null)
}

fn text(b: &[u8]) -> String {
    String::from_utf8_lossy(b).into_owned()
}

const AT_0530: &str = "[schedule]\nat = \"05:30\"\n";

#[test]
fn due_paper_prints_once() {
    let (_tmp, home) = setup(AT_0530);
    let o = daemon(&home, "06:00");
    assert!(o.status.success(), "{}", text(&o.stderr));
    assert!(edition(&home).is_file(), "{}", text(&o.stderr));
    assert!(text(&o.stdout).contains("[main] "), "{}", text(&o.stdout));
    assert_eq!(state(&home)["printed"]["main"], today());
    let mtime = std::fs::metadata(edition(&home))
        .unwrap()
        .modified()
        .unwrap();

    let o = daemon(&home, "06:00");
    assert!(o.status.success(), "{}", text(&o.stderr));
    assert!(!text(&o.stdout).contains("[main]"), "{}", text(&o.stdout));
    assert!(
        text(&o.stderr).contains("main: printed"),
        "{}",
        text(&o.stderr)
    );
    let again = std::fs::metadata(edition(&home))
        .unwrap()
        .modified()
        .unwrap();
    assert_eq!(mtime, again);
}

#[test]
fn after_noon_logs_skip() {
    let (_tmp, home) = setup(AT_0530);
    let o = daemon(&home, "13:00");
    assert!(o.status.success(), "{}", text(&o.stderr));
    assert!(!edition(&home).exists());
    let err = text(&o.stderr);
    assert!(
        err.contains("main") && err.contains("skipped") && err.contains("12:00"),
        "{err}"
    );
    assert_eq!(state(&home)["printed"].get("main"), None);
}

#[test]
fn config_changes_apply_next_tick() {
    let (_tmp, home) = setup("");
    let o = daemon(&home, "06:00");
    assert!(o.status.success(), "{}", text(&o.stderr));
    assert!(!edition(&home).exists());

    std::fs::write(
        config_dir(&home).join("digest.toml"),
        digest_toml(&home, AT_0530),
    )
    .unwrap();
    let o = daemon(&home, "06:00");
    assert!(o.status.success(), "{}", text(&o.stderr));
    assert!(edition(&home).is_file(), "{}", text(&o.stderr));
}

#[test]
fn unscheduled_paper_never_runs() {
    let (_tmp, home) = setup(AT_0530);
    let papers = config_dir(&home).join("papers");
    std::fs::create_dir_all(&papers).unwrap();
    std::fs::write(
        papers.join("kids.toml"),
        "title = \"The Kid Courier\"\nsections = [\"horoscope\"]\n",
    )
    .unwrap();
    let o = daemon(&home, "06:00");
    assert!(o.status.success(), "{}", text(&o.stderr));
    assert!(edition(&home).is_file(), "{}", text(&o.stderr));
    assert!(!text(&o.stdout).contains("[kids]"), "{}", text(&o.stdout));
    assert_eq!(state(&home)["printed"].get("kids"), None);
}
