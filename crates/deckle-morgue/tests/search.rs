// SPDX-License-Identifier: GPL-3.0-or-later
//! Searching the kept editions and listing a month (Morgue design §4).

use std::path::Path;

use chrono::{Days, NaiveDate};
use deckle_morgue::folder::Morgue;
use deckle_morgue::search::{month, search};
use deckle_morgue::sidecar::{Files, Lead, SectionText, Sidecar, Story};

fn d(s: &str) -> NaiveDate {
    s.parse().unwrap()
}

fn story(title: &str, text: &str) -> Story {
    Story {
        title: title.into(),
        teaser: None,
        text: text.into(),
    }
}

fn edition(date: NaiveDate, stories: Vec<(&str, Vec<Story>)>, lead: Option<&str>) -> Sidecar {
    Sidecar {
        date,
        number: 1,
        paper: "The Deckle Daily".into(),
        place: String::new(),
        weather: None,
        lead: lead.map(|t| Lead {
            section: "Local".into(),
            title: t.into(),
            teaser: None,
        }),
        sections: stories
            .into_iter()
            .map(|(title, stories)| SectionText {
                key: title.to_lowercase(),
                title: title.into(),
                stories,
            })
            .collect(),
        files: Files {
            epub: format!("{date}.epub"),
            kfx: None,
        },
        stem: format!("deckle-daily-{date}"),
    }
}

fn keep(m: &Morgue, tmp: &Path, s: &Sidecar) {
    let epub = tmp.join("e.epub");
    std::fs::write(&epub, b"E").unwrap();
    m.archive_edition(&epub, s).unwrap();
}

fn morgue(tmp: &Path) -> Morgue {
    Morgue {
        root: tmp.join("Morgue"),
        keep_days: 0,
    }
}

#[test]
fn search_folds_case_and_accents() {
    let tmp = tempfile::tempdir().unwrap();
    let m = morgue(tmp.path());
    keep(
        &m,
        tmp.path(),
        &edition(
            d("2026-10-21"),
            vec![("Local", vec![story("Café reopens", "x")])],
            None,
        ),
    );
    for q in ["cafe", "CAFÉ", "Cafe", "cAfÉ"] {
        let hits = search(&m, q);
        assert_eq!(hits.len(), 1, "{q}");
        assert_eq!(
            hits[0].stories,
            [("Local".to_owned(), "Café reopens".to_owned())]
        );
    }
    assert!(search(&m, "tea").is_empty());
}

#[test]
fn search_needs_every_word() {
    let tmp = tempfile::tempdir().unwrap();
    let m = morgue(tmp.path());
    keep(
        &m,
        tmp.path(),
        &edition(
            d("2026-10-21"),
            vec![(
                "Local",
                vec![
                    story("Mill sold", "the old mill and brewery"),
                    story("Mill closes", "only the mill"),
                ],
            )],
            None,
        ),
    );
    let hits = search(&m, "mill brewery");
    assert_eq!(
        hits[0].stories,
        [("Local".to_owned(), "Mill sold".to_owned())]
    );
}

#[test]
fn search_matches_story_text_and_groups_newest_first() {
    let tmp = tempfile::tempdir().unwrap();
    let m = morgue(tmp.path());
    for (date, title) in [
        ("2026-10-19", "A"),
        ("2026-10-21", "B"),
        ("2026-10-20", "C"),
    ] {
        keep(
            &m,
            tmp.path(),
            &edition(
                d(date),
                vec![
                    ("Local", vec![story(title, "deep inside: the mill")]),
                    ("Lottery", vec![story("Draw", "nothing")]),
                ],
                None,
            ),
        );
    }
    let hits = search(&m, "mill");
    let dates: Vec<NaiveDate> = hits.iter().map(|h| h.date).collect();
    assert_eq!(dates, [d("2026-10-21"), d("2026-10-20"), d("2026-10-19")]);
    assert_eq!(hits[0].paper, "The Deckle Daily");
    assert_eq!(hits[0].stories.len(), 1, "only the matching story");
}

#[test]
fn search_matches_words_by_prefix() {
    let tmp = tempfile::tempdir().unwrap();
    let m = morgue(tmp.path());
    keep(
        &m,
        tmp.path(),
        &edition(
            d("2026-10-21"),
            vec![("Local", vec![story("Brewery tour", "x")])],
            None,
        ),
    );
    assert_eq!(search(&m, "brew").len(), 1);
    assert!(search(&m, "ewery").is_empty());
}

#[test]
fn search_ignores_punctuation_and_blank() {
    let tmp = tempfile::tempdir().unwrap();
    let m = morgue(tmp.path());
    keep(
        &m,
        tmp.path(),
        &edition(
            d("2026-10-21"),
            vec![(
                "Local",
                vec![story("Café's new owner", "The mill\u{2014}brewery")],
            )],
            None,
        ),
    );
    assert_eq!(search(&m, "café's").len(), 1);
    assert_eq!(search(&m, "mill—brewery").len(), 1);
    assert_eq!(search(&m, "mill brewery").len(), 1);
    assert!(search(&m, "  ").is_empty());
    assert!(search(&m, "").is_empty());
    assert!(
        search(&m, "—…!").is_empty(),
        "no words at all finds nothing"
    );
}

#[test]
fn month_lists_days_with_leads() {
    let tmp = tempfile::tempdir().unwrap();
    let m = morgue(tmp.path());
    keep(
        &m,
        tmp.path(),
        &edition(d("2026-10-21"), vec![], Some("Mill sold")),
    );
    keep(&m, tmp.path(), &edition(d("2026-10-03"), vec![], None));
    keep(
        &m,
        tmp.path(),
        &edition(d("2026-09-30"), vec![], Some("Elsewhere")),
    );
    let days = month(&m, 2026, 10);
    let got: Vec<(NaiveDate, Option<&str>)> =
        days.iter().map(|x| (x.date, x.lead.as_deref())).collect();
    assert_eq!(
        got,
        [
            (d("2026-10-03"), None),
            (d("2026-10-21"), Some("Mill sold"))
        ]
    );
    assert_eq!(days[0].paper, "The Deckle Daily");
    assert!(month(&m, 2025, 10).is_empty());
}

#[test]
#[ignore = "run with --release --include-ignored"]
fn search_a_year_fast() {
    let tmp = tempfile::tempdir().unwrap();
    let m = morgue(tmp.path());
    let filler =
        "the quick brown fox jumps over the lazy dog while the mill stands idle ".repeat(80);
    let start = d("2026-01-01");
    for i in 0..365u64 {
        let date = start + Days::new(i);
        let stories: Vec<Story> = (0..12)
            .map(|n| story(&format!("Story {n}"), &filler))
            .collect();
        keep(
            &m,
            tmp.path(),
            &edition(date, vec![("Local", stories)], None),
        );
    }
    let t = std::time::Instant::now();
    let hits = search(&m, "brewery mill");
    assert!(hits.is_empty());
    let hits = search(&m, "quick lazy");
    assert_eq!(hits.len(), 365);
    assert!(
        t.elapsed().as_secs_f64() < 1.0,
        "two searches took {:?}",
        t.elapsed()
    );
}
