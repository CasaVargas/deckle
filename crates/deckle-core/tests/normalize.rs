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
