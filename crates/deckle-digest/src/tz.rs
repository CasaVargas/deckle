// SPDX-License-Identifier: GPL-3.0-or-later
//! The Mac's time zone and the paper's clock style (almanac design §3).

use chrono::NaiveTime;
use chrono_tz::Tz;

use crate::config::Units;

/// The machine's IANA zone; UTC when it can't be read.
pub fn local_tz() -> Tz {
    iana_time_zone::get_timezone()
        .ok()
        .and_then(|name| name.parse().ok())
        .unwrap_or(Tz::UTC)
}

/// `7:28 PM` for imperial papers, `19:28` for metric ones.
pub fn clock(t: NaiveTime, units: Units) -> String {
    match units {
        Units::Imperial => t.format("%-I:%M %p").to_string(),
        Units::Metric => t.format("%H:%M").to_string(),
    }
}
