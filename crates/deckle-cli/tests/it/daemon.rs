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

/// `deckle digest <args>` in a daemon install (`DECKLE_DAEMON=1`), `PATH` empty so nothing
/// could reach launchd, schtasks or cron.
fn in_daemon_install(home: &Path, args: &[&str]) -> std::process::Output {
    in_daemon_install_with(home, &[], args)
}

/// [`in_daemon_install`] with `envs` set; `--json` in `args` goes before `digest`.
fn in_daemon_install_with(
    home: &Path,
    envs: &[(&str, &str)],
    args: &[&str],
) -> std::process::Output {
    let (json, args): (Vec<&str>, Vec<&str>) = args.iter().partition(|a| **a == "--json");
    Command::cargo_bin("deckle")
        .unwrap()
        .env("HOME", home)
        .env("USERPROFILE", home)
        .env("XDG_CONFIG_HOME", home)
        .env("APPDATA", home)
        .env("LOCALAPPDATA", home)
        .env("PATH", "/nonexistent")
        .env("TZ", "UTC")
        .env("DECKLE_DAEMON", "1")
        .env_remove("DECKLE_SMTP_PASSWORD")
        .env_remove("DECKLE_SITE_PASSWORD_GAZETTE_EXAMPLE")
        .envs(envs.iter().copied())
        .args(json)
        .arg("digest")
        .args(args)
        .output()
        .unwrap()
}

#[test]
fn start_lists_bad_paths_and_still_prints() {
    let (_tmp, home) = setup(AT_0530);
    let mut toml = digest_toml(&home, AT_0530);
    toml.push_str("\n[[export]]\nname = \"Mac\"\ndir = \"/no-such-root/someone/iCloud/Deckle\"\n");
    std::fs::write(config_dir(&home).join("digest.toml"), toml).unwrap();
    let o = daemon(&home, "06:00");
    assert!(o.status.success(), "{}", text(&o.stderr));
    let err = text(&o.stderr);
    assert!(
        err.contains("export.dir")
            && err.contains("/no-such-root/someone/iCloud/Deckle")
            && err.contains("main paper"),
        "{err}"
    );
    assert!(edition(&home).is_file(), "{err}");
}

#[test]
fn schedule_off_in_daemon_install_clears_main_at() {
    let (_tmp, home) = setup(AT_0530);
    let o = in_daemon_install(&home, &["schedule", "--off"]);
    assert!(o.status.success(), "{}", text(&o.stderr));
    let toml = std::fs::read_to_string(config_dir(&home).join("digest.toml")).unwrap();
    assert!(!toml.contains("05:30"), "{toml}");
}

#[test]
fn schedule_at_in_daemon_install_saves_for_the_daemon() {
    let (_tmp, home) = setup("");
    let o = in_daemon_install(&home, &["schedule", "--at", "06:15"]);
    assert!(o.status.success(), "{}", text(&o.stderr));
    assert!(
        text(&o.stdout).contains("saved; the daemon picks it up within a minute"),
        "{}",
        text(&o.stdout)
    );
    let toml = std::fs::read_to_string(config_dir(&home).join("digest.toml")).unwrap();
    assert!(toml.contains("06:15"), "{toml}");
}

#[test]
fn email_password_in_daemon_install_names_env() {
    let (_tmp, home) = setup("");
    let o = in_daemon_install(&home, &["email", "password", "--stdin"]);
    assert!(!o.status.success());
    assert!(
        text(&o.stderr).contains("no keychain in the container: set DECKLE_SMTP_PASSWORD"),
        "{}",
        text(&o.stderr)
    );
}

#[test]
fn subscribe_password_in_daemon_install_names_site_env() {
    let (_tmp, home) = setup("");
    let o = in_daemon_install(
        &home,
        &[
            "subscribe",
            "gazette.example",
            "--username",
            "me",
            "--stdin",
        ],
    );
    assert!(!o.status.success());
    assert!(
        text(&o.stderr).contains("set DECKLE_SITE_PASSWORD_GAZETTE_EXAMPLE"),
        "{}",
        text(&o.stderr)
    );
}

/// `[email]` sending to a port nothing listens on: a send gets as far as connecting.
const EMAIL: &str = "[email]\nenabled = true\nto = \"x@kindle.com\"\nfrom = \"me@example.com\"\n\
                     smtp_host = \"127.0.0.1\"\nsmtp_port = 587\nusername = \"me\"\n";

const GAZETTE: &str = "[[subscriptions]]\nsite = \"gazette.example\"\nusername = \"me\"\n";

/// A daemon install never asks the system keychain (Ruling 8): the password is the
/// environment's, whatever keychain the machine has.
#[test]
fn daemon_install_takes_the_smtp_password_from_env_only() {
    let (_tmp, home) = setup(EMAIL);
    let o = in_daemon_install(&home, &["email", "test"]);
    let err = text(&o.stderr);
    assert!(!o.status.success());
    assert!(
        err.contains("daemon install: passwords come from the environment")
            && err.contains("DECKLE_SMTP_PASSWORD isn't set"),
        "{err}"
    );
    // With the variable the send gets past the password and fails on the connection.
    let o = in_daemon_install_with(&home, &[("DECKLE_SMTP_PASSWORD", "x")], &["email", "test"]);
    let err = text(&o.stderr);
    assert!(!o.status.success());
    assert!(
        !err.contains("keychain") && !err.contains("DECKLE_SMTP_PASSWORD"),
        "{err}"
    );
}

#[test]
fn daemon_install_takes_site_passwords_from_env_only() {
    let (_tmp, home) = setup(GAZETTE);
    let status = |envs: &[(&str, &str)]| {
        let o = in_daemon_install_with(&home, envs, &["--json", "subscribe"]);
        assert!(o.status.success(), "{}", text(&o.stderr));
        let v: serde_json::Value = serde_json::from_slice(&o.stdout).unwrap();
        v[0].clone()
    };
    let with = status(&[("DECKLE_SITE_PASSWORD_GAZETTE_EXAMPLE", "x")]);
    assert_eq!(with["has_password"], true, "{with}");
    let without = status(&[]);
    assert_eq!(without["has_password"], false, "{without}");
    let why = without["state"]["NoPassword"].as_str().unwrap_or_default();
    assert!(
        why.contains("daemon install: passwords come from the environment")
            && why.contains("DECKLE_SITE_PASSWORD_GAZETTE_EXAMPLE"),
        "{without}"
    );
}

fn stopped_date(o: &std::process::Output) -> String {
    let v: serde_json::Value = serde_json::from_str(&text(&o.stdout)).unwrap();
    v["stopped"].as_str().unwrap().to_owned()
}

#[test]
fn stop_in_daemon_install_uses_config_at() {
    let (_tmp, home) = setup(AT_0530);
    let o = in_daemon_install(&home, &["--json", "stop"]);
    assert!(o.status.success(), "{}", text(&o.stderr));
    let day = stopped_date(&o);
    let today = deckle_digest::chrono::Utc::now().date_naive();
    let tomorrow = today + deckle_digest::chrono::Days::new(1);
    assert!(
        day == today.to_string() || day == tomorrow.to_string(),
        "{day}"
    );
    let state: Vec<_> = walk(&home)
        .into_iter()
        .filter(|p| p.ends_with("press.json"))
        .collect();
    assert_eq!(state.len(), 1, "{state:?}");
    assert!(std::fs::read_to_string(&state[0]).unwrap().contains(&day));
}

#[test]
fn stop_in_daemon_install_honours_days() {
    use deckle_digest::chrono::{Datelike, Days, Utc};
    let target = Utc::now().date_naive() + Days::new(3);
    let key =
        deckle_digest::config::Day::ALL[target.weekday().num_days_from_monday() as usize].key();
    let (_tmp, home) = setup(&format!("[schedule]\nat = \"05:30\"\ndays = [\"{key}\"]\n"));
    let o = in_daemon_install(&home, &["--json", "stop"]);
    assert!(o.status.success(), "{}", text(&o.stderr));
    assert_eq!(stopped_date(&o), target.to_string());
}

#[test]
fn stop_in_daemon_install_without_at_still_errors() {
    let (_tmp, home) = setup("");
    let o = in_daemon_install(&home, &["stop"]);
    assert!(!o.status.success());
    assert!(
        text(&o.stderr).contains("nothing is scheduled; there's no press to stop"),
        "{}",
        text(&o.stderr)
    );
}

fn walk(dir: &Path) -> Vec<PathBuf> {
    let mut out = Vec::new();
    for e in std::fs::read_dir(dir).into_iter().flatten().flatten() {
        let p = e.path();
        if p.is_dir() {
            out.extend(walk(&p));
        } else {
            out.push(p);
        }
    }
    out
}
