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

/// A landscape spread: red on the left half, blue on the right.
fn spread(w: u32, h: u32) -> Vec<u8> {
    let img = image::RgbImage::from_fn(w, h, |x, _| {
        if x < w / 2 {
            image::Rgb([220, 20, 20])
        } else {
            image::Rgb([20, 20, 220])
        }
    });
    let mut out = Cursor::new(Vec::new());
    img.write_to(&mut out, image::ImageFormat::Png).unwrap();
    out.into_inner()
}

/// The middle pixel's colour of each page, in spine order: 'r'ed, 'b'lue or 'g'rey.
fn page_colours(files: &Files) -> String {
    spine_images(files)
        .iter()
        .map(|img| {
            let px = image::load_from_memory(&files[&format!("OEBPS/{img}")])
                .unwrap()
                .to_rgb8();
            match px.get_pixel(px.width() / 2, px.height() / 2).0 {
                [r, _, b] if r > 150 && b < 100 => 'r',
                [r, _, b] if b > 150 && r < 100 => 'b',
                _ => 'g',
            }
        })
        .collect()
}

#[test]
fn a_spread_after_the_cover_becomes_two_pages_in_reading_order() {
    let dir = tempfile::tempdir().unwrap();
    let files = |info: &str| {
        vec![
            ("ComicInfo.xml", info.as_bytes().to_vec()),
            // A landscape cover (a wraparound) stays whole.
            ("00.png", spread(40, 20)),
            ("01.png", png(20, 30)),
            ("02.png", spread(40, 30)),
            ("03.png", png(20, 30)),
        ]
    };
    let (ltr, report) = import(&cbz(dir.path(), "ltr.cbz", &files("<ComicInfo/>")));
    assert_eq!((report.pages, report.spreads), (5, 1));
    // cover (whole, so its middle is the seam: blue), grey, red half, blue half, grey.
    assert_eq!(page_colours(&ltr), "bgrbg");
    assert!(text(&ltr, "OEBPS/pages/p0003.xhtml").contains("width=20, height=30"));
    assert!(text(&ltr, "OEBPS/pages/p0001.xhtml").contains("width=40, height=20"));

    let manga = "<ComicInfo><Manga>YesAndRightToLeft</Manga></ComicInfo>";
    let (rtl, _) = import(&cbz(dir.path(), "rtl.cbz", &files(manga)));
    // Right to left: the spread's right half is read first.
    assert_eq!(page_colours(&rtl), "bgbrg");
}

/// Whether this machine has bsdtar (macOS and Windows do); CBR tests that need it skip without.
fn has_bsdtar() -> bool {
    ["bsdtar", "tar"].iter().any(|t| {
        std::process::Command::new(t)
            .arg("--version")
            .output()
            .is_ok_and(|o| String::from_utf8_lossy(&o.stdout).contains("bsdtar"))
    })
}

#[test]
fn a_cbr_that_is_really_a_zip_reads_without_bsdtar() {
    let dir = tempfile::tempdir().unwrap();
    let zip = cbz(
        dir.path(),
        "x.cbz",
        &[("1.png", png(4, 6)), ("2.png", png(4, 6))],
    );
    let cbr = dir.path().join("Night Train.CBR");
    std::fs::rename(&zip, &cbr).unwrap();
    assert!(deckle_comic::is_cbr(&cbr) && is_comic(&cbr));
    let (_, report) = import(&cbr);
    assert_eq!((report.title.as_str(), report.pages), ("Night Train", 2));
}

#[test]
fn a_real_archive_cbr_is_unpacked_by_bsdtar_and_nothing_escapes_it() {
    if !has_bsdtar() {
        eprintln!("skipped: no bsdtar");
        return;
    }
    let dir = tempfile::tempdir().unwrap();
    let pages = dir.path().join("pages");
    std::fs::create_dir(&pages).unwrap();
    std::fs::write(pages.join("1.png"), png(4, 6)).unwrap();
    std::fs::write(pages.join("2.png"), png(5, 6)).unwrap();
    // A tar under a .cbr name: bsdtar reads it the way it reads RAR.
    let cbr = dir.path().join("Picture Book.cbr");
    let tar = |args: &[&str]| {
        let ok = std::process::Command::new("tar")
            .args(args)
            .status()
            .unwrap()
            .success();
        assert!(ok, "{args:?}");
    };
    tar(&[
        "-cf",
        cbr.to_str().unwrap(),
        "-C",
        pages.to_str().unwrap(),
        "1.png",
        "2.png",
    ]);
    let (files, report) = import(&cbr);
    assert_eq!((report.title.as_str(), report.pages), ("Picture Book", 2));
    assert!(text(&files, "OEBPS/pages/p0002.xhtml").contains("width=5, height=6"));

    // An entry named `../escaped-1.png` is refused, never written outside the temp folder. The
    // tar is written by hand: not every bsdtar build can rename entries (`-s`) to make one.
    let evil = dir.path().join("evil.cbr");
    std::fs::write(&evil, tar_with("../escaped-1.png", &png(4, 6))).unwrap();
    let listed = std::process::Command::new("tar")
        .arg("-tf")
        .arg(&evil)
        .output()
        .unwrap();
    assert_eq!(
        String::from_utf8_lossy(&listed.stdout).trim(),
        "../escaped-1.png",
        "the hand-made tar holds the escaping entry"
    );
    let _ = convert(&evil, &dir.path().join("evil.epub"));
    let tmp = std::env::temp_dir();
    assert!(!tmp.join("escaped-1.png").exists());

    // Not an archive at all: bsdtar's own reason.
    let junk = dir.path().join("junk.cbr");
    std::fs::write(&junk, b"definitely not an archive").unwrap();
    assert!(matches!(
        convert(&junk, &dir.path().join("junk.epub")),
        Err(ComicError::NotArchive(_))
    ));
}

/// A one-entry ustar archive naming `name`, whatever that name is.
fn tar_with(name: &str, data: &[u8]) -> Vec<u8> {
    let mut h = [0u8; 512];
    let put = |h: &mut [u8; 512], at: usize, v: &[u8]| h[at..at + v.len()].copy_from_slice(v);
    put(&mut h, 0, name.as_bytes());
    put(&mut h, 100, b"0000644\0");
    put(&mut h, 108, b"0000000\0");
    put(&mut h, 116, b"0000000\0");
    put(&mut h, 124, format!("{:011o}\0", data.len()).as_bytes());
    put(&mut h, 136, b"00000000000\0");
    put(&mut h, 148, b"        ");
    h[156] = b'0';
    put(&mut h, 257, b"ustar\0");
    put(&mut h, 263, b"00");
    let sum: u32 = h.iter().map(|b| u32::from(*b)).sum();
    put(&mut h, 148, format!("{sum:06o}\0 ").as_bytes());
    let mut out = h.to_vec();
    out.extend(data);
    out.resize(out.len().next_multiple_of(512) + 1024, 0);
    out
}
