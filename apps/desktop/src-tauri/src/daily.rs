// SPDX-License-Identifier: GPL-3.0-or-later
//! The Daily tab (Daily design §5c): the paper's settings, Your Reading, the schedule, and
//! building or sending an edition. Builds run the app's own binary as the CLI in a child
//! process, so the window never blocks and the result is the CLI's JSON report.

use std::path::{Path, PathBuf};

use base64::Engine;
use deckle_digest::Config;
use deckle_digest::chrono::{DateTime, Utc};
use deckle_digest::config::EmailConfig;
use deckle_digest::reading::{Added, Entry, ReadingQueue, State};
use deckle_digest::secret::{self, Keyring, SystemKeyring};
use deckle_digest::zeroize::Zeroizing;
use serde::Serialize;
use tauri::{AppHandle, Emitter};

#[derive(Clone, Debug, Serialize)]
pub struct DailyState {
    pub config_path: String,
    pub exists: bool,
    pub config: Option<Config>,
    /// Why the config couldn't be read.
    pub error: Option<String>,
}

pub fn state_at(path: &Path) -> DailyState {
    let exists = path.exists();
    let (config, error) = if exists {
        match Config::load(path) {
            Ok(c) => (Some(c), None),
            Err(e) => (None, Some(e)),
        }
    } else {
        (None, None)
    };
    DailyState {
        config_path: path.to_string_lossy().into_owned(),
        exists,
        config,
        error,
    }
}

fn config_path() -> PathBuf {
    deckle_digest::config::default_path()
}

fn queue_for(cfg: Option<&Config>) -> ReadingQueue {
    ReadingQueue::new(
        cfg.and_then(|c| c.reading.queue_path.clone())
            .unwrap_or_else(ReadingQueue::default_path),
    )
}

/// The queue as the tab lists it: pending first, then the last 7 days, newest first.
pub fn reading_rows(queue: &ReadingQueue, now: DateTime<Utc>) -> Vec<Entry> {
    let week_ago = now - deckle_digest::chrono::Duration::days(7);
    let mut rows: Vec<Entry> = queue
        .load()
        .into_iter()
        .filter(|e| e.state == State::Pending || e.saved_at > week_ago)
        .collect();
    rows.sort_by_key(|e| (e.state != State::Pending, std::cmp::Reverse(e.saved_at)));
    rows
}

#[derive(Clone, Debug, Serialize)]
pub struct Preview {
    pub title: String,
    pub file: String,
    /// `data:image/jpeg;base64,…` of the edition's cover.
    pub cover: Option<String>,
    /// Editions waiting in the outbox for a Kindle.
    pub outbox: usize,
}

/// The newest built edition in `dir` (`deckle-daily-YYYY-MM-DD.epub`).
pub fn newest_edition(dir: &Path) -> Option<PathBuf> {
    let mut eds: Vec<PathBuf> = std::fs::read_dir(dir)
        .ok()?
        .flatten()
        .map(|e| e.path())
        .filter(|p| {
            p.file_name().is_some_and(|n| {
                let n = n.to_string_lossy();
                n.starts_with(deckle_digest::EDITION_PREFIX) && n.ends_with(".epub")
            })
        })
        .collect();
    eds.sort();
    eds.pop()
}

pub fn preview_of(epub: &Path, outbox: usize) -> Preview {
    let info = deckle_core::inspect::inspect(epub).ok();
    let cover = deckle_core::epub::EpubReader::open(epub)
        .ok()
        .and_then(|mut r| r.read_bytes("OEBPS/cover.jpg").ok())
        .map(|b| {
            format!(
                "data:image/jpeg;base64,{}",
                base64::engine::general_purpose::STANDARD.encode(b)
            )
        });
    Preview {
        title: info.map(|i| i.title).unwrap_or_else(|| {
            epub.file_stem()
                .unwrap_or_default()
                .to_string_lossy()
                .into()
        }),
        file: epub.to_string_lossy().into_owned(),
        cover,
        outbox,
    }
}

fn digest_cache() -> PathBuf {
    deckle_core::wordwise::default_cache_dir().join("digest")
}

#[derive(Clone, Debug, Serialize)]
pub struct RunResult {
    pub ok: bool,
    /// The CLI's JSON report (`digest build` / `digest run`), when it printed one.
    pub report: Option<serde_json::Value>,
    /// The last lines of stderr (inbox notes, errors).
    pub log: String,
}

/// The arguments for the app's own binary acting as the CLI.
pub fn run_args(send: bool, preview_dir: &Path) -> Vec<String> {
    let mut a = vec!["digest".to_owned(), "--json".to_owned()];
    if send {
        // Send now is an explicit ask: it runs past a stopped press
        a.extend([
            "run".into(),
            "--to".into(),
            "device".into(),
            "--force".into(),
        ]);
    } else {
        a.extend([
            "build".into(),
            "-o".into(),
            preview_dir.to_string_lossy().into_owned(),
        ]);
    }
    a
}

pub fn parse_run(status_ok: bool, stdout: &[u8], stderr: &[u8]) -> RunResult {
    let report = serde_json::from_slice(stdout).ok();
    let err = String::from_utf8_lossy(stderr);
    let lines: Vec<&str> = err.lines().filter(|l| !l.trim().is_empty()).collect();
    RunResult {
        ok: status_ok,
        report,
        log: lines[lines.len().saturating_sub(6)..].join("\n"),
    }
}

/// The "Email to Kindle" card's state (email design §8): whether a password is there, never
/// the password.
#[derive(Clone, Debug, Serialize)]
pub struct EmailStatus {
    /// `[email]` has every address and the host it needs.
    pub configured: bool,
    pub has_password: bool,
}

pub fn email_status_with(
    cfg: &EmailConfig,
    keys: &dyn Keyring,
    env: &dyn Fn(&str) -> Option<String>,
) -> EmailStatus {
    EmailStatus {
        configured: cfg.missing().is_empty() && cfg.validate().is_ok(),
        has_password: secret::has_password(cfg, keys, env),
    }
}

/// Saves the password for `cfg`'s account; the plain copy is zeroed when it's dropped here.
pub fn set_password_with(
    cfg: &EmailConfig,
    keys: &dyn Keyring,
    password: String,
) -> Result<(), String> {
    let password = Zeroizing::new(password);
    secret::set(cfg, keys, &password).map_err(|e| e.to_string())
}

fn email_config() -> Result<EmailConfig, String> {
    Ok(Config::load(&config_path())?.email)
}

fn env_var(k: &str) -> Option<String> {
    std::env::var(k).ok()
}

// ---- Tauri commands -----------------------------------------------------------------

#[tauri::command]
pub fn daily_state() -> DailyState {
    state_at(&config_path())
}

/// First run: write the starter config.
#[tauri::command]
pub fn daily_setup() -> Result<DailyState, String> {
    let path = config_path();
    if !path.exists() {
        if let Some(dir) = path.parent() {
            std::fs::create_dir_all(dir).map_err(|e| e.to_string())?;
        }
        std::fs::write(&path, deckle_digest::config::STARTER).map_err(|e| e.to_string())?;
    }
    Ok(state_at(&path))
}

#[tauri::command]
pub fn daily_save(config: Config) -> Result<DailyState, String> {
    let path = config_path();
    deckle_digest::config::save(&path, &config)?;
    Ok(state_at(&path))
}

#[derive(Clone, Debug, Serialize)]
pub struct PlaceRow {
    pub label: String,
    pub latitude: f64,
    pub longitude: f64,
}

/// What the Daily tab's Your Day card shows: today's count, each calendar as masked for
/// display, and the calendars that couldn't be read (almanac design §5).
#[derive(Clone, Debug, Serialize)]
pub struct DayPreview {
    pub events: usize,
    pub labels: Vec<String>,
    pub errors: Vec<String>,
}

pub fn day_preview_with(
    day: &deckle_digest::config::DayConfig,
    date: deckle_digest::chrono::NaiveDate,
    tz: deckle_digest::chrono_tz::Tz,
    net: &dyn deckle_digest::fetch::Net,
) -> DayPreview {
    let agenda = deckle_digest::day::agenda(day, date, tz, net);
    DayPreview {
        events: agenda.entries.len(),
        labels: day
            .calendars
            .iter()
            .map(|c| deckle_digest::day::mask(c))
            .collect(),
        errors: agenda.errors,
    }
}

#[tauri::command]
pub async fn daily_day_preview(
    day: deckle_digest::config::DayConfig,
) -> Result<DayPreview, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let tz = deckle_digest::tz::local_tz();
        let date = Utc::now().with_timezone(&tz).date_naive();
        day_preview_with(&day, date, tz, &deckle_digest::Http)
    })
    .await
    .map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn geocode(query: String) -> Result<Vec<PlaceRow>, String> {
    let places = tauri::async_runtime::spawn_blocking(move || {
        deckle_digest::geocode::search(&query, &deckle_digest::Http)
    })
    .await
    .map_err(|e| e.to_string())??;
    Ok(places
        .into_iter()
        .map(|p| PlaceRow {
            label: p.label(),
            latitude: p.latitude,
            longitude: p.longitude,
        })
        .collect())
}

#[tauri::command]
pub fn reading_list() -> Vec<Entry> {
    let cfg = Config::load(&config_path()).ok();
    reading_rows(&queue_for(cfg.as_ref()), deckle_digest::chrono::Utc::now())
}

#[tauri::command]
pub fn reading_save(url: String) -> Result<String, String> {
    let url = url.trim();
    if !(url.starts_with("http://") || url.starts_with("https://")) {
        return Err("Paste a web address (https://…)".into());
    }
    let cfg = Config::load(&config_path()).ok();
    match queue_for(cfg.as_ref()).add_url(url, None, "app", deckle_digest::chrono::Utc::now())? {
        Added::New(_) => Ok("Saved for tomorrow's paper".into()),
        Added::Duplicate(_) => Ok("Already saved".into()),
    }
}

#[tauri::command]
pub fn reading_remove(id: String) -> Result<bool, String> {
    let cfg = Config::load(&config_path()).ok();
    queue_for(cfg.as_ref()).remove(&id)
}

#[tauri::command]
pub fn bookmarklet() -> Result<String, String> {
    let cfg = Config::load(&config_path()).ok();
    let q = queue_for(cfg.as_ref());
    let dir = q.path.parent().unwrap_or(Path::new(".")).to_path_buf();
    let token = deckle_digest::bookmarklet::token(&dir).map_err(|e| e.to_string())?;
    Ok(deckle_digest::bookmarklet::javascript(&token))
}

#[tauri::command]
pub fn edition_preview() -> Option<Preview> {
    let outbox = deckle_core::queue::Queue::new(deckle_core::queue::Queue::default_dir())
        .pending()
        .len();
    let dirs = [digest_cache().join("preview"), digest_cache()];
    let newest = dirs
        .iter()
        .filter_map(|d| newest_edition(d))
        .max_by_key(|p| std::fs::metadata(p).and_then(|m| m.modified()).ok())?;
    Some(preview_of(&newest, outbox))
}

/// Builds (preview) or runs (send) the edition in a child process; emits `daily-finished`
/// with a [`RunResult`].
#[tauri::command]
pub fn daily_run(app: AppHandle, send: bool) -> Result<(), String> {
    let exe = std::env::current_exe().map_err(|e| e.to_string())?;
    let preview = digest_cache().join("preview");
    std::fs::create_dir_all(&preview).map_err(|e| e.to_string())?;
    let args = run_args(send, &preview);
    std::thread::spawn(move || {
        let result = match std::process::Command::new(&exe).args(&args).output() {
            Ok(out) => parse_run(out.status.success(), &out.stdout, &out.stderr),
            Err(e) => RunResult {
                ok: false,
                report: None,
                log: format!("couldn't start {}: {e}", exe.display()),
            },
        };
        let _ = app.emit("daily-finished", result);
    });
    Ok(())
}

#[tauri::command]
pub fn schedule_get() -> deckle_digest::schedule::Status {
    deckle_digest::schedule::status()
}

/// `Some("05:30")` installs (or moves) the schedule with the app's own binary; `None` removes it.
#[tauri::command]
pub fn schedule_set(at: Option<String>) -> Result<deckle_digest::schedule::Status, String> {
    use deckle_digest::schedule;
    match at {
        // The main paper's job only: other papers' jobs, and the watcher they need, stay.
        None => {
            schedule::remove_paper(deckle_digest::papers::MAIN)?;
        }
        Some(at) => {
            let (hour, minute) = schedule::parse_at(&at)?;
            let exe = std::env::current_exe().map_err(|e| e.to_string())?;
            let days = deckle_digest::Config::load(&config_path())
                .map(|c| c.schedule.days)
                .unwrap_or_default();
            schedule::install(&schedule::Schedule {
                hour,
                minute,
                days,
                slug: deckle_digest::papers::MAIN.into(),
                exe: exe.canonicalize().unwrap_or(exe),
                config: None,
            })?;
        }
    }
    Ok(schedule::status())
}

#[tauri::command]
pub fn email_status() -> EmailStatus {
    let cfg = email_config().unwrap_or_default();
    email_status_with(&cfg, &SystemKeyring, &env_var)
}

/// Writes the keychain for the saved `[email]` account. Nothing reads it back to the window.
#[tauri::command]
pub fn email_set_password(password: String) -> Result<EmailStatus, String> {
    let cfg = email_config()?;
    set_password_with(&cfg, &SystemKeyring, password)?;
    Ok(email_status_with(&cfg, &SystemKeyring, &env_var))
}

/// `deckle digest email test` from the card: a one-page EPUB to the Kindle address.
#[tauri::command]
pub async fn email_test() -> Result<String, String> {
    tauri::async_runtime::spawn_blocking(|| {
        let cfg = email_config()?;
        let dir = tempfile::tempdir().map_err(|e| e.to_string())?;
        deckle_digest::mail::send_test(
            &cfg,
            dir.path(),
            deckle_digest::chrono::Local::now().date_naive(),
            &SystemKeyring,
            &env_var,
            |pw| deckle_digest::mail::SmtpMailer::new(&cfg, pw),
        )
        .map_err(|e| e.to_string())?;
        Ok(format!(
            "Test sent to {}. It should reach your Kindle within a few minutes.",
            cfg.to.trim()
        ))
    })
    .await
    .map_err(|e| e.to_string())?
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn day_preview_counts_events_and_masks_addresses() {
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("Home.ics");
        std::fs::write(
            &file,
            "BEGIN:VCALENDAR\r\nBEGIN:VEVENT\r\nUID:a\r\nDTSTART:20261022T090000\r\n\
             SUMMARY:Standup\r\nEND:VEVENT\r\nEND:VCALENDAR\r\n",
        )
        .unwrap();
        let day = deckle_digest::config::DayConfig {
            calendars: vec![
                file.display().to_string(),
                "webcal://cal.example/s3cr3t/x.ics".into(),
            ],
            ..Default::default()
        };
        let net = |_: &str| -> Result<Vec<u8>, String> { Err("offline".into()) };
        let tz: deckle_digest::chrono_tz::Tz = "America/New_York".parse().unwrap();
        let date = deckle_digest::chrono::NaiveDate::from_ymd_opt(2026, 10, 22).unwrap();
        let p = day_preview_with(&day, date, tz, &net);
        assert_eq!(p.events, 1);
        assert_eq!(p.labels, ["Home.ics", "https://cal.example/…"]);
        assert_eq!(p.errors, ["https://cal.example/…: offline"]);
    }

    #[test]
    fn a_missing_config_is_a_first_run_and_a_broken_one_says_why() {
        let dir = tempfile::tempdir().unwrap();
        let missing = state_at(&dir.path().join("digest.toml"));
        assert!(!missing.exists && missing.config.is_none() && missing.error.is_none());
        std::fs::write(dir.path().join("bad.toml"), "titel = 1").unwrap();
        let bad = state_at(&dir.path().join("bad.toml"));
        assert!(bad.exists && bad.config.is_none() && bad.error.is_some());
    }

    #[test]
    fn preview_finds_the_newest_edition() {
        let dir = tempfile::tempdir().unwrap();
        for d in ["2026-10-18", "2026-10-20", "2026-10-19"] {
            std::fs::write(dir.path().join(format!("deckle-daily-{d}.epub")), b"x").unwrap();
        }
        std::fs::write(dir.path().join("deckle-reads-2026-10-21-x.epub"), b"x").unwrap();
        assert!(
            newest_edition(dir.path())
                .unwrap()
                .ends_with("deckle-daily-2026-10-20.epub")
        );
        assert!(newest_edition(&dir.path().join("nope")).is_none());
    }

    #[test]
    fn runs_use_the_cli_with_json_and_read_its_report() {
        let a = run_args(true, Path::new("/p"));
        // Send now is an explicit ask: it runs past a stopped press
        assert_eq!(a, ["digest", "--json", "run", "--to", "device", "--force"]);
        let b = run_args(false, Path::new("/p"));
        assert_eq!(b, ["digest", "--json", "build", "-o", "/p"]);
        let r = parse_run(true, br#"{"title":"T"}"#, b"inbox: saved 1\n\nlast\n");
        assert!(r.ok);
        assert_eq!(r.report.unwrap()["title"], "T");
        assert_eq!(r.log, "inbox: saved 1\nlast");
        assert!(parse_run(false, b"oops", b"").report.is_none());
    }

    #[test]
    fn email_status_never_returns_the_password() {
        use deckle_digest::secret::MemoryKeyring;
        const PW: &str = "hunter2-Secret!";
        let cfg = deckle_digest::config::EmailConfig {
            enabled: true,
            to: "me_1@kindle.com".into(),
            from: "me@example.com".into(),
            smtp_host: "smtp.example.com".into(),
            username: "me@example.com".into(),
            ..Default::default()
        };
        let keys = MemoryKeyring::default();
        let no_env = |_: &str| None;
        let before = email_status_with(&cfg, &keys, &no_env);
        assert!(before.configured && !before.has_password);
        set_password_with(&cfg, &keys, PW.to_owned()).unwrap();
        let after = email_status_with(&cfg, &keys, &no_env);
        assert!(after.configured && after.has_password);
        let shown = format!("{after:?} {}", serde_json::to_string(&after).unwrap());
        assert!(!shown.contains(PW), "{shown}");
        assert_eq!(
            serde_json::to_value(&after).unwrap(),
            serde_json::json!({"configured": true, "has_password": true})
        );
        let unfinished = email_status_with(&Default::default(), &keys, &no_env);
        assert!(!unfinished.configured && !unfinished.has_password);
        let e = set_password_with(&Default::default(), &keys, PW.to_owned()).unwrap_err();
        assert!(!e.contains(PW), "{e}");
    }

    #[test]
    fn reading_rows_put_pending_first() {
        let dir = tempfile::tempdir().unwrap();
        let q = ReadingQueue::new(dir.path().join("reading.jsonl"));
        let now: DateTime<Utc> = "2026-10-20T12:00:00Z".parse().unwrap();
        let id = |a: Added| match a {
            Added::New(i) | Added::Duplicate(i) => i,
        };
        let printed = id(q.add_url("https://ex.com/old", None, "app", now).unwrap());
        q.add_url("https://ex.com/new", None, "app", now).unwrap();
        q.commit(
            deckle_digest::chrono::NaiveDate::from_ymd_opt(2026, 10, 20).unwrap(),
            &[(printed, deckle_digest::reading::Outcome::Printed)],
        )
        .unwrap();
        let rows = reading_rows(&q, now);
        assert_eq!(rows[0].url.as_deref(), Some("https://ex.com/new"));
        assert_eq!(rows[1].state, State::Printed);
    }
}
