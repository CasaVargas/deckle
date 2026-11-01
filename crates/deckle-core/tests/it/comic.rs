// SPDX-License-Identifier: GPL-3.0-or-later
//! A comic (CBZ or folder of page images) through import, inspect and normalize (fixed-layout
//! spec §11).

use std::io::{Cursor, Read, Write};
use std::path::{Path, PathBuf};

use deckle_core::import::{import, needs_import};
use deckle_core::inspect::inspect;

fn png(w: u32, h: u32) -> Vec<u8> {
    let img = image::RgbImage::from_pixel(w, h, image::Rgb([90, 90, 90]));
    let mut out = Cursor::new(Vec::new());
    img.write_to(&mut out, image::ImageFormat::Png).unwrap();
    out.into_inner()
}

/// A 3-page manga CBZ (pages named so that only natural order gets them right).
fn manga(dir: &Path) -> PathBuf {
    let path = dir.join("manga.cbz");
    let mut zw = zip::ZipWriter::new(std::fs::File::create(&path).unwrap());
    let files: [(&str, Vec<u8>); 4] = [
        (
            "ComicInfo.xml",
            b"<ComicInfo><Title>Night Train</Title><Manga>YesAndRightToLeft</Manga></ComicInfo>"
                .to_vec(),
        ),
        ("page10.png", png(30, 40)),
        ("page2.png", png(20, 30)),
        ("page1.png", png(10, 20)),
    ];
    for (n, bytes) in files {
        zw.start_file(n, zip::write::SimpleFileOptions::default())
            .unwrap();
        zw.write_all(&bytes).unwrap();
    }
    zw.finish().unwrap();
    path
}

fn entries(epub: &[u8]) -> Vec<(String, Vec<u8>)> {
    let mut z = zip::ZipArchive::new(Cursor::new(epub)).unwrap();
    (0..z.len())
        .map(|i| {
            let mut f = z.by_index(i).unwrap();
            let mut b = Vec::new();
            f.read_to_end(&mut b).unwrap();
            (f.name().unwrap().into_owned(), b)
        })
        .collect()
}

#[test]
fn a_cbz_imports_as_a_fixed_layout_right_to_left_book() {
    let dir = tempfile::tempdir().unwrap();
    let src = manga(dir.path());
    assert!(needs_import(&src));
    let imported = import(&src, dir.path(), "imported.epub")
        .expect("a comic is importable")
        .unwrap();
    let info = inspect(&imported.epub).unwrap();
    assert_eq!(info.title, "Night Train");
    assert!(info.fixed_layout && info.right_to_left);
    assert_eq!(
        info.spine,
        [
            "pages/p0001.xhtml",
            "pages/p0002.xhtml",
            "pages/p0003.xhtml"
        ]
    );
    assert_eq!(info.cover_href.as_deref(), Some("images/p0001.png"));
}

#[test]
fn a_reflowable_book_is_neither_fixed_layout_nor_right_to_left() {
    let info = inspect(&deckle_kfx::fixtures::path("deckle-mini.epub")).unwrap();
    assert!(!info.fixed_layout && !info.right_to_left);
}

#[test]
fn every_spine_item_pre_paginated_counts_as_fixed_layout() {
    let dir = tempfile::tempdir().unwrap();
    let imported = import(&manga(dir.path()), dir.path(), "imported.epub")
        .unwrap()
        .unwrap()
        .epub;
    // The same book with the layout on each itemref instead of in the metadata.
    let mut out =
        zip::ZipWriter::new(std::fs::File::create(dir.path().join("items.epub")).unwrap());
    for (name, bytes) in entries(&std::fs::read(&imported).unwrap()) {
        let bytes = if name == "OEBPS/content.opf" {
            String::from_utf8(bytes)
                .unwrap()
                .replace(
                    "<meta property=\"rendition:layout\">pre-paginated</meta>",
                    "",
                )
                .replace(
                    "<itemref ",
                    "<itemref properties=\"rendition:layout-pre-paginated\" ",
                )
                .into_bytes()
        } else {
            bytes
        };
        out.start_file(name, zip::write::SimpleFileOptions::default())
            .unwrap();
        out.write_all(&bytes).unwrap();
    }
    out.finish().unwrap();
    assert!(
        inspect(&dir.path().join("items.epub"))
            .unwrap()
            .fixed_layout
    );
}

#[test]
fn a_comic_keeps_its_layout_and_every_page_through_the_reader_path() {
    let dir = tempfile::tempdir().unwrap();
    // `book_epub` is what a PocketBook or an EPUB folder gets: import, then normalize.
    let epub = deckle_core::reader::book_epub(&manga(dir.path()), None).unwrap();
    let files = entries(&epub);
    let get = |name: &str| {
        files
            .iter()
            .find(|(n, _)| n == name)
            .map(|(_, b)| String::from_utf8_lossy(b).into_owned())
    };
    let opf = get("OEBPS/content.opf").unwrap();
    assert!(opf.contains("<meta property=\"rendition:layout\">pre-paginated</meta>"));
    assert!(opf.contains("page-progression-direction=\"rtl\""));
    for (i, size) in [
        (1, "width=10, height=20"),
        (2, "width=20, height=30"),
        (3, "width=30, height=40"),
    ] {
        let page = get(&format!("OEBPS/pages/p{i:04}.xhtml")).unwrap();
        assert!(page.contains(size), "page {i}: {page}");
        // Whatever normalize did to the cover, each page's image is still in the book.
        let src = page.split("src=\"../").nth(1).unwrap();
        let src = &src[..src.find('"').unwrap()];
        assert!(
            files.iter().any(|(n, _)| *n == format!("OEBPS/{src}")),
            "page {i} shows {src}, which is missing"
        );
    }
}

#[test]
fn a_folder_of_page_images_imports_but_a_notes_folder_stays_markdown() {
    let dir = tempfile::tempdir().unwrap();
    let pages = dir.path().join("Picture Book");
    std::fs::create_dir(&pages).unwrap();
    std::fs::write(pages.join("01.png"), png(8, 8)).unwrap();
    std::fs::write(pages.join("02.png"), png(8, 8)).unwrap();
    let book = import(&pages, dir.path(), "pages.epub").unwrap().unwrap();
    let info = inspect(&book.epub).unwrap();
    assert!(info.fixed_layout && !info.right_to_left);
    assert_eq!(info.spine.len(), 2);

    let notes = dir.path().join("notes");
    std::fs::create_dir(&notes).unwrap();
    std::fs::write(notes.join("a.md"), "# A note\n\nHello.\n").unwrap();
    std::fs::write(notes.join("photo.png"), png(8, 8)).unwrap();
    let book = import(&notes, dir.path(), "notes.epub").unwrap().unwrap();
    assert!(!inspect(&book.epub).unwrap().fixed_layout);
}
