// SPDX-License-Identifier: GPL-3.0-or-later
//! The Mac's time zone and the paper's clock style (almanac design §3).

use chrono::{NaiveTime, Timelike};
use chrono_tz::Tz;

use crate::config::Units;

/// The machine's IANA zone; UTC when it can't be read.
pub fn local_tz() -> Tz {
    iana_time_zone::get_timezone()
        .ok()
        .and_then(|name| name.parse().ok())
        .unwrap_or(Tz::UTC)
}

/// The zone this process keeps time in. `TZ` first: chrono's local time (so `digest run`)
/// follows it, while the machine zone on Linux is `/etc/localtime`, which a container's `TZ`
/// doesn't change. Else [`local_tz`].
pub fn process_tz() -> Tz {
    process_tz_from(std::env::var("TZ").ok().as_deref())
}

fn process_tz_from(tz: Option<&str>) -> Tz {
    tz.and_then(|z| z.trim_start_matches(':').parse().ok())
        .unwrap_or_else(local_tz)
}

/// `7:28 PM` for imperial papers, `19:28` for metric ones.
pub fn clock(t: NaiveTime, units: Units) -> String {
    match units {
        Units::Imperial => t.format("%-I:%M %p").to_string(),
        Units::Metric => t.format("%H:%M").to_string(),
    }
}

/// Like [`clock`], but "4 PM" on the hour for imperial papers.
pub fn short_clock(t: NaiveTime, units: Units) -> String {
    match units {
        Units::Imperial if t.minute() == 0 => t.format("%-I %p").to_string(),
        _ => clock(t, units),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn process_tz_reads_tz_first() {
        assert_eq!(
            process_tz_from(Some("America/New_York")),
            Tz::America__New_York
        );
        assert_eq!(process_tz_from(Some(":Asia/Tokyo")), Tz::Asia__Tokyo);
        assert_eq!(process_tz_from(Some("Nope/Zone")), local_tz());
        assert_eq!(process_tz_from(None), local_tz());
    }
}
