// SPDX-License-Identifier: GPL-3.0-or-later
use deckle_highlights::{
    Added, Book, EDITIONS_KEY, Entry, Item, Kind, entry_id, group, parse, render_items,
    render_new_file,
};

fn read(name: &str) -> Vec<u8> {
    std::fs::read(
        std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../fixtures/clippings")
            .join(name),
    )
    .unwrap()
}

fn highlight_at(loc: u32) -> Entry {
    Entry {
        title: "Walden".into(),
        author: "Henry David Thoreau".into(),
        kind: Kind::Highlight,
        page: None,
        location: Some((loc, loc + 2)),
        added: Added::default(),
        text: format!("passage at {loc}"),
        raw_meta: None,
    }
}

#[test]
fn groups_collapse_and_attach() {
    let books = group(parse(&read("variants.txt")).entries);
    let walden = books.iter().find(|b| b.title == "Walden").unwrap();
    let hs: Vec<_> = walden
        .items
        .iter()
        .filter(|i| matches!(i, Item::Highlight { .. }))
        .collect();
    assert_eq!(hs.len(), 2, "(a) collapsed into (h); (j) stays");
    let Item::Highlight { entry, notes } = hs[0] else {
        unreachable!()
    };
    assert_eq!(entry.location, Some((1338, 1345)));
    assert_eq!(notes.len(), 1);
    assert!(
        books
            .iter()
            .all(|b| { b.items.iter().all(|i| i.entry().kind != Kind::Bookmark) })
    );
    assert_eq!(books.len(), 6, "one per book; bookmarks add none");
}

#[test]
fn editions_share_one_book() {
    let e = |t: &str| Entry {
        title: t.into(),
        author: "Deckle".into(),
        ..highlight_at(10)
    };
    let books = group(vec![
        e("The Morning Deckle — Tuesday, October 20, 2026"),
        e("The Morning Deckle — Wednesday, October 21, 2026"),
    ]);
    assert_eq!(books.len(), 1);
    assert_eq!(books[0].key, EDITIONS_KEY);
    assert_eq!(books[0].title, "The Morning Deckle");
    assert_eq!(
        books[0].items.len(),
        2,
        "same location, different editions: no collapse"
    );
    let md = render_new_file(&books[0]);
    assert!(md.contains("## Tuesday, October 20, 2026\n\n> passage at 10"));
    assert!(md.contains("## Wednesday, October 21, 2026\n\n"));
}

#[test]
fn ids_ignore_whitespace_and_case_of_title_but_not_text() {
    let a = highlight_at(10);
    let mut b = a.clone();
    b.title = format!("  {}  ", a.title.to_uppercase());
    assert_eq!(entry_id(&a), entry_id(&b));
    assert_eq!(entry_id(&a).len(), 40);
    b.text.push('!');
    assert_ne!(entry_id(&a), entry_id(&b));
}

#[test]
fn renders_the_golden_file() {
    let books = group(parse(&read("variants.txt")).entries);
    let walden = books.iter().find(|b| b.title == "Walden").unwrap();
    assert_eq!(
        render_new_file(walden),
        String::from_utf8(read("variants.walden.md")).unwrap()
    );
}

#[test]
fn unrecognised_entries_keep_their_raw_line() {
    let books = group(parse(&read("variants.txt")).entries);
    let q = books.iter().find(|b| b.title == "Don Quijote").unwrap();
    assert!(render_new_file(q).contains("- Tu subrayado en la página 12 | posición 100-102"));
}

#[test]
fn multi_paragraph_highlight_stays_one_blockquote() {
    let mut h = highlight_at(10);
    h.text = "one\n\ntwo".into();
    let b = Book {
        key: "k".into(),
        title: "T".into(),
        author: "".into(),
        editions: false,
        items: vec![Item::Highlight {
            entry: h,
            notes: vec![],
        }],
    };
    assert!(render_items(&b, &b.items.iter().collect::<Vec<_>>()).contains("> one\n>\n> two\n"));
}

#[test]
fn standalone_notes_quote_their_passage_when_known() {
    let mut n = highlight_at(10);
    n.kind = Kind::Note;
    n.location = Some((10, 10));
    n.text = "read again".into();
    let b = Book {
        key: "k".into(),
        title: "T".into(),
        author: "".into(),
        editions: false,
        items: vec![
            Item::Note {
                entry: n.clone(),
                of: Some("I went to the woods because I wished to live deliberately".into()),
            },
            Item::Note { entry: n, of: None },
        ],
    };
    let md = render_items(&b, &b.items.iter().collect::<Vec<_>>());
    assert!(
        md.contains(
            "- Note on “I went to the woods because I wished…” (location 10): read again\n"
        )
    );
    assert!(md.contains("- Note (location 10): read again\n"));
}

#[test]
fn titles_with_colons_are_quoted_in_frontmatter() {
    let mut h = highlight_at(10);
    h.title = "Dune: Messiah".into();
    let books = group(vec![h]);
    assert!(render_new_file(&books[0]).contains("title: \"Dune: Messiah\"\n"));
}

#[test]
fn twenty_thousand_entries_group_quickly() {
    let s: String = (0..20_000)
        .map(|i| {
            format!(
                "Book {} (A)\n- Your Highlight on page {i} | Location {i}-{} | Added on Monday, March 3, 2025 10:14:22 AM\n\npassage {i}\n==========\n",
                i % 300,
                i + 1
            )
        })
        .collect();
    let t = std::time::Instant::now();
    let books = group(parse(s.as_bytes()).entries);
    assert_eq!(books.len(), 300);
    assert!(t.elapsed() < std::time::Duration::from_secs(3));
}
