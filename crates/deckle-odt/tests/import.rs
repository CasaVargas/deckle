// SPDX-License-Identifier: GPL-3.0-or-later
use std::collections::{BTreeMap, HashSet};
use std::io::Write;
use std::path::{Path, PathBuf};

use deckle_odt::{ImportReport, OdtError, convert, is_odt, to_epub};

fn fixture(name: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../fixtures/odt")
        .join(name)
}

type Files = BTreeMap<String, Vec<u8>>;

fn import_bytes(bytes: &[u8]) -> (Files, ImportReport) {
    let (files, report) = to_epub(bytes, "stem").unwrap();
    assert_eq!(files[0].0, "mimetype");
    (files.into_iter().collect(), report)
}

fn import(name: &str) -> (Files, ImportReport) {
    import_bytes(&std::fs::read(fixture(name)).unwrap())
}

fn text(files: &Files, name: &str) -> String {
    String::from_utf8(files[name].clone()).unwrap()
}

fn parse(s: &str) -> roxmltree::Document<'_> {
    roxmltree::Document::parse(s).unwrap_or_else(|e| panic!("{e}\n{s}"))
}

fn all_chapters(files: &Files) -> String {
    files
        .iter()
        .filter(|(k, _)| k.starts_with("OEBPS/chapter-"))
        .map(|(_, v)| String::from_utf8(v.clone()).unwrap())
        .collect()
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

/// A zipped ODT from `content.xml` body markup alone.
fn odt(body: &str) -> Vec<u8> {
    let content = format!(
        "<office:document-content xmlns:office=\"urn:oasis:names:tc:opendocument:xmlns:office:1.0\" \
         xmlns:text=\"urn:oasis:names:tc:opendocument:xmlns:text:1.0\" \
         xmlns:draw=\"urn:oasis:names:tc:opendocument:xmlns:drawing:1.0\" \
         xmlns:xlink=\"http://www.w3.org/1999/xlink\"><office:body><office:text>{body}</office:text></office:body></office:document-content>"
    );
    let mut buf = std::io::Cursor::new(Vec::new());
    let mut zw = zip::ZipWriter::new(&mut buf);
    zw.start_file("mimetype", zip::write::SimpleFileOptions::default())
        .unwrap();
    zw.write_all(b"application/vnd.oasis.opendocument.text")
        .unwrap();
    zw.start_file("content.xml", zip::write::SimpleFileOptions::default())
        .unwrap();
    zw.write_all(content.as_bytes()).unwrap();
    zw.start_file("Pictures/not.svg", zip::write::SimpleFileOptions::default())
        .unwrap();
    zw.write_all(b"<svg/>").unwrap();
    zw.finish().unwrap();
    buf.into_inner()
}

#[test]
fn recognises_odf_text_extensions() {
    for ok in ["a.odt", "A.ODT", "a.ott", "a.fodt"] {
        assert!(is_odt(Path::new(ok)), "{ok}");
    }
    for no in ["a.ods", "a.odp", "a.docx", "a.odt.bak"] {
        assert!(!is_odt(Path::new(no)), "{no}");
    }
}

#[test]
fn every_file_is_well_formed_and_every_link_lands() {
    let (files, _) = import("mini.odt");
    let mut ids = HashSet::new();
    for (name, bytes) in &files {
        let is_xml = [".xhtml", ".opf", ".xml"].iter().any(|e| name.ends_with(e));
        if !is_xml {
            continue;
        }
        let s = std::str::from_utf8(bytes).unwrap();
        for n in parse(s).descendants() {
            if let Some(id) = n.attribute("id")
                && name.ends_with(".xhtml")
            {
                assert!(
                    ids.insert(format!("{}#{id}", &name["OEBPS/".len()..])),
                    "{id}"
                );
            }
        }
    }
    let opf = text(&files, "OEBPS/content.opf");
    for item in parse(&opf).descendants().filter(|n| n.has_tag_name("item")) {
        let href = item.attribute("href").unwrap();
        assert!(files.contains_key(&format!("OEBPS/{href}")), "{href}");
    }
    for (name, bytes) in files.iter().filter(|(n, _)| n.ends_with(".xhtml")) {
        let page = &name["OEBPS/".len()..];
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
            let target = match href.strip_prefix('#') {
                Some(frag) => format!("{page}#{frag}"),
                None => href.to_owned(),
            };
            assert!(ids.contains(&target), "{name}: {href}");
        }
    }
}

#[test]
fn metadata_comes_from_meta_xml() {
    let (files, report) = import("mini.odt");
    assert_eq!(report.title, "The Deckle ODT Mini");
    let opf = text(&files, "OEBPS/content.opf");
    assert!(opf.contains("<dc:creator>Ada Testwright</dc:creator>"));
    assert!(!opf.contains("Someone Else"));
    assert!(opf.contains("<dc:language>en-US</dc:language>"));
    assert!(opf.contains("<dc:description>A small document about a print shop.</dc:description>"));
    assert!(opf.contains("<dc:subject>printing</dc:subject><dc:subject>type</dc:subject>"));
    assert!(opf.contains("<meta property=\"dcterms:modified\">2026-10-25T09:30:12Z</meta>"));
    assert!(opf.contains("urn:deckle:odt:"));
}

#[test]
fn headings_become_contents_and_level_one_starts_a_file() {
    let (files, report) = import("mini.odt");
    let entries: Vec<(usize, String, String)> = contents(&files);
    assert_eq!(
        entries,
        [
            (1, "Chapter One".into(), "chapter-002.xhtml#h1".into()),
            (2, "Section 1.1".into(), "chapter-002.xhtml#h2".into()),
            (3, "Subsection".into(), "chapter-002.xhtml#h3".into()),
            (1, "Chapter Two".into(), "chapter-003.xhtml#h5".into()),
        ]
    );
    assert_eq!(report.chapters, 3);
    let ch1 = text(&files, "OEBPS/chapter-002.xhtml");
    // A heading's own bold isn't repeated; level 4 stays in the text.
    assert!(ch1.contains("<h1 id=\"h1\">Chapter One</h1>"));
    assert!(ch1.contains("<h4 id=\"h4\">Deep heading</h4>"));
    let title = text(&files, "OEBPS/chapter-001.xhtml");
    assert!(title.contains("<p class=\"title\">The Deckle ODT Mini</p>"));
    assert!(title.contains("<p class=\"subtitle\">A test document</p>"));
}

#[test]
fn formatting_follows_automatic_and_inherited_styles() {
    let (files, _) = import("mini.odt");
    let ch1 = text(&files, "OEBPS/chapter-002.xhtml");
    assert!(ch1.contains(
        "<p>Plain <b>bold</b> <i>italic</i> <u>under</u> <s>struck</s> E=mc<sup>2</sup> \
         H<sub>2</sub>O <span class=\"sc\">caps</span> <i>emph</i> <b><span class=\"sc\">loud</span></b>.</p>"
    ));
    assert!(ch1.contains("<p>Gap &#160;&#160;here tab<br/>next line."));
    assert!(ch1.contains("<p class=\"center\">Centred line</p>"));
    assert!(ch1.contains("<blockquote><p>A quoted passage.</p></blockquote>"));
}

#[test]
fn lists_nest_with_their_numbering() {
    let (files, _) = import("mini.odt");
    let ch1 = text(&files, "OEBPS/chapter-002.xhtml");
    assert!(ch1.contains(
        "<ol start=\"3\"><li>First</li><li>Second<ul><li>Nested a</li><li>Nested b</li></ul></li><li>Third</li></ol>"
    ));
}

#[test]
fn tables_keep_header_rows_and_spans() {
    let (files, _) = import("mini.odt");
    let ch2 = text(&files, "OEBPS/chapter-003.xhtml");
    assert!(ch2.contains(
        "<table><tr><th><p>Face</p></th><th><p>Size</p></th></tr><tr><td><p>Caslon</p></td><td><p>12</p></td></tr>\
         <tr><td colspan=\"2\"><p>Garamond, all sizes</p></td></tr></table>"
    ));
}

#[test]
fn notes_become_pop_up_asides_in_their_chapter() {
    let (files, report) = import("mini.odt");
    assert_eq!(report.notes, 3);
    let ch1 = text(&files, "OEBPS/chapter-002.xhtml");
    assert!(ch1.contains(
        "<sup><a epub:type=\"noteref\" class=\"noteref\" href=\"#fn1\" id=\"fn1-ref\">1</a></sup>"
    ));
    assert!(ch1.contains(
        "<aside epub:type=\"footnote\" class=\"footnote\" id=\"fn1\"><p><a href=\"#fn1-ref\">1.</a> First footnote.</p></aside></body>"
    ));
    let ch2 = text(&files, "OEBPS/chapter-003.xhtml");
    assert!(ch2.contains("href=\"#en1\" id=\"en1-ref\">i</a>"));
    assert!(ch2.contains(
        "<aside epub:type=\"endnote\" class=\"endnote\" id=\"en1\"><p><a href=\"#en1-ref\">i.</a> An endnote.</p></aside>"
    ));
}

#[test]
fn images_are_sniffed_placed_and_described() {
    let (files, report) = import("mini.odt");
    assert_eq!(report.images, 2);
    let ch2 = text(&files, "OEBPS/chapter-003.xhtml");
    assert!(
        ch2.contains("<p>A dot <img src=\"images/image-001.gif\" alt=\"Inline dot\"/> inline.</p>")
    );
    assert!(ch2.contains(
        "<p class=\"figure\"><img src=\"images/image-002.png\" alt=\"A grey plate\"/></p>"
    ));
    assert!(report.warnings.is_empty(), "{:?}", report.warnings);
}

#[test]
fn links_resolve_to_bookmarks_and_the_web() {
    let (files, _) = import("mini.odt");
    let ch2 = text(&files, "OEBPS/chapter-003.xhtml");
    assert!(ch2.contains("<a id=\"Plate_One\"></a>The plate above."));
    assert!(ch2.contains("<a href=\"chapter-003.xhtml#Plate_One\">the plate</a>"));
    assert!(ch2.contains("<a href=\"https://example.com/press\">the press</a>, page 7."));
}

#[test]
fn generated_indexes_comments_and_deletions_are_left_out() {
    let (files, _) = import("mini.odt");
    let all = all_chapters(&files);
    for sentinel in [
        "STORED-TOC-SENTINEL",
        "COMMENT-SENTINEL",
        "DELETED-SENTINEL",
    ] {
        assert!(!all.contains(sentinel), "{sentinel}");
    }
}

#[test]
fn flat_odf_imports_the_same_book() {
    let (zipped, _) = import("mini.odt");
    let (flat, report) = import("mini.fodt");
    assert!(report.warnings.is_empty(), "{:?}", report.warnings);
    assert_eq!(zipped, flat);
}

#[test]
fn import_is_deterministic() {
    assert_eq!(import("mini.odt").0, import("mini.odt").0);
}

#[test]
fn convert_writes_an_epub() {
    let dir = tempfile::tempdir().unwrap();
    let epub = dir.path().join("out.epub");
    let report = convert(&fixture("mini.odt"), &epub).unwrap();
    assert_eq!(report.title, "The Deckle ODT Mini");
    let mut zip = zip::ZipArchive::new(std::fs::File::open(&epub).unwrap()).unwrap();
    assert_eq!(zip.by_index(0).unwrap().name().unwrap(), "mimetype");
}

#[test]
fn title_falls_back_to_the_file_name() {
    let (_, report) = import_bytes(&odt("<text:p>Just text.</text:p>"));
    assert_eq!(report.title, "stem");
}

#[test]
fn refusals() {
    let err = |bytes: &[u8]| to_epub(bytes, "x").expect_err("an error");
    assert!(matches!(err(b"plain words"), OdtError::NotOdf));
    assert!(matches!(err(b"<html/>"), OdtError::NotOdf));
    let sheet = std::fs::read(fixture("sheet.ods")).unwrap();
    assert!(matches!(err(&sheet), OdtError::WrongType(t) if t == "spreadsheet"));
    let locked = std::fs::read(fixture("encrypted.odt")).unwrap();
    assert!(matches!(err(&locked), OdtError::Encrypted));
    let mut buf = std::io::Cursor::new(Vec::new());
    let mut zw = zip::ZipWriter::new(&mut buf);
    zw.start_file("mimetype", zip::write::SimpleFileOptions::default())
        .unwrap();
    zw.write_all(b"application/vnd.oasis.opendocument.text")
        .unwrap();
    zw.finish().unwrap();
    assert!(matches!(err(&buf.into_inner()), OdtError::NoContent));
    assert!(matches!(
        err(&odt("<text:p>unclosed")),
        OdtError::Corrupt(m) if m.starts_with("content.xml")
    ));
}

#[test]
fn problems_warn_and_keep_the_text() {
    let (files, report) = import_bytes(&odt(
        "<text:p>See <text:a xlink:href=\"#Nowhere\">here</text:a>, \
         <text:a xlink:href=\"other.odt\">there</text:a>.</text:p>\
         <text:p><draw:frame><draw:image xlink:href=\"Pictures/missing.png\"/></draw:frame>\
         <draw:frame><draw:image xlink:href=\"Pictures/not.svg\"/></draw:frame>\
         <draw:frame><draw:image xlink:href=\"https://example.com/x.png\"/></draw:frame></text:p>\
         <text:marquee>Odd element</text:marquee>",
    ));
    let ch = text(&files, "OEBPS/chapter-001.xhtml");
    assert!(ch.contains("<p>See here, there.</p>"), "{ch}");
    assert!(ch.contains("<p>Odd element</p>"));
    assert!(!ch.contains("<img"));
    for w in [
        "link to #Nowhere",
        "link to other.odt",
        "image Pictures/missing.png is missing",
        "image Pictures/not.svg isn't a PNG, JPEG or GIF",
        "image https://example.com/x.png is on the web",
        "unsupported OpenDocument element <marquee>",
    ] {
        assert!(
            report.warnings.iter().any(|x| x.contains(w)),
            "{w}: {:?}",
            report.warnings
        );
    }
}

#[test]
fn text_boxes_keep_captioned_images() {
    let (files, _) = import_bytes(&odt(
        "<text:p><draw:frame><draw:text-box><text:p>Caption text</text:p></draw:text-box></draw:frame></text:p>",
    ));
    assert!(text(&files, "OEBPS/chapter-001.xhtml").contains("<p>Caption text</p>"));
}
