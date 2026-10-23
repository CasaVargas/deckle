// SPDX-License-Identifier: GPL-3.0-or-later
//! Every edition on the Daily tab (Morgue design §6): a month of front pages, search, one day's
//! detail, and sending an edition again. Pure logic is in plain functions for the tests.

use deckle_digest::chrono::{Days, Local, NaiveDate, NaiveTime};
use deckle_morgue::detail::{Detail, detail};
use deckle_morgue::search::{DaySummary, Hit, month, search};
use deckle_morgue::{Morgue, MorgueState};
use serde::Serialize;

/// What the card needs for one month.
#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MonthView {
    pub days: Vec<DaySummary>,
    /// Every edition kept, not just this month's: the footer's size line.
    pub editions: usize,
    pub bytes: u64,
    pub folder: String,
    /// "Tomorrow, 05:30": when the next paper is made, with a schedule installed.
    pub next_build: Option<String>,
    /// The date that label is for.
    pub next_build_date: Option<String>,
}

pub fn month_view(m: &Morgue, year: i32, mon: u32, next: Option<(String, String)>) -> MonthView {
    let (editions, bytes) = m.usage();
    let (next_build, next_build_date) = next.unzip();
    MonthView {
        days: month(m, year, mon),
        editions,
        bytes,
        folder: m.root.to_string_lossy().into_owned(),
        next_build,
        next_build_date,
    }
}

/// When the schedule `at` ("05:30") makes the next paper: today's if that time is still ahead,
/// else tomorrow's. The label and its date.
pub fn next_build(at: &str, today: NaiveDate, now: NaiveTime) -> Option<(String, String)> {
    let time = NaiveTime::parse_from_str(at, "%H:%M").ok()?;
    let (word, date) = if now < time {
        ("Today", today)
    } else {
        ("Tomorrow", today + Days::new(1))
    };
    Some((format!("{word}, {at}"), date.to_string()))
}

/// The queued file's name from `digest --json morgue send`, or its error.
pub fn parse_send(ok: bool, stdout: &[u8], stderr: &[u8]) -> Result<String, String> {
    if ok {
        let v: serde_json::Value = serde_json::from_slice(stdout).map_err(|e| e.to_string())?;
        return v["queued"]
            .as_str()
            .map(str::to_owned)
            .ok_or_else(|| "the send said nothing about what it queued".to_owned());
    }
    let err = String::from_utf8_lossy(stderr);
    let line = err
        .lines()
        .rev()
        .find(|l| !l.trim().is_empty())
        .unwrap_or("the send failed");
    Err(line.trim().trim_start_matches("error: ").to_owned())
}

/// `digest.toml`, or the defaults when there is none. One that won't parse is an error here as
/// it is for the CLI, not a quiet fall back to `~/Deckle/Morgue`.
fn config() -> Result<deckle_digest::Config, String> {
    let path = deckle_digest::config::default_path();
    if !path.exists() {
        return Ok(deckle_digest::Config::default());
    }
    deckle_digest::Config::load(&path)
}

fn morgue() -> Result<Morgue, String> {
    config().map(|c| Morgue::from_config(&c))
}

fn date_of(s: &str) -> Result<NaiveDate, String> {
    NaiveDate::parse_from_str(s, "%Y-%m-%d").map_err(|_| format!("expected YYYY-MM-DD, got {s:?}"))
}

// ---- Tauri commands -----------------------------------------------------------------

// The file reads below run off the main thread: a search folds every kept edition, and the
// folder may sit on iCloud.

#[tauri::command]
pub async fn morgue_month(year: i32, month: u32) -> Result<MonthView, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let now = Local::now();
        let next = deckle_digest::schedule::status()
            .at
            .and_then(|at| next_build(&at, now.date_naive(), now.time()));
        Ok::<_, String>(month_view(&morgue()?, year, month, next))
    })
    .await
    .map_err(|e| e.to_string())?
}

#[tauri::command]
pub async fn morgue_search(query: String) -> Result<Vec<Hit>, String> {
    tauri::async_runtime::spawn_blocking(move || Ok::<_, String>(search(&morgue()?, &query)))
        .await
        .map_err(|e| e.to_string())?
}

#[tauri::command]
pub async fn morgue_show(date: String) -> Result<Option<Detail>, String> {
    let date = date_of(&date)?;
    tauri::async_runtime::spawn_blocking(move || {
        let reads = deckle_readership::State::load(&deckle_readership::state_path());
        let state = MorgueState::load(&deckle_morgue::state::state_path());
        Ok::<_, String>(detail(&morgue()?, date, &reads, &state))
    })
    .await
    .map_err(|e| e.to_string())?
}

/// Queues the edition for the Kindle again, through the bundled CLI (as `daily_run` does), so
/// the Previewer fallback lives in one place.
#[tauri::command]
pub async fn morgue_send(date: String) -> Result<String, String> {
    date_of(&date)?;
    let exe = std::env::current_exe().map_err(|e| e.to_string())?;
    tauri::async_runtime::spawn_blocking(move || {
        let out = std::process::Command::new(&exe)
            .args(["digest", "--json", "morgue", "send", &date])
            .output()
            .map_err(|e| format!("couldn't start {}: {e}", exe.display()))?;
        parse_send(out.status.success(), &out.stdout, &out.stderr)
    })
    .await
    .map_err(|e| e.to_string())?
}

/// Opens the kept EPUB in whatever the system opens EPUBs with.
#[tauri::command]
pub fn morgue_open(date: String) -> Result<(), String> {
    let epub = morgue()?.paths(date_of(&date)?).0;
    if !epub.is_file() {
        return Err(format!("{} isn't there any more", epub.display()));
    }
    #[cfg(target_os = "macos")]
    let r = std::process::Command::new("open").arg(&epub).spawn();
    #[cfg(target_os = "windows")]
    // Not `cmd /C start`: it misreads a path with `&` or `^` in it.
    let r = std::process::Command::new("explorer").arg(&epub).spawn();
    #[cfg(not(any(target_os = "macos", target_os = "windows")))]
    let r = std::process::Command::new("xdg-open").arg(&epub).spawn();
    r.map(|_| ()).map_err(|e| e.to_string())
}

#[cfg(test)]
mod tests {
    use deckle_digest::chrono::NaiveTime;
    use deckle_morgue::folder::Morgue;
    use deckle_morgue::sidecar::{Files, Lead, Sidecar};

    use super::*;

    fn sidecar(date: &str) -> Sidecar {
        let date: deckle_digest::chrono::NaiveDate = date.parse().unwrap();
        Sidecar {
            date,
            number: 1,
            paper: "The Deckle Daily".into(),
            place: String::new(),
            weather: None,
            lead: Some(Lead {
                section: "Local".into(),
                title: format!("Lead of {date}"),
                teaser: None,
            }),
            sections: Vec::new(),
            files: Files {
                epub: format!("{date}.epub"),
                kfx: None,
            },
            stem: format!("deckle-daily-{date}"),
        }
    }

    #[test]
    fn month_view_counts() {
        let tmp = tempfile::tempdir().unwrap();
        let m = Morgue {
            root: tmp.path().join("Morgue"),
            keep_days: 0,
        };
        let epub = tmp.path().join("e.epub");
        std::fs::write(&epub, b"12345").unwrap();
        for d in ["2026-10-21", "2026-10-20", "2026-09-30"] {
            m.archive_edition(&epub, &sidecar(d)).unwrap();
        }
        let v = month_view(
            &m,
            2026,
            10,
            Some(("Tomorrow, 05:30".into(), "2026-10-22".into())),
        );
        let dates: Vec<String> = v.days.iter().map(|d| d.date.to_string()).collect();
        assert_eq!(dates, ["2026-10-20", "2026-10-21"]);
        assert_eq!(v.days[1].lead.as_deref(), Some("Lead of 2026-10-21"));
        assert_eq!(
            v.editions, 3,
            "every month's editions count toward the footer"
        );
        assert!(v.bytes >= 15, "{}", v.bytes);
        assert_eq!(v.folder, tmp.path().join("Morgue").to_string_lossy());
        assert_eq!(v.next_build.as_deref(), Some("Tomorrow, 05:30"));
        assert_eq!(v.next_build_date.as_deref(), Some("2026-10-22"));
        let json = serde_json::to_value(&v).unwrap();
        assert!(
            json["nextBuild"].is_string() && json["nextBuildDate"].is_string(),
            "camelCase"
        );
    }

    #[test]
    fn the_next_paper_is_today_or_tomorrow() {
        let at = |h, m| NaiveTime::from_hms_opt(h, m, 0).unwrap();
        let today: deckle_digest::chrono::NaiveDate = "2026-10-21".parse().unwrap();
        // Before 05:30 it's today's; after, tomorrow's.
        assert_eq!(
            next_build("05:30", today, at(4, 0)),
            Some(("Today, 05:30".to_owned(), "2026-10-21".to_owned()))
        );
        assert_eq!(
            next_build("05:30", today, at(9, 15)),
            Some(("Tomorrow, 05:30".to_owned(), "2026-10-22".to_owned()))
        );
        assert_eq!(next_build("nonsense", today, at(9, 15)), None);
    }

    #[test]
    fn send_output_names_the_queued_file_or_the_error() {
        assert_eq!(
            parse_send(true, br#"{"queued":"deckle-daily-2026-10-01.kfx"}"#, b""),
            Ok("deckle-daily-2026-10-01.kfx".to_owned())
        );
        assert_eq!(
            parse_send(false, b"", b"error: no edition kept for 2026-01-01\n"),
            Err("no edition kept for 2026-01-01".to_owned())
        );
        assert!(parse_send(true, b"not json", b"").is_err());
    }
}
