// SPDX-License-Identifier: GPL-3.0-or-later
//! Full-text extraction and Google News link decoding against saved pages (no network).

use std::path::Path;

use deckle_digest::Net;
use deckle_digest::article::{extract, is_google_news, placeholder, resolve_google_news};

fn fixture(name: &str) -> String {
    std::fs::read_to_string(
        Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("tests/fixtures")
            .join(name),
    )
    .unwrap()
}

const WXII: &str = "https://www.wxii12.com/article/candor-peach-street-standoff/74194295";

#[test]
fn local_tv_story_extracts_to_clean_xhtml_with_its_lead_photo() {
    let a = extract(&fixture("article-wxii.html"), WXII).expect("extracts");
    assert!(a.words >= 150, "{} words", a.words);
    assert!(
        a.xhtml.contains("welfare check"),
        "{}",
        &a.xhtml[..300.min(a.xhtml.len())]
    );
    assert_eq!(a.byline.as_deref(), Some("Michael Pardee"));
    assert!(!a.photos.is_empty());
    assert!(a.xhtml.starts_with(&format!(
        "<div class=\"figure lead\"><img src=\"{}\"",
        placeholder(0)
    )));
    assert!(a.teaser.as_deref().unwrap().len() >= 80);
    // The whitelist: no scripts, links or attributes beyond ours.
    for banned in ["<script", "<a ", "href=", "style=", "<iframe", "<table"] {
        assert!(!a.xhtml.contains(banned), "{banned} in output");
    }
    // Well-formed once wrapped.
    roxmltree::Document::parse(&format!("<div>{}</div>", a.xhtml)).unwrap();
}

#[test]
fn wire_story_extracts_with_unique_photos() {
    let url = "https://www.npr.org/2026/10/20/g-s1-148975/nigeria-confirms-ebola-case";
    let a = extract(&fixture("article-npr.html"), url).expect("extracts");
    assert!(a.words >= 300, "{} words", a.words);
    assert!(a.xhtml.contains("Ebola"));
    let keys: std::collections::HashSet<_> = a
        .photos
        .iter()
        .map(|p| p.split('?').next().unwrap().to_owned())
        .collect();
    assert_eq!(keys.len(), a.photos.len(), "photos are deduplicated");
    roxmltree::Document::parse(&format!("<div>{}</div>", a.xhtml)).unwrap();
}

#[test]
fn a_stub_page_is_not_a_story() {
    let html = "<html><head><title>Subscribe</title></head><body><p>Subscribe to keep reading.</p></body></html>";
    assert!(extract(html, "https://example.com/a").is_none());
}

struct GoogleFake;

impl Net for GoogleFake {
    fn get(&self, url: &str) -> Result<Vec<u8>, String> {
        Err(format!("offline: {url}"))
    }
    fn get_page(&self, url: &str) -> Result<Vec<u8>, String> {
        assert!(is_google_news(url));
        Ok(fixture("googlenews-interstitial.html").into_bytes())
    }
    fn post_form(&self, url: &str, body: &str) -> Result<Vec<u8>, String> {
        assert!(url.ends_with("/batchexecute"));
        assert!(body.starts_with("f.req=") && body.contains("garturlreq"));
        Ok(fixture("googlenews-batchexecute.txt").into_bytes())
    }
}

#[test]
fn google_news_links_decode_to_the_publisher() {
    let link = "https://news.google.com/rss/articles/CBMid0FVX3lxTE5l?oc=5";
    assert_eq!(
        resolve_google_news(link, &GoogleFake).as_deref(),
        Some(WXII)
    );
    let offline = |u: &str| -> Result<Vec<u8>, String> { Err(format!("offline: {u}")) };
    assert_eq!(resolve_google_news(link, &offline), None);
}

fn long_page(words: usize) -> String {
    let para = "The council met again tonight to argue about the new library and its long overdue roof repairs. ";
    let per = para.split_whitespace().count();
    let body: String = (0..words / per).map(|_| format!("<p>{para}</p>")).collect();
    format!(
        "<html><head><title>Long</title></head><body><article><h1>Long</h1>{body}</article></body></html>"
    )
}

#[test]
fn long_read_is_not_capped() {
    let html = long_page(9000);
    let full = deckle_digest::article::extract_with(&html, "https://ex.com/long", None).unwrap();
    assert!(full.words >= 8900, "{} words", full.words);
    let capped = extract(&html, "https://ex.com/long").unwrap();
    assert!(capped.words <= 6000, "{} words", capped.words);
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

#[test]
fn saved_page_photos_from_files_folder_and_data_uri() {
    use base64::Engine;
    let dir = tempfile::tempdir().unwrap();
    std::fs::create_dir_all(dir.path().join("a_files")).unwrap();
    std::fs::write(dir.path().join("a_files/p.png"), png(400, 300)).unwrap();
    let data = base64::engine::general_purpose::STANDARD.encode(png(500, 260));
    let words = "word ".repeat(200);
    let html = format!(
        "<html><body><article><p>{words}</p><img src=\"a_files/p.png\"><p>{words}</p><img src=\"data:image/png;base64,{data}\"></article></body></html>"
    );
    let page = dir.path().join("a.html");
    std::fs::write(&page, &html).unwrap();
    let base = format!("file://{}", page.display());
    let x = deckle_digest::article::extract_with(&html, &base, None).unwrap();
    let offline = |u: &str| -> Result<Vec<u8>, String> { Err(format!("offline: {u}")) };
    let got = deckle_digest::article::fetch_photos(&x.photos, 4, &offline);
    assert_eq!(
        got.iter().flatten().count(),
        2,
        "photos: {:?}",
        x.photos
            .iter()
            .map(|p| &p[..40.min(p.len())])
            .collect::<Vec<_>>()
    );
}

#[test]
fn a_lazy_image_uses_its_real_source_over_a_data_placeholder() {
    let words = "word ".repeat(200);
    let html = format!(
        "<html><body><article><p>{words}</p><img src=\"data:image/svg+xml;utf8,%3Csvg/%3E\" data-src=\"https://cdn.ex.com/real.jpg\"><p>{words}</p></article></body></html>"
    );
    let x = deckle_digest::article::extract_with(&html, "https://ex.com/a", None).unwrap();
    assert!(
        x.photos.iter().any(|p| p == "https://cdn.ex.com/real.jpg"),
        "{:?}",
        x.photos
    );
    assert!(
        x.photos.iter().all(|p| !p.starts_with("data:")),
        "{:?}",
        x.photos
    );
}
