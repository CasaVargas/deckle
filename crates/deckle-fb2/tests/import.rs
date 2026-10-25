// SPDX-License-Identifier: GPL-3.0-or-later
use std::collections::BTreeMap;
use std::io::Write;
use std::path::{Path, PathBuf};

use deckle_fb2::{Fb2Error, ImportReport, convert, is_fb2, to_epub, unzip};

fn fixture(name: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../fixtures/fb2")
        .join(name)
}

type Files = BTreeMap<String, Vec<u8>>;

fn import_bytes(bytes: &[u8], stem: &str) -> (Files, ImportReport) {
    let (files, report) = to_epub(bytes, stem).unwrap();
    assert_eq!(files[0].0, "mimetype");
    (files.into_iter().collect(), report)
}

fn import(name: &str) -> (Files, ImportReport) {
    import_bytes(&std::fs::read(fixture(name)).unwrap(), "stem")
}

fn text(files: &Files, name: &str) -> String {
    String::from_utf8(files[name].clone()).unwrap()
}

fn parse(s: &str) -> roxmltree::Document<'_> {
    roxmltree::Document::parse(s).unwrap_or_else(|e| panic!("{e}\n{s}"))
}

/// Contents entries as (depth, label, href); depth 1 is the top level.
fn contents(files: &Files) -> Vec<(usize, String, String)> {
    let nav = text(files, "OEBPS/nav.xhtml");
    let doc = parse(&nav);
    doc.descendants()
        .filter(|n| n.has_tag_name("a"))
        .map(|a| {
            let depth = a.ancestors().filter(|n| n.has_tag_name("ol")).count();
            let label: String = a
                .descendants()
                .filter(|t| t.is_text())
                .filter_map(|t| t.text())
                .collect();
            (depth, label, a.attribute("href").unwrap().to_owned())
        })
        .collect()
}

fn mini() -> (Files, ImportReport) {
    import("mini.fb2")
}

#[test]
fn recognises_fb2_and_zipped_fb2() {
    assert!(is_fb2(Path::new("a.fb2")));
    assert!(is_fb2(Path::new("A.FB2")));
    assert!(is_fb2(Path::new("a.fb2.zip")));
    assert!(is_fb2(Path::new("a.FB2.ZIP")));
    assert!(!is_fb2(Path::new("a.zip")));
    assert!(!is_fb2(Path::new("a.fb3")));
    assert!(!is_fb2(Path::new("a.epub")));
}

#[test]
fn every_file_is_well_formed_and_every_link_lands() {
    let (files, _) = mini();
    for (name, bytes) in &files {
        if name.ends_with(".xhtml") || name.ends_with(".opf") || name.ends_with(".xml") {
            parse(std::str::from_utf8(bytes).unwrap());
        }
    }
    let opf = text(&files, "OEBPS/content.opf");
    let opf_doc = parse(&opf);
    for item in opf_doc.descendants().filter(|n| n.has_tag_name("item")) {
        let href = item.attribute("href").unwrap();
        assert!(files.contains_key(&format!("OEBPS/{href}")), "{href}");
    }
    // Every id on every page, then every href and src against them.
    let mut ids = std::collections::HashSet::new();
    for (name, bytes) in files.iter().filter(|(n, _)| n.ends_with(".xhtml")) {
        let s = std::str::from_utf8(bytes).unwrap();
        for n in parse(s).descendants() {
            if let Some(id) = n.attribute("id") {
                assert!(
                    ids.insert(format!("{}#{id}", &name["OEBPS/".len()..])),
                    "duplicate {id} in {name}"
                );
            }
        }
    }
    for (name, bytes) in files.iter().filter(|(n, _)| n.ends_with(".xhtml")) {
        let s = std::str::from_utf8(bytes).unwrap();
        for n in parse(s).descendants() {
            if let Some(src) = n.attribute("src") {
                assert!(files.contains_key(&format!("OEBPS/{src}")), "{name}: {src}");
            }
            let Some(href) = n.attribute("href") else {
                continue;
            };
            if href.starts_with("http") || href == "style.css" {
                continue;
            }
            if href.contains('#') {
                assert!(ids.contains(href), "{name}: {href}");
            } else {
                assert!(
                    files.contains_key(&format!("OEBPS/{href}")),
                    "{name}: {href}"
                );
            }
        }
    }
}

#[test]
fn metadata_comes_from_title_info() {
    let (files, report) = mini();
    assert_eq!(report.title, "The Deckle FB2 Mini");
    let opf = text(&files, "OEBPS/content.opf");
    assert!(opf.contains("<dc:title>The Deckle FB2 Mini</dc:title>"));
    assert!(
        opf.contains(
            "<dc:creator>Ada K. Testwright</dc:creator><dc:creator>Typesetter</dc:creator>"
        )
    );
    assert!(opf.contains("<dc:language>en</dc:language>"));
    assert!(
        opf.contains("<dc:subject>sf_fantasy</dc:subject><dc:subject>prose_classic</dc:subject>")
    );
    assert!(opf.contains("<dc:description>A small book about a print shop.\n\nSecond paragraph &amp; more.</dc:description>"));
    assert!(
        opf.contains(
            "<meta property=\"belongs-to-collection\" id=\"series\">Deckle Fixtures</meta>"
        )
    );
    assert!(opf.contains("<meta refines=\"#series\" property=\"group-position\">2</meta>"));
    assert!(opf.contains("<meta name=\"calibre:series\" content=\"Deckle Fixtures\"/>"));
    assert!(opf.contains("<meta name=\"calibre:series_index\" content=\"2\"/>"));
    assert!(opf.contains("urn:deckle:fb2:"));
    assert!(opf.contains(
        "<item id=\"img1\" href=\"images/image-001.png\" media-type=\"image/png\" properties=\"cover-image\"/>"
    ));
}

#[test]
fn sections_become_pages_and_nested_contents() {
    let (files, report) = mini();
    let entries: Vec<(usize, String)> = contents(&files)
        .into_iter()
        .map(|(d, l, _)| (d, l))
        .collect();
    assert_eq!(
        entries,
        [
            (1, "Part One".into()),
            (2, "Chapter 1 — The Cold Press".into()),
            (2, "Chapter 2".into()),
            (1, "Part Two".into()),
            (2, "Chapter 3".into()),
            (2, "Chapter 4".into()),
            (1, "Notes".into()),
        ]
    );
    // Title page, Part One, ch 1, ch 2, Part Two, ch 3, ch 4, notes.
    assert_eq!(report.chapters, 8);
    assert_eq!(report.contents, 7);
    let opf = text(&files, "OEBPS/content.opf");
    assert!(opf.contains("<itemref idref=\"c8\"/></spine>"));
    assert!(opf.contains("<item id=\"c8\" href=\"notes.xhtml\""));
    let title = text(&files, "OEBPS/chapter-001.xhtml");
    assert!(title.contains("<h1 class=\"book-title\">The Deckle FB2 Mini<br/>A Novel</h1>"));
    assert!(title.contains("<blockquote class=\"epigraph\"><p>Ink is patient.</p><p class=\"text-author\">Old proverb</p></blockquote>"));
    let ch1 = text(&files, "OEBPS/chapter-003.xhtml");
    assert!(ch1.contains("<title>Chapter 1 — The Cold Press</title>"));
    assert!(ch1.contains("<h2 id=\"ch1\">Chapter 1<br/>The Cold Press</h2>"));
    let part2 = text(&files, "OEBPS/chapter-005.xhtml");
    assert!(part2.contains("<h1 id=\"fb2-h-"), "{part2}");
}

#[test]
fn inline_and_block_markup_is_kept() {
    let (files, _) = mini();
    let ch1 = text(&files, "OEBPS/chapter-003.xhtml");
    assert!(ch1.contains("It was <em>very</em> cold, and <strong>nobody</strong> spoke."));
    assert!(ch1.contains("<p class=\"empty-line\">&#160;</p>"));
    let ch2 = text(&files, "OEBPS/chapter-004.xhtml");
    assert!(ch2.contains("<div class=\"poem\"><p class=\"poem-title\">The Compositor's Song</p>"));
    assert!(ch2.contains(
        "<p class=\"stanza\">Set the line and lock the forme,<br/>Ink the roller, keep it warm.</p>"
    ));
    assert!(ch2.contains("<p class=\"text-author\">Anonymous</p></div>"));
    let ch3 = text(&files, "OEBPS/chapter-006.xhtml");
    assert!(ch3.contains("<p class=\"subtitle\">* * *</p>"));
    assert!(ch3.contains("<tr><th>Face</th><th>Size</th></tr>"));
    assert!(ch3.contains("<td align=\"right\">12</td>"));
    assert!(ch3.contains("<td colspan=\"2\">Garamond, all sizes</td>"));
    assert!(ch3.contains("<blockquote><p>Quoted from the foreman's notebook.</p></blockquote>"));
    assert!(
        ch3.contains("H<sub>2</sub>O and E=mc<sup>2</sup>, <s>struck</s>, <code>lock()</code>.")
    );
}

#[test]
fn notes_become_pop_up_footnotes_with_back_links() {
    let (files, _) = mini();
    let ch1 = text(&files, "OEBPS/chapter-003.xhtml");
    assert!(ch1.contains("<a epub:type=\"noteref\" href=\"notes.xhtml#n1\" id=\"ref-n1\">[1]</a>"));
    // No type="note", but it points into the notes body.
    let ch2 = text(&files, "OEBPS/chapter-004.xhtml");
    assert!(ch2.contains("<a epub:type=\"noteref\" href=\"notes.xhtml#n2\" id=\"ref-n2\">[2]</a>"));
    let notes = text(&files, "OEBPS/notes.xhtml");
    assert!(notes.contains("<aside epub:type=\"footnote\" id=\"n1\"><p class=\"note-title\"><strong>1</strong></p><p>Colder than the previous winter.</p><p class=\"backlink\"><a href=\"chapter-003.xhtml#ref-n1\">↩</a></p></aside>"));
    assert!(notes.contains("href=\"chapter-004.xhtml#ref-n2\""));
}

#[test]
fn links_resolve_to_the_page_their_target_lands_on() {
    let (files, _) = mini();
    let ch1 = text(&files, "OEBPS/chapter-003.xhtml");
    assert!(ch1.contains("<a href=\"chapter-006.xhtml#ch3\">the third chapter</a>"));
    assert!(ch1.contains("<a href=\"https://example.com/press\">the press site</a>"));
}

#[test]
fn images_are_sniffed_packaged_once_and_unused_ones_skipped() {
    let (files, report) = mini();
    assert_eq!(report.images, 3);
    // Labelled image/jpeg in the file, but it's a PNG.
    assert!(files.contains_key("OEBPS/images/image-002.png"));
    assert!(files.contains_key("OEBPS/images/image-003.gif"));
    assert!(!files.keys().any(|k| k.contains("image-004")));
    let ch1 = text(&files, "OEBPS/chapter-003.xhtml");
    assert!(ch1.contains(
        "<div class=\"image\"><img src=\"images/image-002.png\" alt=\"A grey plate\"/></div>"
    ));
    let ch2 = text(&files, "OEBPS/chapter-004.xhtml");
    assert!(ch2.contains("A dot<img src=\"images/image-003.gif\" alt=\"\"/> in the middle"));
    assert!(report.warnings.is_empty(), "{:?}", report.warnings);
}

#[test]
fn zipped_fb2_imports_the_same_book() {
    let (plain, _) = mini();
    let unzipped = unzip(&std::fs::read(fixture("mini.fb2.zip")).unwrap()).unwrap();
    let (zipped, _) = import_bytes(&unzipped, "stem");
    assert_eq!(plain, zipped);
}

#[test]
fn import_is_deterministic() {
    assert_eq!(mini().0, mini().0);
}

#[test]
fn declared_windows_1251_is_decoded() {
    let (files, report) = import("cp1251.fb2");
    assert_eq!(report.title, "Типография");
    assert!(report.warnings.is_empty(), "{:?}", report.warnings);
    let opf = text(&files, "OEBPS/content.opf");
    assert!(opf.contains("<dc:creator>Иван Наборщиков</dc:creator>"));
    assert!(opf.contains("<dc:language>ru</dc:language>"));
    let ch = text(&files, "OEBPS/chapter-001.xhtml");
    assert!(ch.contains("Утро в типографии было холодным."));
    assert!(ch.contains("xml:lang=\"ru\""));
}

#[test]
fn convert_writes_an_epub_named_after_the_zip() {
    let dir = tempfile::tempdir().unwrap();
    let src = dir.path().join("No Title.fb2.zip");
    let body = "<FictionBook><body><section><p>Only text.</p></section></body></FictionBook>";
    let mut zw = zip::ZipWriter::new(std::fs::File::create(&src).unwrap());
    zw.start_file("inner.fb2", zip::write::SimpleFileOptions::default())
        .unwrap();
    zw.write_all(body.as_bytes()).unwrap();
    zw.finish().unwrap();
    let epub = dir.path().join("out.epub");
    let report = convert(&src, &epub).unwrap();
    assert_eq!(report.title, "No Title");
    let mut zip = zip::ZipArchive::new(std::fs::File::open(&epub).unwrap()).unwrap();
    assert_eq!(zip.by_index(0).unwrap().name().unwrap(), "mimetype");
}

fn err(bytes: &[u8]) -> Fb2Error {
    to_epub(bytes, "x").expect_err("an error")
}

#[test]
fn refusals() {
    assert!(matches!(err(b""), Fb2Error::Empty));
    assert!(matches!(err(b"  \n "), Fb2Error::Empty));
    let broken = std::fs::read(fixture("broken.fb2")).unwrap();
    assert!(matches!(err(&broken), Fb2Error::Xml(_)));
    assert!(matches!(err(b"<html><body/></html>"), Fb2Error::NotFb2(n) if n == "html"));
    assert!(matches!(
        err(b"<FictionBook><description/></FictionBook>"),
        Fb2Error::NoBody
    ));
    assert!(matches!(
        err(b"<FictionBook><body><section> </section></body></FictionBook>"),
        Fb2Error::NoBody
    ));
}

#[test]
fn a_zip_must_hold_exactly_one_fb2() {
    let zip_of = |names: &[&str]| {
        let mut buf = std::io::Cursor::new(Vec::new());
        let mut zw = zip::ZipWriter::new(&mut buf);
        for n in names {
            zw.start_file(*n, zip::write::SimpleFileOptions::default())
                .unwrap();
            zw.write_all(b"<FictionBook/>").unwrap();
        }
        zw.finish().unwrap();
        buf.into_inner()
    };
    assert!(matches!(unzip(&zip_of(&["a.txt"])), Err(Fb2Error::Zip(0))));
    assert!(matches!(
        unzip(&zip_of(&["a.fb2", "b.fb2"])),
        Err(Fb2Error::Zip(2))
    ));
    assert!(unzip(&zip_of(&["a.fb2", "__MACOSX/._a.fb2"])).is_ok());
}

#[test]
fn problems_warn_and_keep_the_text() {
    let book = r##"<FictionBook xmlns:l="http://www.w3.org/1999/xlink"><body><section>
        <p>See <a l:href="#nowhere">here</a>, <a l:href="ftp://x/y">there</a>.</p>
        <image l:href="#missing"/>
        <image l:href="#text"/>
        <marquee>Odd element</marquee>
        </section></body>
        <binary id="text" content-type="image/png">aGVsbG8=</binary></FictionBook>"##;
    let (files, report) = import_bytes(book.as_bytes(), "x");
    let ch = text(&files, "OEBPS/chapter-001.xhtml");
    assert!(ch.contains("<p>See here, there.</p>"), "{ch}");
    assert!(ch.contains("<p>Odd element</p>"));
    assert!(!ch.contains("<img"));
    for w in [
        "link to #nowhere",
        "link to ftp://x/y",
        "image #missing is missing",
        "image #text isn't a PNG, JPEG or GIF",
        "unsupported FB2 element <marquee>",
    ] {
        assert!(
            report.warnings.iter().any(|x| x.contains(w)),
            "{w}: {:?}",
            report.warnings
        );
    }
    // No titled section: one contents entry, the book title (the fallback stem here).
    assert_eq!(contents(&files).len(), 1);
    assert_eq!(report.title, "x");
}

#[test]
fn unknown_language_and_encoding_fall_back() {
    let book = "<?xml version=\"1.0\" encoding=\"x-nonsense\"?><FictionBook><description><title-info>\
        <lang>Russian</lang></title-info></description><body><section><p>Text</p></section></body></FictionBook>";
    let (files, report) = import_bytes(book.as_bytes(), "x");
    assert!(text(&files, "OEBPS/content.opf").contains("<dc:language>en</dc:language>"));
    assert!(
        report
            .warnings
            .iter()
            .any(|w| w.contains("unknown language"))
    );
    assert!(
        report
            .warnings
            .iter()
            .any(|w| w.contains("unknown encoding"))
    );
}
