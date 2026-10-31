// SPDX-License-Identifier: GPL-3.0-or-later
//! KOReader sidecars (docs/superpowers/specs/2026-10-30-koreader-reads-design.md).
use std::path::Path;

use deckle_core::koreader::{find, merge, parse, read};

/// A sidecar as KOReader 2024 writes one: `annotations`, a note, and a bookmark.
pub const NEW: &str = r#"-- we can read Lua syntax here!
return {
    ["annotations"] = {
        [1] = {
            ["chapter"] = "Economy",
            ["datetime"] = "2026-10-21 08:15:03",
            ["drawer"] = "lighten",
            ["page"] = "/body/DocFragment[3]/body/p[5]/text().0",
            ["pageno"] = 12,
            ["pos0"] = "/body/DocFragment[3]/body/p[5]/text().0",
            ["pos1"] = "/body/DocFragment[3]/body/p[5]/text().40",
            ["text"] = "The mass of men lead lives of quiet desperation.",
        },
        [2] = {
            ["datetime"] = "2026-10-22 21:40:00",
            ["page"] = "/body/DocFragment[4]/body/p[1]/text().0",
            ["pageno"] = 30,
            ["text"] = "in Where I Lived",
        },
        [3] = {
            ["datetime"] = "2026-10-20 07:00:00",
            ["drawer"] = "underscore",
            ["note"] = "Said \"simplify\" twice\
on purpose?",
            ["pageno"] = 9,
            ["pos0"] = "x",
            ["pos1"] = "y",
            ["text"] = "Simplify, simplify.",
        },
    },
    ["doc_props"] = {
        ["authors"] = "Henry David Thoreau",
        ["language"] = "en",
        ["title"] = "Walden",
    },
    ["percent_finished"] = 0.4215,
    ["summary"] = {
        ["modified"] = "2026-10-22",
        ["status"] = "reading",
    },
}
"#;

/// A sidecar from KOReader before 2024: highlights by page.
pub const OLD: &str = r#"return {
    ["highlight"] = {
        [7] = {
            [1] = {
                ["datetime"] = "2025-03-03 10:14:22",
                ["drawer"] = "lighten",
                ["text"] = "It was the best of times",
            },
        },
    },
    ["doc_props"] = {
        ["authors"] = "Charles Dickens\nAnother Hand",
        ["title"] = "A Tale of Two Cities",
    },
    ["percent_finished"] = 1,
    ["summary"] = { ["status"] = "complete" },
}
"#;

fn put(path: &Path, text: &str) {
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(path, text).unwrap();
}

#[test]
fn a_new_sidecar_gives_highlights_notes_and_progress() {
    let p = parse(NEW).unwrap();
    assert_eq!(p.title.as_deref(), Some("Walden"));
    assert_eq!(p.authors, ["Henry David Thoreau"]);
    assert!(p.in_progress());
    // the bookmark (no pos0, no drawer) isn't a highlight; oldest first
    assert_eq!(p.highlights.len(), 2);
    assert_eq!(p.highlights[0].text, "Simplify, simplify.");
    assert_eq!(p.highlights[0].note, "Said \"simplify\" twice\non purpose?");
    assert_eq!(p.highlights[0].page.as_deref(), Some("9"));
    assert_eq!(p.highlights[1].page.as_deref(), Some("12"));
}

#[test]
fn an_old_sidecar_gives_highlights_by_page_and_a_finished_book_isnt_in_progress() {
    let p = parse(OLD).unwrap();
    assert_eq!(p.authors, ["Charles Dickens", "Another Hand"]);
    assert_eq!(p.highlights.len(), 1);
    assert_eq!(p.highlights[0].page.as_deref(), Some("7"));
    assert!(!p.in_progress());
}

#[test]
fn sidecars_are_found_beside_books_in_docsettings_and_by_hash() {
    let tmp = tempfile::tempdir().unwrap();
    let root = tmp.path();
    put(&root.join("Deckle/Walden.sdr/metadata.epub.lua"), NEW);
    put(
        &root.join(".adds/koreader/docsettings/mnt/onboard/Books/Tale.sdr/metadata.epub.lua"),
        OLD,
    );
    put(
        &root.join(".adds/koreader/hashdocsettings/1a/1a2b3c.sdr/metadata.pdf.lua"),
        OLD,
    );
    // a Kindle's own .sdr (no metadata.*.lua) and KOReader's backup are not sidecars
    put(&root.join("documents/Book.sdr/Book.yjf"), "x");
    put(&root.join("Other/Old.sdr/metadata.epub.lua.old"), NEW);
    put(&root.join(".kobo/Hidden.sdr/metadata.epub.lua"), NEW);
    let mut books: Vec<String> = find(root).into_iter().map(|s| s.book).collect();
    books.sort();
    assert_eq!(
        books,
        ["Books/Tale.epub", "Deckle/Walden.epub", "koreader:1a2b3c"]
    );
}

#[test]
fn a_read_writes_clippings_oldest_first_and_lists_books_in_progress() {
    let tmp = tempfile::tempdir().unwrap();
    put(&tmp.path().join("Deckle/Walden.sdr/metadata.epub.lua"), NEW);
    put(&tmp.path().join("Tale.sdr/metadata.epub.lua"), OLD);
    put(
        &tmp.path().join("Broken.sdr/metadata.epub.lua"),
        "return { [\"a\" = }",
    );
    let r = read(tmp.path());
    assert_eq!(r.warnings.len(), 1, "{:?}", r.warnings);
    assert!(r.warnings[0].contains("Broken.sdr"));
    let text = String::from_utf8(r.clippings.unwrap()).unwrap();
    let tale = text
        .find("A Tale of Two Cities (Charles Dickens, Another Hand)")
        .unwrap();
    let walden = text.find("Walden (Henry David Thoreau)").unwrap();
    assert!(tale < walden, "2025 before 2026:\n{text}");
    assert!(text.contains(
        "- Your Highlight on page 7 | Added on Monday, March 3, 2025 10:14:22 AM\n\nIt was the best of times\n=========="
    ), "{text}");
    assert!(text.contains("- Your Note on page 9 | Added on"), "{text}");
    assert_eq!(r.books.len(), 1);
    assert_eq!(r.books[0].content_id, "Deckle/Walden.epub");
    assert_eq!(r.books[0].title, "Walden");
    assert!((r.books[0].percent - 42.15).abs() < 1e-9);
    assert!(
        r.books[0]
            .last_read
            .as_deref()
            .is_some_and(|t| t.ends_with('Z'))
    );
}

#[test]
fn merging_keeps_a_book_once_from_the_later_read() {
    use deckle_core::kobo::{Book, Read};
    let book = |id: &str, pct: f64, when: &str| Book {
        content_id: id.into(),
        title: "Walden".into(),
        authors: Vec::new(),
        percent: pct,
        last_read: Some(when.into()),
    };
    let kobo = Read {
        books: vec![book(
            "file:///mnt/onboard/Deckle/Walden.epub",
            10.0,
            "2026-10-01T00:00:00Z",
        )],
        clippings: Some(b"a".to_vec()),
        records: Vec::new(),
    };
    let ko = deckle_core::koreader::Read {
        books: vec![
            book("Deckle/Walden.epub", 40.0, "2026-10-20T00:00:00Z"),
            book("Other.epub", 5.0, "2026-09-01T00:00:00Z"),
        ],
        clippings: Some(b"b".to_vec()),
        warnings: Vec::new(),
        positions: Vec::new(),
        records: Vec::new(),
    };
    let m = merge(kobo, ko);
    assert_eq!(m.clippings.as_deref(), Some(&b"ab"[..]));
    assert_eq!(m.books.len(), 2);
    assert_eq!(m.books[0].percent, 40.0);
}

#[test]
fn a_read_gives_each_books_position_but_not_one_kept_by_hash() {
    let tmp = tempfile::tempdir().unwrap();
    let paper = NEW.replace(
        "[\"percent_finished\"]",
        "[\"last_xpointer\"] = \"/body/DocFragment[3]/body/div[2]/p[4]/text().17\",\n    [\"percent_finished\"]",
    );
    put(
        &tmp.path()
            .join("Deckle/deckle-daily-2026-10-30.kepub.sdr/metadata.epub.lua"),
        &paper,
    );
    put(
        &tmp.path()
            .join(".adds/koreader/hashdocsettings/1a/1a2b3c.sdr/metadata.epub.lua"),
        &paper,
    );
    let r = read(tmp.path());
    assert_eq!(r.positions.len(), 1, "{:?}", r.positions);
    let p = &r.positions[0];
    assert_eq!(p.book, "Deckle/deckle-daily-2026-10-30.kepub.epub");
    assert_eq!(
        p.xpointer.as_deref(),
        Some("/body/DocFragment[3]/body/div[2]/p[4]/text().17")
    );
    assert_eq!(p.percent, Some(0.4215));
    assert!(p.when.ends_with('Z'));
}

/// An EPUB whose OPF carries calibre's id.
fn calibre_epub(path: &Path, uuid: &str) {
    use std::io::Write;
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    let mut z = zip::ZipWriter::new(std::fs::File::create(path).unwrap());
    let o = zip::write::SimpleFileOptions::default();
    z.start_file("mimetype", o).unwrap();
    z.write_all(b"application/epub+zip").unwrap();
    z.start_file("META-INF/container.xml", o).unwrap();
    z.write_all(br#"<?xml version="1.0"?><container version="1.0" xmlns="urn:oasis:names:tc:opendocument:xmlns:container"><rootfiles><rootfile full-path="c.opf" media-type="application/oebps-package+xml"/></rootfiles></container>"#).unwrap();
    z.start_file("c.opf", o).unwrap();
    write!(z, r#"<?xml version="1.0"?><package xmlns="http://www.idpf.org/2007/opf" xmlns:opf="http://www.idpf.org/2007/opf" version="2.0" unique-identifier="i"><metadata xmlns:dc="http://purl.org/dc/elements/1.1/"><dc:title>T</dc:title><dc:language>en</dc:language><dc:identifier id="i">x</dc:identifier><dc:identifier opf:scheme="calibre">{uuid}</dc:identifier></metadata><manifest><item id="a" href="a.xhtml" media-type="application/xhtml+xml"/></manifest><spine><itemref idref="a"/></spine></package>"#).unwrap();
    z.start_file("a.xhtml", o).unwrap();
    z.write_all(b"<html xmlns=\"http://www.w3.org/1999/xhtml\"><body><p>x</p></body></html>")
        .unwrap();
    z.finish().unwrap();
}

#[test]
fn a_read_records_each_book_with_its_calibre_id_finished_and_stars() {
    let tmp = tempfile::tempdir().unwrap();
    let root = tmp.path();
    calibre_epub(&root.join("Books/Two Cities.epub"), "c-two-cities");
    let finished = OLD.replace(
        r#"["summary"] = { ["status"] = "complete" },"#,
        r#"["summary"] = { ["status"] = "complete", ["rating"] = 4 },"#,
    );
    put(
        &root.join("Books/Two Cities.sdr/metadata.epub.lua"),
        &finished,
    );
    put(&root.join("Books/Walden.sdr/metadata.epub.lua"), NEW);
    let r = deckle_core::koreader::read(root);
    assert_eq!(r.records.len(), 2);
    let two = r
        .records
        .iter()
        .find(|x| x.title == "A Tale of Two Cities")
        .unwrap();
    assert_eq!(two.calibre_id.as_deref(), Some("c-two-cities"));
    assert_eq!(two.path.as_deref(), Some("Books/Two Cities.epub"));
    assert!(two.finished);
    assert_eq!((two.percent, two.rating), (Some(100.0), Some(4)));
    assert_eq!(two.from, "koreader");
    let walden = r.records.iter().find(|x| x.title == "Walden").unwrap();
    assert!(!walden.finished && walden.calibre_id.is_none() && walden.rating.is_none());
    assert!(walden.percent.unwrap() > 0.0);
    // A finished book still isn't a book in progress.
    assert!(r.books.iter().all(|b| b.title != "A Tale of Two Cities"));
}

#[test]
fn a_records_highlights_carry_their_chapter_and_spine_file() {
    let tmp = tempfile::tempdir().unwrap();
    put(&tmp.path().join("Walden.sdr/metadata.epub.lua"), NEW);
    let r = read(tmp.path());
    let h = &r.records[0].highlights;
    // The bookmark (no pos0, no drawer) isn't one; oldest first.
    assert_eq!(h.len(), 2);
    assert_eq!(h[0].text, "Simplify, simplify.");
    assert_eq!(h[0].note, "Said \"simplify\" twice\non purpose?");
    assert_eq!((h[0].spine, h[0].chapter.as_deref()), (None, None));
    assert_eq!(h[1].chapter.as_deref(), Some("Economy"));
    assert_eq!(
        h[1].spine,
        Some(2),
        "DocFragment[3] is the third spine file"
    );
    assert_eq!(h[1].page.as_deref(), Some("12"));
    assert_eq!(h[1].datetime, "2026-10-21 08:15:03");
}
