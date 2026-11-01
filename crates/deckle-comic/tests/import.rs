// SPDX-License-Identifier: GPL-3.0-or-later
use std::collections::BTreeMap;
use std::io::{Cursor, Read, Write};
use std::path::Path;

use deckle_comic::{ComicError, ImportReport, convert, is_cbz, is_comic, is_image_folder};

fn image(w: u32, h: u32, format: image::ImageFormat) -> Vec<u8> {
    let img = image::RgbImage::from_pixel(w, h, image::Rgb([120, 120, 120]));
    let mut out = Cursor::new(Vec::new());
    img.write_to(&mut out, format).unwrap();
    out.into_inner()
}

fn png(w: u32, h: u32) -> Vec<u8> {
    image(w, h, image::ImageFormat::Png)
}

fn jpeg(w: u32, h: u32) -> Vec<u8> {
    image(w, h, image::ImageFormat::Jpeg)
}

/// A CBZ at `<dir>/<name>` holding `files`.
fn cbz(dir: &Path, name: &str, files: &[(&str, Vec<u8>)]) -> std::path::PathBuf {
    let path = dir.join(name);
    let mut zw = zip::ZipWriter::new(std::fs::File::create(&path).unwrap());
    for (n, bytes) in files {
        zw.start_file(*n, zip::write::SimpleFileOptions::default())
            .unwrap();
        zw.write_all(bytes).unwrap();
    }
    zw.finish().unwrap();
    path
}

type Files = BTreeMap<String, Vec<u8>>;

fn read_epub(path: &Path) -> Files {
    let mut z = zip::ZipArchive::new(std::fs::File::open(path).unwrap()).unwrap();
    assert_eq!(z.by_index(0).unwrap().name().unwrap(), "mimetype");
    (0..z.len())
        .map(|i| {
            let mut f = z.by_index(i).unwrap();
            let mut b = Vec::new();
            f.read_to_end(&mut b).unwrap();
            (f.name().unwrap().into_owned(), b)
        })
        .collect()
}

fn text(files: &Files, name: &str) -> String {
    String::from_utf8(files[name].clone()).unwrap()
}

fn import(source: &Path) -> (Files, ImportReport) {
    let out = source.with_extension("out.epub");
    let report = convert(source, &out).unwrap();
    (read_epub(&out), report)
}

/// The image each spine page shows, in spine order.
fn spine_images(files: &Files) -> Vec<String> {
    let opf = text(files, "OEBPS/content.opf");
    let doc = roxmltree::Document::parse(&opf).unwrap();
    let href = |id: &str| {
        doc.descendants()
            .find(|n| n.has_tag_name("item") && n.attribute("id") == Some(id))
            .and_then(|n| n.attribute("href"))
            .unwrap()
            .to_owned()
    };
    doc.descendants()
        .filter(|n| n.has_tag_name("itemref"))
        .map(|r| {
            let page = text(
                files,
                &format!("OEBPS/{}", href(r.attribute("idref").unwrap())),
            );
            let src = page.split("src=\"../").nth(1).unwrap();
            src[..src.find('"').unwrap()].to_owned()
        })
        .collect()
}

#[test]
fn pages_come_out_in_natural_order_as_a_fixed_layout_epub() {
    let dir = tempfile::tempdir().unwrap();
    let src = cbz(
        dir.path(),
        "My Comic.cbz",
        &[
            ("page10.png", png(30, 40)),
            ("page2.jpg", jpeg(20, 30)),
            ("page1.png", png(10, 20)),
            ("__MACOSX/._page1.png", b"junk".to_vec()),
            (".DS_Store", b"junk".to_vec()),
            ("notes.txt", b"scanned by someone".to_vec()),
        ],
    );
    let (files, report) = import(&src);
    assert_eq!(report.title, "My Comic");
    assert_eq!(report.pages, 3);
    assert!(!report.right_to_left);
    assert!(report.warnings.is_empty(), "{:?}", report.warnings);

    // page1 (10×20 PNG), page2 (20×30 JPEG), page10 (30×40 PNG): sizes give the order away.
    assert_eq!(
        spine_images(&files),
        ["images/p0001.png", "images/p0002.jpg", "images/p0003.png"]
    );
    assert!(text(&files, "OEBPS/pages/p0002.xhtml").contains("content=\"width=20, height=30\""));
    assert!(text(&files, "OEBPS/pages/p0003.xhtml").contains("content=\"width=30, height=40\""));
    // Copied as they were.
    assert_eq!(files["OEBPS/images/p0002.jpg"], jpeg(20, 30));

    let opf = text(&files, "OEBPS/content.opf");
    roxmltree::Document::parse(&opf).unwrap();
    assert!(opf.contains("<meta property=\"rendition:layout\">pre-paginated</meta>"));
    assert!(opf.contains("<meta name=\"book-type\" content=\"comic\"/>"));
    assert!(opf.contains("<meta name=\"original-resolution\" content=\"10x20\"/>"));
    assert!(opf.contains(
        "<item id=\"img-p0001\" href=\"images/p0001.png\" media-type=\"image/png\" properties=\"cover-image\"/>"
    ));
    assert!(opf.contains("<spine><itemref"));
    assert!(!opf.contains("horizontal-rl"));
    assert_eq!(files.len(), 4 + 3 + 3, "{:?}", files.keys());
}

#[test]
fn manga_reads_right_to_left_with_comicinfo_metadata() {
    let dir = tempfile::tempdir().unwrap();
    let info = "<?xml version=\"1.0\"?><ComicInfo><Series>Akira &amp; Co</Series><Number>3</Number>\
        <Writer>Katsuhiro Otomo</Writer><LanguageISO>ja</LanguageISO>\
        <Manga>YesAndRightToLeft</Manga></ComicInfo>";
    let src = cbz(
        dir.path(),
        "akira-03.cbz",
        &[
            ("Akira 03/ComicInfo.xml", info.as_bytes().to_vec()),
            ("Akira 03/001.jpg", jpeg(16, 24)),
            ("Akira 03/002.jpg", jpeg(16, 24)),
        ],
    );
    let (files, report) = import(&src);
    assert_eq!(report.title, "Akira & Co 3");
    assert!(report.right_to_left);
    let opf = text(&files, "OEBPS/content.opf");
    roxmltree::Document::parse(&opf).unwrap();
    assert!(opf.contains("<spine page-progression-direction=\"rtl\">"));
    assert!(opf.contains("<meta name=\"primary-writing-mode\" content=\"horizontal-rl\"/>"));
    assert!(opf.contains("<dc:creator>Katsuhiro Otomo</dc:creator>"));
    assert!(opf.contains("<dc:language>ja</dc:language>"));
    assert!(opf.contains("<meta name=\"calibre:series\" content=\"Akira &amp; Co\"/>"));
    assert!(opf.contains("<meta name=\"calibre:series_index\" content=\"3\"/>"));
    assert_eq!(spine_images(&files).len(), 2);
}

#[test]
fn the_identifier_is_stable_across_runs() {
    let dir = tempfile::tempdir().unwrap();
    let files = [("1.png", png(4, 4)), ("2.png", png(4, 4))];
    let a = import(&cbz(dir.path(), "a.cbz", &files)).0;
    let b = import(&cbz(dir.path(), "a2.cbz", &files)).0;
    let id = |f: &Files| {
        let opf = text(f, "OEBPS/content.opf");
        opf.split("urn:deckle:comic:").nth(1).unwrap()[..16].to_owned()
    };
    // Same pages, different file names: different titles, so different ids…
    assert_ne!(id(&a), id(&b));
    // …and the same file twice gives the same id.
    assert_eq!(id(&a), id(&import(&cbz(dir.path(), "a.cbz", &files)).0));
}

#[test]
fn a_folder_of_images_converts_like_a_cbz() {
    let dir = tempfile::tempdir().unwrap();
    let book = dir.path().join("Picture Book");
    std::fs::create_dir_all(book.join("Chapter 10")).unwrap();
    std::fs::create_dir_all(book.join("Chapter 9")).unwrap();
    std::fs::write(book.join("Chapter 9/1.png"), png(8, 8)).unwrap();
    std::fs::write(book.join("Chapter 10/1.png"), png(9, 9)).unwrap();
    std::fs::write(book.join(".hidden.png"), png(1, 1)).unwrap();
    assert!(is_image_folder(&book) && is_comic(&book) && !is_cbz(&book));

    let out = dir.path().join("out.epub");
    let report = convert(&book, &out).unwrap();
    assert_eq!((report.title.as_str(), report.pages), ("Picture Book", 2));
    let files = read_epub(&out);
    assert!(text(&files, "OEBPS/pages/p0001.xhtml").contains("width=8, height=8"));
    assert!(text(&files, "OEBPS/pages/p0002.xhtml").contains("width=9, height=9"));
}

#[test]
fn a_notes_folder_or_an_empty_one_is_not_a_comic() {
    let dir = tempfile::tempdir().unwrap();
    let notes = dir.path().join("notes");
    std::fs::create_dir(&notes).unwrap();
    std::fs::write(notes.join("a.md"), "# Hi").unwrap();
    std::fs::write(notes.join("photo.jpg"), jpeg(2, 2)).unwrap();
    assert!(!is_image_folder(&notes));
    let empty = dir.path().join("empty");
    std::fs::create_dir(&empty).unwrap();
    assert!(!is_image_folder(&empty));
    // A shelf of books with a stray image is walked for its books, not read as a comic.
    let shelf = dir.path().join("shelf");
    std::fs::create_dir(&shelf).unwrap();
    std::fs::write(shelf.join("cover.jpg"), jpeg(2, 2)).unwrap();
    std::fs::write(shelf.join("Night Train.cbz"), b"zip").unwrap();
    assert!(!is_image_folder(&shelf));
    assert!(is_cbz(Path::new("x/Y.CBZ")) && !is_cbz(Path::new("y.zip")));
}

#[test]
fn bad_comics_are_refused_with_their_own_message() {
    let dir = tempfile::tempdir().unwrap();
    let out = dir.path().join("out.epub");
    let refuse = |src: &Path| convert(src, &out).unwrap_err();

    let empty = cbz(
        dir.path(),
        "empty.cbz",
        &[("ComicInfo.xml", b"<ComicInfo/>".to_vec())],
    );
    assert!(matches!(refuse(&empty), ComicError::NoPages));

    let corrupt = cbz(
        dir.path(),
        "corrupt.cbz",
        &[("1.png", png(4, 4)), ("2.png", b"not an image".to_vec())],
    );
    assert!(matches!(refuse(&corrupt), ComicError::BadPage(p) if p == "2.png"));

    let truncated = cbz(
        dir.path(),
        "truncated.cbz",
        &[("1.png", png(4, 4)[..12].to_vec())],
    );
    assert!(matches!(refuse(&truncated), ComicError::BadPage(_)));

    let webp = cbz(
        dir.path(),
        "webp.cbz",
        &[("1.webp", b"RIFF\0\0\0\0WEBPVP8 ".to_vec())],
    );
    let e = refuse(&webp);
    assert!(
        matches!(&e, ComicError::UnsupportedPage { page, format } if page == "1.webp" && format == "WEBP"),
        "{e}"
    );
    assert!(e.to_string().contains("convert it to JPEG or PNG"));

    let not_zip = dir.path().join("fake.cbz");
    std::fs::write(&not_zip, b"Rar!\x1a\x07\x00").unwrap();
    assert!(matches!(refuse(&not_zip), ComicError::NotZip(_)));
}
