// SPDX-License-Identifier: GPL-3.0-or-later
use std::collections::BTreeMap;
use std::path::PathBuf;

use deckle_docx::{DocxError, ImportReport, convert, to_epub};

fn fixture() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../fixtures/docx/deckle-mini.docx")
}

fn import() -> (BTreeMap<String, Vec<u8>>, ImportReport) {
    let bytes = std::fs::read(fixture()).unwrap();
    let (files, report) = to_epub(&bytes, "fallback").unwrap();
    assert_eq!(files[0].0, "mimetype");
    (files.into_iter().collect(), report)
}

fn text(files: &BTreeMap<String, Vec<u8>>, name: &str) -> String {
    String::from_utf8(files[name].clone()).unwrap()
}

/// All chapter XHTML in spine order, concatenated.
fn chapters(files: &BTreeMap<String, Vec<u8>>) -> String {
    files
        .keys()
        .filter(|k| k.starts_with("OEBPS/chapter-"))
        .map(|k| text(files, k))
        .collect()
}

#[test]
fn every_file_is_well_formed_xml() {
    let (files, _) = import();
    for (name, bytes) in &files {
        if name.ends_with(".xhtml") || name.ends_with(".opf") || name.ends_with(".xml") {
            let s = std::str::from_utf8(bytes).unwrap();
            let opts = roxmltree::ParsingOptions {
                allow_dtd: true,
                ..Default::default()
            };
            roxmltree::Document::parse_with_options(s, opts)
                .unwrap_or_else(|e| panic!("{name}: {e}\n{s}"));
        }
    }
}

#[test]
fn nav_has_one_entry_per_heading_1_to_3_in_order() {
    let (files, report) = import();
    let nav = text(&files, "OEBPS/nav.xhtml");
    let doc = roxmltree::Document::parse_with_options(
        &nav,
        roxmltree::ParsingOptions {
            allow_dtd: true,
            ..Default::default()
        },
    )
    .unwrap();
    let entries: Vec<(String, String)> = doc
        .descendants()
        .filter(|n| n.has_tag_name("a"))
        .map(|a| {
            (
                a.text().unwrap_or("").to_owned(),
                a.attribute("href").unwrap().to_owned(),
            )
        })
        .collect();
    let titles: Vec<&str> = entries.iter().map(|(t, _)| t.as_str()).collect();
    assert_eq!(
        titles,
        ["Chapter One", "The Section", "A Subsection", "Chapter Two"]
    );
    assert_eq!(report.contents, 4);
    // Heading 1 starts a new file; the Title paragraph before it is front matter.
    assert_eq!(entries[0].1, "chapter-002.xhtml#h1");
    assert_eq!(entries[1].1, "chapter-002.xhtml#h2");
    assert_eq!(entries[3].1, "chapter-003.xhtml#h4");
    assert_eq!(report.chapters, 3);
    // Nesting follows the levels: the section sits under Chapter One.
    assert!(
        nav.contains(
            "<li><a href=\"chapter-002.xhtml#h1\">Chapter One</a><ol><li><a href=\"chapter-002.xhtml#h2\">"
        ),
        "{nav}"
    );
    // Heading 4 is a heading in the text, not a contents entry.
    let all = chapters(&files);
    assert!(
        all.contains("<h4 id=\"h5\">Deeper, not in the contents</h4>"),
        "{all}"
    );
    assert!(all.contains("<h1 id=\"h1\">Chapter One</h1>"), "{all}");
}

#[test]
fn inline_styles_lists_and_tables_survive() {
    let (files, _) = import();
    let all = chapters(&files);
    assert!(all.contains("<b>Bold words</b>"), "{all}");
    assert!(all.contains("<i>italic words</i>"), "{all}");
    assert!(all.contains("<b><i>both</i></b>"), "{all}");
    assert!(
        all.contains(
            "<ul><li>A bullet item<ul><li>A nested bullet</li></ul></li><li>Another bullet</li></ul>\
             <ol><li>First step</li><li>Second step</li></ol>"
        ),
        "{all}"
    );
    assert!(
        all.contains(
            "<table><tr><th><p>Name</p></th><th><p>Weight</p></th></tr>\
             <tr><td><p>Caslon</p></td><td><p>Regular</p></td></tr></table>"
        ),
        "{all}"
    );
    assert!(
        all.contains("<p class=\"title\">Deckle Mini Document</p>"),
        "{all}"
    );
}

#[test]
fn every_media_image_is_packaged_and_placed() {
    let (files, report) = import();
    let opf = text(&files, "OEBPS/content.opf");
    let all = chapters(&files);
    let docx = std::fs::read(fixture()).unwrap();
    let mut zip = zip::ZipArchive::new(std::io::Cursor::new(docx)).unwrap();
    let media: Vec<String> = zip
        .file_names()
        .filter_map(Result::ok)
        .filter(|n| n.starts_with("word/media/"))
        .map(|n| n.into_owned())
        .collect();
    assert_eq!(media.len(), 2);
    for m in &media {
        let file = m.rsplit('/').next().unwrap();
        let packaged = format!("OEBPS/images/{file}");
        let mut original = Vec::new();
        std::io::Read::read_to_end(&mut zip.by_name(m).unwrap(), &mut original).unwrap();
        assert_eq!(files[&packaged], original, "{packaged}");
        assert!(opf.contains(&format!("href=\"images/{file}\"")), "{opf}");
        assert!(
            all.contains(&format!("src=\"images/{file}\"")),
            "{file} not placed"
        );
    }
    assert_eq!(report.images, 2);
    // At its position: the first image sits under The Section, before the text that follows.
    let ch1 = text(&files, "OEBPS/chapter-002.xhtml");
    let section = ch1.find("The Section").unwrap();
    let img = ch1
        .find("<p class=\"figure\"><img src=\"images/image1.png\" alt=\"A blue square\"/></p>")
        .unwrap();
    assert!(img > section);
    assert!(
        text(&files, "OEBPS/chapter-003.xhtml").contains("src=\"images/image2.png\""),
        "second image belongs to Chapter Two"
    );
}

#[test]
fn footnotes_become_linked_asides() {
    let (files, report) = import();
    assert_eq!(report.notes, 3);
    let ch1 = text(&files, "OEBPS/chapter-002.xhtml");
    let ch2 = text(&files, "OEBPS/chapter-003.xhtml");
    for (ch, n) in [(&ch1, 1), (&ch1, 2), (&ch2, 3)] {
        assert!(
            ch.contains(&format!(
                "<sup><a epub:type=\"noteref\" class=\"noteref\" href=\"#fn{n}\" id=\"fn{n}-ref\">{n}</a></sup>"
            )),
            "noteref {n}: {ch}"
        );
        assert!(
            ch.contains(&format!(
                "<aside epub:type=\"footnote\" class=\"footnote\" id=\"fn{n}\"><p><a href=\"#fn{n}-ref\">{n}.</a> "
            )),
            "aside {n}: {ch}"
        );
    }
    assert!(ch1.contains("A note with <i>emphasis</i>."), "{ch1}");
    // Each note lives in the file that references it.
    assert!(!ch2.contains("id=\"fn1\""));
    // The note mark Word draws inside the note is not repeated.
    assert!(ch1.contains("1.</a> The first note.</p></aside>"), "{ch1}");
}

#[test]
fn metadata_comes_from_the_core_properties() {
    let (files, report) = import();
    let opf = text(&files, "OEBPS/content.opf");
    assert_eq!(report.title, "Deckle Mini Document");
    assert!(opf.contains("<dc:title>Deckle Mini Document</dc:title>"));
    assert!(opf.contains("<dc:creator>Jane Testwright</dc:creator>"));
    assert!(opf.contains("<dc:language>en-US</dc:language>"), "{opf}");
    assert!(opf.contains("<meta property=\"dcterms:modified\">2026-10-21T00:00:00Z</meta>"));
    assert!(opf.contains("urn:deckle:docx:"));
    assert!(report.warnings.is_empty(), "{:?}", report.warnings);
}

#[test]
fn convert_writes_an_epub_the_pipeline_can_inspect() {
    let dir = tempfile::tempdir().unwrap();
    let out = dir.path().join("book.epub");
    let report = convert(&fixture(), &out).unwrap();
    assert_eq!(report.title, "Deckle Mini Document");
    let mut zip = zip::ZipArchive::new(std::fs::File::open(&out).unwrap()).unwrap();
    let first = zip.by_index(0).unwrap();
    assert_eq!(first.name().unwrap(), "mimetype");
    assert_eq!(first.compression(), zip::CompressionMethod::Stored);
}

#[test]
fn bad_inputs_get_specific_errors() {
    let err = |bytes: &[u8]| to_epub(bytes, "x").unwrap_err();
    assert!(matches!(
        err(b"plain text, not a document"),
        DocxError::NotZip
    ));

    let mut ole = vec![0xD0, 0xCF, 0x11, 0xE0, 0xA1, 0xB1, 0x1A, 0xE1];
    ole.resize(512, 0);
    assert!(matches!(err(&ole), DocxError::LegacyDoc));
    let mut encrypted = ole.clone();
    encrypted.extend("EncryptedPackage".encode_utf16().flat_map(u16::to_le_bytes));
    let e = err(&encrypted);
    assert!(matches!(e, DocxError::Encrypted));
    assert!(e.to_string().contains("password-protected"), "{e}");

    // A zip that is not a Word document (an EPUB renamed .docx, a spreadsheet).
    let mut buf = std::io::Cursor::new(Vec::new());
    {
        let mut zw = zip::ZipWriter::new(&mut buf);
        zw.start_file("mimetype", zip::write::SimpleFileOptions::default())
            .unwrap();
        std::io::Write::write_all(&mut zw, b"application/epub+zip").unwrap();
        zw.finish().unwrap();
    }
    let e = err(buf.get_ref());
    assert!(matches!(e, DocxError::NoDocument), "{e}");
    assert!(e.to_string().contains("word/document.xml"));

    // A truncated download.
    let docx = std::fs::read(fixture()).unwrap();
    assert!(matches!(
        err(&docx[..docx.len() / 2]),
        DocxError::Corrupt(_)
    ));
}

#[test]
fn word_extensions_are_recognized() {
    use std::path::Path;
    assert!(deckle_docx::is_word(Path::new("a/Report.DOCX")));
    assert!(deckle_docx::is_word(Path::new("old.doc")));
    assert!(!deckle_docx::is_word(Path::new("book.epub")));
}
