// SPDX-License-Identifier: GPL-3.0-or-later
//! Searching every kept edition, and listing a month (design §4). There is no index: the
//! sidecars are scanned in memory, a year of them in well under a second.

use chrono::{Datelike, NaiveDate};
use serde::Serialize;

use crate::folder::Morgue;

/// One edition with matching stories: (section title, story title) each.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct Hit {
    pub date: NaiveDate,
    pub paper: String,
    pub stories: Vec<(String, String)>,
}

/// A day on the calendar: the paper's name and its lead headline.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct DaySummary {
    pub date: NaiveDate,
    pub paper: String,
    pub lead: Option<String>,
}

/// Lower-case, accent-folded words: split on anything that isn't a letter or digit.
fn words(text: &str) -> Vec<String> {
    deunicode::deunicode(text)
        .to_lowercase()
        .split(|c: char| !c.is_ascii_alphanumeric())
        .filter(|w| !w.is_empty())
        .map(str::to_owned)
        .collect()
}

/// Every edition with a story that has, for each query word, a word starting with it in its
/// title, teaser or text. Newest first; a blank query finds nothing.
pub fn search(m: &Morgue, query: &str) -> Vec<Hit> {
    let wanted = words(query);
    if wanted.is_empty() {
        return Vec::new();
    }
    let mut hits = Vec::new();
    for date in m.editions() {
        let Some(sidecar) = m.sidecar(date) else {
            continue;
        };
        let mut stories = Vec::new();
        for section in &sidecar.sections {
            for story in &section.stories {
                let have = words(&format!(
                    "{} {} {}",
                    story.title,
                    story.teaser.as_deref().unwrap_or(""),
                    story.text
                ));
                if wanted
                    .iter()
                    .all(|q| have.iter().any(|w| w.starts_with(q.as_str())))
                {
                    stories.push((section.title.clone(), story.title.clone()));
                }
            }
        }
        if !stories.is_empty() {
            hits.push(Hit {
                date,
                paper: sidecar.paper,
                stories,
            });
        }
    }
    hits
}

/// The editions kept in a month, oldest first.
pub fn month(m: &Morgue, year: i32, month: u32) -> Vec<DaySummary> {
    let mut days: Vec<DaySummary> = m
        .editions()
        .into_iter()
        .filter(|d| d.year() == year && d.month() == month)
        .filter_map(|d| {
            let s = m.sidecar(d)?;
            Some(DaySummary {
                date: d,
                paper: s.paper,
                lead: s.lead.map(|l| l.title),
            })
        })
        .collect();
    days.reverse();
    days
}
