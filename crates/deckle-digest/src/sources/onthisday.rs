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

/// A Wikipedia language code as it appears in the API path ("it", "pt", "zh-yue", "simple"):
/// ASCII letters, then hyphen-separated parts. Anything else would change the path, so it's
/// refused.
fn language_code(language: &str) -> Option<&str> {
    let code = language.trim();
    let ok = !code.is_empty()
        && code.len() <= 20
        && code.starts_with(|c: char| c.is_ascii_alphabetic())
        && code
            .split('-')
            .all(|part| !part.is_empty() && part.chars().all(|c| c.is_ascii_alphanumeric()));
    ok.then_some(code)
}

pub fn url(date: NaiveDate, language: Option<&str>) -> Result<String, String> {
    let code = match language.map(str::trim).filter(|s| !s.is_empty()) {
        None => "en",
        Some(l) => {
            language_code(l).ok_or_else(|| format!("\"{l}\" isn't a Wikipedia language code"))?
        }
    };
    Ok(format!(
        "https://api.wikimedia.org/feed/v1/wikipedia/{}/onthisday/selected/{:02}/{:02}",
        code,
        date.month(),
        date.day()
    ))
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
    match url(date, language)
        .and_then(|u| net.get(&u))
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
            url(date, None).unwrap(),
            "https://api.wikimedia.org/feed/v1/wikipedia/en/onthisday/selected/11/13"
        );
        assert_eq!(url(date, Some(" ")), url(date, None));
    }

    #[test]
    fn wikipedia_url_i18n() {
        let date = NaiveDate::from_ymd_opt(2026, 11, 13).unwrap();

        assert_eq!(
            url(date, Some("it")).unwrap(),
            "https://api.wikimedia.org/feed/v1/wikipedia/it/onthisday/selected/11/13"
        );
        assert!(url(date, Some("zh-yue")).is_ok());
    }

    #[test]
    fn a_language_that_would_change_the_path_is_refused() {
        let date = NaiveDate::from_ymd_opt(2026, 11, 13).unwrap();
        for bad in ["../en", "en/x", "it?x=1", "-it", "it-", "e n"] {
            assert!(url(date, Some(bad)).is_err(), "{bad}");
        }
    }
}
