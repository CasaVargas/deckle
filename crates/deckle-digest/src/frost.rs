// SPDX-License-Identifier: GPL-3.0-or-later
//! The usual first autumn and last spring frost, from a 30-year climate normal
//! (1991–2020, Open-Meteo's historical archive), fetched once per place and cached in
//! `almanac.json` (almanac design §4.3).

use std::collections::BTreeMap;
use std::path::Path;

use chrono::{Datelike, Duration, NaiveDate};
use serde::{Deserialize, Serialize};

use crate::fetch::Net;

/// A day with a low at or below this is a frost day.
const FROST_C: f64 = 0.0;
/// Years (of 30) that must see frost for a place to have a "usual" date.
const MIN_YEARS: usize = 15;
/// The line shows this many days ahead of the usual date.
const LEAD_DAYS: i64 = 45;

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Usual {
    /// (month, day)
    pub first_autumn: Option<(u32, u32)>,
    pub last_spring: Option<(u32, u32)>,
}

pub fn url(lat: f64, lon: f64) -> String {
    format!(
        "https://archive-api.open-meteo.com/v1/archive?latitude={lat}&longitude={lon}\
         &start_date=1991-01-01&end_date=2020-12-31&daily=temperature_2m_min&timezone=auto"
    )
}

/// Midsummer, where each frost season is measured from: July 1 north, January 1 south.
fn midsummer(year: i32, lat: f64) -> NaiveDate {
    let m = if lat >= 0.0 { 7 } else { 1 };
    NaiveDate::from_ymd_opt(year, m, 1).expect("valid")
}

/// The lower median of day offsets, mapped back to a (month, day) from a non-leap year.
fn usual_day(mut offsets: Vec<i64>, lat: f64) -> Option<(u32, u32)> {
    if offsets.len() < MIN_YEARS {
        return None;
    }
    offsets.sort_unstable();
    let day = midsummer(2001, lat) + Duration::days(offsets[(offsets.len() - 1) / 2]);
    Some((day.month(), day.day()))
}

pub fn usual_from_series(days: &[(NaiveDate, f64)], lat: f64) -> Usual {
    let (Some(&(start, _)), Some(&(end, _))) = (days.first(), days.last()) else {
        return Usual::default();
    };
    // A frost season runs from midsummer to the next: July to June up north, the calendar
    // year down south. A January frost in a mild winter belongs to the season before.
    let season_of = |day: NaiveDate| {
        let y = day.year();
        if day >= midsummer(y, lat) { y } else { y - 1 }
    };
    let mut first: BTreeMap<i32, NaiveDate> = BTreeMap::new();
    let mut last: BTreeMap<i32, NaiveDate> = BTreeMap::new();
    for &(day, low) in days {
        if low > FROST_C {
            continue;
        }
        let s = season_of(day);
        first.entry(s).or_insert(day);
        last.insert(s, day);
    }
    // Only seasons the record covers from end to end.
    let whole =
        |s: &i32| midsummer(*s, lat) >= start && midsummer(s + 1, lat) - Duration::days(1) <= end;
    let offsets = |m: &BTreeMap<i32, NaiveDate>| {
        m.iter()
            .filter(|(s, _)| whole(s))
            .map(|(s, d)| (*d - midsummer(*s, lat)).num_days())
            .collect::<Vec<_>>()
    };
    Usual {
        first_autumn: usual_day(offsets(&first), lat),
        last_spring: usual_day(offsets(&last), lat),
    }
}

#[derive(Deserialize)]
struct Archive {
    daily: ArchiveDaily,
}

#[derive(Deserialize)]
struct ArchiveDaily {
    time: Vec<String>,
    temperature_2m_min: Vec<Option<f64>>,
}

fn parse(json: &[u8]) -> Result<Vec<(NaiveDate, f64)>, String> {
    let a: Archive = serde_json::from_slice(json).map_err(|e| e.to_string())?;
    Ok(a.daily
        .time
        .iter()
        .zip(a.daily.temperature_2m_min)
        .filter_map(|(t, v)| Some((NaiveDate::parse_from_str(t, "%Y-%m-%d").ok()?, v?)))
        .collect())
}

/// The usual frost dates at (`lat`, `lon`), from `cache` or fetched once and stored there.
/// A failed fetch isn't cached, so the next build tries again.
pub fn usual(lat: f64, lon: f64, cache: &Path, net: &dyn Net) -> Result<Usual, String> {
    let key = format!("{lat:.1},{lon:.1}");
    let mut map: BTreeMap<String, Usual> = std::fs::read(cache)
        .ok()
        .and_then(|b| serde_json::from_slice(&b).ok())
        .unwrap_or_default();
    if let Some(u) = map.get(&key) {
        return Ok(u.clone());
    }
    let u = usual_from_series(&parse(&net.get(&url(lat, lon))?)?, lat);
    map.insert(key, u.clone());
    // The cache only saves a request: a failed write still prints the line.
    if let Ok(bytes) = serde_json::to_vec_pretty(&map) {
        let tmp = cache.with_extension("json.tmp");
        if std::fs::write(&tmp, bytes).is_ok() {
            let _ = std::fs::rename(&tmp, cache);
        }
    }
    Ok(u)
}

/// "First frost usually Oct 29 here", from 45 days before the usual date until it.
pub fn line(u: &Usual, date: NaiveDate) -> Option<String> {
    [(u.first_autumn, "First"), (u.last_spring, "Last")]
        .into_iter()
        .find_map(|(md, which)| {
            let (m, d) = md?;
            [date.year(), date.year() + 1].into_iter().find_map(|y| {
                let t = NaiveDate::from_ymd_opt(y, m, d)?;
                (0..=LEAD_DAYS)
                    .contains(&(t - date).num_days())
                    .then(|| format!("{which} frost usually {} here", t.format("%b %-d")))
            })
        })
}
