// SPDX-License-Identifier: GPL-3.0-or-later
//! PDF fixtures (made by tools/pdf-fixtures/make.py) through PDFium to an EPUB. PDFium comes
//! from tools/fetch-pdfium.sh or DECKLE_PDFIUM; without it these tests fail and say so.

use std::io::Read;
use std::path::{Path, PathBuf};

use deckle_pdf::{PdfError, convert};

fn fixture(name: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../fixtures/pdf")
        .join(name)
}

/// The EPUB's files, by name.
fn files(epub: &Path) -> Vec<(String, Vec<u8>)> {
    let mut zip = zip::ZipArchive::new(std::fs::File::open(epub).unwrap()).unwrap();
    (0..zip.len())
        .map(|i| {
            let mut f = zip.by_index(i).unwrap();
            let mut bytes = Vec::new();
            f.read_to_end(&mut bytes).unwrap();
            (f.name().unwrap().into_owned(), bytes)
        })
        .collect()
}

/// The chapters' XHTML, in order.
fn chapters(epub: &Path) -> String {
    let mut ch: Vec<(String, Vec<u8>)> = files(epub)
        .into_iter()
        .filter(|(n, _)| n.starts_with("OEBPS/chapter-"))
        .collect();
    ch.sort();
    ch.into_iter()
        .map(|(_, b)| String::from_utf8(b).unwrap())
        .collect::<Vec<_>>()
        .join("\n")
}

fn convert_ok(name: &str) -> (tempfile::TempDir, PathBuf, deckle_pdf::ImportReport) {
    let dir = tempfile::tempdir().unwrap();
    let out = dir.path().join("out.epub");
    let report = match convert(&fixture(name), &out) {
        Ok(r) => r,
        Err(e @ PdfError::NoEngine(_)) => panic!("{e}\nrun tools/fetch-pdfium.sh first"),
        Err(e) => panic!("{name}: {e}"),
    };
    (dir, out, report)
}

fn at(html: &str, needle: &str) -> usize {
    html.find(needle)
        .unwrap_or_else(|| panic!("{needle:?} not in:\n{html}"))
}

#[test]
fn two_columns_read_column_by_column_without_running_heads() {
    let (_d, epub, report) = convert_ok("two-column.pdf");
    let html = chapters(&epub);
    let order = [
        "Alpha begins",
        "Bravo opens",
        "Charlie starts",
        "Delta closes",
    ]
    .map(|w| at(&html, w));
    assert!(order.windows(2).all(|w| w[0] < w[1]), "{order:?}\n{html}");
    for gone in ["Proceedings of the Deckle", "Vol. 3", "Page 1", "Page 2"] {
        assert!(!html.contains(gone), "{gone:?} left in:\n{html}");
    }
    assert_eq!(report.stats.columns, 2);
    assert_eq!(report.title, "On Reading Order");
    assert!(report.warnings[0].contains("reflowed from a PDF"));
    assert!(
        html.contains("<h1 id=\"h1\">On Reading Order in Two Columns</h1>"),
        "{html}"
    );
}

#[test]
fn broken_words_rejoin_and_compounds_keep_their_hyphen() {
    let (_d, epub, _) = convert_ok("two-column.pdf");
    let html = chapters(&epub);
    assert!(html.contains("the age of information into"), "{html}");
    assert!(!html.contains("informa-"), "{html}");
    assert!(
        html.contains("phrase breaks as well-known across"),
        "{html}"
    );
}

#[test]
fn tables_equations_and_images_are_pictures() {
    let (_d, epub, report) = convert_ok("table.pdf");
    let html = chapters(&epub);
    for gone in ["Apple", "1.20", "Plum"] {
        assert!(!html.contains(gone), "table text {gone:?} left in:\n{html}");
    }
    let order = [
        "Echo introduces",
        "alt=\"Table from page 1 of the PDF\"",
        "Foxtrot follows",
        "alt=\"Equation from page 1 of the PDF\"",
        "Golf comes after",
        "alt=\"Figure from page 1 of the PDF\"",
        "Hotel ends",
    ]
    .map(|w| at(&html, w));
    assert!(order.windows(2).all(|w| w[0] < w[1]), "{order:?}\n{html}");
    let pngs: Vec<_> = files(&epub)
        .into_iter()
        .filter(|(n, _)| n.ends_with(".png"))
        .collect();
    assert_eq!(pngs.len(), 3);
    for (name, bytes) in pngs {
        let img = image::load_from_memory(&bytes).unwrap_or_else(|e| panic!("{name}: {e}"));
        assert!(
            img.width() > 40 && img.height() > 20,
            "{name}: {}x{}",
            img.width(),
            img.height()
        );
    }
    assert_eq!(
        (
            report.stats.tables,
            report.stats.equations,
            report.stats.images
        ),
        (1, 1, 1)
    );
}

#[test]
fn chapters_split_at_headings_with_contents() {
    let (_d, epub, report) = convert_ok("book.pdf");
    assert_eq!(report.chapters, 2);
    let all = files(&epub);
    let nav = String::from_utf8(
        all.iter()
            .find(|(n, _)| n == "OEBPS/nav.xhtml")
            .unwrap()
            .1
            .clone(),
    )
    .unwrap();
    assert!(
        nav.contains(">Chapter One</a>") && nav.contains(">Chapter Two</a>"),
        "{nav}"
    );
    let opf = String::from_utf8(
        all.iter()
            .find(|(n, _)| n == "OEBPS/content.opf")
            .unwrap()
            .1
            .clone(),
    )
    .unwrap();
    assert!(
        opf.contains("<dc:title>A Small Book</dc:title>")
            && opf.contains("<dc:creator>B. Writer</dc:creator>"),
        "{opf}"
    );
    let html = chapters(&epub);
    assert!(html.contains("<p>India begins the first chapter. It is short, but it is a chapter. The second line ends the first paragraph.</p>"), "{html}");
    assert!(
        !html.contains(">1<") && !html.contains(">2<"),
        "page numbers left in:\n{html}"
    );
}

#[test]
fn bookmarks_are_the_chapters_and_contents() {
    let (_d, epub, report) = convert_ok("outlined.pdf");
    // The title page, then a file per top-level bookmark.
    assert_eq!(report.chapters, 3);
    assert_eq!(report.contents, 3);
    let all = files(&epub);
    let file = |name: &str| {
        String::from_utf8(all.iter().find(|(n, _)| n == name).unwrap().1.clone()).unwrap()
    };
    let nav = file("OEBPS/nav.xhtml");
    let one = at(&nav, ">Part One: Beginnings</a>");
    let two = at(&nav, ">The Second Part</a>");
    let aside = at(&nav, ">An Aside</a>");
    assert!(one < two && two < aside, "{nav}");
    assert!(
        nav[two..aside].contains("<ol>"),
        "the aside nests under its part:\n{nav}"
    );
    assert!(!nav.contains("Loud Words"), "{nav}");
    let part_one = file("OEBPS/chapter-002.xhtml");
    assert!(
        part_one.contains(">Part One: Beginnings</h1><p>Quebec opens"),
        "{part_one}"
    );
    assert!(part_one.contains(">Loud Words</h3>"), "{part_one}");
    let part_two = file("OEBPS/chapter-003.xhtml");
    assert!(part_two.contains(">The Second Part</h1>"), "{part_two}");
    assert!(part_two.contains(">An Aside</h2>"), "{part_two}");
}

#[test]
fn a_scan_is_refused_with_a_reason() {
    let dir = tempfile::tempdir().unwrap();
    match convert(&fixture("scan.pdf"), &dir.path().join("out.epub")) {
        Err(PdfError::Scanned) => {}
        Err(e @ PdfError::NoEngine(_)) => panic!("{e}\nrun tools/fetch-pdfium.sh first"),
        other => panic!("expected Scanned, got {other:?}"),
    }
}

#[test]
fn not_a_pdf_is_unreadable() {
    let dir = tempfile::tempdir().unwrap();
    let bad = dir.path().join("bad.pdf");
    std::fs::write(&bad, b"not a pdf at all").unwrap();
    assert!(matches!(
        convert(&bad, &dir.path().join("o.epub")),
        Err(PdfError::Unreadable(_))
    ));
}

#[test]
fn a_scan_renders_as_page_images_for_a_comic() {
    let dir = tempfile::tempdir().unwrap();
    let scan = match deckle_pdf::render_scan(&fixture("scan.pdf"), dir.path()) {
        Ok(s) => s,
        Err(e @ PdfError::NoEngine(_)) => panic!("{e}\nrun tools/fetch-pdfium.sh first"),
        Err(e) => panic!("{e}"),
    };
    assert_eq!(scan.pages, 1);
    let page =
        image::load_from_memory(&std::fs::read(dir.path().join("p0001.jpg")).unwrap()).unwrap();
    // Rendered at the tallest panel's height, keeping the page's shape.
    assert_eq!(page.height(), 2480);
    assert!(page.width() > 1000 && page.width() < page.height());
}
