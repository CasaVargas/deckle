// SPDX-License-Identifier: GPL-3.0-or-later
//! When the next paper goes to press, and Stop the presses (tomorrow's paper design §4).

use std::path::Path;

use chrono::{DateTime, Days, FixedOffset, LocalResult, NaiveDate, TimeZone, Utc};
use chrono_tz::Tz;
use serde::{Deserialize, Serialize};

/// The press, as the Daily tab and `digest next` show it.
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct Press {
    /// `HH:MM` of the schedule, when one is installed.
    pub at: Option<String>,
    /// The press that will print: the next one, or the one after a stopped morning.
    pub next: Option<DateTime<FixedOffset>>,
    /// The date of the paper the preview shows.
    pub edition: NaiveDate,
    pub stopped: Option<NaiveDate>,
    /// The last press instant, or local midnight with no schedule: "new since" counts from here.
    pub since: DateTime<FixedOffset>,
}

/// `at` on `date` in `tz`: a time a spring-forward skips becomes the first valid instant after
/// it, and one a fall-back repeats the earlier of the two.
fn on(date: NaiveDate, at: (u8, u8), tz: Tz) -> DateTime<Tz> {
    let mut t = date
        .and_hms_opt(u32::from(at.0), u32::from(at.1), 0)
        .unwrap_or_else(|| date.and_hms_opt(0, 0, 0).expect("midnight"));
    for _ in 0..=120 {
        match tz.from_local_datetime(&t) {
            LocalResult::Single(d) | LocalResult::Ambiguous(d, _) => return d,
            LocalResult::None => t += chrono::Duration::minutes(1),
        }
    }
    // no zone has a gap over two hours; UTC is a safe answer if one ever does
    Utc.from_utc_datetime(&t).with_timezone(&tz)
}

/// The next time the press runs at `at`: today's if `now` is before it, else tomorrow's.
pub fn next_press(at: (u8, u8), now: DateTime<Tz>) -> DateTime<Tz> {
    let tz = now.timezone();
    let today = on(now.date_naive(), at, tz);
    if now < today {
        today
    } else {
        on(now.date_naive() + Days::new(1), at, tz)
    }
}

/// The press clock: when a stopped morning is the next press, the paper after it.
pub fn press(at: Option<(u8, u8)>, stopped: Option<NaiveDate>, now: DateTime<Tz>) -> Press {
    let tz = now.timezone();
    let Some(at) = at else {
        return Press {
            at: None,
            next: None,
            edition: now.date_naive(),
            stopped,
            since: on(now.date_naive(), (0, 0), tz).fixed_offset(),
        };
    };
    let first = next_press(at, now);
    let since = on(first.date_naive() - Days::new(1), at, tz);
    // Only a stop on the next press shows: one whose morning has passed is spent (it lapses at
    // midnight for `digest run`), and one further on isn't what the bar is about.
    let stopped = stopped.filter(|s| *s == first.date_naive());
    let next = match stopped {
        Some(s) => on(s + Days::new(1), at, tz),
        None => first,
    };
    Press {
        at: Some(format!("{:02}:{:02}", at.0, at.1)),
        next: Some(next.fixed_offset()),
        edition: next.date_naive(),
        stopped,
        since: since.fixed_offset(),
    }
}

/// "in 14 h 22 m", "in 38 m", or "now". Minutes round down.
pub fn countdown(from: DateTime<Utc>, to: DateTime<Utc>) -> String {
    let mins = (to - from).num_seconds() / 60;
    if (to - from).num_seconds() <= 0 {
        return "now".into();
    }
    match (mins / 60, mins % 60) {
        (0, m) => format!("in {m} m"),
        (h, m) => format!("in {h} h {m} m"),
    }
}

/// The press bar's line (tomorrow's paper design §4.1).
pub fn bar_line(p: &Press, now: DateTime<Utc>) -> String {
    let (Some(at), Some(next)) = (&p.at, p.next) else {
        return "No press time set".into();
    };
    match p.stopped {
        Some(s) => format!(
            "Presses stopped: no paper {} · next at {at} {}",
            s.format("%A"),
            next.format("%A")
        ),
        None => format!(
            "Goes to press at {at} · {}",
            countdown(now, next.with_timezone(&Utc))
        ),
    }
}

#[derive(Serialize, Deserialize)]
struct StopFile {
    stopped: NaiveDate,
}

/// The stopped morning in `press.json`; one before `today` is over and reads as none.
pub fn read_stop(path: &Path, today: NaiveDate) -> Option<NaiveDate> {
    let bytes = std::fs::read(path).ok()?;
    let f: StopFile = serde_json::from_slice(&bytes).ok()?;
    (f.stopped >= today).then_some(f.stopped)
}

/// Writes the stop (temp file, then rename), or removes the file for `None`.
pub fn write_stop(path: &Path, stopped: Option<NaiveDate>) -> Result<(), String> {
    let Some(stopped) = stopped else {
        return match std::fs::remove_file(path) {
            Err(e) if e.kind() != std::io::ErrorKind::NotFound => {
                Err(format!("{}: {e}", path.display()))
            }
            _ => Ok(()),
        };
    };
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir).map_err(|e| format!("{}: {e}", dir.display()))?;
    }
    let tmp = path.with_extension("json.tmp");
    let body = serde_json::to_string(&StopFile { stopped }).map_err(|e| e.to_string())?;
    std::fs::write(&tmp, body + "\n").map_err(|e| format!("{}: {e}", tmp.display()))?;
    std::fs::rename(&tmp, path).map_err(|e| format!("{}: {e}", path.display()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::TimeZone;

    fn ny() -> Tz {
        "America/New_York".parse().unwrap()
    }

    fn at(tz: Tz, y: i32, mo: u32, d: u32, h: u32, mi: u32) -> DateTime<Tz> {
        tz.with_ymd_and_hms(y, mo, d, h, mi, 0).single().unwrap()
    }

    fn day(d: u32) -> NaiveDate {
        NaiveDate::from_ymd_opt(2026, 10, d).unwrap()
    }

    #[test]
    fn next_press_before_and_after() {
        let n = next_press((5, 30), at(ny(), 2026, 10, 22, 15, 8));
        assert_eq!(n, at(ny(), 2026, 10, 23, 5, 30));
        let n = next_press((5, 30), at(ny(), 2026, 10, 22, 4, 0));
        assert_eq!(n, at(ny(), 2026, 10, 22, 5, 30));
    }

    #[test]
    fn countdown_copy() {
        let from = Utc.with_ymd_and_hms(2026, 10, 22, 0, 0, 0).unwrap();
        let secs = |s: i64| from + chrono::Duration::seconds(s);
        assert_eq!(
            countdown(from, secs(14 * 3600 + 22 * 60 + 59)),
            "in 14 h 22 m"
        );
        assert_eq!(countdown(from, secs(38 * 60)), "in 38 m");
        assert_eq!(countdown(from, secs(-1)), "now");
        assert_eq!(countdown(from, secs(0)), "now");
    }

    #[test]
    fn clock_change_is_absolute() {
        let london: Tz = "Europe/London".parse().unwrap();
        let now = at(london, 2026, 10, 24, 23, 0);
        let n = next_press((5, 30), now);
        assert_eq!(
            countdown(now.with_timezone(&Utc), n.with_timezone(&Utc)),
            "in 7 h 30 m"
        );
    }

    #[test]
    fn spring_gap() {
        // 02:30 doesn't exist on 2026-03-08 in New York: the first valid instant is 03:00 EDT
        let n = next_press((2, 30), at(ny(), 2026, 3, 7, 23, 0));
        assert_eq!(
            n.with_timezone(&Utc),
            Utc.with_ymd_and_hms(2026, 3, 8, 7, 0, 0).unwrap()
        );
    }

    #[test]
    fn stop_after_todays_press_stops_tomorrow() {
        let p = press(Some((5, 30)), None, at(ny(), 2026, 10, 22, 6, 0));
        assert_eq!(p.next.unwrap().date_naive(), day(23));
        assert_eq!(p.edition, day(23));
        assert_eq!(p.at.as_deref(), Some("05:30"));
        // the last press was this morning's
        assert_eq!(p.since, at(ny(), 2026, 10, 22, 5, 30).fixed_offset());
    }

    #[test]
    fn stopped_edition_moves_on() {
        let p = press(Some((5, 30)), Some(day(23)), at(ny(), 2026, 10, 22, 15, 8));
        assert_eq!(p.edition, day(24));
        assert_eq!(
            p.next.unwrap(),
            at(ny(), 2026, 10, 24, 5, 30).fixed_offset()
        );
        assert_eq!(p.stopped, Some(day(23)));
        assert_eq!(p.since, at(ny(), 2026, 10, 22, 5, 30).fixed_offset());
    }

    #[test]
    fn a_spent_stop_isnt_shown() {
        // Friday's paper was stopped; at 06:00 Friday that morning is over: the bar is back to
        // Saturday's press, with Stop available again
        let p = press(Some((5, 30)), Some(day(23)), at(ny(), 2026, 10, 23, 6, 0));
        assert_eq!(p.stopped, None);
        assert_eq!(p.edition, day(24));
        // a stop for a morning further on isn't the next press either
        let p = press(Some((5, 30)), Some(day(27)), at(ny(), 2026, 10, 22, 15, 8));
        assert_eq!((p.stopped, p.edition), (None, day(23)));
    }

    #[test]
    fn stop_file_round_trip_and_expiry() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("press.json");
        assert_eq!(read_stop(&path, day(22)), None);
        write_stop(&path, Some(day(23))).unwrap();
        assert_eq!(
            std::fs::read_to_string(&path).unwrap().trim(),
            r#"{"stopped":"2026-10-23"}"#
        );
        assert_eq!(read_stop(&path, day(22)), Some(day(23)));
        assert_eq!(read_stop(&path, day(23)), Some(day(23)));
        assert_eq!(read_stop(&path, day(24)), None, "a past stop is ignored");
        write_stop(&path, None).unwrap();
        assert!(!path.exists());
        write_stop(&path, None).unwrap(); // removing nothing is fine
        std::fs::write(&path, "not json").unwrap();
        assert_eq!(read_stop(&path, day(22)), None);
    }

    #[test]
    fn press_lines() {
        let now = at(ny(), 2026, 10, 22, 15, 8);
        let utc = now.with_timezone(&Utc);
        assert_eq!(
            bar_line(&press(Some((5, 30)), None, now), utc),
            "Goes to press at 05:30 · in 14 h 22 m"
        );
        assert_eq!(
            bar_line(&press(Some((5, 30)), Some(day(23)), now), utc),
            "Presses stopped: no paper Friday · next at 05:30 Saturday"
        );
        assert_eq!(bar_line(&press(None, None, now), utc), "No press time set");
    }

    #[test]
    fn no_schedule() {
        let now = at(ny(), 2026, 10, 22, 15, 8);
        let p = press(None, None, now);
        assert_eq!((p.edition, p.next, p.at), (day(22), None, None));
        assert_eq!(p.since, at(ny(), 2026, 10, 22, 0, 0).fixed_offset());
    }
}

mod protocol;
pub use protocol::*;
