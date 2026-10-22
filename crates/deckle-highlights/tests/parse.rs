// SPDX-License-Identifier: GPL-3.0-or-later
use deckle_highlights::{Kind, parse};

fn fixture(name: &str) -> std::path::PathBuf {
    std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../fixtures/clippings")
        .join(name)
}

#[test]
fn parses_every_variant() {
    let p = parse(&std::fs::read(fixture("variants.txt")).unwrap());
    assert!(p.error.is_none(), "{:?}", p.error);
    assert_eq!(p.entries.len(), 10);
    let a = &p.entries[0];
    assert_eq!(
        (a.title.as_str(), a.author.as_str()),
        ("Walden", "Henry David Thoreau")
    );
    assert_eq!(a.kind, Kind::Highlight);
    assert_eq!(a.page.as_deref(), Some("88"));
    assert_eq!(a.location, Some((1340, 1342)));
    assert_eq!(a.added.date.as_deref(), Some("2025-03-03"));
    assert_eq!(p.entries[1].kind, Kind::Note);
    assert_eq!(p.entries[1].location, Some((1342, 1342)));
    assert_eq!(p.entries[2].kind, Kind::Bookmark);
    assert_eq!(
        p.entries[3].title,
        "The Count of Monte Cristo (Illustrated)"
    );
    assert_eq!(p.entries[3].author, "Alexandre Dumas");
    assert_eq!(p.entries[3].added.date.as_deref(), Some("2026-10-19"));
    assert_eq!(
        (p.entries[4].title.as_str(), p.entries[4].author.as_str()),
        ("notes-2026", "")
    );
    assert_eq!(p.entries[5].page, None);
    assert_eq!(p.entries[5].location, Some((220, 221)));
    assert_eq!(
        (p.entries[6].page.as_deref(), p.entries[6].location),
        (Some("4"), None)
    );
    let es = &p.entries[8];
    assert_eq!(es.kind, Kind::Highlight);
    assert!(
        es.raw_meta
            .as_deref()
            .unwrap()
            .starts_with("- Tu subrayado")
    );
    assert_eq!(es.added.date, None);
    assert!(p.entries[0].raw_meta.is_none());
    assert_eq!(
        p.entries[9].text,
        "Our life is frittered away by detail.\n\nSimplify, simplify."
    );
}

#[test]
fn invalid_utf8_is_decoded_lossily() {
    let mut b = b"Walden (Henry David Thoreau)\n- Your Highlight on page 1 | Location 1-2 | Added on Monday, March 3, 2025 10:14:22 AM\n\nwoods \xFF here\n==========\n".to_vec();
    let p = parse(&b);
    assert_eq!(p.entries.len(), 1);
    assert!(p.entries[0].text.contains('\u{FFFD}'));
    b.clear();
    assert!(parse(&b).entries.is_empty());
    assert!(parse(&b).error.is_none());
}

#[test]
fn a_broken_entry_reports_its_line_and_keeps_the_rest() {
    let s = "A (B)\n- Your Highlight on page 1 | Location 1-2 | Added on Monday, March 3, 2025 10:14:22 AM\n\none\n==========\nC (D)\nnot a meta line\n==========\nE (F)\n- Your Note on Location 5 | Added on Monday, March 3, 2025 10:14:22 AM\n\nthree\n==========\n";
    let p = parse(s.as_bytes());
    assert_eq!(p.entries.len(), 2);
    assert_eq!(p.error.as_ref().unwrap().line, 7);
    assert_eq!(p.entries[1].text, "three");
}
