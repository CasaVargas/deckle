// SPDX-License-Identifier: GPL-3.0-or-later
//! The line-by-line detail of one edition (Morgue design §4).

use chrono::NaiveDate;
use deckle_morgue::detail::detail;
use deckle_morgue::folder::Morgue;
use deckle_morgue::sidecar::{Files, Lead, Sidecar};
use deckle_morgue::state::MorgueState;
use deckle_readership::layout::{Layout, SectionStart};
use deckle_readership::state::{Read, State};

fn d(s: &str) -> NaiveDate {
    s.parse().unwrap()
}

fn sidecar(date: NaiveDate) -> Sidecar {
    Sidecar {
        date,
        number: 294,
        paper: "The Deckle Daily".into(),
        place: "Linden Falls, NC".into(),
        weather: Some("Clear · 74°/40°F".into()),
        lead: Some(Lead {
            section: "Local".into(),
            title: "Mill sold".into(),
            teaser: Some("The old mill".into()),
        }),
        sections: Vec::new(),
        files: Files {
            epub: format!("{date}.epub"),
            kfx: None,
        },
        stem: format!("deckle-daily-{date}"),
    }
}

fn layout(date: NaiveDate) -> Layout {
    let sec = |key: &str, name: &str, start| SectionStart {
        key: key.into(),
        name: name.into(),
        start,
        items: None,
        stories: Vec::new(),
    };
    Layout {
        date,
        title: "The Deckle Daily".into(),
        file: format!("deckle-daily-{date}"),
        end: 10_000,
        epub: None,
        sections: vec![
            sec("front", "Front page", 0),
            sec("weather", "Weather", 1000),
            sec("local", "Local", 5000),
            sec("lottery", "Lottery", 9500),
        ],
    }
}

fn read(opened: bool, furthest: Option<i64>, last: Option<i64>) -> Read {
    Read {
        opened,
        furthest,
        last,
        last_read: None,
        seen: "2026-10-22T10:00:00Z".into(),
        history: Vec::new(),
    }
}

fn setup() -> (tempfile::TempDir, Morgue) {
    let tmp = tempfile::tempdir().unwrap();
    let m = Morgue {
        root: tmp.path().join("Morgue"),
        keep_days: 0,
    };
    let epub = tmp.path().join("e.epub");
    std::fs::write(&epub, b"E").unwrap();
    m.archive_edition(&epub, &sidecar(d("2026-10-21"))).unwrap();
    (tmp, m)
}

#[test]
fn detail_lines() {
    let (_tmp, m) = setup();
    let date = d("2026-10-21");
    let mut reads = State::default();
    reads.record_layout(layout(date));
    reads
        .reads
        .entry(date)
        .or_default()
        .insert("K".into(), read(true, Some(9100), Some(9100)));
    let mut state = MorgueState::default();
    state.highlights.insert(date, 2);
    state
        .on_kindle
        .insert("deckle-daily-2026-10-21".into(), false);

    let got = detail(&m, date, &reads, &state).unwrap();
    assert_eq!(got.heading, "Wednesday, October 21 · No. 294");
    assert_eq!(got.weather.as_deref(), Some("Clear · 74°/40°F"));
    assert_eq!(got.lead.as_ref().unwrap().title, "Mill sold");
    assert_eq!(got.read.as_deref(), Some("You read about 91%, to Local"));
    assert_eq!(got.highlights.as_deref(), Some("2 passages highlighted"));
    assert_eq!(got.on_kindle.as_deref(), Some("No longer on the Kindle"));
    assert_eq!(got.epub, m.paths(date).0);

    state.highlights.insert(date, 1);
    state
        .on_kindle
        .insert("deckle-daily-2026-10-21".into(), true);
    let got = detail(&m, date, &reads, &state).unwrap();
    assert_eq!(got.highlights.as_deref(), Some("1 passage highlighted"));
    assert_eq!(got.on_kindle.as_deref(), Some("On the Kindle"));
}

#[test]
fn the_further_of_furthest_and_last_across_kindles() {
    let (_tmp, m) = setup();
    let date = d("2026-10-21");
    let mut reads = State::default();
    reads.record_layout(layout(date));
    let by = reads.reads.entry(date).or_default();
    by.insert("P".into(), read(true, Some(2000), Some(6000)));
    by.insert("S".into(), read(true, Some(3000), Some(3000)));
    let got = detail(&m, date, &reads, &MorgueState::default()).unwrap();
    assert_eq!(got.read.as_deref(), Some("You read about 60%, to Local"));
    assert_eq!((got.highlights, got.on_kindle), (None, None));
}

#[test]
fn only_unopened_reads_say_not_opened() {
    let (_tmp, m) = setup();
    let date = d("2026-10-21");
    let mut reads = State::default();
    reads.record_layout(layout(date));
    reads
        .reads
        .entry(date)
        .or_default()
        .insert("K".into(), read(false, None, None));
    assert_eq!(
        detail(&m, date, &reads, &MorgueState::default())
            .unwrap()
            .read
            .as_deref(),
        Some("Not opened")
    );
}

#[test]
fn no_record_says_nothing_about_reading() {
    let (_tmp, m) = setup();
    let got = detail(
        &m,
        d("2026-10-21"),
        &State::default(),
        &MorgueState::default(),
    )
    .unwrap();
    assert_eq!(got.read, None);
    assert!(
        detail(
            &m,
            d("2026-10-01"),
            &State::default(),
            &MorgueState::default()
        )
        .is_none()
    );
}
