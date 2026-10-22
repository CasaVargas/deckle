// SPDX-License-Identifier: GPL-3.0-or-later
//! Equinoxes and solstices (Meeus, *Astronomical Algorithms*, ch. 27) and the local clock
//! changes, for the almanac's "Coming up" line (almanac design §4.6).

use chrono::{DateTime, Datelike, Duration, NaiveDate, NaiveTime, Offset, TimeZone, Utc};
use chrono_tz::Tz;

use crate::config::Units;
use crate::tz::clock;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Season {
    MarchEquinox,
    JuneSolstice,
    SeptemberEquinox,
    DecemberSolstice,
}

/// TT − UT for the 2020s–2030s, in seconds.
pub(crate) const DELTA_T_SECS: f64 = 69.2;

/// Meeus table 27.C: A, B (degrees), C (degrees per Julian century).
const TERMS: [(f64, f64, f64); 24] = [
    (485.0, 324.96, 1934.136),
    (203.0, 337.23, 32964.467),
    (199.0, 342.08, 20.186),
    (182.0, 27.85, 445267.112),
    (156.0, 73.14, 45036.886),
    (136.0, 171.52, 22518.443),
    (77.0, 222.54, 65928.934),
    (74.0, 296.72, 3034.906),
    (70.0, 243.58, 9037.513),
    (58.0, 119.81, 33718.147),
    (52.0, 297.17, 150.678),
    (50.0, 21.02, 2281.226),
    (45.0, 247.54, 29929.562),
    (44.0, 325.15, 31555.956),
    (29.0, 60.93, 4443.417),
    (18.0, 155.12, 67555.328),
    (17.0, 288.79, 4562.452),
    (16.0, 198.04, 62894.029),
    (14.0, 199.76, 31436.921),
    (12.0, 95.39, 14577.848),
    (12.0, 287.11, 31931.756),
    (12.0, 320.81, 34777.259),
    (9.0, 227.73, 1222.114),
    (8.0, 15.45, 16859.074),
];

/// A Julian Ephemeris Day (TT) as a UTC instant.
pub(crate) fn from_jde(jde: f64) -> DateTime<Utc> {
    let secs = (jde - 2440587.5) * 86400.0 - DELTA_T_SECS;
    Utc.timestamp_opt(secs.round() as i64, 0)
        .single()
        .expect("in range")
}

/// The instant of `s` in `year` (valid 2000–3000).
pub fn instant(year: i32, s: Season) -> DateTime<Utc> {
    let y = (f64::from(year) - 2000.0) / 1000.0;
    let (a, b, c, d, e) = match s {
        Season::MarchEquinox => (2451623.80984, 365242.37404, 0.05169, -0.00411, -0.00057),
        Season::JuneSolstice => (2451716.56767, 365241.62603, 0.00325, 0.00888, -0.00030),
        Season::SeptemberEquinox => (2451810.21715, 365242.01767, -0.11575, 0.00337, 0.00078),
        Season::DecemberSolstice => (2451900.05952, 365242.74049, -0.06223, -0.00823, 0.00032),
    };
    let jde0 = a + y * (b + y * (c + y * (d + y * e)));
    let t = (jde0 - 2451545.0) / 36525.0;
    let w = (35999.373 * t - 2.47).to_radians();
    let dl = 1.0 + 0.0334 * w.cos() + 0.0007 * (2.0 * w).cos();
    let s: f64 = TERMS
        .iter()
        .map(|&(a, b, c)| a * (b + c * t).to_radians().cos())
        .sum();
    from_jde(jde0 + 0.00001 * s / dl)
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ClockChange {
    pub date: NaiveDate,
    /// Wall-clock time the change happens at, before it.
    pub at: NaiveTime,
    pub forward: bool,
}

fn offset_secs(tz: Tz, t: DateTime<Utc>) -> i32 {
    tz.offset_from_utc_datetime(&t.naive_utc())
        .fix()
        .local_minus_utc()
}

/// The first clock change in `tz` from `date` through `within_days` days later.
pub fn clock_change(date: NaiveDate, tz: Tz, within_days: u32) -> Option<ClockChange> {
    // Step through the window a quarter hour at a time, in UTC.
    let start = tz
        .from_local_datetime(&date.and_hms_opt(0, 0, 0)?)
        .earliest()?
        .with_timezone(&Utc);
    let end = start + Duration::days(i64::from(within_days) + 1);
    let mut t = start;
    let mut before = offset_secs(tz, t);
    while t < end {
        let next = t + Duration::minutes(15);
        let after = offset_secs(tz, next);
        if after != before {
            let wall = (next + Duration::seconds(i64::from(before))).naive_utc();
            return Some(ClockChange {
                date: wall.date(),
                at: wall.time(),
                forward: after > before,
            });
        }
        before = after;
        t = next;
    }
    None
}

fn day_word(today: NaiveDate, day: NaiveDate) -> String {
    match (day - today).num_days() {
        0 => "today".into(),
        1 => "tomorrow".into(),
        _ => day.format("%A").to_string(),
    }
}

/// The nearest equinox, solstice or clock change within 7 days, worded for the almanac.
pub fn coming_up(date: NaiveDate, tz: Tz, lat: f64, units: Units) -> Option<String> {
    const WINDOW: i64 = 7;
    let north = lat >= 0.0;
    let mut best: Option<(i64, String)> = None;
    let mut offer = |day: NaiveDate, text: String| {
        let n = (day - date).num_days();
        if (0..=WINDOW).contains(&n) && best.as_ref().is_none_or(|(m, _)| n < *m) {
            best = Some((n, text));
        }
    };
    for year in [date.year(), date.year() + 1] {
        for s in [
            Season::MarchEquinox,
            Season::JuneSolstice,
            Season::SeptemberEquinox,
            Season::DecemberSolstice,
        ] {
            let day = instant(year, s).with_timezone(&tz).date_naive();
            let name = match (s, north) {
                (Season::MarchEquinox, true) | (Season::SeptemberEquinox, false) => {
                    "Spring equinox"
                }
                (Season::SeptemberEquinox, true) | (Season::MarchEquinox, false) => {
                    "Autumn equinox"
                }
                (Season::JuneSolstice, true) | (Season::DecemberSolstice, false) => {
                    "Summer solstice"
                }
                _ => "Winter solstice",
            };
            let gloss = match name {
                "Summer solstice" => "the longest day",
                "Winter solstice" => "the shortest day",
                _ => "day and night about equal",
            };
            offer(day, format!("{name} {}: {gloss}", day_word(date, day)));
        }
    }
    if let Some(c) = clock_change(date, tz, WINDOW as u32) {
        let at = match units {
            Units::Imperial if c.at.format("%M").to_string() == "00" => {
                c.at.format("%-I %p").to_string()
            }
            _ => clock(c.at, units),
        };
        let way = if c.forward { "forward" } else { "back" };
        offer(
            c.date,
            format!("Clocks go {way} an hour {} at {at}", day_word(date, c.date)),
        );
    }
    best.map(|(_, text)| text)
}
