// SPDX-License-Identifier: GPL-3.0-or-later
//! Moon phases against USNO, and their names (almanac design §4.4).

use std::collections::BTreeMap;

use chrono::{DateTime, Duration, NaiveDate, NaiveDateTime, TimeZone, Utc};
use chrono_tz::Tz;
use deckle_digest::moon::{Phase, full_moon_name, next_line, phases_between};
use serde::Deserialize;

#[derive(Deserialize)]
struct Row {
    phase: String,
    utc: String,
}

fn fixture() -> BTreeMap<i32, Vec<(Phase, DateTime<Utc>)>> {
    let raw: BTreeMap<i32, Vec<Row>> =
        serde_json::from_str(include_str!("fixtures/usno/moon.json")).unwrap();
    raw.into_iter()
        .map(|(y, rows)| {
            let rows = rows
                .into_iter()
                .map(|r| {
                    let p = if r.phase == "Full Moon" {
                        Phase::Full
                    } else {
                        Phase::New
                    };
                    let t = NaiveDateTime::parse_from_str(&r.utc, "%Y-%m-%dT%H:%M").unwrap();
                    (p, Utc.from_utc_datetime(&t))
                })
                .collect();
            (y, rows)
        })
        .collect()
}

fn tz(name: &str) -> Tz {
    name.parse().unwrap()
}

fn utc(y: i32, m: u32, d: u32) -> DateTime<Utc> {
    Utc.with_ymd_and_hms(y, m, d, 0, 0, 0).unwrap()
}

#[test]
fn phases_match_usno_local_dates() {
    let want = &fixture()[&2026];
    assert_eq!(want.len(), 25);
    let got = phases_between(utc(2025, 12, 31), utc(2027, 1, 2));
    for (phase, at) in want {
        let hit = got
            .iter()
            .find(|(p, t)| p == phase && (*t - *at).num_minutes().abs() <= 10)
            .unwrap_or_else(|| panic!("{phase:?} at {at}: not found within 10 min"));
        for zone in ["America/New_York", "Australia/Sydney"] {
            let z = tz(zone);
            assert_eq!(
                hit.1.with_timezone(&z).date_naive(),
                at.with_timezone(&z).date_naive()
            );
        }
    }
}

fn full_moons(year: i32) -> Vec<DateTime<Utc>> {
    fixture()[&year]
        .iter()
        .filter(|(p, _)| *p == Phase::Full)
        .map(|(_, t)| *t)
        .collect()
}

#[test]
fn harvest_and_hunters() {
    let ny = tz("America/New_York");
    // (year, Harvest, Hunter's) as UTC dates; 2025's Harvest Moon fell in October.
    for (year, harvest, hunters) in [
        (2025, (10, 7), (11, 5)),
        (2026, (9, 26), (10, 26)),
        (2027, (9, 15), (10, 15)),
    ] {
        let fulls = full_moons(year);
        let on = |(m, d): (u32, u32)| {
            *fulls
                .iter()
                .find(|t| t.date_naive() == NaiveDate::from_ymd_opt(year, m, d).unwrap())
                .unwrap()
        };
        assert_eq!(
            full_moon_name(on(harvest), 40.7, ny),
            Some("Harvest Moon"),
            "{year}"
        );
        assert_eq!(
            full_moon_name(on(hunters), 40.7, ny),
            Some("Hunter's Moon"),
            "{year}"
        );
    }
}

#[test]
fn blue_moon_2023() {
    let fulls = full_moons(2023);
    let aug31 = fulls
        .iter()
        .find(|t| t.date_naive() == NaiveDate::from_ymd_opt(2023, 8, 31).unwrap())
        .unwrap();
    assert_eq!(
        full_moon_name(*aug31, 40.7, tz("America/New_York")),
        Some("Blue Moon")
    );
}

#[test]
fn month_names() {
    let ny = tz("America/New_York");
    let fulls = full_moons(2026);
    assert_eq!(full_moon_name(fulls[0], 40.7, ny), Some("Wolf Moon"));
    let june = fulls
        .iter()
        .find(|t| t.date_naive().to_string() == "2026-06-29")
        .unwrap();
    assert_eq!(full_moon_name(*june, 40.7, ny), Some("Strawberry Moon"));
}

#[test]
fn south_has_no_names() {
    assert_eq!(
        full_moon_name(full_moons(2026)[0], -33.9, tz("Australia/Sydney")),
        None
    );
}

#[test]
fn next_line_wording() {
    let ny = tz("America/New_York");
    // Full moon 2026-10-26 04:12 UTC = Monday Oct 26, 00:12 in New York: the Hunter's Moon.
    let full = NaiveDate::from_ymd_opt(2026, 10, 26).unwrap();
    assert_eq!(
        next_line(full, ny, 40.7),
        "Full moon tonight, the Hunter's Moon"
    );
    assert_eq!(
        next_line(full - Duration::days(1), ny, 40.7),
        "Full moon tomorrow, the Hunter's Moon"
    );
    assert_eq!(
        next_line(full - Duration::days(4), ny, 40.7),
        "Full moon Monday, the Hunter's Moon"
    );
    // The next phase after the 2026-11-24 full moon is the new moon of December 8 (00:52 UTC on the 9th).
    assert_eq!(
        next_line(NaiveDate::from_ymd_opt(2026, 11, 25).unwrap(), ny, 40.7),
        "New moon on December 8"
    );
    // New moon 2026-11-09 07:02 UTC = Monday in New York, four days after Thursday Nov 5.
    assert_eq!(
        next_line(NaiveDate::from_ymd_opt(2026, 11, 5).unwrap(), ny, 40.7),
        "New moon Monday"
    );
    assert_eq!(
        next_line(full, tz("Australia/Sydney"), -33.9),
        "Full moon tonight"
    );
}
