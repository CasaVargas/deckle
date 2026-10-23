// SPDX-License-Identifier: GPL-3.0-or-later
//! Section sizes and the week, for tomorrow's paper (tomorrow's paper design §6.3, §7).
mod common;

use common::*;
use deckle_digest::next::WeekState;
use deckle_readership::layout::{Layout, SectionStart};
use deckle_readership::sizes::{history, recent};
use deckle_readership::state::{Read, State};

/// A layout with the front page at 0 and `local` (with `items`) at 1000, ending at 1000 + len.
fn local_layout(date: &str, len: i64, items: Option<u32>) -> Layout {
    Layout {
        date: d(date),
        title: "The Deckle Daily".into(),
        file: format!("deckle-daily-{date}"),
        end: 1000 + len,
        sections: vec![
            SectionStart {
                key: "front".into(),
                name: "Front page".into(),
                start: 0,
                items: None,
            },
            SectionStart {
                key: "local".into(),
                name: "Local".into(),
                start: 1000,
                items,
            },
        ],
    }
}

#[test]
fn lengths_from_starts() {
    let mut s = State::default();
    s.record_layout(layout("2026-10-20", &[0, 1000, 4000]));
    let h = history(&s);
    assert_eq!(h.whole["front"], 1000.0);
    assert_eq!(h.whole["s1"], 3000.0);
    assert_eq!(h.whole["s2"], 6000.0);
    assert!(h.per_item.is_empty());
}

#[test]
fn per_item_median() {
    let mut s = State::default();
    s.record_layout(local_layout("2026-10-18", 3000, Some(3)));
    s.record_layout(local_layout("2026-10-19", 4000, Some(2)));
    s.record_layout(local_layout("2026-10-20", 900, Some(3)));
    let h = history(&s);
    // the lower-middle median of 1000, 2000 and 300
    assert_eq!(h.per_item["local"], 1000.0);
    assert_eq!(h.whole["local"], 3000.0);
}

#[test]
fn items_none_skips_per_item() {
    let mut s = State::default();
    s.record_layout(local_layout("2026-10-20", 3000, None));
    s.record_layout(local_layout("2026-10-21", 3000, Some(0)));
    let h = history(&s);
    assert!(!h.per_item.contains_key("local"));
    assert_eq!(h.whole["local"], 3000.0);
}

#[test]
fn only_the_newest_fourteen_count() {
    let mut s = State::default();
    for i in 1..=14 {
        s.record_layout(local_layout(
            &format!("2026-10-{:02}", i + 10),
            1000,
            Some(1),
        ));
    }
    // an old, much longer edition is outside the window
    s.record_layout(local_layout("2026-09-01", 90_000, Some(1)));
    assert_eq!(history(&s).whole["local"], 1000.0);
}

fn read(furthest: i64) -> Read {
    Read {
        opened: true,
        furthest: Some(furthest),
        last: Some(furthest),
        last_read: None,
        seen: "x".into(),
    }
}

#[test]
fn recent_states() {
    let mut s = State::default();
    // 2026-10-17: read on two Kindles, the best counts
    s.record_layout(layout("2026-10-17", &[0, 1000]));
    let r = s.reads.entry(d("2026-10-17")).or_default();
    r.insert("A".into(), read(6000));
    r.insert("B".into(), read(9000));
    // 2026-10-18: built and laid out, never opened
    s.record_layout(layout("2026-10-18", &[0, 1000]));
    let built = |date: chrono::NaiveDate| date == d("2026-10-19");
    let week = recent(&s, d("2026-10-23"), 6, &built);
    let got: Vec<(chrono::NaiveDate, WeekState)> =
        week.into_iter().map(|w| (w.date, w.state)).collect();
    assert_eq!(
        got,
        [
            (d("2026-10-17"), WeekState::Read(90)),
            (d("2026-10-18"), WeekState::NotOpened),
            (d("2026-10-19"), WeekState::Sent),
            (d("2026-10-20"), WeekState::NoPaper),
            (d("2026-10-21"), WeekState::NoPaper),
            (d("2026-10-22"), WeekState::NoPaper),
        ]
    );
}

#[test]
fn old_json_still_loads() {
    let tmp = tempfile::tempdir().unwrap();
    let path = tmp.path().join("readership.json");
    std::fs::write(
        &path,
        r#"{"editions":{"2026-10-20":{"date":"2026-10-20","title":"T","file":"f","end":10,
        "sections":[{"key":"front","name":"Front page","start":0}]}},"reads":{},"dismissed":{}}"#,
    )
    .unwrap();
    let s = State::load(&path);
    assert_eq!(s.editions[&d("2026-10-20")].sections[0].items, None);
}
