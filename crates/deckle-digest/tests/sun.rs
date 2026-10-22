// SPDX-License-Identifier: GPL-3.0-or-later
//! Sunrise and sunset against USNO's published times (almanac design §4.2).

use chrono::{Duration, NaiveDate, NaiveTime, TimeZone, Timelike};
use chrono_tz::Tz;
use deckle_digest::config::Units;
use deckle_digest::sun::{SunDay, day_length, daylight_line, sun_day};
use deckle_digest::tz::clock;
use serde::Deserialize;

#[derive(Deserialize)]
struct Row {
    place: String,
    date: String,
    lat: f64,
    lon: f64,
    tz: String,
    rise: String,
    set: String,
}

fn minutes(t: NaiveTime) -> i64 {
    i64::from(t.hour() * 60 + t.minute()) + i64::from(t.second() >= 30)
}

#[test]
fn matches_usno_within_two_minutes() {
    let rows: Vec<Row> =
        serde_json::from_str(include_str!("fixtures/usno/sun.json")).expect("fixture");
    assert_eq!(rows.len(), 12);
    for r in rows {
        let tz: Tz = r.tz.parse().unwrap();
        let date = NaiveDate::parse_from_str(&r.date, "%Y-%m-%d").unwrap();
        let SunDay::Normal { rise, set } = sun_day(date, r.lat, r.lon, tz) else {
            panic!("{} {}: not a normal day", r.place, r.date);
        };
        for (got, want, what) in [(rise, &r.rise, "rise"), (set, &r.set, "set")] {
            let want = NaiveTime::parse_from_str(want, "%H:%M").unwrap();
            assert_eq!(got.date_naive(), date, "{} {} {what} date", r.place, r.date);
            let diff = (minutes(got.time()) - minutes(want)).abs();
            assert!(
                diff <= 2,
                "{} {} {what}: got {}, USNO {want}",
                r.place,
                r.date,
                got.time()
            );
        }
    }
}

#[test]
fn tromso_polar_cases() {
    let tz: Tz = "Europe/Oslo".parse().unwrap();
    let d = |m, day| NaiveDate::from_ymd_opt(2026, m, day).unwrap();
    assert_eq!(sun_day(d(6, 21), 69.65, 18.96, tz), SunDay::AlwaysUp);
    assert_eq!(sun_day(d(12, 21), 69.65, 18.96, tz), SunDay::AlwaysDown);
}

fn day_of(len: Duration) -> SunDay {
    let tz: Tz = "America/New_York".parse().unwrap();
    let rise = tz.with_ymd_and_hms(2026, 10, 22, 7, 0, 0).unwrap();
    SunDay::Normal {
        rise,
        set: rise + len,
    }
}

#[test]
fn daylight_wording() {
    let hm = |h, m| Duration::hours(h) + Duration::minutes(m);
    let today = day_of(hm(11, 3));
    assert_eq!(day_length(&today), hm(11, 3));
    assert_eq!(
        daylight_line(&today, &day_of(hm(11, 5))),
        "11 h 03 m, 2 min less"
    );
    assert_eq!(
        daylight_line(&today, &day_of(hm(11, 1))),
        "11 h 03 m, 2 min more"
    );
    let close = day_of(hm(11, 3) - Duration::seconds(30));
    assert_eq!(daylight_line(&today, &close), "11 h 03 m, about the same");
    assert_eq!(
        daylight_line(&SunDay::AlwaysUp, &SunDay::AlwaysUp),
        "The sun doesn't set today"
    );
    assert_eq!(
        daylight_line(&SunDay::AlwaysDown, &today),
        "The sun doesn't rise today"
    );
}

#[test]
fn clock_by_units() {
    let t = NaiveTime::from_hms_opt(19, 28, 0).unwrap();
    assert_eq!(clock(t, Units::Imperial), "7:28 PM");
    assert_eq!(clock(t, Units::Metric), "19:28");
}
