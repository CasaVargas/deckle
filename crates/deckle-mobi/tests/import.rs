// SPDX-License-Identifier: GPL-3.0-or-later
mod common;

use std::collections::BTreeMap;
use std::io::Read;
use std::path::{Path, PathBuf};

use deckle_mobi::fixture::{Book, Compression, Mobi6, build, build_mobi6};
use deckle_mobi::{ImportReport, MobiError, is_mobi_file, to_epub};

type Files = BTreeMap<String, Vec<u8>>;

fn fixtures() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../fixtures/mobi")
}

fn import(bytes: &[u8]) -> (Files, ImportReport) {
    let dir = tempfile::tempdir().unwrap();
    let out = dir.path().join("book.epub");
    let report = to_epub(bytes, &out, "stem").unwrap();
    let mut z = zip::ZipArchive::new(std::fs::File::open(&out).unwrap()).unwrap();
    let files = (0..z.len())
        .map(|i| {
            let mut f = z.by_index(i).unwrap();
            let mut b = Vec::new();
            f.read_to_end(&mut b).unwrap();
            (f.name().unwrap().to_string(), b)
        })
        .collect();
    (files, report)
}

fn refuse(bytes: &[u8]) -> MobiError {
    let dir = tempfile::tempdir().unwrap();
    let out = dir.path().join("book.epub");
    let err = to_epub(bytes, &out, "stem").expect_err("refused");
    assert!(!out.exists(), "nothing is written for a refused book");
    err
}

fn text(files: &Files, name: &str) -> String {
    String::from_utf8(files[name].clone()).unwrap()
}

#[test]
fn recognises_kindle_extensions() {
    for ok in ["a.mobi", "a.AZW3", "a.azw", "a.prc"] {
        assert!(is_mobi_file(Path::new(ok)), "{ok}");
    }
    for no in ["a.kfx", "a.pdb", "a.azw1", "a.epub"] {
        assert!(!is_mobi_file(Path::new(no)), "{no}");
    }
}

#[test]
fn committed_fixtures_match_their_builders() {
    for (name, bytes) in [
        ("mini6.mobi", common::mini6(Compression::PalmDoc)),
        ("huff.mobi", common::mini6(Compression::Huff)),
        ("mini.azw3", common::mini_azw3()),
    ] {
        let committed = std::fs::read(fixtures().join(name)).unwrap();
        assert!(committed == bytes, "{name} is stale; run write_fixtures");
    }
}

#[test]
#[ignore = "writes fixtures/mobi; run explicitly after changing the builders"]
fn write_fixtures() {
    std::fs::create_dir_all(fixtures()).unwrap();
    std::fs::write(
        fixtures().join("mini6.mobi"),
        common::mini6(Compression::PalmDoc),
    )
    .unwrap();
    std::fs::write(
        fixtures().join("huff.mobi"),
        common::mini6(Compression::Huff),
    )
    .unwrap();
    std::fs::write(fixtures().join("mini.azw3"), common::mini_azw3()).unwrap();
}

#[test]
fn mobi6_parts_split_at_page_breaks_with_links_both_ways() {
    let (files, report) = import(&common::mini6(Compression::None));
    assert_eq!(
        (report.title.as_str(), report.parts, report.images),
        ("The Deckle MOBI Mini", 3, 2)
    );
    assert!(report.warnings.is_empty(), "{:?}", report.warnings);
    let p0 = text(&files, "OEBPS/Text/part0000.xhtml");
    let p1 = text(&files, "OEBPS/Text/part0001.xhtml");
    let p2 = text(&files, "OEBPS/Text/part0002.xhtml");
    let id = |p: &str, h: &str| {
        let at = p.find(h).unwrap();
        let s = p[..at].rfind("id=\"filepos").unwrap() + 4;
        p[s..s + p[s..].find('"').unwrap()].to_owned()
    };
    let one = id(&p1, "<h2");
    let two = id(&p2, "<h2");
    assert!(
        p0.contains(&format!(
            "<a href=\"part0002.xhtml#{two}\">the second chapter</a>"
        )),
        "{p0}"
    );
    assert!(
        p2.contains(&format!("<a href=\"part0001.xhtml#{one}\">chapter one</a>")),
        "{p2}"
    );
    assert!(p2.contains("<a href=\"https://example.com/press\">the press</a>"));
    for p in [&p0, &p1, &p2] {
        roxmltree::Document::parse(p).unwrap();
        assert!(!p.contains("filepos="), "{p}");
    }
}

#[test]
fn mobi6_loose_markup_becomes_whitelisted_xhtml() {
    let (files, _) = import(&common::mini6(Compression::None));
    let p0 = text(&files, "OEBPS/Text/part0000.xhtml");
    assert!(
        p0.contains("<h1 class=\"center\" id=\"h1\">The Deckle MOBI Mini</h1>"),
        "{p0}"
    );
    let p1 = text(&files, "OEBPS/Text/part0001.xhtml");
    assert!(p1.contains(
        "<p>Unquoted <b>bold</b> and <i>italic</i> text<br/>with a break.</p><p class=\"right\">Right aligned</p><div class=\"center\">Centred</div>"
    ), "{p1}");
    assert!(p1.contains("<p><img src=\"../Images/image00001.png\" alt=\"A grey plate\"/></p>"));
    let p2 = text(&files, "OEBPS/Text/part0002.xhtml");
    assert!(!p2.contains("alert"), "scripts go");
    assert!(p2.contains("<td colspan=\"2\">Wide cell</td>"));
}

#[test]
fn mobi6_metadata_cover_and_contents_from_the_ncx() {
    let (files, _) = import(&common::mini6(Compression::None));
    let opf = text(&files, "OEBPS/content.opf");
    assert!(opf.contains("<dc:title>The Deckle MOBI Mini</dc:title>"));
    assert!(opf.contains("<dc:creator>Ada Testwright</dc:creator>"));
    assert!(
        opf.contains(
            "href=\"Images/image00002.png\" media-type=\"image/png\" properties=\"cover-image\""
        ),
        "{opf}"
    );
    let nav = text(&files, "OEBPS/nav.xhtml");
    assert!(
        nav.contains(">Chapter One</a>") && nav.contains(">Chapter Two</a>"),
        "{nav}"
    );
    assert!(nav.contains("href=\"Text/part0001.xhtml#filepos"));
    assert!(nav.contains("href=\"Text/part0002.xhtml#filepos"));
}

#[test]
fn without_an_ncx_the_headings_are_the_contents() {
    let plate = common::png(2, 2, [0, 0, 0]);
    let bytes = build_mobi6(&Mobi6 {
        title: "No Index",
        html: common::MINI6_HTML,
        images: &[&plate],
        ..Default::default()
    });
    let (files, _) = import(&bytes);
    let nav = text(&files, "OEBPS/nav.xhtml");
    assert!(nav.contains(
        "<li><a href=\"Text/part0000.xhtml#h1\">The Deckle MOBI Mini</a><ol><li><a href=\"Text/part0001.xhtml#h2\">Chapter One</a></li><li><a href=\"Text/part0002.xhtml#h3\">Chapter Two</a></li></ol></li>"
    ), "{nav}");
}

#[test]
fn palmdoc_and_huff_cdic_text_read_the_same_book() {
    let (plain, _) = import(&common::mini6(Compression::None));
    let (palmdoc, _) = import(&common::mini6(Compression::PalmDoc));
    let (huff, _) = import(&common::mini6(Compression::Huff));
    let parts = |f: &Files| -> Vec<Vec<u8>> {
        f.iter()
            .filter(|(k, _)| k.contains("Text/"))
            .map(|(_, v)| v.clone())
            .collect()
    };
    assert_eq!(parts(&plain), parts(&palmdoc));
    assert_eq!(parts(&plain), parts(&huff));
}

#[test]
fn a_plain_palmdoc_file_becomes_paragraphs() {
    let bytes = build_mobi6(&Mobi6 {
        title: "Old Palm Text",
        html: "First line\nsecond line\n\nA new paragraph & more.\n",
        plain: true,
        ..Default::default()
    });
    let (files, report) = import(&bytes);
    assert_eq!(report.title, "Old Palm Text");
    assert!(
        text(&files, "OEBPS/Text/part0000.xhtml")
            .contains("<p>First line<br/>second line</p><p>A new paragraph &amp; more.</p>")
    );
}

#[test]
fn a_kf8_book_goes_through_the_existing_rebuild() {
    let (files, report) = import(&common::mini_azw3());
    assert_eq!(
        (report.title.as_str(), report.parts),
        ("The Deckle AZW3 Mini", 3)
    );
    let one = text(&files, "OEBPS/Text/part0001.xhtml");
    assert!(
        one.contains("<a href=\"part0002.xhtml#c2\">dark</a>"),
        "{one}"
    );
}

#[test]
fn missing_pictures_warn_and_keep_the_text() {
    let bytes = build_mobi6(&Mobi6 {
        title: "Gaps",
        html: "<html><body><p>Before <img recindex=\"00007\"> after.</p></body></html>",
        ..Default::default()
    });
    let (files, report) = import(&bytes);
    assert!(text(&files, "OEBPS/Text/part0000.xhtml").contains("<p>Before after.</p>"));
    assert!(
        report
            .warnings
            .iter()
            .any(|w| w.contains("picture record 7 is missing")),
        "{:?}",
        report.warnings
    );
}

#[test]
fn drm_fixed_layout_kfx_and_other_files_are_refused() {
    let drm6 = build_mobi6(&Mobi6 {
        title: "Store",
        html: "<p>Secret</p>",
        drm: true,
        ..Default::default()
    });
    assert!(matches!(refuse(&drm6), MobiError::Drm));
    let drm8 = build(&Book {
        title: "Store",
        drm: true,
        kf8: true,
        ..Default::default()
    });
    let err = refuse(&drm8);
    assert!(matches!(err, MobiError::Drm));
    assert!(err.to_string().contains("never removes DRM"));
    let fixed = build(&Book {
        title: "Comic",
        fixed_layout: true,
        kf8: true,
        ..Default::default()
    });
    assert!(matches!(refuse(&fixed), MobiError::Unsupported(w) if w.contains("fixed-layout")));
    assert!(
        matches!(refuse(b"CONT\x02\x00 a kfx container"), MobiError::Unsupported(w) if w.contains(".kfx"))
    );
    assert!(matches!(
        refuse(b"PK\x03\x04 an epub"),
        MobiError::NotMobi(_)
    ));
}

#[test]
fn import_is_deterministic() {
    assert_eq!(
        import(&common::mini6(Compression::Huff)).0,
        import(&common::mini6(Compression::Huff)).0
    );
}
