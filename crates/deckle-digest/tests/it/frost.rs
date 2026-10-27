// SPDX-License-Identifier: GPL-3.0-or-later
//! The usual first and last frost from a 30-year climate series (almanac design §4.3).

use std::collections::BTreeMap;

use chrono::{Datelike, Duration, NaiveDate};
use deckle_digest::frost::{Usual, line, url, usual, usual_from_series};

fn d(y: i32, m: u32, day: u32) -> NaiveDate {
    NaiveDate::from_ymd_opt(y, m, day).unwrap()
}

/// 1991–2020 lows: 10 °C, except frost on `frosty(year, date)`.
fn series(frosty: impl Fn(usize, NaiveDate) -> bool) -> Vec<(NaiveDate, f64)> {
    let mut out = Vec::new();
    let mut day = d(1991, 1, 1);
    while day <= d(2020, 12, 31) {
        let i = (day.year() - 1991) as usize;
        out.push((day, if frosty(i, day) { -2.0 } else { 10.0 }));
        day += Duration::days(1);
    }
    out
}

/// Per year: 7 early, 15 on the day, 8 late.
fn spread(i: usize, early: NaiveDate, on: NaiveDate, late: NaiveDate) -> NaiveDate {
    match i {
        0..7 => early,
        7..22 => on,
        _ => late,
    }
}

/// Northern winters: frost from the first autumn frost through the last spring frost.
fn northern(i: usize, day: NaiveDate) -> bool {
    let y = day.year();
    let first = spread(i, d(y, 10, 20), d(y, 10, 29), d(y, 11, 7));
    let last = spread(i, d(y, 4, 3), d(y, 4, 12), d(y, 4, 21));
    day >= first || day <= last
}

#[test]
fn medians_from_synthetic_series() {
    let u = usual_from_series(&series(northern), 40.7);
    assert_eq!(u.first_autumn, Some((10, 29)));
    assert_eq!(u.last_spring, Some((4, 12)));
}

#[test]
fn needs_fifteen_years() {
    let u = usual_from_series(&series(|i, day| i < 14 && northern(i, day)), 40.7);
    assert_eq!(
        u,
        Usual {
            first_autumn: None,
            last_spring: None
        }
    );
    let u = usual_from_series(&series(|i, day| i < 15 && northern(i, day)), 40.7);
    assert!(u.first_autumn.is_some() && u.last_spring.is_some());
}

#[test]
fn southern_midsummer() {
    // Frost from May 20 to September 15 each year: May is autumn down south.
    let s = series(|_, day| {
        let y = day.year();
        day >= d(y, 5, 20) && day <= d(y, 9, 15)
    });
    let u = usual_from_series(&s, -35.0);
    assert_eq!(u.first_autumn, Some((5, 20)));
    assert_eq!(u.last_spring, Some((9, 15)));
}

#[test]
fn window_edges() {
    let u = Usual {
        first_autumn: Some((10, 29)),
        last_spring: Some((4, 12)),
    };
    let first = Some("First frost usually Oct 29 here".to_owned());
    assert_eq!(line(&u, d(2026, 9, 14)), first);
    assert_eq!(line(&u, d(2026, 9, 13)), None);
    assert_eq!(line(&u, d(2026, 10, 29)), first);
    assert_eq!(line(&u, d(2026, 10, 30)), None);
    assert_eq!(
        line(&u, d(2026, 3, 1)).as_deref(),
        Some("Last frost usually Apr 12 here")
    );
    assert_eq!(line(&u, d(2026, 6, 1)), None);
}

#[test]
fn southern_window_across_new_year() {
    let u = Usual {
        first_autumn: None,
        last_spring: Some((1, 20)),
    };
    assert_eq!(
        line(&u, d(2026, 12, 20)).as_deref(),
        Some("Last frost usually Jan 20 here")
    );
}

fn archive_json(s: &[(NaiveDate, f64)]) -> Vec<u8> {
    let time: Vec<String> = s.iter().map(|(t, _)| t.to_string()).collect();
    let min: Vec<f64> = s.iter().map(|(_, v)| *v).collect();
    serde_json::json!({ "daily": { "time": time, "temperature_2m_min": min } })
        .to_string()
        .into_bytes()
}

fn read_cache(p: &std::path::Path) -> BTreeMap<String, Usual> {
    serde_json::from_slice(&std::fs::read(p).unwrap()).unwrap()
}

#[test]
fn cache_hit_skips_net() {
    let dir = tempfile::tempdir().unwrap();
    let cache = dir.path().join("almanac.json");
    let want = Usual {
        first_autumn: Some((11, 2)),
        last_spring: Some((4, 1)),
    };
    std::fs::write(
        &cache,
        serde_json::to_vec(&BTreeMap::from([("40.7,-74.0", &want)])).unwrap(),
    )
    .unwrap();
    let net = |u: &str| -> Result<Vec<u8>, String> { panic!("fetched {u}") };
    assert_eq!(usual(40.71, -74.01, &cache, &net).unwrap(), want);
}

#[test]
fn moved_location_refetches() {
    let dir = tempfile::tempdir().unwrap();
    let cache = dir.path().join("almanac.json");
    let london = Usual {
        first_autumn: Some((11, 20)),
        last_spring: Some((3, 10)),
    };
    std::fs::write(
        &cache,
        serde_json::to_vec(&BTreeMap::from([("51.5,-0.1", &london)])).unwrap(),
    )
    .unwrap();
    let body = archive_json(&series(northern));
    let want_url = url(40.71, -74.01);
    let net = |u: &str| -> Result<Vec<u8>, String> {
        assert_eq!(u, want_url);
        Ok(body.clone())
    };
    let got = usual(40.71, -74.01, &cache, &net).unwrap();
    assert_eq!(got.first_autumn, Some((10, 29)));
    let stored = read_cache(&cache);
    assert_eq!(stored.len(), 2);
    assert_eq!(stored["51.5,-0.1"], london);
    assert_eq!(stored["40.7,-74.0"], got);
}

#[test]
fn fetch_failure_not_cached() {
    let dir = tempfile::tempdir().unwrap();
    let cache = dir.path().join("almanac.json");
    std::fs::write(&cache, b"{}").unwrap();
    let net = |_: &str| -> Result<Vec<u8>, String> { Err("timeout".into()) };
    assert!(usual(40.71, -74.01, &cache, &net).is_err());
    assert_eq!(std::fs::read(&cache).unwrap(), b"{}");
}

#[test]
fn archive_url() {
    assert_eq!(
        url(40.71, -74.01),
        "https://archive-api.open-meteo.com/v1/archive?latitude=40.71&longitude=-74.01\
         &start_date=1991-01-01&end_date=2020-12-31&daily=temperature_2m_min&timezone=auto"
    );
}

#[test]
fn mild_winters_count_january_frosts_as_autumn() {
    // Frost every winter, but after New Year in 20 of 30 seasons (Gulf Coast, Florida).
    let s = series(|i, day| {
        let y = day.year();
        if i < 20 {
            day == d(y, 1, 10)
        } else {
            day == d(y, 12, 20)
        }
    });
    let u = usual_from_series(&s, 30.0);
    assert_eq!(u.first_autumn, Some((1, 10)), "{u:?}");
}
