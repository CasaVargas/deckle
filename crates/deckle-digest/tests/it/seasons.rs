// SPDX-License-Identifier: GPL-3.0-or-later
//! Equinoxes, solstices and clock changes (almanac design §4.6).

use std::collections::BTreeMap;

use chrono::{NaiveDate, NaiveDateTime, NaiveTime};
use chrono_tz::Tz;
use deckle_digest::config::Units;
use deckle_digest::seasons::{ClockChange, Season, clock_change, coming_up, instant};
use serde::Deserialize;

#[derive(Deserialize)]
struct Row {
    phenom: String,
    utc: String,
}

fn d(y: i32, m: u32, day: u32) -> NaiveDate {
    NaiveDate::from_ymd_opt(y, m, day).unwrap()
}

fn tz(name: &str) -> Tz {
    name.parse().unwrap()
}

#[test]
fn matches_usno_within_five_minutes() {
    let years: BTreeMap<i32, Vec<Row>> =
        serde_json::from_str(include_str!("../fixtures/usno/seasons.json")).unwrap();
    assert_eq!(years.len(), 7);
    let order = [
        Season::MarchEquinox,
        Season::JuneSolstice,
        Season::SeptemberEquinox,
        Season::DecemberSolstice,
    ];
    for (year, rows) in years {
        assert_eq!(rows.len(), 4);
        for (row, season) in rows.iter().zip(order) {
            let want = NaiveDateTime::parse_from_str(&row.utc, "%Y-%m-%dT%H:%M").unwrap();
            let got = instant(year, season).naive_utc();
            let diff = (got - want).num_seconds().abs();
            assert!(
                diff <= 5 * 60,
                "{year} {}: got {got}, USNO {want}",
                row.phenom
            );
        }
    }
}

#[test]
fn clock_changes_2026() {
    let t = |h| NaiveTime::from_hms_opt(h, 0, 0).unwrap();
    assert_eq!(
        clock_change(d(2026, 10, 29), tz("America/New_York"), 7),
        Some(ClockChange {
            date: d(2026, 11, 1),
            at: t(2),
            forward: false
        })
    );
    assert_eq!(
        clock_change(d(2026, 3, 25), tz("Europe/London"), 7),
        Some(ClockChange {
            date: d(2026, 3, 29),
            at: t(1),
            forward: true
        })
    );
    assert_eq!(clock_change(d(2026, 3, 25), tz("Asia/Tokyo"), 7), None);
    // Outside the window.
    assert_eq!(
        clock_change(d(2026, 10, 20), tz("America/New_York"), 7),
        None
    );
}

#[test]
fn coming_up_wording() {
    let ny = tz("America/New_York");
    let imp = Units::Imperial;
    assert_eq!(
        coming_up(d(2026, 12, 17), ny, 40.7, imp).as_deref(),
        Some("Winter solstice Monday: the shortest day")
    );
    // 2026-12-21 20:50 UTC is already Tuesday in Sydney.
    assert_eq!(
        coming_up(d(2026, 12, 17), tz("Australia/Sydney"), -33.9, imp).as_deref(),
        Some("Summer solstice Tuesday: the longest day")
    );
    // 2026-09-23 00:05 UTC is Tuesday evening in New York.
    assert_eq!(
        coming_up(d(2026, 9, 20), ny, 40.7, imp).as_deref(),
        Some("Autumn equinox Tuesday: day and night about equal")
    );
    assert_eq!(
        coming_up(d(2026, 10, 29), ny, 40.7, imp).as_deref(),
        Some("Clocks go back an hour Sunday at 2 AM")
    );
    assert_eq!(
        coming_up(d(2026, 10, 29), ny, 40.7, Units::Metric).as_deref(),
        Some("Clocks go back an hour Sunday at 02:00")
    );
    assert_eq!(
        coming_up(d(2026, 12, 21), ny, 40.7, imp).as_deref(),
        Some("Winter solstice today: the shortest day")
    );
    assert_eq!(
        coming_up(d(2026, 12, 20), ny, 40.7, imp).as_deref(),
        Some("Winter solstice tomorrow: the shortest day")
    );
    assert_eq!(coming_up(d(2026, 2, 1), ny, 40.7, imp), None);
}

#[test]
fn nearer_event_wins() {
    let london = tz("Europe/London");
    // 2029: equinox March 20, clocks forward March 25.
    assert_eq!(
        coming_up(d(2029, 3, 19), london, 51.5, Units::Metric).as_deref(),
        Some("Spring equinox tomorrow: day and night about equal")
    );
    assert_eq!(
        coming_up(d(2029, 3, 23), london, 51.5, Units::Metric).as_deref(),
        Some("Clocks go forward an hour Sunday at 01:00")
    );
}
