// SPDX-License-Identifier: GPL-3.0-or-later
mod common;

use common::*;
use deckle_digest::Config;
use deckle_digest::config::SectionKind;
use deckle_readership::layout::{Layout, SectionStart};
use deckle_readership::report::{SuggestionKind, apply, dismiss, report};
use deckle_readership::state::{Read, State};

/// The usual paper: (key, printed name, start), end 10 000.
const PAPER: &[(&str, &str, i64)] = &[
    ("front", "Front page", 0),
    ("weather", "Weather", 1000),
    ("local", "Local", 2000),
    ("feeds:HN", "HN", 5000),
    ("lottery", "Lottery", 8000),
    ("horoscope", "Horoscope", 8600),
];

fn date(i: u64) -> chrono::NaiveDate {
    d("2026-09-01") + chrono::Days::new(i)
}

/// Edition `i` with `secs`; `furthest` per Kindle (`None`: on the Kindle, unopened).
fn edition(s: &mut State, i: u64, secs: &[(&str, &str, i64)], reads: &[(&str, Option<i64>)]) {
    s.record_layout(Layout {
        date: date(i),
        title: "The Deckle Daily".into(),
        file: format!("deckle-daily-{}", date(i)),
        end: 10_000,
        sections: secs
            .iter()
            .map(|(k, n, start)| SectionStart {
                key: (*k).into(),
                name: (*n).into(),
                start: *start,
                items: None,
            })
            .collect(),
    });
    for (dev, f) in reads {
        let r = Read {
            opened: f.is_some(),
            furthest: *f,
            last: *f,
            last_read: None,
            seen: NOW.into(),
            history: Vec::new(),
        };
        s.reads.entry(date(i)).or_default().insert((*dev).into(), r);
    }
}

/// One edition per furthest value, all on Kindle "K", the usual paper.
fn papers(furthest: &[Option<i64>]) -> State {
    let mut s = State::default();
    for (i, f) in furthest.iter().enumerate() {
        edition(&mut s, i as u64, PAPER, &[("K", *f)]);
    }
    s
}

fn cfg(sections: &[&str], feeds: &[&str]) -> Config {
    let feeds: String = feeds
        .iter()
        .map(|f| format!("[[feeds]]\nname = \"{f}\"\nurl = \"https://{f}.example/feed\"\n\n"))
        .collect();
    let sections = sections
        .iter()
        .map(|s| format!("\"{s}\""))
        .collect::<Vec<_>>()
        .join(", ");
    Config::parse(&format!("sections = [{sections}]\n\n{feeds}")).unwrap()
}

fn usual() -> Config {
    cfg(
        &["weather", "local", "feeds", "lottery", "horoscope"],
        &["HN"],
    )
}

fn ids(r: &deckle_readership::report::Report) -> Vec<&str> {
    r.suggestions.iter().map(|s| s.id.as_str()).collect()
}

#[test]
fn opened_count_and_median_percent() {
    let mut f: Vec<Option<i64>> = [4000, 4500, 4900, 4900, 4900, 4900, 4900, 5500, 6000, 6000]
        .into_iter()
        .map(Some)
        .collect();
    f.extend([None; 4]);
    let r = report(&papers(&f), &usual());
    assert_eq!((r.window, r.opened), (14, 10));
    assert_eq!(r.median_percent, Some(49));
    assert_eq!(r.stop_section.as_deref(), Some("Local"));
}

#[test]
fn the_window_is_the_last_fourteen_editions() {
    let mut f = vec![Some(9999); 6];
    f.extend(vec![None; 14]);
    let r = report(&papers(&f), &usual());
    assert_eq!((r.window, r.opened), (14, 0));
    assert_eq!(r.median_percent, None);
}

#[test]
fn sections_follow_the_current_config_order_and_drop_removed_ones() {
    let r = report(
        &papers(&[Some(9000); 8]),
        &cfg(&["lottery", "weather", "feeds"], &["HN"]),
    );
    let keys: Vec<_> = r.sections.iter().map(|s| s.key.as_str()).collect();
    assert_eq!(keys, ["lottery", "weather", "feeds:HN"]);
    assert_eq!(r.sections[2].name, "HN");
}

#[test]
fn a_new_section_has_no_share_until_five_editions() {
    let mut s = papers(&[Some(9900); 10]);
    let with_new: Vec<_> = PAPER
        .iter()
        .copied()
        .chain([("onthisday", "On This Day", 9500)])
        .collect();
    for i in 10..13 {
        edition(&mut s, i, &with_new, &[("K", Some(9900))]);
    }
    let r = report(&s, &cfg(&["weather", "onthisday"], &[]));
    let row = r.sections.iter().find(|s| s.key == "onthisday").unwrap();
    assert_eq!((row.share, row.seen_in), (None, 3));
    assert_eq!(row.name, "On This Day");
}

#[test]
fn share_counts_only_editions_that_had_the_section() {
    let mut s = State::default();
    let no_lottery: Vec<_> = PAPER.iter().copied().filter(|p| p.0 != "lottery").collect();
    for i in 0..10u64 {
        let secs = if i.is_multiple_of(2) {
            PAPER.to_vec()
        } else {
            no_lottery.clone()
        };
        edition(&mut s, i, &secs, &[("K", Some(8300))]);
    }
    let r = report(&s, &usual());
    let row = r.sections.iter().find(|s| s.key == "lottery").unwrap();
    assert_eq!((row.share, row.seen_in), (Some(100), 5));
}

#[test]
fn two_devices_take_the_furthest() {
    let mut s = State::default();
    edition(&mut s, 0, PAPER, &[("K1", Some(3000)), ("K2", Some(8500))]);
    let r = report(&s, &usual());
    assert_eq!((r.opened, r.median_percent), (1, Some(85)));
}

#[test]
fn fewer_than_seven_opened_means_no_suggestions() {
    let r = report(&papers(&[Some(1500); 6]), &usual());
    assert!(r.suggestions.is_empty(), "{:?}", ids(&r));
}

#[test]
fn a_rarely_reached_section_early_in_the_paper_is_not_dropped() {
    let mut f = vec![Some(500); 9];
    f.push(Some(9500));
    let r = report(&papers(&f), &usual());
    assert!(!ids(&r).contains(&"drop:weather"), "{:?}", ids(&r));
    assert!(!ids(&r).contains(&"drop:local"), "{:?}", ids(&r));
}

#[test]
fn a_late_rarely_reached_section_gets_drop_with_the_exact_copy() {
    let mut f = vec![Some(8500); 2];
    f.extend(vec![Some(4900); 12]);
    let r = report(&papers(&f), &usual());
    let s = r
        .suggestions
        .iter()
        .find(|s| s.id == "drop:lottery")
        .unwrap();
    assert_eq!(s.kind, SuggestionKind::Drop);
    assert_eq!(s.title, "Drop Lottery");
    assert_eq!(
        s.why,
        "You got to it on 2 of 14 days. It's about 6% of the paper."
    );
}

#[test]
fn a_feed_that_meets_both_rules_gets_move_up_only() {
    let late_feed: Vec<_> = PAPER
        .iter()
        .map(|&(k, n, st)| {
            if k == "feeds:HN" {
                (k, n, 7000)
            } else {
                (k, n, st)
            }
        })
        .collect();
    let mut s = State::default();
    for i in 0..10 {
        edition(
            &mut s,
            i,
            &late_feed,
            &[("K", Some(if i == 0 { 7500 } else { 3000 }))],
        );
    }
    let r = report(&s, &usual());
    assert!(!ids(&r).contains(&"drop:feeds:HN"), "{:?}", ids(&r));
    let up = r
        .suggestions
        .iter()
        .find(|s| s.id == "up:feeds:HN")
        .unwrap();
    assert_eq!(up.title, "Move HN up");
    assert_eq!(
        up.why,
        "You got to it on 1 of 10 days, about 70% in. Move it ahead of Local?"
    );
}

#[test]
fn front_and_reading_never_get_a_suggestion() {
    let late_reading: Vec<_> = PAPER
        .iter()
        .copied()
        .chain([("reading", "Your Reading", 9500)])
        .collect();
    let mut s = State::default();
    for i in 0..10 {
        edition(&mut s, i, &late_reading, &[("K", Some(100))]);
    }
    let r = report(&s, &cfg(&["weather", "reading"], &[]));
    assert!(
        ids(&r)
            .iter()
            .all(|id| !id.contains("reading") && !id.contains("front")),
        "{:?}",
        ids(&r)
    );
}

#[test]
fn stop_shows_below_75_percent() {
    let r = report(&papers(&[Some(4900); 8]), &cfg(&["weather", "local"], &[]));
    let s = r.suggestions.iter().find(|s| s.id == "stop").unwrap();
    assert_eq!(s.kind, SuggestionKind::Stop);
    assert_eq!(s.title, "Where you stop");
    assert_eq!(s.why, "You usually stop about 49% in, in Local.");
    let r = report(&papers(&[Some(8000); 8]), &cfg(&["weather", "local"], &[]));
    assert!(!ids(&r).contains(&"stop"));
}

#[test]
fn at_most_three_rule_order_then_lowest_share() {
    let secs: Vec<_> = PAPER
        .iter()
        .copied()
        .chain([("onthisday", "On This Day", 9500)])
        .collect();
    let mut s = State::default();
    for i in 0..10 {
        // lottery reached twice, horoscope once, onthisday never, HN twice
        let f = match i {
            0 => 8700,
            1 => 8100,
            _ => 4900,
        };
        edition(&mut s, i, &secs, &[("K", Some(f))]);
    }
    let r = report(
        &s,
        &cfg(
            &[
                "weather",
                "local",
                "feeds",
                "lottery",
                "horoscope",
                "onthisday",
            ],
            &["HN"],
        ),
    );
    assert_eq!(
        ids(&r),
        ["drop:onthisday", "drop:horoscope", "drop:lottery"]
    );
}

#[test]
fn interest_sections_get_rows_move_up_and_drop_removes_the_entry() {
    let cfg = Config::parse(
        "sections = [\"weather\", \"interests\"]\n\n[[interests]]\nname = \"Rust\"\n\n\
         [[interests]]\nname = \"Cargo\"\n",
    )
    .unwrap();
    let mut s = State::default();
    let secs = [
        ("weather", "Weather", 100),
        ("interests:Rust", "Rust", 2000),
        ("interests:Cargo", "Cargo", 9000),
    ];
    for i in 0..10 {
        edition(&mut s, i, &secs, &[("K", Some(9500))]);
    }
    let r = report(&s, &cfg);
    let keys: Vec<_> = r.sections.iter().map(|s| s.key.as_str()).collect();
    assert_eq!(keys, ["weather", "interests:Rust", "interests:Cargo"]);
    assert_eq!(r.sections[2].name, "Cargo");
    // reached late and rarely: an interest gets "Move up", as a feed does
    let mut f = vec![Some(500); 9];
    f.push(Some(9500));
    let mut s2 = State::default();
    for (i, furthest) in f.iter().enumerate() {
        edition(&mut s2, i as u64, &secs, &[("K", *furthest)]);
    }
    let r2 = report(&s2, &cfg);
    assert!(ids(&r2).contains(&"up:interests:Cargo"), "{:?}", ids(&r2));
    let drop = deckle_readership::report::Suggestion {
        id: "drop:interests:Cargo".into(),
        kind: SuggestionKind::Drop,
        key: "interests:Cargo".into(),
        title: String::new(),
        why: String::new(),
    };
    let mut c = cfg.clone();
    apply(&drop, &mut c).unwrap();
    assert_eq!(
        c.interests
            .iter()
            .map(|i| i.name.as_str())
            .collect::<Vec<_>>(),
        ["Rust"]
    );
    assert!(apply(&drop, &mut c).is_err());
    // Move up swaps within [[interests]], then moves the block ahead of the section before it
    let up = deckle_readership::report::Suggestion {
        id: "up:interests:Cargo".into(),
        kind: SuggestionKind::MoveUp,
        key: "interests:Cargo".into(),
        title: String::new(),
        why: String::new(),
    };
    let mut c = cfg.clone();
    apply(&up, &mut c).unwrap();
    assert_eq!(
        c.interests
            .iter()
            .map(|i| i.name.as_str())
            .collect::<Vec<_>>(),
        ["Cargo", "Rust"]
    );
    apply(&up, &mut c).unwrap();
    assert_eq!(c.sections, [SectionKind::Interests, SectionKind::Weather]);
    assert!(apply(&up, &mut c).is_err());
}

#[test]
fn apply_drop_and_move_up_edit_the_config() {
    let mut s = State::default();
    for i in 0..10 {
        edition(&mut s, i, PAPER, &[("K", Some(100))]);
    }
    let drop = |key: &str| deckle_readership::report::Suggestion {
        id: format!("drop:{key}"),
        kind: SuggestionKind::Drop,
        key: key.into(),
        title: String::new(),
        why: String::new(),
    };
    let up = |key: &str| deckle_readership::report::Suggestion {
        id: format!("up:{key}"),
        kind: SuggestionKind::MoveUp,
        ..drop(key)
    };
    let mut c = cfg(&["weather", "local", "feeds", "lottery"], &["HN", "Ars"]);
    apply(&drop("lottery"), &mut c).unwrap();
    assert!(!c.sections.contains(&SectionKind::Lottery));
    apply(&up("feeds:Ars"), &mut c).unwrap();
    assert_eq!(
        c.feeds.iter().map(|f| f.name.as_str()).collect::<Vec<_>>(),
        ["Ars", "HN"]
    );
    apply(&up("feeds:Ars"), &mut c).unwrap();
    assert_eq!(
        c.sections,
        [SectionKind::Weather, SectionKind::Feeds, SectionKind::Local]
    );
    apply(&drop("feeds:HN"), &mut c).unwrap();
    assert_eq!(
        c.feeds.iter().map(|f| f.name.as_str()).collect::<Vec<_>>(),
        ["Ars"]
    );
    let stop = deckle_readership::report::Suggestion {
        kind: SuggestionKind::Stop,
        ..drop("x")
    };
    assert!(apply(&stop, &mut c).is_err());
}

#[test]
fn dismissed_suggestions_return_after_fourteen_editions() {
    let pattern = |i: u64| if i.is_multiple_of(7) { 8500 } else { 4900 };
    let mut s = State::default();
    for i in 0..14 {
        edition(&mut s, i, PAPER, &[("K", Some(pattern(i)))]);
    }
    let c = usual();
    let lottery = report(&s, &c)
        .suggestions
        .into_iter()
        .find(|x| x.id == "drop:lottery")
        .unwrap();
    dismiss(&mut s, &lottery);
    assert!(!ids(&report(&s, &c)).contains(&"drop:lottery"));
    for i in 14..27 {
        edition(&mut s, i, PAPER, &[("K", Some(pattern(i)))]);
    }
    assert!(
        !ids(&report(&s, &c)).contains(&"drop:lottery"),
        "13 newer: still hidden"
    );
    edition(&mut s, 27, PAPER, &[("K", Some(pattern(27)))]);
    assert!(
        ids(&report(&s, &c)).contains(&"drop:lottery"),
        "14 newer: back"
    );
}

#[test]
fn opened_editions_without_positions_suggest_nothing() {
    let mut s = State::default();
    for i in 0..8 {
        edition(&mut s, i, PAPER, &[("K", None)]);
        s.reads
            .get_mut(&date(i))
            .unwrap()
            .get_mut("K")
            .unwrap()
            .opened = true;
    }
    let r = report(&s, &usual());
    assert_eq!(r.opened, 8);
    assert_eq!(r.median_percent, None);
    assert!(r.suggestions.is_empty(), "{:?}", ids(&r));
    assert!(r.sections.iter().all(|row| row.share.is_none()));
}

#[test]
fn a_section_no_edition_had_yet_gets_its_paper_name() {
    let r = report(
        &papers(&[Some(9000); 3]),
        &cfg(&["reading", "onthisday", "feeds"], &["Ars"]),
    );
    let names: Vec<_> = r.sections.iter().map(|s| s.name.as_str()).collect();
    assert_eq!(names, ["Your Reading", "On This Day", "Ars"]);
}

#[test]
fn the_shelf_page_is_a_row_of_its_own() {
    let r = report(
        &papers(&[Some(9000); 3]),
        &cfg(&["weather", "shelf", "lottery"], &[]),
    );
    let rows: Vec<_> = r
        .sections
        .iter()
        .map(|s| (s.key.as_str(), s.name.as_str()))
        .collect();
    assert_eq!(
        rows,
        [
            ("weather", "Weather"),
            ("shelf", "From Your Shelf"),
            ("lottery", "Lottery")
        ]
    );
}

#[test]
fn the_further_of_furthest_and_last_counts() {
    // Seen on a Paperwhite: lpr 89882 ahead of fpr 89410.
    let mut s = State::default();
    edition(&mut s, 0, PAPER, &[("K", Some(3000))]);
    s.reads
        .get_mut(&date(0))
        .unwrap()
        .get_mut("K")
        .unwrap()
        .last = Some(8500);
    assert_eq!(report(&s, &usual()).median_percent, Some(85));
}

#[test]
fn other_places_are_their_own_rows_and_drop_alone() {
    let mut c = cfg(&["weather", "local", "lottery"], &[]);
    c.local.places = vec![
        deckle_digest::config::PlaceNews {
            name: "Asheville, NC".into(),
            google_news: "Asheville NC".into(),
            feeds: Vec::new(),
            max_items: None,
        },
        deckle_digest::config::PlaceNews {
            name: "Boone".into(),
            google_news: "Boone NC".into(),
            feeds: Vec::new(),
            max_items: None,
        },
    ];
    let r = deckle_readership::report(&State::default(), &c);
    let rows: Vec<(&str, &str)> = r
        .sections
        .iter()
        .map(|s| (s.key.as_str(), s.name.as_str()))
        .collect();
    assert_eq!(
        rows,
        [
            ("weather", "Weather"),
            ("local", "Local"),
            ("local:Asheville, NC", "Asheville, NC"),
            ("local:Boone", "Boone"),
            ("lottery", "Lottery"),
        ]
    );
    let drop = deckle_readership::report::Suggestion {
        id: "drop:local:Boone".into(),
        kind: SuggestionKind::Drop,
        key: "local:Boone".into(),
        title: String::new(),
        why: String::new(),
    };
    apply(&drop, &mut c).unwrap();
    assert_eq!(c.local.places.len(), 1);
    assert!(
        c.sections.contains(&SectionKind::Local),
        "Local itself stays"
    );
    assert!(apply(&drop, &mut c).is_err(), "already gone");
}
