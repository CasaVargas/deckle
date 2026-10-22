// SPDX-License-Identifier: GPL-3.0-or-later
//! Tomorrow's paper, so far (docs/superpowers/specs/2026-10-22-tomorrows-paper-design.md): what
//! the next edition will hold, from what's known before press time.

pub mod live;

use std::collections::BTreeMap;

use chrono::NaiveDate;
use serde::Serialize;

/// Section sizes from past editions, in KFX positions (readership layouts).
#[derive(Clone, Debug, Default, PartialEq, Serialize)]
pub struct History {
    /// The median length of one item, by section key.
    pub per_item: BTreeMap<String, f64>,
    /// The median length of the whole section, by key (`front` for the front page).
    pub whole: BTreeMap<String, f64>,
}

/// What became of one recent day's paper.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(tag = "state", content = "percent", rename_all = "camelCase")]
pub enum WeekState {
    /// Read this far, in percent, on the Kindle that got furthest.
    Read(u8),
    NotOpened,
    /// Built, but there's no layout to measure it against (email only, or before Readership).
    Sent,
    NoPaper,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct WeekDay {
    pub date: NaiveDate,
    pub state: WeekState,
}
