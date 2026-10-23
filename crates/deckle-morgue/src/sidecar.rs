// SPDX-License-Identifier: GPL-3.0-or-later
//! `YYYY-MM-DD.json`: what an edition said, small enough to scan a year of them (design §4).

use chrono::NaiveDate;
use deckle_digest::Gathered;
use serde::{Deserialize, Serialize};

/// The most text kept per story.
pub const TEXT_CAP: usize = 20_000;

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Lead {
    /// The section's title.
    pub section: String,
    pub title: String,
    pub teaser: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Story {
    pub title: String,
    pub teaser: Option<String>,
    /// Plain text, whitespace collapsed, at most [`TEXT_CAP`] characters.
    pub text: String,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct SectionText {
    pub key: String,
    pub title: String,
    pub stories: Vec<Story>,
}

/// The files kept for the edition, relative to its year folder.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Files {
    pub epub: String,
    pub kfx: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Sidecar {
    pub date: NaiveDate,
    pub number: u32,
    pub paper: String,
    pub place: String,
    pub weather: Option<String>,
    pub lead: Option<Lead>,
    pub sections: Vec<SectionText>,
    pub files: Files,
    /// `deckle-daily-YYYY-MM-DD`: the name the readership state knows the edition by.
    pub stem: String,
}

/// `html` as plain text: tags and comments dropped, entities decoded, whitespace collapsed,
/// cut to `cap` characters.
pub fn plain_text(html: &str, cap: usize) -> String {
    let mut text = String::with_capacity(html.len());
    let mut rest = html;
    while let Some(at) = rest.find('<') {
        text.push_str(&rest[..at]);
        text.push(' ');
        rest = &rest[at..];
        rest = if let Some(after) = rest.strip_prefix("<!--") {
            after.split_once("-->").map_or("", |(_, r)| r)
        } else {
            rest.split_once('>').map_or("", |(_, r)| r)
        };
    }
    text.push_str(rest);
    let decoded = html_escape::decode_html_entities(&text);
    decoded
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .chars()
        .take(cap)
        .collect()
}

/// The sidecar of a gathered edition.
pub fn from_gathered(g: &Gathered) -> Sidecar {
    let e = &g.edition;
    // The lead the front page and the cover make (by tier and teaser), not just the first story.
    let lead = deckle_digest::cover::lead_of(g).map(|(s, a)| {
        let article = &e.sections[s].articles[a];
        Lead {
            section: e.sections[s].title.clone(),
            title: article.title.clone(),
            teaser: article.teaser.clone(),
        }
    });
    Sidecar {
        date: e.date,
        number: e.number(),
        paper: e.paper.clone(),
        place: e.place.clone(),
        weather: e.weather_line.clone(),
        lead,
        sections: e
            .sections
            .iter()
            .map(|s| SectionText {
                key: s.key.clone(),
                title: s.title.clone(),
                stories: s
                    .articles
                    .iter()
                    .map(|a| Story {
                        title: a.title.clone(),
                        teaser: a.teaser.clone(),
                        text: plain_text(&a.body, TEXT_CAP),
                    })
                    .collect(),
            })
            .collect(),
        files: Files {
            epub: format!("{}.epub", e.date),
            kfx: None,
        },
        stem: deckle_digest::edition_filename(e.date)
            .trim_end_matches(".kfx")
            .to_owned(),
    }
}
