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

fn owned_keys(text: &str) -> Vec<String> {
    phrases(text).into_iter().map(|(k, _)| k).collect()
}

#[test]
fn punctuation_ends_a_run_and_an_outlet_tag_is_dropped() {
    let k = owned_keys("Man found dead in Cary, NC - Cremation Society of the Carolinas");
    assert!(!k.iter().any(|p| p.contains("cary nc")), "{k:?}");
    assert!(k.iter().any(|p| p == "cary"), "{k:?}");
    assert!(
        !k.iter().any(|p| p.contains("cremation")),
        "the outlet tag: {k:?}"
    );
    let k = owned_keys("Standoff in Candor - WXII");
    assert!(
        k.iter().any(|p| p == "candor") && !k.iter().any(|p| p == "wxii"),
        "{k:?}"
    );
    let k = owned_keys("Panthers: Bryce Young benched; Dalton starts");
    for want in ["panthers", "bryce young", "dalton"] {
        assert!(k.iter().any(|p| p == want), "{k:?} lacks {want}");
    }
    assert!(!k.iter().any(|p| p == "panthers bryce young"), "{k:?}");
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
    // the hit is dated by the day you read, not the day the Kindle was plugged in
    let earlier = shelf_with(
        "Brandon Sanderson",
        "2026-10-25T08:00:00Z",
        "2026-10-20T19:00:00Z",
    );
    assert_eq!(record_shelf(&mut learning, &earlier, now, &stop), 1);
    let dates: Vec<_> = learning.candidates["brandon sanderson"]
        .hits
        .iter()
        .map(|h| h.date)
        .collect();
    assert_eq!(dates, [d("2026-10-24"), d("2026-10-20")]);
    // plugging in again on another day without reading adds nothing
    let again = shelf_with(
        "Brandon Sanderson",
        "2026-10-26T08:00:00Z",
        "2026-10-20T19:00:00Z",
    );
    assert_eq!(record_shelf(&mut learning, &again, now, &stop), 0);
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
    let mut off = cfg();
    off.learning.enabled = false;
    assert!(
        suggest(&l, &off, today()).is_empty(),
        "learning off shows nothing"
    );
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

#[test]
fn renaming_moves_the_acceptance_and_the_candidate() {
    let mut l = seeded();
    let mut c = cfg();
    accept(&mut l, &mut c, "bryce young", today()).unwrap();
    assert!(l.rename("bryce young", "bryce young (qb)"));
    assert!(l.accepted.contains_key("bryce young (qb)"));
    assert!(!l.accepted.contains_key("bryce young"));
    assert_eq!(l.candidates["bryce young (qb)"].display, "Bryce Young");
    assert!(!l.rename("nobody", "x"));
}

// ---- naming through the editor (interests design §6) ----

use deckle_readership::learn::{naming_candidates, record_names};

#[test]
fn naming_candidates_skip_shelf_and_highlight_only_and_named() {
    let mut l = seeded();
    let stop = HashSet::new();
    // a highlight-only subject (its hits' titles are clipped text) and a shelf author
    for (date, title) in [
        ("2026-10-20", "Dalton looked sharp in relief"),
        ("2026-10-22", "Dalton again"),
        ("2026-10-23", "Dalton starts"),
    ] {
        note(&mut l, &stop, title, hit(date, title, Signal::Highlight));
    }
    let now: DateTime<Utc> = "2026-10-25T12:00:00Z".parse().unwrap();
    let shelf = shelf_with(
        "Brandon Sanderson",
        "2026-10-24T20:00:00Z",
        "2026-10-24T19:00:00Z",
    );
    record_shelf(&mut l, &shelf, now, &stop);
    let c = naming_candidates(&l, &cfg(), today());
    let keys: Vec<&str> = c.iter().map(|(k, _, _)| k.as_str()).collect();
    assert_eq!(keys, ["bryce young"], "{keys:?}");
    let (_, phrase, headlines) = &c[0];
    assert_eq!(phrase, "Bryce Young");
    assert_eq!(headlines.len(), 3);
    assert!(headlines.iter().all(|h| h.contains("Bryce Young")));
    // once named, it isn't sent again
    assert_eq!(
        record_names(
            &mut l,
            &[
                (
                    "bryce young".into(),
                    "Bryce Young".into(),
                    "\"Bryce Young\" Panthers".into()
                ),
                ("nobody".into(), "X".into(), "x".into()),
            ]
        ),
        1
    );
    assert!(naming_candidates(&l, &cfg(), today()).is_empty());
    let s = suggest(&l, &cfg(), today());
    let young = s.iter().find(|n| n.key == "bryce young").unwrap();
    assert_eq!(young.name, "Bryce Young");
    assert_eq!(young.query.as_deref(), Some("\"Bryce Young\" Panthers"));
    let mut c = cfg();
    let entry = accept(&mut l, &mut c, "bryce young", today()).unwrap();
    assert_eq!(entry.query, "\"Bryce Young\" Panthers");
}

#[test]
fn naming_candidates_send_at_most_ten_with_five_newest_headlines() {
    let mut l = Learning::default();
    let stop = HashSet::new();
    for n in 0..12 {
        let name = format!("Topic{n}");
        for i in 0..7 {
            let date = if i % 2 == 0 {
                "2026-10-20"
            } else {
                "2026-10-23"
            };
            let title = format!("{name} story {i}");
            note(&mut l, &stop, &title, hit(date, &title, Signal::Tap));
        }
    }
    let c = naming_candidates(&l, &cfg(), today());
    assert_eq!(c.len(), 10);
    assert!(c.iter().all(|(_, _, h)| h.len() == 5));
    assert!(
        c[0].2[0].contains("story 1")
            || c[0].2[0].contains("story 3")
            || c[0].2[0].contains("story 5"),
        "newest first: {:?}",
        c[0].2
    );
}

#[test]
fn naming_sends_only_public_headlines_inside_the_window_and_the_phrase_as_printed() {
    let mut l = Learning::default();
    let stop = HashSet::new();
    let private = |date: &str, section: &str, title: &str| Hit {
        date: d(date),
        section: section.into(),
        title: title.into(),
        signal: Signal::Tap,
    };
    // three taps on saved articles and one on the shelf: a candidate, but nothing to send
    note(
        &mut l,
        &stop,
        "Secret Project memo",
        private("2026-10-20", "reading", "Secret Project memo"),
    );
    note(
        &mut l,
        &stop,
        "Secret Project update",
        private("2026-10-22", "reading", "Secret Project update"),
    );
    note(
        &mut l,
        &stop,
        "Secret Project book",
        private("2026-10-23", "shelf", "Secret Project book"),
    );
    // a public subject first spelled oddly in a private title, then printed in the news
    note(
        &mut l,
        &stop,
        "BRYCE YOUNG notes",
        private("2026-10-19", "reading", "BRYCE YOUNG notes"),
    );
    note(
        &mut l,
        &stop,
        "Bryce Young benched",
        hit("2026-10-20", "Bryce Young benched", Signal::Tap),
    );
    note(
        &mut l,
        &stop,
        "Bryce Young returns",
        hit("2026-10-22", "Bryce Young returns", Signal::Tap),
    );
    note(
        &mut l,
        &stop,
        "Why Bryce Young sat",
        hit("2026-10-22", "Why Bryce Young sat", Signal::Tap),
    );
    // an old public headline, outside the window
    note(
        &mut l,
        &stop,
        "Bryce Young drafted",
        hit("2026-09-01", "Bryce Young drafted", Signal::Tap),
    );
    let c = naming_candidates(&l, &cfg(), today());
    let keys: Vec<&str> = c.iter().map(|(k, _, _)| k.as_str()).collect();
    assert_eq!(keys, ["bryce young"], "{keys:?}");
    let (_, phrase, headlines) = &c[0];
    assert_eq!(
        phrase, "Bryce Young",
        "as printed in a public headline, not the private spelling"
    );
    assert_eq!(headlines.len(), 3, "{headlines:?}");
    assert!(
        headlines
            .iter()
            .all(|h| !h.contains("notes") && !h.contains("drafted")),
        "{headlines:?}"
    );
}

// ---- nudges (interests design §7, as amended by §15) ----

use deckle_readership::learn::{accept_nudges, nudges_for};

fn state_with_nudge(histories: &[(&str, Vec<i64>)]) -> State {
    let mut state = State::default();
    let mut l = layout("2026-10-25", &[0, 100, 9000]);
    l.end = 9500;
    l.sections[1].key = "horoscope".into();
    l.sections[2].key = "nudge:bryce young".into();
    state.record_layout(l);
    for (device, history) in histories {
        state.reads.entry(d("2026-10-25")).or_default().insert(
            (*device).into(),
            Read {
                opened: true,
                history: history.clone(),
                seen: "x".into(),
                ..Read::default()
            },
        );
    }
    state
}

#[test]
fn a_tap_on_a_nudge_page_accepts_it_once() {
    let mut l = seeded();
    let mut c = cfg();
    c.sections = vec![SectionKind::Local, SectionKind::Feeds];
    let state = state_with_nudge(&[("K", vec![4, 9100]), ("K2", vec![9100])]);
    assert_eq!(accept_nudges(&mut l, &mut c, &state, today()), ["Bryce Young"]);
    assert!(c.interests.iter().any(|i| i.name == "Bryce Young"));
    assert_eq!(
        c.sections,
        [SectionKind::Local, SectionKind::Interests, SectionKind::Feeds]
    );
    assert_eq!(l.accepted["bryce young"], today());
    // again: nothing new
    assert!(accept_nudges(&mut l, &mut c, &state, today()).is_empty());
    // a position elsewhere, or a nudge for a key no longer known, does nothing
    let mut l2 = seeded();
    let mut c2 = cfg();
    assert!(accept_nudges(&mut l2, &mut c2, &state_with_nudge(&[("K", vec![150])]), today()).is_empty());
    l2.candidates.remove("bryce young");
    assert!(accept_nudges(&mut l2, &mut c2, &state, today()).is_empty());
    // dismissed on the Mac after the paper was built: the tap is the newer decision
    let mut l3 = seeded();
    let mut c3 = cfg();
    dismiss(&mut l3, "bryce young", today()).unwrap();
    assert_eq!(accept_nudges(&mut l3, &mut c3, &state, today()), ["Bryce Young"]);
}

#[test]
fn nudges_for_are_the_top_three_suggestions_when_the_flag_is_on() {
    let l = seeded();
    let mut c = cfg();
    assert!(nudges_for(&l, &c, today()).is_empty(), "flag off");
    c.learning.nudges = true;
    let n = nudges_for(&l, &c, today());
    assert_eq!(n.len(), 1);
    assert_eq!(n[0].0, "bryce young");
    assert_eq!(n[0].1, "Bryce Young");
    assert!(n[0].2.starts_with("You read 3 stories"));
}
