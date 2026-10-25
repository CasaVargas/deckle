// SPDX-License-Identifier: GPL-3.0-or-later
use std::collections::{BTreeMap, HashSet};
use std::path::{Path, PathBuf};

use deckle_rtf::{ImportReport, RtfError, convert, is_rtf, to_epub};

fn fixture(name: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../fixtures/rtf")
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

/// A one-paragraph-or-more document around `body`.
fn rtf(body: &str) -> (Files, ImportReport) {
    import_bytes(format!("{{\\rtf1\\ansi\\deff0 {body}}}").as_bytes())
}

fn text(files: &Files, name: &str) -> String {
    String::from_utf8(files[name].clone()).unwrap()
}

fn chapter(files: &Files) -> String {
    text(files, "OEBPS/chapter-001.xhtml")
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

#[test]
fn recognises_rtf() {
    assert!(is_rtf(Path::new("a.rtf")));
    assert!(is_rtf(Path::new("A.RTF")));
    assert!(!is_rtf(Path::new("a.rtfd")));
    assert!(!is_rtf(Path::new("a.txt")));
}

#[test]
fn every_file_is_well_formed_and_every_link_lands() {
    let (files, _) = import("mini.rtf");
    let mut ids = HashSet::new();
    for (name, bytes) in files.iter().filter(|(n, _)| n.ends_with(".xhtml")) {
        for n in parse(std::str::from_utf8(bytes).unwrap()).descendants() {
            if let Some(id) = n.attribute("id") {
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
        for n in parse(std::str::from_utf8(bytes).unwrap()).descendants() {
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
fn metadata_comes_from_info() {
    let (files, report) = import("mini.rtf");
    assert_eq!(report.title, "The Deckle RTF Mini");
    let opf = text(&files, "OEBPS/content.opf");
    assert!(opf.contains("<dc:creator>Ada Testwright</dc:creator>"));
    assert!(opf.contains("<dc:language>en-GB</dc:language>"));
    assert!(opf.contains("<dc:description>A small document about a print shop.</dc:description>"));
    assert!(opf.contains(
        "<dc:subject>printing</dc:subject><dc:subject>type</dc:subject><dc:subject>presses</dc:subject>"
    ));
    assert!(opf.contains("<meta property=\"dcterms:modified\">2026-10-25T09:30:00Z</meta>"));
    assert!(opf.contains("urn:deckle:rtf:"));
}

#[test]
fn headings_become_contents_and_level_one_starts_a_file() {
    let (files, report) = import("mini.rtf");
    assert_eq!(
        contents(&files),
        [
            (1, "Chapter One".into(), "chapter-002.xhtml#h1".into()),
            (2, "Section 1.1".into(), "chapter-002.xhtml#h2".into()),
            (3, "Subsection".into(), "chapter-002.xhtml#h3".into()),
            (1, "Chapter Two".into(), "chapter-003.xhtml#h5".into()),
        ]
    );
    assert_eq!(report.chapters, 3);
    let ch1 = text(&files, "OEBPS/chapter-002.xhtml");
    // Stylesheet names and \outlinelevel both make headings; their own bold isn't repeated.
    assert!(ch1.contains("<h1 id=\"h1\">Chapter One</h1>"));
    assert!(ch1.contains("<h4 id=\"h4\">Deep heading</h4>"));
    assert!(text(&files, "OEBPS/chapter-003.xhtml").contains("<h1 id=\"h5\">Chapter Two</h1>"));
    assert!(chapter(&files).contains("<p class=\"title center\">The Deckle RTF Mini</p>"));
}

#[test]
fn character_formatting_and_special_characters() {
    let (files, _) = import("mini.rtf");
    let ch1 = text(&files, "OEBPS/chapter-002.xhtml");
    assert!(ch1.contains(
        "<p>Plain <b>bold</b> <i>italic</i> <u>under</u> <s>struck</s> E=mc<sup>2</sup> \
         H<sub>2</sub>O <span class=\"sc\">caps</span> <b><u>loud</u></b>.</p>"
    ));
    assert!(ch1.contains("<p>Café €uro ‘single’ “double” em—dash en–dash tab<br/>next line<sup>"));
    assert!(ch1.contains("<p class=\"center\">Centred line</p>"));
    assert!(ch1.contains("<blockquote><p>A quoted passage.</p></blockquote>"));
}

#[test]
fn lists_follow_the_list_table() {
    let (files, _) = import("mini.rtf");
    let ch1 = text(&files, "OEBPS/chapter-002.xhtml");
    assert!(ch1.contains(
        "<ol start=\"3\"><li>First</li><li>Second<ul><li>Nested a</li><li>Nested b</li></ul></li><li>Third</li></ol>"
    ));
}

#[test]
fn old_style_pn_numbering_is_a_list() {
    let (files, _) = rtf(
        "\\pard{\\*\\pn\\pnlvlblt{\\pntxtb\\'b7}}One\\par\\pard{\\*\\pn\\pnlvlbody\\pndec\\pnstart4}Four\\par",
    );
    assert!(chapter(&files).contains("<ul><li>One</li></ul><ol start=\"4\"><li>Four</li></ol>"));
}

#[test]
fn tables_keep_header_rows_and_merges() {
    let (files, _) = import("mini.rtf");
    let ch2 = text(&files, "OEBPS/chapter-003.xhtml");
    assert!(ch2.contains(
        "<table><tr><th><p>Face</p></th><th><p>Size</p></th></tr><tr><td><p>Caslon</p></td><td><p>12</p></td></tr>\
         <tr><td colspan=\"2\"><p>Garamond, all sizes</p></td></tr></table>"
    ));
}

#[test]
fn vertical_merges_become_rowspans() {
    let (files, _) = rtf(
        "\\trowd\\clvmgf\\cellx1000\\cellx2000\\pard\\intbl Tall\\cell B1\\cell\\row\
         \\trowd\\clvmrg\\cellx1000\\cellx2000\\pard\\intbl\\cell B2\\cell\\row\\pard After\\par",
    );
    assert!(chapter(&files).contains(
        "<table><tr><td rowspan=\"2\"><p>Tall</p></td><td><p>B1</p></td></tr><tr><td><p>B2</p></td></tr></table><p>After</p>"
    ));
}

#[test]
fn notes_become_pop_up_asides_in_their_chapter() {
    let (files, report) = import("mini.rtf");
    assert_eq!(report.notes, 3);
    let ch1 = text(&files, "OEBPS/chapter-002.xhtml");
    assert!(ch1.contains(
        "next line<sup><a epub:type=\"noteref\" class=\"noteref\" href=\"#fn1\" id=\"fn1-ref\">1</a></sup></p>"
    ));
    assert!(ch1.contains(
        "<aside epub:type=\"footnote\" class=\"footnote\" id=\"fn1\"><p><a href=\"#fn1-ref\">1.</a> First footnote.</p></aside></body>"
    ));
    let ch2 = text(&files, "OEBPS/chapter-003.xhtml");
    assert!(ch2.contains(
        "<aside epub:type=\"endnote\" class=\"endnote\" id=\"en1\"><p><a href=\"#en1-ref\">1.</a> An endnote.</p></aside>"
    ));
}

#[test]
fn pictures_come_from_hex_and_bin_once_each() {
    let (files, report) = import("mini.rtf");
    assert_eq!(report.images, 2);
    let ch2 = text(&files, "OEBPS/chapter-003.xhtml");
    // The \nonshppict WMF copy is ignored.
    assert!(
        ch2.contains("<p>A dot <img src=\"images/image-001.png\" alt=\"Inline dot\"/> inline.</p>")
    );
    assert!(ch2.contains("<p class=\"figure\"><img src=\"images/image-002.jpg\" alt=\"\"/></p>"));
    assert!(files["OEBPS/images/image-002.jpg"].starts_with(b"\xff\xd8\xff"));
    assert!(report.warnings.is_empty(), "{:?}", report.warnings);
}

#[test]
fn fields_become_links_and_keep_their_results() {
    let (files, _) = import("mini.rtf");
    let ch2 = text(&files, "OEBPS/chapter-003.xhtml");
    assert!(ch2.contains("<a id=\"Plate_One\"></a>The plate above."));
    assert!(ch2.contains("See <a href=\"chapter-003.xhtml#Plate_One\">the plate</a> and"));
    assert!(ch2.contains("<a href=\"https://example.com/press\"><u>the press</u></a>, page 7."));
}

#[test]
fn hidden_deleted_comments_headers_and_stored_contents_are_left_out() {
    let (files, _) = import("mini.rtf");
    let all: String = files
        .iter()
        .filter(|(k, _)| k.ends_with(".xhtml"))
        .map(|(_, v)| String::from_utf8(v.clone()).unwrap())
        .collect();
    for sentinel in [
        "HIDDEN-SENTINEL",
        "DELETED-SENTINEL",
        "COMMENT-SENTINEL",
        "HEADER-SENTINEL",
        "STORED-TOC-SENTINEL",
    ] {
        assert!(!all.contains(sentinel), "{sentinel}");
    }
}

#[test]
fn code_pages_decode_by_document_and_font() {
    let (files, report) = import("cyrillic.rtf");
    assert!(chapter(&files).contains("<p>Утро в типографии было холодным.</p>"));
    assert!(text(&files, "OEBPS/content.opf").contains("<dc:creator>Иван Наборщиков</dc:creator>"));
    assert!(text(&files, "OEBPS/content.opf").contains("<dc:language>ru-RU</dc:language>"));
    assert_eq!(report.title, "stem");
    let (files, _) = import("japanese.rtf");
    assert!(chapter(&files).contains("<p>Latin, then 活版印刷.</p>"));
    assert!(chapter(&files).contains("xml:lang=\"ja-JP\""));
}

#[test]
fn unicode_skips_its_replacement_characters() {
    let (files, _) = rtf("\\uc2 A\\u8212\\'97\\'97B\\uc0\\u8212 C\\par");
    // The space after `\u8212` is its delimiter, not text.
    assert!(
        chapter(&files).contains("<p>A—B—C</p>"),
        "{}",
        chapter(&files)
    );
}

#[test]
fn control_symbols() {
    let (files, _) = rtf("a\\~b\\_c\\-d \\{e\\} \\\\f\\par");
    assert!(chapter(&files).contains("<p>a&#160;b\u{2011}cd {e} \\f</p>"));
}

#[test]
fn a_missing_final_brace_is_tolerated() {
    let (files, report) = import("unclosed.rtf");
    assert!(chapter(&files).contains("<p>Still <b>readable</b></p>"));
    assert!(report.warnings.iter().any(|w| w.contains("braces")));
}

#[test]
fn import_is_deterministic() {
    assert_eq!(import("mini.rtf").0, import("mini.rtf").0);
}

#[test]
fn convert_writes_an_epub_named_after_the_file() {
    let dir = tempfile::tempdir().unwrap();
    let src = dir.path().join("Notes Draft.rtf");
    std::fs::write(&src, "{\\rtf1 Only text.\\par}").unwrap();
    let epub = dir.path().join("out.epub");
    let report = convert(&src, &epub).unwrap();
    assert_eq!(report.title, "Notes Draft");
    assert!(epub.exists());
}

#[test]
fn refusals() {
    let err = |b: &[u8]| to_epub(b, "x").expect_err("an error");
    assert!(matches!(err(b"plain text"), RtfError::NotRtf));
    assert!(matches!(err(b"{\\rtx1 nope}"), RtfError::NotRtf));
    assert!(matches!(
        err(b"{\\rtf1\\ansi{\\fonttbl{\\f0 Times;}}}"),
        RtfError::Empty
    ));
    assert!(matches!(
        err(b"{\\rtf1 {\\v hidden only}\\par}"),
        RtfError::Empty
    ));
}

#[test]
fn problems_warn_and_keep_the_text() {
    let (files, report) = rtf(
        "See {\\field{\\*\\fldinst HYPERLINK \"file:///x.doc\"}{\\fldrslt there}} and \
         {\\field{\\*\\fldinst HYPERLINK \\\\l \"Nowhere\"}{\\fldrslt here}}.\
         {\\pict\\wmetafile8 0102}{\\pict\\pngblip 41424344}{\\object{\\*\\objdata 00}}\\par",
    );
    let ch = chapter(&files);
    assert!(
        ch.contains("<p>See there and <a href=\"#\">here</a>.</p>"),
        "{ch}"
    );
    for w in [
        "link to file:///x.doc",
        "bookmark Nowhere wasn't found",
        "a WMF picture isn't supported",
        "a picture's data isn't a PNG, JPEG or GIF",
        "embedded objects",
    ] {
        assert!(
            report.warnings.iter().any(|x| x.contains(w)),
            "{w}: {:?}",
            report.warnings
        );
    }
}

#[test]
fn unknown_language_falls_back_to_english() {
    let (files, report) = import_bytes(b"{\\rtf1\\deflang9999 Text\\par}");
    assert!(text(&files, "OEBPS/content.opf").contains("<dc:language>en</dc:language>"));
    assert!(
        report
            .warnings
            .iter()
            .any(|w| w.contains("unknown language code 9999"))
    );
}
