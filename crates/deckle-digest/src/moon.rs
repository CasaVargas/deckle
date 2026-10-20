// SPDX-License-Identifier: GPL-3.0-or-later
//! Moon phase, computed locally from a reference new moon.

use chrono::{NaiveDate, NaiveDateTime};

const SYNODIC_DAYS: f64 = 29.530_588_853;

pub struct Moon {
    pub name: &'static str,
    /// Percent of the disc lit, 0–100.
    pub illumination: u32,
    pub age_days: f64,
    /// "Full moon in 4 days." / "New moon tomorrow."
    pub next: String,
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
    let half = SYNODIC_DAYS / 2.0;
    let (what, until) = if age < half {
        ("Full moon", half - age)
    } else {
        ("New moon", SYNODIC_DAYS - age)
    };
    let next = match until.round() as i64 {
        0 => format!("{what} today."),
        1 => format!("{what} tomorrow."),
        n => format!("{what} in {n} days."),
    };
    Moon {
        name,
        illumination: illumination as u32,
        age_days: age,
        next,
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
