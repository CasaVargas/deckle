// SPDX-License-Identifier: GPL-3.0-or-later
//! Where each story starts (learning plan, Task 2): found among a section's runs, looked up by
//! position.

use crate::common::*;
use deckle_kfx::text::Run;
use deckle_readership::layout::{Layout, SectionStart, StoryStart, stories_from_runs, story_at};

fn paper() -> Layout {
    let mut l = layout("2026-10-21", &[0, 100, 500]);
    l.end = 1000;
    l.sections[1].key = "interests:Rust".into();
    l.sections[2].key = "feeds:HN".into();
    l
}

fn run(pid: i64, text: &str) -> Run {
    Run {
        pid,
        text: text.into(),
    }
}

#[test]
fn stories_are_the_title_runs_inside_each_sections_range() {
    let runs = [
        run(100, "Rust"),
        run(110, "Rust 2.0  lands"),
        run(300, "Rust jobs grow"),
        run(500, "HN"),
        run(520, "Rust 2.0 lands"),
    ];
    let titles = [
        (
            "interests:Rust".to_owned(),
            vec![
                "Rust 2.0 lands".to_owned(),
                "Rust jobs grow".to_owned(),
                "Missing".to_owned(),
            ],
        ),
        ("feeds:HN".to_owned(), vec!["Rust 2.0 lands".to_owned()]),
    ];
    let found = stories_from_runs(&runs, &paper(), &titles);
    assert_eq!(
        found,
        [
            (
                "interests:Rust".to_owned(),
                vec![
                    StoryStart {
                        title: "Rust 2.0 lands".into(),
                        start: 110
                    },
                    StoryStart {
                        title: "Rust jobs grow".into(),
                        start: 300
                    },
                ]
            ),
            (
                "feeds:HN".to_owned(),
                vec![StoryStart {
                    title: "Rust 2.0 lands".into(),
                    start: 520
                }]
            ),
        ]
    );
}

#[test]
fn the_in_this_section_list_is_skipped_and_the_headings_are_the_starts() {
    // a section front lists every title before the stories themselves (render.rs)
    let runs = [
        run(100, "Rust"),
        run(101, "Your interests"),
        run(102, "In this section"),
        run(103, "Rust 2.0 lands"),
        run(104, "Rust jobs grow"),
        run(110, "Rust 2.0 lands"),
        run(111, "Outlet · Oct 25"),
        run(112, "A long first paragraph of the story."),
        run(300, "Rust jobs grow"),
        run(301, "Brief."),
    ];
    let titles = [(
        "interests:Rust".to_owned(),
        vec!["Rust 2.0 lands".to_owned(), "Rust jobs grow".to_owned()],
    )];
    let found = stories_from_runs(&runs, &paper(), &titles);
    assert_eq!(
        found[0].1.iter().map(|s| s.start).collect::<Vec<_>>(),
        [110, 300]
    );
}

#[test]
fn story_at_is_the_last_story_starting_at_or_before_the_position() {
    let mut l = paper();
    l.sections[1].stories = vec![
        StoryStart {
            title: "Rust 2.0 lands".into(),
            start: 110,
        },
        StoryStart {
            title: "Rust jobs grow".into(),
            start: 300,
        },
    ];
    l.sections[2].stories = vec![StoryStart {
        title: "Comet".into(),
        start: 520,
    }];
    let at = |p: i64| story_at(&l, p).map(|(s, st)| (s.key.as_str(), st.title.as_str()));
    assert_eq!(at(4), None, "the front has no stories");
    assert_eq!(at(105), None, "a section's own front");
    assert_eq!(at(110), Some(("interests:Rust", "Rust 2.0 lands")));
    assert_eq!(at(299), Some(("interests:Rust", "Rust 2.0 lands")));
    assert_eq!(at(300), Some(("interests:Rust", "Rust jobs grow")));
    assert_eq!(at(700), Some(("feeds:HN", "Comet")));
    assert_eq!(at(1000), None, "past the end");
}

#[test]
fn a_layout_without_stories_loads_with_none() {
    let json = serde_json::to_string(&paper()).unwrap();
    assert!(!json.contains("stories"));
    let back: Layout = serde_json::from_str(&json).unwrap();
    assert!(back.sections.iter().all(|s| s.stories.is_empty()));
    let _ = SectionStart {
        key: "x".into(),
        name: "x".into(),
        start: 0,
        items: None,
        stories: Vec::new(),
    };
}
