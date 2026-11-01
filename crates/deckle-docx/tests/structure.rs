// SPDX-License-Identifier: GPL-3.0-or-later
//! Headings recovered from documents that set them by hand (structure-recovery spec §3).
use std::collections::BTreeMap;
use std::io::Write;

use deckle_docx::{ImportReport, to_epub};

const W: &str = "http://schemas.openxmlformats.org/wordprocessingml/2006/main";

/// A paragraph: `size` in points (`None` leaves the style's), all bold or not, plain style.
fn p(text: &str, size: Option<u32>, bold: bool) -> String {
    let mut rpr = String::new();
    if bold {
        rpr.push_str("<w:b/>");
    }
    if let Some(s) = size {
        rpr.push_str(&format!("<w:sz w:val=\"{}\"/>", s * 2));
    }
    format!("<w:p><w:r><w:rPr>{rpr}</w:rPr><w:t xml:space=\"preserve\">{text}</w:t></w:r></w:p>")
}

/// Body text at the document's default size.
fn body(text: &str) -> String {
    p(text, None, false)
}

fn styled(style: &str, text: &str) -> String {
    format!("<w:p><w:pPr><w:pStyle w:val=\"{style}\"/></w:pPr><w:r><w:t>{text}</w:t></w:r></w:p>")
}

const STYLES: &str = r#"<w:styles xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main">
<w:docDefaults><w:rPrDefault><w:rPr><w:sz w:val="24"/></w:rPr></w:rPrDefault></w:docDefaults>
<w:style w:type="paragraph" w:default="1" w:styleId="Normal"><w:name w:val="Normal"/></w:style>
<w:style w:type="paragraph" w:styleId="Heading1"><w:name w:val="heading 1"/><w:basedOn w:val="Normal"/></w:style>
<w:style w:type="paragraph" w:styleId="ListParagraph"><w:name w:val="List Paragraph"/><w:pPr><w:numPr><w:numId w:val="1"/></w:numPr></w:pPr></w:style>
</w:styles>"#;

const NUMBERING: &str = r#"<w:numbering xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main">
<w:abstractNum w:abstractNumId="0"><w:lvl w:ilvl="0"><w:numFmt w:val="bullet"/></w:lvl></w:abstractNum>
<w:num w:numId="1"><w:abstractNumId w:val="0"/></w:num>
</w:numbering>"#;

/// A .docx whose body is `paragraphs`, with 12 pt default text.
fn docx(paragraphs: &[String]) -> Vec<u8> {
    let document = format!(
        "<w:document xmlns:w=\"{W}\"><w:body>{}</w:body></w:document>",
        paragraphs.concat()
    );
    let files = [
        (
            "[Content_Types].xml",
            r#"<Types xmlns="http://schemas.openxmlformats.org/package/2006/content-types"/>"#
                .to_owned(),
        ),
        (
            "_rels/.rels",
            r#"<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="r1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/officeDocument" Target="word/document.xml"/></Relationships>"#.to_owned(),
        ),
        (
            "word/_rels/document.xml.rels",
            r#"<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="s1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/styles" Target="styles.xml"/><Relationship Id="n1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/numbering" Target="numbering.xml"/></Relationships>"#.to_owned(),
        ),
        ("word/document.xml", document),
        ("word/styles.xml", STYLES.to_owned()),
        ("word/numbering.xml", NUMBERING.to_owned()),
    ];
    let mut zip = zip::ZipWriter::new(std::io::Cursor::new(Vec::new()));
    for (name, content) in files {
        zip.start_file(name, zip::write::SimpleFileOptions::default())
            .unwrap();
        zip.write_all(content.as_bytes()).unwrap();
    }
    zip.finish().unwrap().into_inner()
}

fn import(paragraphs: &[String]) -> (BTreeMap<String, String>, ImportReport) {
    let (files, report) = to_epub(&docx(paragraphs), "fallback").unwrap();
    let files = files
        .into_iter()
        .map(|(k, v)| (k, String::from_utf8_lossy(&v).into_owned()))
        .collect();
    (files, report)
}

fn chapters(files: &BTreeMap<String, String>) -> Vec<&str> {
    files
        .iter()
        .filter(|(k, _)| k.starts_with("OEBPS/chapter-"))
        .map(|(_, v)| v.as_str())
        .collect()
}

fn guessed(report: &ImportReport) -> bool {
    report.warnings.iter().any(|w| w.contains("guessed"))
}

const PROSE: &str = "It was a dark and stormy night, and the rain fell in torrents.";

/// Two chapters of two sections each, titles set by hand at 16 pt and 12 pt bold.
fn manuscript() -> Vec<String> {
    let mut out = Vec::new();
    for ch in ["Chapter One", "Chapter Two"] {
        out.push(p(ch, Some(16), true));
        for sec in ["The Storm", "The Calm"] {
            out.push(p(sec, Some(12), true));
            for _ in 0..3 {
                out.push(body(PROSE));
            }
        }
    }
    out
}

#[test]
fn hand_set_titles_become_headings_chapters_and_contents() {
    let (files, report) = import(&manuscript());
    let ch = chapters(&files);
    assert_eq!(ch.len(), 2, "one file per h1");
    assert!(
        ch[0].contains("<h1 id=\"h1\">Chapter One</h1>"),
        "{}",
        ch[0]
    );
    assert!(ch[0].contains("<h2 id=\"h2\">The Storm</h2>"), "{}", ch[0]);
    assert!(ch[1].contains(">Chapter Two</h1>"), "{}", ch[1]);
    assert!(
        !ch[0].contains("<b>"),
        "the bold that made a heading is dropped"
    );
    assert_eq!(report.chapters, 2);
    assert_eq!(report.contents, 6);
    let nav = &files["OEBPS/nav.xhtml"];
    let one = nav.find("Chapter One").unwrap();
    let storm = nav.find("The Storm").unwrap();
    let two = nav.find("Chapter Two").unwrap();
    assert!(one < storm && storm < two);
    assert!(guessed(&report), "{:?}", report.warnings);
}

#[test]
fn bold_titles_at_body_size_become_h1() {
    let mut paras = Vec::new();
    for ch in ["Prologue", "The Long Road", "Homecoming"] {
        paras.push(p(ch, None, true));
        paras.extend((0..3).map(|_| body(PROSE)));
    }
    let (files, report) = import(&paras);
    assert_eq!(report.chapters, 3);
    assert!(chapters(&files)[1].contains("<h1 id=\"h2\">The Long Road</h1>"));
}

#[test]
fn emphasis_lists_and_tables_stay_text() {
    let paras = vec![
        p("Chapter One", Some(16), true),
        p("Never open the door after dark.", None, true),
        p("Bold but it goes on, ", None, true),
        "<w:p><w:pPr><w:pStyle w:val=\"ListParagraph\"/></w:pPr><w:r><w:rPr><w:b/></w:rPr><w:t>A bold item</w:t></w:r></w:p>".to_owned(),
        format!("<w:tbl><w:tr><w:tc>{}</w:tc></w:tr></w:tbl>", p("Cell", None, true)),
        body(PROSE),
        body(PROSE),
        body(PROSE),
    ];
    let (files, report) = import(&paras);
    let html = chapters(&files).concat();
    assert!(html.contains("<h1 id=\"h1\">Chapter One</h1>"), "{html}");
    assert!(html.contains("<p><b>Never open the door after dark.</b></p>"));
    assert!(html.contains("<p><b>Bold but it goes on, </b></p>"));
    assert!(html.contains("<li><b>A bold item</b></li>"), "{html}");
    assert!(html.contains("<td><p><b>Cell</b></p></td>"), "{html}");
    assert_eq!(report.contents, 1);
}

#[test]
fn a_document_set_mostly_in_bold_promotes_nothing() {
    let mut paras = vec![p("Chapter One", None, true)];
    paras.extend((0..4).map(|_| p(PROSE, None, true)));
    paras.push(p("Chapter Two", None, true));
    let (files, report) = import(&paras);
    assert!(!chapters(&files).concat().contains("<h1"));
    assert_eq!(report.chapters, 1);
    assert!(!guessed(&report));
}

#[test]
fn too_many_candidates_promote_nothing() {
    // Short bold lines on every other paragraph: a cue that marks everything marks nothing.
    let mut paras = Vec::new();
    for i in 0..4 {
        paras.push(p(&format!("Line {i}"), Some(16), false));
        paras.push(body(PROSE));
    }
    let (files, report) = import(&paras);
    assert!(!chapters(&files).concat().contains("<h1"));
    assert_eq!(report.chapters, 1);
    assert!(!guessed(&report));
}

#[test]
fn real_heading_styles_switch_recovery_off() {
    let mut paras = vec![styled("Heading1", "Real Chapter")];
    paras.extend(manuscript());
    let (files, report) = import(&paras);
    let html = chapters(&files).concat();
    assert!(html.contains("<h1 id=\"h1\">Real Chapter</h1>"));
    assert!(html.contains("<p><b>Chapter One</b></p>"), "{html}");
    assert_eq!(report.contents, 1);
    assert!(!guessed(&report));
}
