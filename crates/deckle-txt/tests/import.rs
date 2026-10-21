// SPDX-License-Identifier: GPL-3.0-or-later
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use deckle_txt::{ImportReport, TxtError, convert, is_text, to_epub};

fn fixture(name: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../fixtures/txt")
        .join(name)
}

type Files = BTreeMap<String, Vec<u8>>;

fn import_bytes(bytes: &[u8], stem: &str) -> (Files, ImportReport) {
    let (files, report) = to_epub(bytes, stem).unwrap();
    assert_eq!(files[0].0, "mimetype");
    (files.into_iter().collect(), report)
}

fn import(name: &str) -> (Files, ImportReport) {
    let bytes = std::fs::read(fixture(name)).unwrap();
    let stem = Path::new(name).file_stem().unwrap().to_str().unwrap();
    import_bytes(&bytes, stem)
}

fn text(files: &Files, name: &str) -> String {
    String::from_utf8(files[name].clone()).unwrap()
}

/// Chapter files in spine order.
fn chapter_names(files: &Files) -> Vec<String> {
    files
        .keys()
        .filter(|k| k.starts_with("OEBPS/chapter-"))
        .cloned()
        .collect()
}

/// All chapter XHTML in spine order, concatenated.
fn chapters(files: &Files) -> String {
    chapter_names(files)
        .iter()
        .map(|k| text(files, k))
        .collect()
}

fn parse(s: &str) -> roxmltree::Document<'_> {
    roxmltree::Document::parse_with_options(
        s,
        roxmltree::ParsingOptions {
            allow_dtd: true,
            ..Default::default()
        },
    )
    .unwrap_or_else(|e| panic!("{e}\n{s}"))
}

/// Contents entries as (depth, title, href); depth 1 is the top level.
fn contents(files: &Files) -> Vec<(usize, String, String)> {
    let nav = text(files, "OEBPS/nav.xhtml");
    let doc = parse(&nav);
    doc.descendants()
        .filter(|n| n.has_tag_name("a"))
        .map(|a| {
            let depth = a.ancestors().filter(|n| n.has_tag_name("ol")).count();
            (
                depth,
                a.text().unwrap_or("").to_owned(),
                a.attribute("href").unwrap().to_owned(),
            )
        })
        .collect()
}

#[test]
fn only_txt_is_text() {
    assert!(is_text(Path::new("a/book.txt")));
    assert!(is_text(Path::new("BOOK.TXT")));
    assert!(!is_text(Path::new("book.md")));
    assert!(!is_text(Path::new("book.epub")));
    assert!(!is_text(Path::new("txt")));
}

#[test]
fn every_file_is_well_formed_xml() {
    for name in [
        "gutenberg-mini.txt",
        "unwrapped.txt",
        "verse.txt",
        "utf16.txt",
        "noheadings.txt",
    ] {
        let (files, _) = import(name);
        for (path, bytes) in &files {
            if path.ends_with(".xhtml") || path.ends_with(".opf") || path.ends_with(".xml") {
                parse(std::str::from_utf8(bytes).unwrap());
            }
        }
    }
}

#[test]
fn gutenberg_contents_nest_chapters_under_parts_in_order() {
    let (files, report) = import("gutenberg-mini.txt");
    let entries = contents(&files);
    let shape: Vec<(usize, &str)> = entries.iter().map(|(d, t, _)| (*d, t.as_str())).collect();
    assert_eq!(
        shape,
        [
            (1, "PART ONE"),
            (2, "CHAPTER I."),
            (2, "Chapter II — The Ink"),
            (1, "PART TWO"),
            (2, "III."),
            (2, "Chapter 4 — THE LAST PROOF"),
        ]
    );
    assert_eq!(report.contents, 6);
    // Front matter, then one file per heading.
    assert_eq!(report.chapters, 7);
    assert_eq!(chapter_names(&files).len(), 7);
    assert_eq!(entries[0].2, "chapter-002.xhtml#h1");
    assert_eq!(entries[1].2, "chapter-003.xhtml#h2");
    assert_eq!(entries[5].2, "chapter-007.xhtml#h6");
    let c3 = text(&files, "OEBPS/chapter-003.xhtml");
    assert!(c3.contains("<h2 id=\"h2\">CHAPTER I.</h2>"), "{c3}");
    let p1 = text(&files, "OEBPS/chapter-002.xhtml");
    assert!(p1.contains("<h1 id=\"h1\">PART ONE</h1>"), "{p1}");
    let c7 = text(&files, "OEBPS/chapter-007.xhtml");
    assert!(
        c7.contains("Chapter 4<br/><span class=\"subtitle\">THE LAST PROOF</span>"),
        "{c7}"
    );
    // The all-caps line went into the heading, not into the previous chapter.
    let c6 = text(&files, "OEBPS/chapter-006.xhtml");
    assert!(!c6.contains("THE LAST PROOF"), "{c6}");
}

#[test]
fn gutenberg_licence_is_stripped_and_header_sets_metadata() {
    let (files, report) = import("gutenberg-mini.txt");
    let all = chapters(&files);
    assert!(!all.contains("SENTINEL"), "licence text leaked");
    assert!(!all.contains("PROJECT GUTENBERG"), "markers leaked");
    assert!(all.contains("THE DECKLE MINI NOVEL"));
    assert_eq!(report.title, "The Deckle Mini Novel");
    let opf = text(&files, "OEBPS/content.opf");
    assert!(
        opf.contains("<dc:title>The Deckle Mini Novel</dc:title>"),
        "{opf}"
    );
    assert!(
        opf.contains("<dc:creator>Ada Testwright</dc:creator>"),
        "{opf}"
    );
    assert!(opf.contains("<dc:language>en</dc:language>"), "{opf}");
    assert!(opf.contains("urn:deckle:txt:"), "{opf}");
    assert!(report.warnings.is_empty(), "{:?}", report.warnings);
}

#[test]
fn identifier_is_stable_for_the_same_title_and_author() {
    let (a, _) = import("gutenberg-mini.txt");
    let (b, _) = import("gutenberg-mini.txt");
    let id = |f: &Files| {
        let opf = text(f, "OEBPS/content.opf");
        let start = opf.find("urn:deckle:txt:").unwrap();
        opf[start..start + 31].to_owned()
    };
    assert_eq!(id(&a), id(&b));
    let (c, _) = import_bytes(b"Another book entirely.\n", "other");
    assert_ne!(id(&a), id(&c));
}

#[test]
fn hard_wrapped_lines_join_into_paragraphs_and_keep_hyphens() {
    let (files, _) = import("gutenberg-mini.txt");
    let all = chapters(&files);
    assert!(
        all.contains("<p>The press was cold that morning, and the type sat in its cases like rain waiting to fall."),
        "lines were not joined"
    );
    assert!(all.contains("a well-known habit"), "hyphenated line end");
    assert!(!all.contains("well- known"));
    // A short-line block in a hard-wrapped book keeps its breaks.
    assert!(all.contains("Yours, in haste,<br/>A. T."), "{all}");
}

#[test]
fn scene_breaks_become_rules() {
    let (files, _) = import("gutenberg-mini.txt");
    let c4 = text(&files, "OEBPS/chapter-004.xhtml");
    assert!(c4.contains("<hr class=\"scene\"/>"), "{c4}");
    assert!(!c4.contains("* * *"));
}

#[test]
fn one_paragraph_per_line_and_windows_1252_with_a_warning() {
    let (files, report) = import("unwrapped.txt");
    let all = chapters(&files);
    assert_eq!(all.matches("<p>").count(), 4, "{all}");
    assert!(all.contains("<p>“It’s late,” said the foreman"), "{all}");
    assert!(
        report.warnings.iter().any(|w| w.contains("Windows-1252")),
        "{:?}",
        report.warnings
    );
    assert_eq!(report.title, "unwrapped");
    let opf = text(&files, "OEBPS/content.opf");
    assert!(!opf.contains("dc:creator"));
    // No headings: one contents entry, the title.
    let entries = contents(&files);
    assert_eq!(entries.len(), 1);
    assert_eq!(entries[0].1, "unwrapped");
}

#[test]
fn verse_keeps_its_line_breaks() {
    let (files, report) = import("verse.txt");
    let all = chapters(&files);
    assert!(
        all.contains(
            "The press is cold, the type is set,<br/>the ink has not been opened yet;<br/>"
        ),
        "{all}"
    );
    assert_eq!(all.matches("<p class=\"verse\">").count(), 2, "{all}");
    assert!(report.warnings.is_empty(), "{:?}", report.warnings);
}

#[test]
fn utf16_with_a_bom_is_decoded() {
    let (files, report) = import("utf16.txt");
    let all = chapters(&files);
    assert!(all.contains("Café Notes"), "{all}");
    assert!(all.contains("The naïve compositor set “façade”"), "{all}");
    assert!(report.warnings.is_empty(), "{:?}", report.warnings);
}

#[test]
fn a_long_book_without_headings_is_split_at_paragraphs() {
    let (files, report) = import("noheadings.txt");
    let names = chapter_names(&files);
    assert!(names.len() >= 3, "{names:?}");
    assert_eq!(report.chapters, names.len());
    for n in &names {
        let html = text(&files, n);
        assert!(html.len() < 260_000, "{n} is {} bytes", html.len());
        assert!(html.contains("<p>Paragraph "), "{n}");
    }
    let entries = contents(&files);
    assert_eq!(entries.len(), 1);
    assert_eq!(entries[0].1, "noheadings");
    assert_eq!(entries[0].2, "chapter-001.xhtml");
}

#[test]
fn headings_need_blank_lines_and_a_pattern() {
    let body = "Opening words of the story, which are plain prose and nothing else.\n\n\
                Chapter of accidents, said the doctor.\n\n\
                MCMXCIX\n\n\
                Introduction to the problem\n\n\
                Chapter One: Beginnings\n\n\
                Body text after the real heading, long enough to be prose.\n\n\
                Epilogue\n\n\
                The end of it.\n";
    let (files, _) = import_bytes(body.as_bytes(), "h");
    let titles: Vec<String> = contents(&files).into_iter().map(|(_, t, _)| t).collect();
    assert_eq!(titles, ["Chapter One: Beginnings", "Epilogue"]);
}

#[test]
fn long_unwrapped_lines_in_blocks_are_separate_paragraphs() {
    let long = "This is a single long paragraph written on one line, which goes on well past eighty characters before it stops.";
    let body = format!("{long}\n{long}\n\n{long}\n{long}\n");
    let (files, _) = import_bytes(body.as_bytes(), "x");
    let all = chapters(&files);
    assert_eq!(all.matches("<p>").count(), 4, "{all}");
    assert!(!all.contains("<br/>"));
}

#[test]
fn markup_in_text_is_escaped() {
    let (files, _) = import_bytes(b"Fish & <chips> \"now\".\n", "esc");
    let all = chapters(&files);
    assert!(
        all.contains("Fish &amp; &lt;chips&gt; &quot;now&quot;."),
        "{all}"
    );
}

#[test]
fn refusals() {
    assert!(matches!(to_epub(b"", "e"), Err(TxtError::Empty)));
    assert!(matches!(to_epub(b" \r\n\n\t\n", "e"), Err(TxtError::Empty)));
    let mut binary = b"Some text that then turns into ".to_vec();
    binary.extend(std::iter::repeat_n(0u8, 200));
    binary.extend(b"binary data.");
    assert!(matches!(to_epub(&binary, "b"), Err(TxtError::Binary)));

    let dir = tempfile::tempdir().unwrap();
    let big = dir.path().join("big.txt");
    let f = std::fs::File::create(&big).unwrap();
    f.set_len(51 * 1024 * 1024).unwrap();
    drop(f);
    let err = convert(&big, &dir.path().join("big.epub")).unwrap_err();
    assert!(matches!(err, TxtError::TooLarge(_)), "{err}");
    assert!(err.to_string().contains("50 MB"), "{err}");
}

#[test]
fn convert_writes_an_epub_zip_with_mimetype_first() {
    let dir = tempfile::tempdir().unwrap();
    let out = dir.path().join("out.epub");
    let report = convert(&fixture("gutenberg-mini.txt"), &out).unwrap();
    assert_eq!(report.title, "The Deckle Mini Novel");
    let mut zip = zip::ZipArchive::new(std::fs::File::open(&out).unwrap()).unwrap();
    let first = zip.by_index(0).unwrap();
    assert_eq!(first.name().unwrap(), "mimetype");
    assert_eq!(first.compression(), zip::CompressionMethod::Stored);
}
