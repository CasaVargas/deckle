// SPDX-License-Identifier: GPL-3.0-or-later
//! "On this day" from Wikipedia's featured feed.

use chrono::{Datelike, NaiveDate};
use serde::Deserialize;

use crate::fetch::Net;
use crate::{Article, Section, esc};

const TITLE: &str = "On This Day";
const MAX_EVENTS: usize = 6;

#[derive(Debug, Deserialize)]
struct Feed {
    selected: Vec<Event>,
}

#[derive(Debug, Deserialize)]
struct Event {
    text: String,
    year: Option<i32>,
}

pub fn url(date: NaiveDate) -> String {
    format!(
        "https://api.wikimedia.org/feed/v1/wikipedia/en/onthisday/selected/{:02}/{:02}",
        date.month(),
        date.day()
    )
}

pub fn article(json: &[u8], date: NaiveDate) -> Result<Article, String> {
    let mut feed: Feed = serde_json::from_slice(json).map_err(|e| e.to_string())?;
    if feed.selected.is_empty() {
        return Err("no events".into());
    }
    feed.selected.truncate(MAX_EVENTS);
    feed.selected.sort_by_key(|e| e.year);
    let body: String = feed
        .selected
        .iter()
        .map(|e| {
            let year = e.year.map(|y| y.to_string()).unwrap_or_default();
            format!(
                "<p class=\"event\"><span class=\"year\">{}</span> {}</p>",
                esc(&year),
                esc(&e.text)
            )
        })
        .collect();
    Ok(Article {
        teaser: None,
        photos: Vec::new(),
        title: date.format("%B %-d in history").to_string(),
        meta: Some("From Wikipedia".into()),
        body,
    })
}

pub fn section(date: NaiveDate, net: &dyn Net) -> Section {
    match net.get(&url(date)).and_then(|b| article(&b, date)) {
        Ok(a) => Section {
            title: TITLE.into(),
            kicker: None,
            articles: vec![a],
            unavailable: None,
        },
        Err(e) => super::unavailable(TITLE, e),
    }
}
