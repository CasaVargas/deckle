// SPDX-License-Identifier: GPL-3.0-or-later
use chrono::NaiveDate;
use deckle_kfx::text::{content_runs, section_first_runs};
use deckle_readership::layout::{Layout, SectionStart, layout_from_kfx, section_at};

fn mini() -> Vec<u8> {
    std::fs::read(deckle_kfx::fixtures::path("deckle-mini-pdoc.golden.kfx")).unwrap()
}

fn d(s: &str) -> NaiveDate {
    s.parse().unwrap()
}

/// The mini book's chapters after the first, keyed "b", "c": chapter one stands in for the
/// front page.
fn chapter_names() -> Vec<(String, String)> {
    section_first_runs(&mini()).unwrap()[1..]
        .iter()
        .zip(["b", "c"])
        .map(|(r, k)| (k.to_owned(), r.text.clone()))
        .collect()
}

#[test]
fn section_starts_come_from_section_boundaries_not_text_search() {
    // "Chapter One" also opens the book's text, as a front-page line naming a section would;
    // matching runs by section keeps the later sections at their own headings.
    let l = layout_from_kfx(
        &mini(),
        d("2026-10-21"),
        "Mini",
        "deckle-daily-2026-10-21",
        &chapter_names(),
    )
    .unwrap();
    let keys: Vec<_> = l.sections.iter().map(|s| s.key.as_str()).collect();
    assert_eq!(keys, ["front", "b", "c"]);
    assert_eq!(l.sections[0].name, "Front page");
    assert_eq!(l.sections[0].start, 3);
    assert!(l.sections.windows(2).all(|w| w[0].start < w[1].start));
    let firsts = section_first_runs(&mini()).unwrap();
    assert_eq!(l.sections[1].start, firsts[1].pid);
    assert_eq!(l.end, content_runs(&mini()).unwrap().1);
    assert_eq!(
        (l.title.as_str(), l.file.as_str()),
        ("Mini", "deckle-daily-2026-10-21")
    );
}

#[test]
fn a_heading_repeated_in_an_earlier_section_is_not_taken() {
    // Ask for the first chapter's heading as if it were a later section: section boundaries
    // say it isn't the first run of any section after the front page.
    let names = vec![("x".to_owned(), "Chapter One".to_owned())];
    assert!(layout_from_kfx(&mini(), d("2026-10-21"), "Mini", "f", &names).is_err());
}

#[test]
fn a_section_missing_from_the_kfx_is_an_error() {
    let names = vec![("x".to_owned(), "No Such Heading".to_owned())];
    let e = layout_from_kfx(&mini(), d("2026-10-21"), "Mini", "f", &names).unwrap_err();
    assert!(e.contains("No Such Heading"), "{e}");
}

fn layout_fixture() -> Layout {
    let s = |key: &str, start| SectionStart {
        key: key.into(),
        name: key.into(),
        start,
    };
    Layout {
        date: d("2026-10-21"),
        title: "T".into(),
        file: "f".into(),
        end: 30000,
        sections: vec![s("front", 4), s("reading", 1204), s("weather", 20511)],
    }
}

#[test]
fn positions_map_to_the_section_starting_at_or_before_them() {
    let l = layout_fixture();
    assert_eq!(section_at(&l, 1204).unwrap().key, "reading");
    assert_eq!(section_at(&l, 1203).unwrap().key, "front");
    assert_eq!(section_at(&l, 999_999).unwrap().key, "weather");
    assert!(section_at(&l, 0).is_none());
}

#[test]
fn layouts_round_trip_as_json() {
    let l = layout_fixture();
    let back: Layout = serde_json::from_str(&serde_json::to_string(&l).unwrap()).unwrap();
    assert_eq!(back, l);
}

#[test]
fn headings_match_with_whitespace_normalised() {
    let mut names = chapter_names();
    names[0].1 = names[0].1.replace(' ', "  ");
    names[0].1.push('\n');
    assert!(layout_from_kfx(&mini(), d("2026-10-21"), "Mini", "f", &names).is_ok());
}
