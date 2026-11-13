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

pub fn url(date: NaiveDate, language: Option<&str>) -> String {
    format!(
        "https://api.wikimedia.org/feed/v1/wikipedia/{}/onthisday/selected/{:02}/{:02}",
        language.filter(|s| !s.is_empty()).unwrap_or("en"),
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
        extra: Default::default(),
        url: None,
        outlet: None,
        teaser: None,
        photos: Vec::new(),
        title: date.format("%B %-d in history").to_string(),
        meta: Some("From Wikipedia".into()),
        body,
    })
}

pub fn section(date: NaiveDate, language: Option<&str>, net: &dyn Net) -> Section {
    match net
        .get(&url(date, language))
        .and_then(|b| article(&b, date))
    {
        Ok(a) => Section {
            weather: None,
            puzzle: None,
            brief: Vec::new(),
            key: String::new(),
            title: TITLE.into(),
            kicker: None,
            articles: vec![a],
            unavailable: None,
        },
        Err(e) => super::unavailable(TITLE, e),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn wikipedia_url() {
        let date = NaiveDate::from_ymd_opt(2026, 11, 13).unwrap();

        assert_eq!(
            url(date, None),
            "https://api.wikimedia.org/feed/v1/wikipedia/en/onthisday/selected/11/13"
        );
    }

    #[test]
    fn wikipedia_url_i18n() {
        let date = NaiveDate::from_ymd_opt(2026, 11, 13).unwrap();

        assert_eq!(
            url(date, Some("it")),
            "https://api.wikimedia.org/feed/v1/wikipedia/it/onthisday/selected/11/13"
        );
    }
}
