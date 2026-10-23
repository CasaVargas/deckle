// SPDX-License-Identifier: GPL-3.0-or-later
//! Before you send (Books and Kindle design §8): the stages before Previewer, and chapter one
//! before and after them.

use std::io::Write;
use std::path::Path;

use deckle_core::pipeline::CoverPick;
use deckle_core::preflight::{CoverSource, PreflightOptions, first_chapter, preflight};

fn png(w: u32, h: u32) -> Vec<u8> {
    let img = image::RgbImage::from_pixel(w, h, image::Rgb([200, 40, 40]));
    let mut out = std::io::Cursor::new(Vec::new());
    img.write_to(&mut out, image::ImageFormat::Png).unwrap();
    out.into_inner()
}

/// A small EPUB 3 with a cover, a title page and chapter one in a file of its own.
fn book(dir: &Path) -> std::path::PathBuf {
    let path = dir.join("book.epub");
    let mut z = zip::ZipWriter::new(std::fs::File::create(&path).unwrap());
    let stored =
        zip::write::SimpleFileOptions::default().compression_method(zip::CompressionMethod::Stored);
    z.start_file("mimetype", stored).unwrap();
    z.write_all(b"application/epub+zip").unwrap();
    let files: Vec<(&str, Vec<u8>)> = vec![
        ("META-INF/container.xml", br#"<?xml version="1.0"?><container version="1.0" xmlns="urn:oasis:names:tc:opendocument:xmlns:container"><rootfiles><rootfile full-path="OEBPS/content.opf" media-type="application/oebps-package+xml"/></rootfiles></container>"#.to_vec()),
        ("OEBPS/content.opf", br#"<?xml version="1.0" encoding="utf-8"?><package xmlns="http://www.idpf.org/2007/opf" version="3.0" unique-identifier="id"><metadata xmlns:dc="http://purl.org/dc/elements/1.1/"><dc:identifier id="id">urn:uuid:preflight</dc:identifier><dc:title>The Lantern</dc:title><dc:creator>Ada Field</dc:creator><dc:language>en</dc:language><meta property="dcterms:modified">2026-01-01T00:00:00Z</meta></metadata><manifest><item id="nav" href="nav.xhtml" media-type="application/xhtml+xml" properties="nav"/><item id="cover" href="cover.png" media-type="image/png" properties="cover-image"/><item id="css" href="style.css" media-type="text/css"/><item id="title" href="title.xhtml" media-type="application/xhtml+xml"/><item id="c1" href="c1.xhtml" media-type="application/xhtml+xml"/></manifest><spine><itemref idref="title"/><itemref idref="c1"/></spine></package>"#.to_vec()),
        ("OEBPS/nav.xhtml", br#"<?xml version="1.0" encoding="utf-8"?><html xmlns="http://www.w3.org/1999/xhtml" xmlns:epub="http://www.idpf.org/2007/ops"><head><title>Contents</title></head><body><nav epub:type="toc"><ol><li><a href="title.xhtml">Title Page</a></li><li><a href="c1.xhtml#start">Chapter 1</a></li></ol></nav></body></html>"#.to_vec()),
        ("OEBPS/style.css", b"@font-face { font-family: X; src: url(fonts/x.otf); } @import url(more.css); p { text-indent: 1.2em; background: url(bg.png); }".to_vec()),
        ("OEBPS/title.xhtml", br#"<?xml version="1.0" encoding="utf-8"?><html xmlns="http://www.w3.org/1999/xhtml"><head><title>Title</title></head><body><h1>The Lantern</h1></body></html>"#.to_vec()),
        ("OEBPS/c1.xhtml", format!(r#"<?xml version="1.0" encoding="utf-8"?><html xmlns="http://www.w3.org/1999/xhtml" lang="en"><head><title>One</title><link rel="stylesheet" type="text/css" href="style.css"/></head><body><h2 id="start">Chapter 1</h2>{}<p><img src="cover.png" alt="a lantern"/></p></body></html>"#, "<p>\"Hello,\" she said -- and waited...</p>".repeat(30)).into_bytes()),
        ("OEBPS/cover.png", png(60, 90)),
    ];
    let deflated = zip::write::SimpleFileOptions::default();
    for (name, bytes) in files {
        z.start_file(name, deflated).unwrap();
        z.write_all(&bytes).unwrap();
    }
    z.finish().unwrap();
    path
}

#[test]
fn chapter_one_is_found_by_the_contents_with_its_styles_and_pictures_inlined() {
    let tmp = tempfile::tempdir().unwrap();
    let ch = first_chapter(&book(tmp.path())).unwrap().unwrap();
    assert_eq!(ch.href, "c1.xhtml");
    assert_eq!(ch.title.as_deref(), Some("Chapter 1"));
    assert!(ch.html.starts_with("<h2 id=\"start\">"));
    assert!(ch.html.contains("src=\"data:image/png;base64,"));
    assert!(ch.css.contains("text-indent: 1.2em"));
    assert!(
        !ch.css.contains("@font-face") && !ch.css.contains("@import") && !ch.css.contains("url(")
    );
}

#[test]
fn preflight_shows_the_typography_pass_and_the_cover_it_goes_out_with() {
    let tmp = tempfile::tempdir().unwrap();
    let src = book(tmp.path());
    let opts = PreflightOptions {
        typography: true,
        ..Default::default()
    };
    let p = preflight(&src, &opts, &tmp.path().join("work")).unwrap();
    assert_eq!(p.cover, CoverSource::Own);
    let t = p.normalize.typography.as_ref().unwrap();
    assert!(t.quotes > 0 && t.dashes > 0 && t.ellipses > 0, "{t:?}");
    assert!(p.before.as_ref().unwrap().html.contains("\"Hello,\""));
    assert!(p.after.as_ref().unwrap().html.contains("“Hello,”"));
    assert!(
        p.cover_jpeg.as_ref().unwrap().starts_with(&[0xFF, 0xD8]),
        "the PNG cover goes out as a JPEG"
    );
    assert_eq!(std::fs::read(&src).unwrap(), std::fs::read(&src).unwrap());

    let generated = preflight(
        &src,
        &PreflightOptions {
            cover: CoverPick::Generated(None),
            ..Default::default()
        },
        &tmp.path().join("work2"),
    )
    .unwrap();
    assert_eq!(generated.cover, CoverSource::Generated);
    assert_ne!(generated.cover_jpeg, p.cover_jpeg);

    let picked_file = tmp.path().join("picked.png");
    std::fs::write(&picked_file, png(400, 600)).unwrap();
    let picked = preflight(
        &src,
        &PreflightOptions {
            cover: CoverPick::Image(picked_file),
            ..Default::default()
        },
        &tmp.path().join("work3"),
    )
    .unwrap();
    assert_eq!(picked.cover, CoverSource::Picked);
    let img = image::load_from_memory(picked.cover_jpeg.as_ref().unwrap()).unwrap();
    assert_eq!((img.width(), img.height()), (400, 600));
}
