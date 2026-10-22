// SPDX-License-Identifier: GPL-3.0-or-later
use std::fs;
use std::path::Path;

use deckle_highlights::{Added, Book, Entry, Kind, State, export, file_name, group, pending};

fn entry(title: &str, author: &str, spec: &str) -> Entry {
    let mut it = spec.splitn(3, ':');
    let kind = match it.next().unwrap() {
        "h" => Kind::Highlight,
        _ => Kind::Note,
    };
    let loc: u32 = it.next().unwrap().parse().unwrap();
    Entry {
        title: title.into(),
        author: author.into(),
        kind,
        page: None,
        location: Some(match kind {
            Kind::Highlight => (loc, loc + 2),
            _ => (loc, loc),
        }),
        added: Added::default(),
        text: it.next().unwrap().into(),
        raw_meta: None,
    }
}

/// Walden entries from `kind:location:text` specs.
fn books(specs: &[&str]) -> Vec<Book> {
    group(
        specs
            .iter()
            .map(|s| entry("Walden", "Henry David Thoreau", s))
            .collect(),
    )
}

#[test]
fn first_export_creates_then_appends_only_new() {
    let dir = tempfile::tempdir().unwrap();
    let mut st = State::default();
    let r = export(&books(&["h:10:one"]), dir.path(), &mut st);
    assert!(r.failed.is_empty(), "{:?}", r.failed);
    assert_eq!((r.written[0].new, r.written[0].created), (1, true));
    let path = dir.path().join("Walden.md");
    let mut text = fs::read_to_string(&path).unwrap();
    text.push_str("\nmy own thoughts");
    fs::write(&path, &text).unwrap();
    let r = export(&books(&["h:10:one", "h:20:two"]), dir.path(), &mut st);
    assert_eq!((r.written[0].new, r.written[0].created), (1, false));
    let after = fs::read_to_string(&path).unwrap();
    assert!(after.starts_with(&text), "existing bytes untouched");
    assert!(after[text.len()..].starts_with("\n> two"), "{after}");
    assert_eq!(after.matches("> two").count(), 1);
    assert!(
        export(&books(&["h:10:one", "h:20:two"]), dir.path(), &mut st)
            .written
            .is_empty()
    );
}

#[test]
fn a_second_kindle_with_the_same_highlight_adds_nothing() {
    let dir = tempfile::tempdir().unwrap();
    let mut st = State::default();
    export(&books(&["h:10:one"]), dir.path(), &mut st);
    let again = group(vec![entry("  WALDEN ", "Henry David Thoreau", "h:10:one")]);
    assert!(export(&again, dir.path(), &mut st).written.is_empty());
}

#[test]
fn a_note_added_later_is_appended_with_its_passage() {
    let dir = tempfile::tempdir().unwrap();
    let mut st = State::default();
    let h = "h:10:I went to the woods because I wished to live deliberately";
    export(&books(&[h]), dir.path(), &mut st);
    let r = export(&books(&[h, "n:10:read again"]), dir.path(), &mut st);
    assert_eq!(r.written[0].new, 1);
    let text = fs::read_to_string(dir.path().join("Walden.md")).unwrap();
    assert!(
        text.contains(
            "- Note on “I went to the woods because I wished…” (location 10): read again"
        )
    );
    assert_eq!(
        text.matches("> I went").count(),
        1,
        "the passage isn't repeated"
    );
}

#[test]
fn a_deleted_file_is_written_fresh() {
    let dir = tempfile::tempdir().unwrap();
    let mut st = State::default();
    export(&books(&["h:10:one"]), dir.path(), &mut st);
    fs::remove_file(dir.path().join("Walden.md")).unwrap();
    let r = export(&books(&["h:10:one"]), dir.path(), &mut st);
    assert_eq!((r.written[0].new, r.written[0].created), (1, true));
    assert!(
        fs::read_to_string(dir.path().join("Walden.md"))
            .unwrap()
            .contains("> one")
    );
}

#[test]
fn colliding_titles_get_the_author() {
    let dir = tempfile::tempdir().unwrap();
    let mut st = State::default();
    let two = group(vec![
        entry("Walden", "Henry David Thoreau", "h:10:one"),
        entry("Walden", "Someone Else", "h:10:two"),
    ]);
    export(&two, dir.path(), &mut st);
    assert!(dir.path().join("Walden.md").exists());
    assert!(dir.path().join("Walden (Someone Else).md").exists());

    let dir = tempfile::tempdir().unwrap();
    fs::write(dir.path().join("Walden.md"), "the user's own note").unwrap();
    let mut st = State::default();
    export(&books(&["h:10:one"]), dir.path(), &mut st);
    assert_eq!(
        fs::read_to_string(dir.path().join("Walden.md")).unwrap(),
        "the user's own note"
    );
    assert!(dir.path().join("Walden (Henry David Thoreau).md").exists());
}

#[test]
fn editions_keep_their_file_after_the_paper_is_renamed() {
    let dir = tempfile::tempdir().unwrap();
    let mut st = State::default();
    let ed = |t: &str| group(vec![entry(t, "Deckle", "h:10:story")]);
    export(
        &ed("The Morning Deckle — Tuesday, October 20, 2026"),
        dir.path(),
        &mut st,
    );
    let r = export(
        &ed("The Lakeside Gazette — Wednesday, October 21, 2026"),
        dir.path(),
        &mut st,
    );
    assert_eq!(r.written[0].file, "The Morning Deckle.md");
    let text = fs::read_to_string(dir.path().join("The Morning Deckle.md")).unwrap();
    assert!(text.contains("## Tuesday, October 20, 2026"));
    assert!(text.contains("## Wednesday, October 21, 2026"));
    assert!(!dir.path().join("The Lakeside Gazette.md").exists());
}

#[test]
fn a_failed_write_leaves_state_untouched() {
    let mut st = State::default();
    let r = export(
        &books(&["h:10:one"]),
        Path::new("/nonexistent/deckle-test"),
        &mut st,
    );
    assert_eq!(r.failed.len(), 1);
    assert!(
        r.failed[0].1.contains("/nonexistent/deckle-test"),
        "{:?}",
        r.failed
    );
    assert!(st.books.is_empty());
}

#[test]
fn file_names_are_safe() {
    assert_eq!(file_name("A/B: C?"), "A-B- C-.md");
    assert!(file_name(&"x".repeat(300)).len() <= 123);
    assert_eq!(file_name("  .hidden. "), "hidden.md");
    assert_eq!(file_name("   "), "Untitled.md");
    assert!(file_name(&"é".repeat(100)).ends_with(".md"));
}

#[test]
fn pending_matches_what_export_writes() {
    let dir = tempfile::tempdir().unwrap();
    let mut st = State::default();
    let first = books(&["h:10:one"]);
    let p = pending(&first, Some(dir.path()), &st);
    assert_eq!(
        (p[0].total, p[0].new, p[0].file.as_str()),
        (1, 1, "Walden.md")
    );
    export(&first, dir.path(), &mut st);
    assert_eq!(
        fs::read_to_string(dir.path().join("Walden.md")).unwrap(),
        p[0].markdown
    );

    let next = books(&["h:10:one", "h:20:two"]);
    let p = pending(&next, Some(dir.path()), &st);
    assert_eq!((p[0].total, p[0].new), (2, 1));
    let before = fs::read_to_string(dir.path().join("Walden.md")).unwrap();
    export(&next, dir.path(), &mut st);
    let after = fs::read_to_string(dir.path().join("Walden.md")).unwrap();
    assert_eq!(&after[before.len()..], p[0].markdown);
}

#[test]
fn state_round_trips_and_tolerates_a_bad_file() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("highlights.json");
    let mut st = State::default();
    st.folder = Some(dir.path().to_owned());
    export(&books(&["h:10:one"]), dir.path(), &mut st);
    st.save(&path).unwrap();
    let back = State::load(&path);
    assert_eq!(back.folder.as_deref(), Some(dir.path()));
    assert_eq!(back.books.len(), 1);
    fs::write(&path, "{not json").unwrap();
    assert!(State::load(&path).books.is_empty());
    assert!(
        State::load(&dir.path().join("missing.json"))
            .folder
            .is_none()
    );
}

#[test]
fn pending_without_a_folder_looks_at_no_disk() {
    let cwd_has_it = std::env::current_dir().unwrap().join("Walden.md");
    assert!(!cwd_has_it.exists());
    let st = State::default();
    let p = pending(&books(&["h:10:one"]), None, &st);
    assert_eq!((p[0].file.as_str(), p[0].new), ("Walden.md", 1));
}

#[test]
fn export_clippings_remembers_the_folder() {
    let tmp = tempfile::tempdir().unwrap();
    let state = tmp.path().join("cfg/highlights.json");
    let bytes = b"Walden (H)\n- Your Highlight on page 1 | Location 1-2 | Added on Monday, March 3, 2025 10:14:22 AM\n\none\n==========\n";
    assert!(
        deckle_highlights::export_clippings(bytes, &state, None)
            .unwrap_err()
            .contains("--to")
    );
    let out = tmp.path().join("notes");
    let o = deckle_highlights::export_clippings(bytes, &state, Some(&out)).unwrap();
    assert_eq!(o.report.written.len(), 1);
    assert!(o.describe().starts_with("highlights: 1 new in 1 file"));
    let o = deckle_highlights::export_clippings(bytes, &state, None).unwrap();
    assert!(o.report.written.is_empty());
}
