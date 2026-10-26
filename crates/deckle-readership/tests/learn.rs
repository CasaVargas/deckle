// SPDX-License-Identifier: GPL-3.0-or-later
//! Learning (interests design §5): candidates from taps, paper highlights and shelf authors;
//! suggestions; accept and dismiss.
mod common;

use std::collections::HashSet;

use chrono::{DateTime, NaiveDate, Utc};
use common::*;
use deckle_digest::Config;
use deckle_digest::config::SectionKind;
use deckle_digest::shelf::store::{BookSnap, KindleSnapshot, Shelf};
use deckle_readership::layout::StoryStart;
use deckle_readership::learn::{
    Hit, Learning, Signal, accept, dismiss, note, phrases, prune, record_highlights, record_shelf,
    record_taps, stoplist, suggest,
};
use deckle_readership::state::{Read, State};

fn cfg() -> Config {
    Config::parse(
        "title = \"The Deckle Daily\"\nplace = \"Carthage, NC\"\n\
         sections = [\"local\", \"interests\", \"feeds\"]\n\
         [[interests]]\nname = \"Rust\"\nnot = [\"Rust Belt\"]\n\
         [[feeds]]\nname = \"HN\"\nurl = \"https://x/f\"\n\
         [learning]\nmin_stories = 3\nwindow_days = 14\n",
    )
    .unwrap()
}

fn today() -> NaiveDate {
    d("2026-10-25")
}

fn hit(date: &str, title: &str, signal: Signal) -> Hit {
    Hit {
        date: d(date),
        section: "interests:Panthers".into(),
        title: title.into(),
        signal,
    }
}

fn keys(v: &[(String, String)]) -> Vec<&str> {
    v.iter().map(|(k, _)| k.as_str()).collect()
}

#[test]
fn phrases_find_capitalised_runs_and_long_words() {
    let p = phrases(
        "Bryce Young benched as Carolina Panthers fall; Rust 2.0 lands in Moore County Schools",
    );
    let k = keys(&p);
    for want in [
        "bryce young",
        "carolina panthers",
        "rust",
        "moore county schools",
    ] {
        assert!(k.contains(&want), "{k:?} lacks {want}");
    }
    assert!(!k.contains(&"lands") && !k.contains(&"in") && !k.contains(&"fall"));
    let p = phrases("The Trial of Alec Baldwin");
    let k = keys(&p);
    assert!(
        k.contains(&"trial of alec baldwin") || k.contains(&"alec baldwin"),
        "{k:?}"
    );
    assert!(!k.contains(&"the") && !k.contains(&"of"));
    // display keeps the spelling it saw
    assert!(
        p.iter()
            .any(|(_, d)| d == "Alec Baldwin" || d == "Trial of Alec Baldwin")
    );
}

#[test]
fn stoplist_holds_config_and_builtins() {
    let s = stoplist(&cfg());
    for w in [
        "rust",
        "rust belt",
        "the deckle daily",
        "deckle daily",
        "carthage",
        "breaking",
        "monday",
        "october",
        "local",
        "hn",
    ] {
        assert!(s.contains(w), "{w}");
    }
    assert!(!s.contains("panthers"));
}

#[test]
fn note_dedupes_the_same_hit() {
    let mut l = Learning::default();
    let stop = HashSet::new();
    let h = hit("2026-10-20", "Bryce Young benched", Signal::Tap);
    note(&mut l, &stop, "Bryce Young benched", h.clone());
    note(&mut l, &stop, "Bryce Young benched", h);
    assert_eq!(l.candidates["bryce young"].hits.len(), 1);
    assert_eq!(l.candidates["bryce young"].display, "Bryce Young");
}

#[test]
fn taps_resolve_through_the_layout() {
    let mut state = State::default();
    let mut l = layout("2026-10-20", &[0, 100]);
    l.end = 1000;
    l.sections[1].key = "interests:Panthers".into();
    l.sections[1].stories = vec![
        StoryStart {
            title: "Bryce Young benched".into(),
            start: 110,
        },
        StoryStart {
            title: "Panthers trade talk".into(),
            start: 300,
        },
    ];
    state.record_layout(l);
    let read = |history: Vec<i64>| Read {
        opened: true,
        history,
        seen: "x".into(),
        ..Read::default()
    };
    let date = d("2026-10-20");
    state
        .reads
        .entry(date)
        .or_default()
        .insert("K".into(), read(vec![4, 150, 310, 5000]));
    state
        .reads
        .entry(date)
        .or_default()
        .insert("K2".into(), read(vec![150]));
    let mut learning = Learning::default();
    let added = record_taps(&mut learning, &state, &stoplist(&cfg()));
    assert_eq!(added, 2, "{:?}", learning.candidates.keys());
    let young = &learning.candidates["bryce young"];
    assert_eq!(young.hits.len(), 1);
    assert_eq!(young.hits[0].signal, Signal::Tap);
    assert_eq!(young.hits[0].section, "interests:Panthers");
    assert!(
        learning.candidates.contains_key("panthers"),
        "the lowercase rest is no phrase"
    );
}

const CLIPPINGS: &str = "Walden (Thoreau, Henry David)\n- Your Highlight on page 1 | Location 1-2 | Added on Monday, October 20, 2026 10:14:22 AM\n\nI went to the woods\n==========\n\
The Deckle Daily — Tuesday, October 20, 2026 (Deckle)\n- Your Highlight on page 3 | Location 40-41 | Added on Tuesday, October 20, 2026 7:14:22 AM\n\nBryce Young looked lost against the Bucs\n==========\n\
The Kid Courier — Tuesday, October 20, 2026 (Deckle)\n- Your Highlight on page 1 | Location 2-3 | Added on Tuesday, October 20, 2026 7:20:00 AM\n\nDinosaurs roamed Carthage\n==========\n";

#[test]
fn highlights_count_only_this_papers_editions() {
    let mut learning = Learning::default();
    let added = record_highlights(
        &mut learning,
        CLIPPINGS.as_bytes(),
        "The Deckle Daily",
        &stoplist(&cfg()),
    );
    assert!(added >= 1, "{:?}", learning.candidates.keys());
    let young = &learning.candidates["bryce young"];
    assert_eq!(young.hits[0].date, d("2026-10-20"));
    assert_eq!(young.hits[0].signal, Signal::Highlight);
    assert!(!learning.candidates.contains_key("dinosaurs"));
    assert!(!learning.candidates.contains_key("woods"));
}

fn shelf_with(author: &str, seen: &str, last_read: &str) -> Shelf {
    let mut shelf = Shelf::default();
    shelf.kindles.insert(
        "K".into(),
        KindleSnapshot {
            name: "Scribe".into(),
            seen: seen.into(),
            books: vec![BookSnap {
                file: "book.kfx".into(),
                title: "Mistborn".into(),
                authors: vec![author.into()],
                last_read: Some(last_read.into()),
                ..Default::default()
            }],
            ..Default::default()
        },
    );
    shelf
}

#[test]
fn shelf_authors_get_one_hit_per_snapshot_day() {
    let now: DateTime<Utc> = "2026-10-25T12:00:00Z".parse().unwrap();
    let shelf = shelf_with(
        "Brandon Sanderson",
        "2026-10-24T20:00:00Z",
        "2026-10-24T19:00:00Z",
    );
    let mut learning = Learning::default();
    let stop = stoplist(&cfg());
    assert_eq!(record_shelf(&mut learning, &shelf, now, &stop), 1);
    assert_eq!(record_shelf(&mut learning, &shelf, now, &stop), 0);
    let c = &learning.candidates["brandon sanderson"];
    assert_eq!(
        (c.hits[0].date, c.hits[0].signal),
        (d("2026-10-24"), Signal::Shelf)
    );
    // a book not touched for months is not in progress
    let stale = shelf_with("Old Author", "2026-10-24T20:00:00Z", "2026-05-01T00:00:00Z");
    assert_eq!(record_shelf(&mut learning, &stale, now, &stop), 0);
}

fn seeded() -> Learning {
    let mut l = Learning::default();
    let stop = HashSet::new();
    for (date, title) in [
        ("2026-10-20", "Bryce Young benched"),
        ("2026-10-22", "Bryce Young returns"),
        ("2026-10-22", "Why Bryce Young sat"),
    ] {
        note(&mut l, &stop, title, hit(date, title, Signal::Tap));
    }
    for title in ["Cargo one", "Cargo two", "Cargo three"] {
        note(&mut l, &stop, title, hit("2026-10-22", title, Signal::Tap));
    }
    for (date, title) in [
        ("2026-10-20", "Rust 2.0"),
        ("2026-10-21", "Rust jobs"),
        ("2026-10-22", "Rust books"),
    ] {
        note(&mut l, &stop, title, hit(date, title, Signal::Tap));
    }
    l
}

#[test]
fn suggest_needs_three_stories_on_two_days_and_ranks() {
    let l = seeded();
    let s = suggest(&l, &cfg(), today());
    let names: Vec<&str> = s.iter().map(|n| n.name.as_str()).collect();
    assert_eq!(names, ["Bryce Young"], "{s:?}");
    assert_eq!(s[0].stories, 3);
    assert_eq!(
        s[0].why,
        "You read 3 stories about Bryce Young in the last 14 days."
    );
    assert_eq!(s[0].key, "bryce young");
    assert_eq!(s[0].query, None);
    // dismissed last week: hidden; dismissed long ago: back after prune
    let mut l2 = seeded();
    dismiss(&mut l2, "bryce young", d("2026-10-15")).unwrap();
    assert!(suggest(&l2, &cfg(), today()).is_empty());
    l2.dismissed.insert("bryce young".into(), d("2026-07-01"));
    prune(&mut l2, today(), 14);
    assert_eq!(suggest(&l2, &cfg(), today()).len(), 1);
    // at most five, more stories first, then the latest date
    let mut l3 = Learning::default();
    let stop = HashSet::new();
    for n in 0..7 {
        let name = format!("Topic{n}");
        let count = 3 + n;
        for i in 0..count {
            let date = if i % 2 == 0 {
                "2026-10-20"
            } else {
                "2026-10-23"
            };
            let title = format!("{name} story {i}");
            note(&mut l3, &stop, &title, hit(date, &title, Signal::Tap));
        }
    }
    let s = suggest(&l3, &cfg(), today());
    assert_eq!(s.len(), 5);
    assert_eq!(s[0].name, "Topic6");
    assert!(s[0].stories > s[4].stories);
}

#[test]
fn accept_writes_the_entry_and_dismiss_records() {
    let mut l = seeded();
    let mut c = cfg();
    c.sections = vec![SectionKind::Local, SectionKind::Feeds];
    let entry = accept(&mut l, &mut c, "bryce young", today()).unwrap();
    assert_eq!(entry.name, "Bryce Young");
    assert!(c.interests.iter().any(|i| i.name == "Bryce Young"));
    assert_eq!(
        c.sections,
        [
            SectionKind::Local,
            SectionKind::Interests,
            SectionKind::Feeds
        ]
    );
    assert_eq!(l.accepted["bryce young"], today());
    assert!(
        suggest(&l, &c, today()).is_empty(),
        "accepted and now an interest"
    );
    assert!(accept(&mut l, &mut c, "nobody", today()).is_err());
    // a shelf author gets a narrower search
    let now: DateTime<Utc> = "2026-10-25T12:00:00Z".parse().unwrap();
    let shelf = shelf_with(
        "Brandon Sanderson",
        "2026-10-24T20:00:00Z",
        "2026-10-24T19:00:00Z",
    );
    record_shelf(&mut l, &shelf, now, &HashSet::new());
    let entry = accept(&mut l, &mut c, "brandon sanderson", today()).unwrap();
    assert_eq!(entry.query, "\"Brandon Sanderson\" author OR book");
    dismiss(&mut l, "cargo", today()).unwrap();
    assert_eq!(l.dismissed["cargo"], today());
    assert!(dismiss(&mut l, "nobody", today()).is_err());
}

#[test]
fn prune_drops_old_hits_and_empty_candidates() {
    let mut l = seeded();
    let stop = HashSet::new();
    note(
        &mut l,
        &stop,
        "Ancient News",
        hit("2026-09-01", "Ancient News", Signal::Tap),
    );
    prune(&mut l, today(), 14);
    assert!(!l.candidates.contains_key("ancient news"));
    assert_eq!(l.candidates["bryce young"].hits.len(), 3);
    prune(&mut l, d("2026-12-01"), 14);
    assert!(l.candidates.is_empty());
}

#[test]
fn learning_file_round_trips_and_lives_beside_readership() {
    let tmp = tempfile::tempdir().unwrap();
    let path = tmp.path().join("deckle").join("interests.json");
    let l = seeded();
    l.save(&path).unwrap();
    assert_eq!(Learning::load(&path), l);
    assert_eq!(
        Learning::load(&tmp.path().join("none.json")),
        Learning::default()
    );
    let main = Learning::path_for(&cfg());
    assert!(main.ends_with("interests.json"), "{}", main.display());
    assert_eq!(
        main.parent(),
        deckle_readership::state_path().parent(),
        "beside readership.json"
    );
    let mut paper = cfg();
    paper.paper = Some(deckle_digest::config::PaperRef {
        slug: "kid".into(),
        state_dir: tmp.path().join("papers/kid"),
    });
    assert_eq!(
        Learning::path_for(&paper),
        tmp.path().join("papers/kid/interests.json")
    );
}
