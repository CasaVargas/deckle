// SPDX-License-Identifier: GPL-3.0-or-later
//! The edition sidecar (Morgue design §4).

use chrono::NaiveDate;
use deckle_digest::{Article, Edition, Gathered, Section};
use deckle_morgue::sidecar::{Sidecar, TEXT_CAP, from_gathered, plain_text};

fn date() -> NaiveDate {
    NaiveDate::from_ymd_opt(2026, 10, 21).unwrap()
}

fn article(title: &str, body: &str, teaser: Option<&str>) -> Article {
    Article {
        title: title.into(),
        body: body.into(),
        teaser: teaser.map(str::to_owned),
        ..Default::default()
    }
}

fn section(key: &str, title: &str, articles: Vec<Article>) -> Section {
    Section {
        key: key.into(),
        title: title.into(),
        articles,
        ..Default::default()
    }
}

/// Two sections (news first), three articles and a weather line.
fn gathered() -> Gathered {
    Gathered {
        edition: Edition {
            paper: "The Deckle Daily".into(),
            date: date(),
            place: "Linden Falls, NC".into(),
            weather_line: Some("Clear · 74°/40°F".into()),
            sections: vec![
                section(
                    "local",
                    "Local",
                    vec![
                        article(
                            "Mill sold",
                            "<p>The old <b>mill</b> &amp; brewery.</p>",
                            Some("The old mill"),
                        ),
                        article("Fair opens", "<p>Rides.</p>", None),
                    ],
                ),
                section(
                    "lottery",
                    "Lottery",
                    vec![article("Powerball", "<p>12 34 56</p>", None)],
                ),
            ],
            assets: Vec::new(),
        },
        news: vec![0],
        reading: None,
        long_reads: Vec::new(),
        reading_outcomes: Vec::new(),
        notes: Vec::new(),
    }
}

#[test]
fn sidecar_matches_the_edition() {
    let g = gathered();
    let s = from_gathered(&g);
    assert_eq!(s.date, date());
    assert_eq!(s.number, g.edition.number());
    assert_eq!(
        (s.paper.as_str(), s.place.as_str()),
        ("The Deckle Daily", "Linden Falls, NC")
    );
    assert_eq!(s.weather.as_deref(), Some("Clear · 74°/40°F"));
    assert_eq!(s.stem, "deckle-daily-2026-10-21");
    let titles: Vec<Vec<&str>> = s
        .sections
        .iter()
        .map(|sec| sec.stories.iter().map(|st| st.title.as_str()).collect())
        .collect();
    assert_eq!(titles, [vec!["Mill sold", "Fair opens"], vec!["Powerball"]]);
    assert_eq!(s.sections[0].key, "local");
    assert_eq!(s.sections[0].stories[0].text, "The old mill & brewery.");
    assert_eq!(
        s.sections[0].stories[0].teaser.as_deref(),
        Some("The old mill")
    );
    let lead = s.lead.expect("a lead");
    assert_eq!(
        (lead.section.as_str(), lead.title.as_str()),
        ("Local", "Mill sold")
    );
    assert_eq!(lead.teaser.as_deref(), Some("The old mill"));
    assert_eq!(s.files.epub, "2026-10-21.epub");
    assert_eq!(s.files.kfx, None);
}

#[test]
fn an_edition_with_no_news_has_no_lead() {
    let mut g = gathered();
    g.news.clear();
    assert!(from_gathered(&g).lead.is_none());
}

#[test]
fn plain_text_strips_tags_and_entities() {
    assert_eq!(
        plain_text("<p>Caf&eacute; &amp; <b>mill</b></p>\n\n<p>x</p>", TEXT_CAP),
        "Café & mill x"
    );
    assert_eq!(plain_text("a<br/>b <!-- c --> d", TEXT_CAP), "a b d");
}

#[test]
fn plain_text_caps_at_a_char_boundary() {
    let long = "é".repeat(30_000);
    let t = plain_text(&long, TEXT_CAP);
    assert!(t.chars().count() <= TEXT_CAP, "{}", t.chars().count());
    assert!(t.chars().all(|c| c == 'é'));
}

#[test]
fn sidecar_round_trips_json() {
    let s = from_gathered(&gathered());
    let json = serde_json::to_string(&s).unwrap();
    assert_eq!(serde_json::from_str::<Sidecar>(&json).unwrap(), s);
}
