// SPDX-License-Identifier: GPL-3.0-or-later
//! The headless daemon's scheduling rule (Linux Docker design): when is a paper's edition due,
//! given the clock, what already printed, and the last failed try. Pure, so it's tested without
//! a clock; the daemon loop only feeds it.

use std::collections::BTreeMap;
use std::io;
use std::path::Path;

use chrono::{DateTime, Datelike, NaiveDate, Timelike, Utc, Weekday};
use chrono_tz::Tz;
use serde::{Deserialize, Serialize};

use crate::config::Day;
use crate::schedule::Schedule;

/// Local hour before which a missed edition is still caught up (a restart at 9 prints).
pub const CATCH_UP_UNTIL: u8 = 12;
/// A failed attempt isn't retried for this long.
pub const RETRY_EVERY_MIN: i64 = 15;
/// A paper always runs on time: from its minute through the next this-many minutes, any hour.
const ON_TIME_MIN: u32 = 15;

/// What the daemon remembers between restarts (`/config/deckle/daemon.json`).
#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct DaemonState {
    /// Paper slug -> the edition date last printed.
    pub printed: BTreeMap<String, NaiveDate>,
    /// Paper slug -> when the last failed try began.
    pub last_attempt: BTreeMap<String, DateTime<Utc>>,
}

impl DaemonState {
    /// Missing or unreadable state is a fresh start; the caller logs it.
    pub fn load(path: &Path) -> DaemonState {
        std::fs::read(path)
            .ok()
            .and_then(|b| serde_json::from_slice(&b).ok())
            .unwrap_or_default()
    }

    /// Written aside, then renamed, so a crash never leaves half a file.
    pub fn save(&self, path: &Path) -> io::Result<()> {
        if let Some(dir) = path.parent() {
            std::fs::create_dir_all(dir)?;
        }
        let tmp = path.with_extension("json.tmp");
        std::fs::write(&tmp, serde_json::to_vec_pretty(self)?)?;
        std::fs::rename(&tmp, path)
    }
}

#[derive(Debug, PartialEq, Eq)]
pub enum Decision {
    Run,
    Wait,
    Done,
    Skip(SkipWhy),
}

#[derive(Debug, PartialEq, Eq)]
pub enum SkipWhy {
    NotToday,
    /// Missed its on-time window and it's past noon.
    PastNoon,
    RetryLater,
}

fn weekday(d: Day) -> Weekday {
    match d {
        Day::Mon => Weekday::Mon,
        Day::Tue => Weekday::Tue,
        Day::Wed => Weekday::Wed,
        Day::Thu => Weekday::Thu,
        Day::Fri => Weekday::Fri,
        Day::Sat => Weekday::Sat,
        Day::Sun => Weekday::Sun,
    }
}

/// Whether `job`'s edition is due at `now`. `exists`: today's edition is already on disk
/// (Morgue sidecar), whatever printed it.
pub fn decide(job: &Schedule, now: DateTime<Tz>, state: &DaemonState, exists: bool) -> Decision {
    if !job.days.is_empty() && !job.days.iter().any(|d| weekday(*d) == now.weekday()) {
        return Decision::Skip(SkipWhy::NotToday);
    }
    // Keyed on the date, not the clock, so a DST repeat of the hour can't print twice.
    if exists || state.printed.get(&job.slug) == Some(&now.date_naive()) {
        return Decision::Done;
    }
    let at = u32::from(job.hour) * 60 + u32::from(job.minute);
    let minute = now.hour() * 60 + now.minute();
    if minute < at {
        return Decision::Wait;
    }
    let on_time = minute < at + ON_TIME_MIN;
    if !on_time && now.hour() >= u32::from(CATCH_UP_UNTIL) {
        return Decision::Skip(SkipWhy::PastNoon);
    }
    // A future-dated attempt (clock stepped back) is stale, or it would block retries for hours.
    let recent_failure = state.last_attempt.get(&job.slug).is_some_and(|t| {
        let age = now.with_timezone(&Utc) - *t;
        age >= chrono::Duration::zero() && age < chrono::Duration::minutes(RETRY_EVERY_MIN)
    });
    if recent_failure {
        Decision::Skip(SkipWhy::RetryLater)
    } else {
        Decision::Run
    }
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use chrono::TimeZone;

    use super::*;

    const NY: Tz = chrono_tz::America::New_York;

    fn job(hour: u8, minute: u8, days: &[Day]) -> Schedule {
        Schedule {
            hour,
            minute,
            days: days.to_vec(),
            slug: "main".into(),
            exe: PathBuf::from("deckle"),
            config: None,
        }
    }

    /// 2026-10-26 is a Monday.
    fn at(h: u32, m: u32) -> DateTime<Tz> {
        NY.with_ymd_and_hms(2026, 10, 26, h, m, 0).unwrap()
    }

    fn morning(now: DateTime<Tz>) -> Decision {
        decide(&job(5, 30, &[]), now, &DaemonState::default(), false)
    }

    fn printed_on(date: NaiveDate) -> DaemonState {
        let mut s = DaemonState::default();
        s.printed.insert("main".into(), date);
        s
    }

    #[test]
    fn before_time_waits() {
        assert_eq!(morning(at(5, 29)), Decision::Wait);
    }

    #[test]
    fn at_time_runs() {
        assert_eq!(morning(at(5, 30)), Decision::Run);
    }

    #[test]
    fn restart_at_9_catches_up() {
        assert_eq!(morning(at(9, 0)), Decision::Run);
    }

    #[test]
    fn restart_after_noon_skips() {
        assert_eq!(morning(at(12, 0)), Decision::Skip(SkipWhy::PastNoon));
    }

    #[test]
    fn afternoon_paper_runs_on_time() {
        let d = decide(&job(14, 0, &[]), at(14, 5), &DaemonState::default(), false);
        assert_eq!(d, Decision::Run);
    }

    #[test]
    fn afternoon_paper_late_skips() {
        let d = decide(&job(14, 0, &[]), at(14, 20), &DaemonState::default(), false);
        assert_eq!(d, Decision::Skip(SkipWhy::PastNoon));
    }

    #[test]
    fn printed_today_is_done() {
        let s = printed_on(at(5, 31).date_naive());
        assert_eq!(
            decide(&job(5, 30, &[]), at(5, 31), &s, false),
            Decision::Done
        );
    }

    #[test]
    fn existing_edition_is_done() {
        let d = decide(&job(5, 30, &[]), at(6, 0), &DaemonState::default(), true);
        assert_eq!(d, Decision::Done);
    }

    #[test]
    fn wrong_day_skips() {
        let d = decide(
            &job(5, 30, &[Day::Sat]),
            at(6, 0),
            &DaemonState::default(),
            false,
        );
        assert_eq!(d, Decision::Skip(SkipWhy::NotToday));
        let d = decide(
            &job(5, 30, &[Day::Mon]),
            at(6, 0),
            &DaemonState::default(),
            false,
        );
        assert_eq!(d, Decision::Run);
    }

    #[test]
    fn failed_run_retries_until_noon() {
        let mut s = DaemonState::default();
        s.last_attempt
            .insert("main".into(), at(5, 30).with_timezone(&Utc));
        let j = job(5, 30, &[]);
        assert_eq!(
            decide(&j, at(5, 40), &s, false),
            Decision::Skip(SkipWhy::RetryLater)
        );
        assert_eq!(decide(&j, at(5, 46), &s, false), Decision::Run);
        assert_eq!(
            decide(&j, at(12, 5), &s, false),
            Decision::Skip(SkipWhy::PastNoon)
        );
    }

    #[test]
    fn future_dated_attempt_does_not_block_retries() {
        // The clock stepped back (or the RTC was ahead at boot) after the failed attempt.
        let mut s = DaemonState::default();
        s.last_attempt
            .insert("main".into(), at(6, 40).with_timezone(&Utc));
        assert_eq!(
            decide(&job(5, 30, &[]), at(5, 40), &s, false),
            Decision::Run
        );
    }

    #[test]
    fn daemon_restart_inside_the_minute_prints_once() {
        let now = NY.with_ymd_and_hms(2026, 10, 26, 5, 30, 30).unwrap();
        let s = printed_on(now.date_naive());
        assert_eq!(decide(&job(5, 30, &[]), now, &s, false), Decision::Done);
    }

    #[test]
    fn dst_fall_back_does_not_reprint() {
        // 2026-11-01: 01:30 happens twice (EDT, then EST).
        let second = NY.with_ymd_and_hms(2026, 11, 1, 1, 30, 0).latest().unwrap();
        let first = NY
            .with_ymd_and_hms(2026, 11, 1, 1, 30, 0)
            .earliest()
            .unwrap();
        assert!(second > first);
        let s = printed_on(first.date_naive());
        assert_eq!(decide(&job(1, 30, &[]), second, &s, false), Decision::Done);
    }

    #[test]
    fn state_round_trips() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("deckle/daemon.json");
        let mut s = printed_on(NaiveDate::from_ymd_opt(2026, 10, 26).unwrap());
        s.last_attempt
            .insert("main".into(), at(5, 30).with_timezone(&Utc));
        s.save(&path).unwrap();
        assert_eq!(DaemonState::load(&path), s);
    }

    #[test]
    fn corrupt_state_loads_default() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("daemon.json");
        assert_eq!(DaemonState::load(&path), DaemonState::default());
        std::fs::write(&path, b"{ not json").unwrap();
        assert_eq!(DaemonState::load(&path), DaemonState::default());
    }
}
