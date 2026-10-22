// SPDX-License-Identifier: GPL-3.0-or-later
//! The moon, computed locally: today's phase from a reference new moon, and exact new and
//! full moons with their traditional names (Meeus, *Astronomical Algorithms*, ch. 49;
//! almanac design §4.4).

use chrono::{DateTime, Datelike, Duration, NaiveDate, NaiveDateTime, TimeZone, Utc};
use chrono_tz::Tz;

use crate::seasons::{Season, from_jde, instant};

const SYNODIC_DAYS: f64 = 29.530_588_853;

pub struct Moon {
    pub name: &'static str,
    /// Percent of the disc lit, 0–100.
    pub illumination: u32,
    pub age_days: f64,
}

/// The phase at local noon on `date`.
pub fn phase(date: NaiveDate) -> Moon {
    // New moon of 2000-01-06 18:14 UTC.
    let reference = NaiveDate::from_ymd_opt(2000, 1, 6)
        .and_then(|d| d.and_hms_opt(18, 14, 0))
        .expect("valid reference date");
    let noon: NaiveDateTime = date.and_hms_opt(12, 0, 0).expect("noon exists");
    let days = (noon - reference).num_seconds() as f64 / 86_400.0;
    let age = days.rem_euclid(SYNODIC_DAYS);
    let fraction = age / SYNODIC_DAYS;
    let illumination = ((1.0 - (fraction * std::f64::consts::TAU).cos()) / 2.0 * 100.0).round();
    let name = match age {
        a if a < 1.85 => "New moon",
        a if a < 5.54 => "Waxing crescent",
        a if a < 9.23 => "First quarter",
        a if a < 12.92 => "Waxing gibbous",
        a if a < 16.61 => "Full moon",
        a if a < 20.30 => "Waning gibbous",
        a if a < 23.99 => "Last quarter",
        a if a < 27.68 => "Waning crescent",
        _ => "New moon",
    };
    Moon {
        name,
        illumination: illumination as u32,
        age_days: age,
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Phase {
    New,
    Full,
}

/// Periodic terms shared by new and full moon (Meeus 49, after the first fourteen).
const SMALL_TERMS: [(f64, [f64; 4]); 10] = [
    // (coefficient, [M, M', F, Ω] multipliers)
    (-0.00007, [2.0, 1.0, 0.0, 0.0]),
    (0.00004, [0.0, 2.0, -2.0, 0.0]),
    (0.00004, [3.0, 0.0, 0.0, 0.0]),
    (0.00003, [1.0, 1.0, -2.0, 0.0]),
    (0.00003, [0.0, 2.0, 2.0, 0.0]),
    (-0.00003, [1.0, 1.0, 2.0, 0.0]),
    (0.00003, [-1.0, 1.0, 2.0, 0.0]),
    (-0.00002, [-1.0, 1.0, -2.0, 0.0]),
    (-0.00002, [1.0, 3.0, 0.0, 0.0]),
    (0.00002, [0.0, 4.0, 0.0, 0.0]),
];

/// The phase instant for lunation `k` (integer: new moon; +0.5: full moon).
fn phase_instant(k: f64, phase: Phase) -> DateTime<Utc> {
    let t = k / 1236.85;
    let jde = 2451550.09766 + 29.530588861 * k + 0.00015437 * t * t - 0.00000015 * t.powi(3)
        + 0.00000000073 * t.powi(4);
    let e = 1.0 - 0.002516 * t - 0.0000074 * t * t;
    let m = (2.5534 + 29.1053567 * k - 0.0000014 * t * t - 0.00000011 * t.powi(3)).to_radians();
    let mp = (201.5643 + 385.81693528 * k + 0.0107582 * t * t + 0.00001238 * t.powi(3)
        - 0.000000058 * t.powi(4))
    .to_radians();
    let f = (160.7108 + 390.67050284 * k - 0.0016118 * t * t - 0.00000227 * t.powi(3)
        + 0.000000011 * t.powi(4))
    .to_radians();
    let om = (124.7746 - 1.56375588 * k + 0.0020672 * t * t + 0.00000215 * t.powi(3)).to_radians();
    let (c1, c2, c3, c4, c5, c6, c7) = match phase {
        Phase::New => (
            -0.40720, 0.17241, 0.01608, 0.01039, 0.00739, -0.00514, 0.00208,
        ),
        Phase::Full => (
            -0.40614, 0.17302, 0.01614, 0.01043, 0.00734, -0.00515, 0.00209,
        ),
    };
    let mut corr = c1 * mp.sin()
        + c2 * e * m.sin()
        + c3 * (2.0 * mp).sin()
        + c4 * (2.0 * f).sin()
        + c5 * e * (mp - m).sin()
        + c6 * e * (mp + m).sin()
        + c7 * e * e * (2.0 * m).sin()
        - 0.00111 * (mp - 2.0 * f).sin()
        - 0.00057 * (mp + 2.0 * f).sin()
        + 0.00056 * e * (2.0 * mp + m).sin()
        - 0.00042 * (3.0 * mp).sin()
        + 0.00042 * e * (m + 2.0 * f).sin()
        + 0.00038 * e * (m - 2.0 * f).sin()
        - 0.00024 * e * (2.0 * mp - m).sin()
        - 0.00017 * om.sin();
    for (c, [a, b, cc, d]) in SMALL_TERMS {
        corr += c * (a * m + b * mp + cc * f + d * om).sin();
    }
    from_jde(jde + corr)
}

/// Every new and full moon in `[from, to)`, in order.
pub fn phases_between(from: DateTime<Utc>, to: DateTime<Utc>) -> Vec<(Phase, DateTime<Utc>)> {
    let year_frac = |t: DateTime<Utc>| f64::from(t.year()) + f64::from(t.ordinal0()) / 365.25;
    let mut k = ((year_frac(from) - 2000.0) * 12.3685).floor() - 1.0;
    let mut out = Vec::new();
    loop {
        let new = phase_instant(k, Phase::New);
        if new >= to {
            break;
        }
        let full = phase_instant(k + 0.5, Phase::Full);
        for (p, t) in [(Phase::New, new), (Phase::Full, full)] {
            if t >= from && t < to {
                out.push((p, t));
            }
        }
        k += 1.0;
    }
    out
}

fn full_moons(from: DateTime<Utc>, to: DateTime<Utc>) -> Vec<DateTime<Utc>> {
    phases_between(from, to)
        .into_iter()
        .filter(|(p, _)| *p == Phase::Full)
        .map(|(_, t)| t)
        .collect()
}

const MONTH_NAMES: [&str; 12] = [
    "Wolf Moon",
    "Snow Moon",
    "Worm Moon",
    "Pink Moon",
    "Flower Moon",
    "Strawberry Moon",
    "Buck Moon",
    "Sturgeon Moon",
    "Corn Moon",
    "Hunter's Moon",
    "Beaver Moon",
    "Cold Moon",
];

/// The traditional name of the full moon at `at`; northern hemisphere only.
pub fn full_moon_name(at: DateTime<Utc>, lat: f64, tz: Tz) -> Option<&'static str> {
    if lat < 0.0 {
        return None;
    }
    let equinox = instant(at.year(), Season::SeptemberEquinox);
    let around = full_moons(equinox - Duration::days(45), equinox + Duration::days(75));
    let close = |a: &DateTime<Utc>, b: &DateTime<Utc>| (*a - *b).num_minutes().abs() < 60;
    if let Some(i) = around
        .iter()
        .enumerate()
        .min_by_key(|(_, t)| (**t - equinox).num_seconds().abs())
        .map(|(i, _)| i)
    {
        if close(&around[i], &at) {
            return Some("Harvest Moon");
        }
        if around.get(i + 1).is_some_and(|t| close(t, &at)) {
            return Some("Hunter's Moon");
        }
    }
    let local = at.with_timezone(&tz).date_naive();
    let previous = full_moons(at - Duration::days(32), at - Duration::days(1));
    if previous.last().is_some_and(|p| {
        let p = p.with_timezone(&tz).date_naive();
        (p.year(), p.month()) == (local.year(), local.month())
    }) {
        return Some("Blue Moon");
    }
    Some(MONTH_NAMES[local.month0() as usize])
}

/// "Full moon Sunday, the Hunter's Moon" / "New moon tomorrow": the next phase from the
/// start of the local `date`.
pub fn next_line(date: NaiveDate, tz: Tz, lat: f64) -> String {
    let start = tz
        .from_local_datetime(&date.and_hms_opt(0, 0, 0).expect("midnight"))
        .earliest()
        .map(|t| t.with_timezone(&Utc))
        .unwrap_or_else(|| Utc.from_utc_datetime(&date.and_hms_opt(0, 0, 0).expect("midnight")));
    let Some((phase, at)) = phases_between(start, start + Duration::days(31))
        .into_iter()
        .next()
    else {
        return String::new();
    };
    let day = at.with_timezone(&tz).date_naive();
    let what = match phase {
        Phase::New => "New moon",
        Phase::Full => "Full moon",
    };
    let when = match (day - date).num_days() {
        0 if phase == Phase::Full => "tonight".to_owned(),
        0 => "today".to_owned(),
        1 => "tomorrow".to_owned(),
        2..=6 => day.format("%A").to_string(),
        _ => day.format("on %B %-d").to_string(),
    };
    match (phase, full_moon_name(at, lat, tz)) {
        (Phase::Full, Some(name)) => format!("{what} {when}, the {name}"),
        _ => format!("{what} {when}"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn known_full_and_new_moons() {
        // Full moon 2024-04-23, new moon 2024-04-08 (total eclipse day).
        let full = phase(NaiveDate::from_ymd_opt(2024, 4, 23).unwrap());
        assert_eq!(full.name, "Full moon");
        assert!(full.illumination > 95);
        let new = phase(NaiveDate::from_ymd_opt(2024, 4, 8).unwrap());
        assert_eq!(new.name, "New moon");
        assert!(new.illumination < 5);
    }
}
