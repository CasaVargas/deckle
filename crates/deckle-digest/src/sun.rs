// SPDX-License-Identifier: GPL-3.0-or-later
//! Sunrise, sunset and day length, worked out locally with NOAA's solar equations
//! (almanac design §4.2), so the almanac prints even when the forecast fails.

use chrono::{DateTime, Duration, NaiveDate, TimeZone, Utc};
use chrono_tz::Tz;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SunDay {
    Normal {
        rise: DateTime<Tz>,
        set: DateTime<Tz>,
    },
    /// Polar day.
    AlwaysUp,
    /// Polar night.
    AlwaysDown,
}

/// Altitude of the sun's centre at rise and set: refraction plus the disc's radius.
const HORIZON_DEG: f64 = -0.833;

/// Declination (degrees) and equation of time (minutes) at `jd` (NOAA's spreadsheet).
fn solar(jd: f64) -> (f64, f64) {
    let jc = (jd - 2451545.0) / 36525.0;
    let l0 = (280.46646 + jc * (36000.76983 + jc * 0.0003032)).rem_euclid(360.0);
    let m = 357.52911 + jc * (35999.05029 - 0.0001537 * jc);
    let e = 0.016708634 - jc * (0.000042037 + 0.0000001267 * jc);
    let mr = m.to_radians();
    let c = mr.sin() * (1.914602 - jc * (0.004817 + 0.000014 * jc))
        + (2.0 * mr).sin() * (0.019993 - 0.000101 * jc)
        + (3.0 * mr).sin() * 0.000289;
    let omega = (125.04 - 1934.136 * jc).to_radians();
    let app_long = (l0 + c - 0.00569 - 0.00478 * omega.sin()).to_radians();
    let mean_obliq =
        23.0 + (26.0 + (21.448 - jc * (46.815 + jc * (0.00059 - jc * 0.001813))) / 60.0) / 60.0;
    let obliq = (mean_obliq + 0.00256 * omega.cos()).to_radians();
    let decl = (obliq.sin() * app_long.sin()).asin().to_degrees();
    let y = (obliq / 2.0).tan().powi(2);
    let l0r = l0.to_radians();
    let eqtime = 4.0
        * (y * (2.0 * l0r).sin() - 2.0 * e * mr.sin() + 4.0 * e * y * mr.sin() * (2.0 * l0r).cos()
            - 0.5 * y * y * (4.0 * l0r).sin()
            - 1.25 * e * e * (2.0 * mr).sin())
        .to_degrees();
    (decl, eqtime)
}

/// Cosine of the hour angle at rise/set; outside [-1, 1] the sun never crosses the horizon.
fn cos_hour_angle(lat: f64, decl: f64) -> f64 {
    let (lat, decl) = (lat.to_radians(), decl.to_radians());
    (90.0 - HORIZON_DEG).to_radians().cos() / (lat.cos() * decl.cos()) - lat.tan() * decl.tan()
}

fn julian(t: DateTime<Utc>) -> f64 {
    t.timestamp() as f64 / 86400.0 + 2440587.5
}

/// Sunrise and sunset on the local `date` at (`lat`, `lon`), in `tz`.
pub fn sun_day(date: NaiveDate, lat: f64, lon: f64, tz: Tz) -> SunDay {
    // Minutes are counted from 00:00 UTC of the local date; events land near local noon.
    let base = Utc.from_utc_datetime(&date.and_hms_opt(0, 0, 0).expect("midnight"));
    let at = |minutes: f64| base + Duration::seconds((minutes * 60.0).round() as i64);
    let noon = 720.0 - 4.0 * lon;
    let (decl, _) = solar(julian(at(noon)));
    let c = cos_hour_angle(lat, decl);
    if c > 1.0 {
        return SunDay::AlwaysDown;
    }
    if c < -1.0 {
        return SunDay::AlwaysUp;
    }
    // Refine each event by re-evaluating the sun at its own time.
    let event = |sign: f64| {
        let mut minutes = noon;
        for _ in 0..3 {
            let (decl, eqtime) = solar(julian(at(minutes)));
            let h = cos_hour_angle(lat, decl)
                .clamp(-1.0, 1.0)
                .acos()
                .to_degrees();
            minutes = 720.0 - 4.0 * (lon + sign * h) - eqtime;
        }
        at(minutes).with_timezone(&tz)
    };
    SunDay::Normal {
        rise: event(1.0),
        set: event(-1.0),
    }
}

pub fn day_length(s: &SunDay) -> Duration {
    match s {
        SunDay::Normal { rise, set } => *set - *rise,
        SunDay::AlwaysUp => Duration::hours(24),
        SunDay::AlwaysDown => Duration::zero(),
    }
}

/// "11 h 03 m, 2 min less" against yesterday.
pub fn daylight_line(today: &SunDay, yesterday: &SunDay) -> String {
    match today {
        SunDay::AlwaysUp => return "The sun doesn't set today".into(),
        SunDay::AlwaysDown => return "The sun doesn't rise today".into(),
        SunDay::Normal { .. } => {}
    }
    let len = day_length(today);
    let mins = len.num_minutes();
    let diff = (len - day_length(yesterday)).num_seconds();
    let change = if diff.abs() < 60 {
        "about the same".to_owned()
    } else {
        let n = (diff.abs() as f64 / 60.0).round() as i64;
        format!("{n} min {}", if diff > 0 { "more" } else { "less" })
    };
    format!("{} h {:02} m, {change}", mins / 60, mins % 60)
}
