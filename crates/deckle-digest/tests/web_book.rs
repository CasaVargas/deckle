// SPDX-License-Identifier: GPL-3.0-or-later
//! One web article as its own book (web-article design §4).

use std::io::Read;
use std::path::{Path, PathBuf};
use std::sync::Mutex;

use chrono::NaiveDate;
use deckle_digest::Config;
use deckle_digest::web_book::build;

const URL: &str = "https://ex.com/essays/the-long-one?utm_source=feed";

fn date() -> NaiveDate {
    NaiveDate::from_ymd_opt(2026, 10, 21).unwrap()
}

fn png(w: u32, h: u32) -> Vec<u8> {
    let mut out = Vec::new();
    image::DynamicImage::ImageRgb8(image::RgbImage::from_fn(w, h, |x, y| {
        image::Rgb([(x % 256) as u8, (y % 256) as u8, ((x * y) % 256) as u8])
    }))
    .write_to(&mut std::io::Cursor::new(&mut out), image::ImageFormat::Png)
    .unwrap();
    out
}

/// About `words` words under a byline, with one photo.
fn page(words: usize) -> String {
    let para = "The council met again tonight to argue about the new library and its long overdue roof repairs. ";
    let per = para.split_whitespace().count();
    let body: String = (0..words / per).map(|_| format!("<p>{para}</p>")).collect();
    format!(
        "<html><head><title>The Long One</title><meta name=\"author\" content=\"Ada Writer\"></head>\
         <body><article><h1>The Long One</h1><p class=\"byline\">By Ada Writer</p>\
         <img src=\"https://ex.com/p.png\" width=\"400\" height=\"300\">{body}</article></body></html>"
    )
}

/// Serves the article (9,000 words, past the paper's cap) and its photo; records each URL.
struct Fake(Mutex<Vec<String>>);

impl deckle_digest::Net for Fake {
    fn get(&self, url: &str) -> Result<Vec<u8>, String> {
        self.0.lock().unwrap().push(url.to_owned());
        match url {
            URL => Ok(page(9000).into_bytes()),
            "https://ex.com/p.png" => Ok(png(400, 300)),
            _ => Err(format!("offline: {url}")),
        }
    }
}

fn fake() -> Fake {
    Fake(Mutex::new(Vec::new()))
}

fn entry(epub: &Path, name: &str) -> Option<String> {
    let mut zip = zip::ZipArchive::new(std::fs::File::open(epub).unwrap()).unwrap();
    let mut f = zip.by_name(name).ok()?;
    let mut s = String::new();
    f.read_to_string(&mut s).ok()?;
    Some(s)
}

fn names(epub: &PathBuf) -> Vec<String> {
    let zip = zip::ZipArchive::new(std::fs::File::open(epub).unwrap()).unwrap();
    zip.file_names().map(|n| n.unwrap().into_owned()).collect()
}

#[test]
fn an_address_becomes_a_whole_book_with_its_photo() {
    let out = tempfile::tempdir().unwrap();
    let net = fake();
    let book = build(URL, &Config::default(), &net, out.path(), date()).unwrap();
    assert_eq!(book.title, "The Long One");
    assert!(book.words > 8000, "no word cap: {}", book.words);
    assert!(book.path.starts_with(out.path()));
    assert!(
        book.path
            .file_name()
            .unwrap()
            .to_string_lossy()
            .starts_with("the-long-one-")
    );
    let opf = entry(&book.path, "OEBPS/content.opf").unwrap();
    assert!(opf.contains("<dc:title>The Long One</dc:title>"), "{opf}");
    assert!(opf.contains("urn:deckle:article:"), "{opf}");
    assert!(opf.contains("<dc:publisher>ex.com</dc:publisher>"), "{opf}");
    let text = entry(&book.path, "OEBPS/text.xhtml").unwrap();
    assert!(text.contains("images/p1.jpg"), "photo placed");
    assert!(!text.contains("__DECKLE_"), "no placeholder left");
    assert!(names(&book.path).contains(&"OEBPS/images/p1.jpg".to_owned()));
    // Valid XHTML, as the pipeline's inspect step needs.
    roxmltree::Document::parse(&text).unwrap();
}

#[test]
fn the_same_article_keeps_its_identifier() {
    let out = tempfile::tempdir().unwrap();
    let uid = |dir: &Path| {
        let b = build(URL, &Config::default(), &fake(), dir, date()).unwrap();
        let opf = entry(&b.path, "OEBPS/content.opf").unwrap();
        let start = opf.find("urn:deckle:article:").unwrap();
        opf[start..start + 35].to_owned()
    };
    let first = uid(&out.path().join("a"));
    let other_day = tempfile::tempdir().unwrap();
    assert_eq!(first, uid(other_day.path()));
}

#[test]
fn link_files_name_the_address() {
    let dir = tempfile::tempdir().unwrap();
    let webloc = dir.path().join("Essay.webloc");
    std::fs::write(
        &webloc,
        format!(
            "<?xml version=\"1.0\"?><plist version=\"1.0\"><dict><key>URL</key><string>{}</string></dict></plist>",
            URL.replace('&', "&amp;")
        ),
    )
    .unwrap();
    let url = dir.path().join("Essay.url");
    std::fs::write(&url, format!("[InternetShortcut]\r\nURL={URL}\r\n")).unwrap();
    for f in [webloc, url] {
        let net = fake();
        let book = build(
            &f.to_string_lossy(),
            &Config::default(),
            &net,
            dir.path(),
            date(),
        )
        .unwrap();
        assert_eq!(book.title, "The Long One");
        assert_eq!(net.0.lock().unwrap()[0], URL);
    }
}

#[test]
fn a_saved_page_is_read_from_disk_with_photos_beside_it() {
    let dir = tempfile::tempdir().unwrap();
    std::fs::create_dir_all(dir.path().join("a_files")).unwrap();
    std::fs::write(dir.path().join("a_files/p.png"), png(400, 300)).unwrap();
    let html = page(600).replace("https://ex.com/p.png", "a_files/p.png");
    let saved = dir.path().join("The Long One.html");
    std::fs::write(&saved, html).unwrap();
    let net = fake();
    let book = build(
        &saved.to_string_lossy(),
        &Config::default(),
        &net,
        &dir.path().join("out"),
        date(),
    )
    .unwrap();
    assert!(net.0.lock().unwrap().is_empty(), "nothing fetched");
    assert_eq!(book.title, "The Long One");
    assert!(names(&book.path).contains(&"OEBPS/images/p1.jpg".to_owned()));
    let opf = entry(&book.path, "OEBPS/content.opf").unwrap();
    assert!(
        opf.contains("<dc:publisher>a saved page</dc:publisher>"),
        "{opf}"
    );
}

#[test]
fn a_stub_fails_with_the_reason_and_writes_nothing() {
    let dir = tempfile::tempdir().unwrap();
    let saved = dir.path().join("paywall.html");
    std::fs::write(
        &saved,
        "<html><body><p>Subscribe to keep reading.</p></body></html>",
    )
    .unwrap();
    let out = dir.path().join("out");
    let err = build(
        &saved.to_string_lossy(),
        &Config::default(),
        &fake(),
        &out,
        date(),
    )
    .unwrap_err();
    assert!(err.contains("under 120 words"), "{err}");
    assert!(!out.exists());
}

#[test]
fn an_unreachable_address_says_why() {
    let out = tempfile::tempdir().unwrap();
    let err = build(
        "https://ex.com/gone",
        &Config::default(),
        &fake(),
        out.path(),
        date(),
    )
    .unwrap_err();
    assert!(err.contains("offline"), "{err}");
}
