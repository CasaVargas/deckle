// SPDX-License-Identifier: GPL-3.0-or-later
use std::io::{Read, Write};

use deckle_core::inspect::inspect;
use deckle_core::normalize::{NormalizeOptions, normalize};

fn mini() -> std::path::PathBuf {
    deckle_kfx::fixtures::path("deckle-mini.epub")
}

/// Copies the mini fixture, replacing or adding entries; `None` data removes the entry.
fn variant(edits: &[(&str, Option<&[u8]>)]) -> tempfile::NamedTempFile {
    let src = std::fs::File::open(mini()).unwrap();
    let mut zin = zip::ZipArchive::new(src).unwrap();
    let out = tempfile::Builder::new().suffix(".epub").tempfile().unwrap();
    let mut zout = zip::ZipWriter::new(std::fs::File::create(out.path()).unwrap());
    for i in 0..zin.len() {
        let mut f = zin.by_index(i).unwrap();
        let name = f.name().unwrap().into_owned();
        if edits.iter().any(|(n, _)| *n == name) {
            continue;
        }
        let method = if name == "mimetype" {
            zip::CompressionMethod::Stored
        } else {
            zip::CompressionMethod::Deflated
        };
        zout.start_file(
            &name,
            zip::write::SimpleFileOptions::default().compression_method(method),
        )
        .unwrap();
        std::io::copy(&mut f, &mut zout).unwrap();
    }
    for (name, data) in edits {
        if let Some(d) = data {
            zout.start_file(*name, zip::write::SimpleFileOptions::default())
                .unwrap();
            zout.write_all(d).unwrap();
        }
    }
    zout.finish().unwrap();
    out
}

fn read_entry(path: &std::path::Path, name: &str) -> Vec<u8> {
    let mut z = zip::ZipArchive::new(std::fs::File::open(path).unwrap()).unwrap();
    let mut v = Vec::new();
    z.by_name(name).unwrap().read_to_end(&mut v).unwrap();
    v
}

fn png(w: u32, h: u32) -> Vec<u8> {
    let img = image::RgbImage::from_pixel(w, h, image::Rgb([40, 80, 160]));
    let mut buf = std::io::Cursor::new(Vec::new());
    img.write_to(&mut buf, image::ImageFormat::Png).unwrap();
    buf.into_inner()
}

#[test]
fn mini_cover_becomes_jfif_jpeg_and_book_still_inspects() {
    let out = tempfile::Builder::new().suffix(".epub").tempfile().unwrap();
    let report = normalize(&mini(), out.path(), &NormalizeOptions::default()).unwrap();
    assert!(report.cover_converted_to_jpeg);
    assert_eq!(report.images_downscaled, 0);
    assert_eq!(report.scripts_removed, 0);
    assert!(!report.toc_synthesized);
    let info = inspect(out.path()).unwrap();
    assert_eq!(info.title, "Deckle Mini Fixture");
    assert_eq!(info.spine.len(), 3);
    assert_eq!(info.toc.len(), 3);
    let cover = info.cover_href.unwrap();
    assert!(cover.ends_with(".jpg"), "{cover}");
    let bytes = read_entry(out.path(), &format!("OEBPS/{cover}"));
    assert_eq!(&bytes[..4], &[0xFF, 0xD8, 0xFF, 0xE0], "JFIF header");
    // mimetype stays first and stored
    let mut z = zip::ZipArchive::new(std::fs::File::open(out.path()).unwrap()).unwrap();
    let first = z.by_index(0).unwrap();
    assert_eq!(first.name().unwrap(), "mimetype");
    assert_eq!(first.compression(), zip::CompressionMethod::Stored);
}

#[test]
fn oversized_images_are_downscaled_to_the_limit() {
    let big = png(4000, 1000);
    let f = variant(&[("OEBPS/cover.png", Some(&big))]);
    let out = tempfile::Builder::new().suffix(".epub").tempfile().unwrap();
    let report = normalize(
        f.path(),
        out.path(),
        &NormalizeOptions {
            max_image_px: 1920,
            jpeg_quality: 85,
        },
    )
    .unwrap();
    assert_eq!(report.images_downscaled, 1);
    let info = inspect(out.path()).unwrap();
    let bytes = read_entry(out.path(), &format!("OEBPS/{}", info.cover_href.unwrap()));
    let img = image::load_from_memory(&bytes).unwrap();
    assert_eq!(img.width().max(img.height()), 1920);
}

#[test]
fn scripts_and_remote_resources_are_removed() {
    let chapter = br#"<?xml version="1.0" encoding="utf-8"?><html xmlns="http://www.w3.org/1999/xhtml"><head><title>c</title><script src="x.js"></script><link rel="stylesheet" href="https://cdn.example/style.css"/></head><body><h1 id="h0">Chapter One</h1><p>Text <img src="https://example.com/a.png" alt="a"/> more.</p><script>alert(1)</script></body></html>"#;
    let f = variant(&[("OEBPS/c0.xhtml", Some(chapter))]);
    let out = tempfile::Builder::new().suffix(".epub").tempfile().unwrap();
    let report = normalize(f.path(), out.path(), &NormalizeOptions::default()).unwrap();
    assert_eq!(report.scripts_removed, 2);
    assert_eq!(report.remote_refs_removed, 2);
    let xhtml = String::from_utf8(read_entry(out.path(), "OEBPS/c0.xhtml")).unwrap();
    assert!(!xhtml.contains("<script"), "{xhtml}");
    assert!(!xhtml.contains("https://"), "{xhtml}");
    assert!(xhtml.contains("Chapter One"));
}

#[test]
fn missing_nav_gets_a_synthesized_toc_from_headings() {
    let opf = String::from_utf8(read_entry(&mini(), "OEBPS/content.opf")).unwrap()
        .replace(r#"<item id="nav" href="nav.xhtml" media-type="application/xhtml+xml" properties="nav"/>"#, "");
    assert!(!opf.contains("properties=\"nav\""));
    let f = variant(&[
        ("OEBPS/content.opf", Some(opf.as_bytes())),
        ("OEBPS/nav.xhtml", None),
    ]);
    assert!(inspect(f.path()).unwrap().toc.is_empty());
    let out = tempfile::Builder::new().suffix(".epub").tempfile().unwrap();
    let report = normalize(f.path(), out.path(), &NormalizeOptions::default()).unwrap();
    assert!(report.toc_synthesized);
    let info = inspect(out.path()).unwrap();
    assert_eq!(info.toc.len(), info.spine.len());
    assert_eq!(info.toc[1].title, "Chapter Two");
}

/// Sigil-style layout: OEBPS/Text, OEBPS/Images, OEBPS/Styles, with a cover XHTML page.
fn nested_epub() -> tempfile::NamedTempFile {
    let out = tempfile::Builder::new().suffix(".epub").tempfile().unwrap();
    let mut z = zip::ZipWriter::new(std::fs::File::create(out.path()).unwrap());
    let stored =
        zip::write::SimpleFileOptions::default().compression_method(zip::CompressionMethod::Stored);
    let deflated = zip::write::SimpleFileOptions::default();
    z.start_file("mimetype", stored).unwrap();
    z.write_all(b"application/epub+zip").unwrap();
    z.start_file("META-INF/container.xml", deflated).unwrap();
    z.write_all(br#"<?xml version="1.0"?><container version="1.0" xmlns="urn:oasis:names:tc:opendocument:xmlns:container"><rootfiles><rootfile full-path="OEBPS/content.opf" media-type="application/oebps-package+xml"/></rootfiles></container>"#).unwrap();
    z.start_file("OEBPS/content.opf", deflated).unwrap();
    z.write_all(br#"<?xml version="1.0" encoding="utf-8"?><package xmlns="http://www.idpf.org/2007/opf" version="3.0" unique-identifier="uid"><metadata xmlns:dc="http://purl.org/dc/elements/1.1/"><dc:identifier id="uid">urn:uuid:nested-0001</dc:identifier><dc:title>Nested</dc:title><dc:creator>A. Writer</dc:creator><dc:language>en</dc:language><meta name="cover" content="cover"/></metadata><manifest><item id="nav" href="Text/nav.xhtml" media-type="application/xhtml+xml" properties="nav"/><item id="cover" href="Images/cover.png" media-type="image/png"/><item id="css" href="Styles/style.css" media-type="text/css"/><item id="coverpage" href="Text/cover.xhtml" media-type="application/xhtml+xml"/><item id="c0" href="Text/c0.xhtml" media-type="application/xhtml+xml"/></manifest><spine><itemref idref="coverpage"/><itemref idref="c0"/></spine></package>"#).unwrap();
    z.start_file("OEBPS/Text/nav.xhtml", deflated).unwrap();
    z.write_all(br#"<?xml version="1.0" encoding="utf-8"?><html xmlns="http://www.w3.org/1999/xhtml" xmlns:epub="http://www.idpf.org/2007/ops"><head><title>nav</title></head><body><nav epub:type="toc"><ol><li><a href="c0.xhtml">Chapter One</a></li></ol></nav></body></html>"#).unwrap();
    z.start_file("OEBPS/Text/cover.xhtml", deflated).unwrap();
    z.write_all(br#"<?xml version="1.0" encoding="utf-8"?><html xmlns="http://www.w3.org/1999/xhtml" xmlns:xlink="http://www.w3.org/1999/xlink"><head><title>Cover</title><link rel="stylesheet" href="../Styles/style.css"/></head><body><div class="cover"><img src="../Images/cover.png" alt="cover"/><svg xmlns="http://www.w3.org/2000/svg"><image xlink:href="../Images/cover.png"/></svg></div></body></html>"#).unwrap();
    z.start_file("OEBPS/Text/c0.xhtml", deflated).unwrap();
    z.write_all(br#"<?xml version="1.0" encoding="utf-8"?><html xmlns="http://www.w3.org/1999/xhtml"><head><title>c0</title></head><body><h1>Chapter One</h1><p>Text.</p></body></html>"#).unwrap();
    z.start_file("OEBPS/Styles/style.css", deflated).unwrap();
    z.write_all(b".cover { background: url(../Images/cover.png) no-repeat; } .x { background-image: url('../Images/cover.png'); }").unwrap();
    z.start_file("OEBPS/Images/cover.png", deflated).unwrap();
    z.write_all(&png(600, 900)).unwrap();
    z.finish().unwrap();
    out
}

#[test]
fn nested_layout_cover_rename_rewrites_every_reference() {
    let f = nested_epub();
    let out = tempfile::Builder::new().suffix(".epub").tempfile().unwrap();
    let report = normalize(f.path(), out.path(), &NormalizeOptions::default()).unwrap();
    assert!(report.cover_converted_to_jpeg);
    assert!(report.warnings.is_empty(), "{:?}", report.warnings);
    let mut z = zip::ZipArchive::new(std::fs::File::open(out.path()).unwrap()).unwrap();
    assert!(
        z.by_name("OEBPS/Images/cover.png").is_err(),
        "old cover entry must be gone"
    );
    assert!(z.by_name("OEBPS/Images/cover.jpg").is_ok());
    let cover_page = String::from_utf8(read_entry(out.path(), "OEBPS/Text/cover.xhtml")).unwrap();
    assert!(!cover_page.contains("cover.png"), "{cover_page}");
    assert_eq!(
        cover_page.matches("../Images/cover.jpg").count(),
        2,
        "{cover_page}"
    );
    let css = String::from_utf8(read_entry(out.path(), "OEBPS/Styles/style.css")).unwrap();
    assert!(!css.contains("cover.png"), "{css}");
    assert_eq!(css.matches("../Images/cover.jpg").count(), 2, "{css}");
    let info = inspect(out.path()).unwrap();
    assert_eq!(info.cover_href.as_deref(), Some("Images/cover.jpg"));
    // and no reference anywhere in the book still points at a missing file
    for name in [
        "OEBPS/Text/nav.xhtml",
        "OEBPS/Text/c0.xhtml",
        "OEBPS/content.opf",
    ] {
        let t = String::from_utf8(read_entry(out.path(), name)).unwrap();
        assert!(!t.contains("cover.png"), "{name}: {t}");
    }
}
