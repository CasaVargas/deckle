// SPDX-License-Identifier: GPL-3.0-or-later
//! Tonight's sky: the major annual meteor showers (almanac design §4.5). Dates are the IMO's
//! typical calendar dates; a peak drifts a day at most between years.

use chrono::{Datelike, NaiveDate};

pub struct Shower {
    pub name: &'static str,
    /// (month, day)
    pub start: (u32, u32),
    pub end: (u32, u32),
    pub peak: (u32, u32),
    /// Meteors an hour at best, under a dark sky.
    pub rate: u32,
    /// Radiant declination, degrees.
    pub dec: f64,
    /// Which way to face.
    pub look: &'static str,
    pub when: &'static str,
}

#[allow(clippy::too_many_arguments)] // one table row
const fn s(
    name: &'static str,
    start: (u32, u32),
    end: (u32, u32),
    peak: (u32, u32),
    rate: u32,
    dec: f64,
    look: &'static str,
    when: &'static str,
) -> Shower {
    Shower {
        name,
        start,
        end,
        peak,
        rate,
        dec,
        look,
        when,
    }
}

pub const SHOWERS: [Shower; 10] = [
    s(
        "Quadrantids",
        (12, 28),
        (1, 12),
        (1, 3),
        110,
        49.0,
        "northeast",
        "before dawn",
    ),
    s(
        "Lyrids",
        (4, 14),
        (4, 30),
        (4, 22),
        18,
        34.0,
        "northeast",
        "after midnight",
    ),
    s(
        "Eta Aquariids",
        (4, 19),
        (5, 28),
        (5, 6),
        50,
        -1.0,
        "east",
        "before dawn",
    ),
    s(
        "Southern Delta Aquariids",
        (7, 12),
        (8, 23),
        (7, 31),
        25,
        -16.0,
        "south",
        "after midnight",
    ),
    s(
        "Perseids",
        (7, 17),
        (8, 24),
        (8, 12),
        100,
        58.0,
        "northeast",
        "after midnight",
    ),
    s(
        "Draconids",
        (10, 6),
        (10, 10),
        (10, 8),
        10,
        54.0,
        "northwest",
        "in the evening",
    ),
    s(
        "Orionids",
        (10, 2),
        (11, 7),
        (10, 21),
        20,
        16.0,
        "east",
        "after midnight",
    ),
    s(
        "Leonids",
        (11, 6),
        (11, 30),
        (11, 17),
        15,
        22.0,
        "east",
        "after midnight",
    ),
    s(
        "Geminids",
        (12, 4),
        (12, 20),
        (12, 14),
        150,
        33.0,
        "east",
        "from 9 PM on",
    ),
    s(
        "Ursids",
        (12, 17),
        (12, 26),
        (12, 22),
        10,
        75.0,
        "north",
        "before dawn",
    ),
];

/// A shower's (start, peak, end) for the window that contains `date`, across New Year.
fn window(sh: &Shower, date: NaiveDate) -> Option<(NaiveDate, NaiveDate, NaiveDate)> {
    let on = |y: i32, (m, d): (u32, u32)| NaiveDate::from_ymd_opt(y, m, d);
    [date.year() - 1, date.year()].into_iter().find_map(|y| {
        let start = on(y, sh.start)?;
        let later = |md: (u32, u32)| on(if md < sh.start { y + 1 } else { y }, md);
        let (peak, end) = (later(sh.peak)?, later(sh.end)?);
        (start <= date && date <= end).then_some((start, peak, end))
    })
}

/// Whether the radiant climbs at least 10° above the horizon at `lat` (plan ruling).
fn visible(sh: &Shower, lat: f64) -> bool {
    if lat >= 0.0 {
        sh.dec >= lat - 80.0
    } else {
        sh.dec <= lat + 80.0
    }
}

/// The paragraph for the active shower nearest its peak, if any.
pub fn tonight(date: NaiveDate, lat: f64, moon_lit: u32) -> Option<String> {
    let (sh, peak) = SHOWERS
        .iter()
        .filter(|sh| visible(sh, lat))
        .filter_map(|sh| window(sh, date).map(|(_, peak, _)| (sh, peak)))
        .min_by_key(|(_, peak)| (*peak - date).num_days().abs())?;
    let days = (peak - date).num_days();
    let lead = match days {
        0 => format!("The {} peak tonight", sh.name),
        1 => format!("The {} peak tomorrow night", sh.name),
        2..=6 => format!("The {} peak {} night", sh.name, peak.format("%A")),
        d if d > 6 => format!("The {} peak on {}", sh.name, peak.format("%B %-d")),
        _ => format!("The {} are past their peak but still worth a look", sh.name),
    };
    let rate = if days >= 0 {
        format!(", with up to {} an hour at their best", sh.rate)
    } else {
        String::new()
    };
    let moon = if moon_lit > 60 {
        ", though a bright moon will wash out the fainter ones"
    } else {
        ""
    };
    Some(format!(
        "{lead}{rate}. Look {}: face {} and give your eyes 20 minutes{moon}.",
        sh.when, sh.look
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn d(y: i32, m: u32, day: u32) -> NaiveDate {
        NaiveDate::from_ymd_opt(y, m, day).unwrap()
    }

    #[test]
    fn before_peak() {
        let t = tonight(d(2026, 10, 19), 40.7, 10).unwrap();
        assert!(t.starts_with("The Orionids peak Wednesday night"), "{t}");
    }

    #[test]
    fn at_peak() {
        assert!(
            tonight(d(2026, 10, 21), 40.7, 10)
                .unwrap()
                .contains("peak tonight")
        );
    }

    #[test]
    fn after_peak() {
        let t = tonight(d(2026, 10, 22), 40.7, 10).unwrap();
        for want in [
            "past their peak but still worth a look",
            "face east",
            "after midnight",
            "give your eyes 20 minutes",
        ] {
            assert!(t.contains(want), "{want}: {t}");
        }
    }

    #[test]
    fn moonlit() {
        let words = "a bright moon will wash out the fainter ones";
        assert!(tonight(d(2026, 10, 22), 40.7, 61).unwrap().contains(words));
        assert!(!tonight(d(2026, 10, 22), 40.7, 60).unwrap().contains(words));
    }

    #[test]
    fn overlap_prefers_nearer_peak() {
        assert!(
            tonight(d(2026, 8, 12), 40.7, 0)
                .unwrap()
                .contains("Perseids")
        );
    }

    #[test]
    fn year_wrap() {
        for date in [d(2026, 12, 30), d(2027, 1, 2)] {
            assert!(
                tonight(date, 40.7, 0).unwrap().contains("Quadrantids"),
                "{date}"
            );
        }
    }

    #[test]
    fn none_outside_windows() {
        assert_eq!(tonight(d(2026, 3, 1), 40.7, 0), None);
    }

    #[test]
    fn latitude_skip() {
        // Late July: only the Southern Delta Aquariids and Perseids are active.
        let at = |lat| tonight(d(2026, 7, 14), lat, 0);
        assert!(at(40.0).unwrap().contains("Southern Delta Aquariids"));
        assert_eq!(at(80.0), None);
    }
}
