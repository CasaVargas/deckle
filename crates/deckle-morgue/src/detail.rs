// SPDX-License-Identifier: GPL-3.0-or-later
//! What the Daily tab says about one kept edition (design §4): its heading, the front page's
//! weather and lead, how far you read, passages highlighted, and whether it's on the Kindle.

use std::path::PathBuf;

use chrono::NaiveDate;
use deckle_readership::layout::section_at;
use serde::Serialize;

use crate::folder::Morgue;
use crate::sidecar::Lead;
use crate::state::MorgueState;

#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Detail {
    /// "Wednesday, October 21 · No. 294".
    pub heading: String,
    pub weather: Option<String>,
    pub lead: Option<Lead>,
    /// "You read about 91%, to Local", or "Not opened"; `None` with no record.
    pub read: Option<String>,
    /// "2 passages highlighted", when a count is known.
    pub highlights: Option<String>,
    /// "On the Kindle" or "No longer on the Kindle", when known.
    pub on_kindle: Option<String>,
    pub epub: PathBuf,
}

/// How far the furthest Kindle got, from the readership state.
fn read_line(date: NaiveDate, reads: &deckle_readership::State) -> Option<String> {
    let by_kindle = reads.reads.get(&date)?;
    let furthest = by_kindle
        .values()
        .filter(|r| r.opened)
        .filter_map(|r| r.furthest.max(r.last))
        .max();
    let Some(furthest) = furthest else {
        return by_kindle
            .values()
            .all(|r| !r.opened)
            .then(|| "Not opened".to_owned());
    };
    let layout = reads.editions.get(&date)?;
    let percent = (furthest.max(0) * 100 / layout.end.max(1)).min(100);
    let section = section_at(layout, furthest).map(|s| s.name.clone())?;
    Some(format!("You read about {percent}%, to {section}"))
}

pub fn detail(
    m: &Morgue,
    date: NaiveDate,
    reads: &deckle_readership::State,
    state: &MorgueState,
) -> Option<Detail> {
    let sidecar = m.sidecar(date)?;
    let passages = |n: usize| format!("{n} passage{} highlighted", if n == 1 { "" } else { "s" });
    Some(Detail {
        heading: format!("{} · No. {}", date.format("%A, %B %-d"), sidecar.number),
        weather: sidecar.weather.clone(),
        lead: sidecar.lead.clone(),
        read: read_line(date, reads),
        highlights: state.highlights.get(&date).map(|&n| passages(n)),
        on_kindle: state.on_kindle.get(&sidecar.stem).map(|&on| {
            if on {
                "On the Kindle"
            } else {
                "No longer on the Kindle"
            }
            .to_owned()
        }),
        epub: m.paths(date).0,
    })
}
