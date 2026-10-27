// SPDX-License-Identifier: GPL-3.0-or-later
//! Opt-in illustrations (cookbook design §7.2): what is asked of the image endpoint and what is
//! sent, what a failure keeps, and that a rebuild never draws again. No test touches the network:
//! the transport is a closure.

use std::cell::RefCell;

use base64::Engine;
use chrono::{TimeZone, Utc};
use deckle_digest::cookbook::book::{build, printed_picture};
use deckle_digest::cookbook::illustrate::{self, FAILED, Post, illustrate, prompt};
use deckle_digest::cookbook::store::{
    BookConfig, IllustrationsConfig, ImageKind, Picture, Saved, Store,
};
use deckle_digest::cookbook::{Group, Recipe};

type Call = (String, Vec<(String, String)>, String);

fn blank(title: &str) -> Recipe {
    Recipe {
        title: title.to_owned(),
        description: Some("A velvety soup, secret-description.".to_owned()),
        yield_: None,
        prep: None,
        cook: None,
        total: None,
        ingredients: vec![
            Group {
                heading: None,
                items: vec![
                    "1 butternut squash".to_owned(),
                    "2 tbsp olive oil".to_owned(),
                ],
            },
            Group {
                heading: Some("For the garnish".to_owned()),
                items: vec!["1/4 cup pumpkin seeds".to_owned(), "1 onion".to_owned()],
            },
        ],
        steps: vec![],
        category: vec![],
        cuisine: None,
        author: None,
        image_url: None,
        source_url: "https://kitchen.test/secret-url".to_owned(),
        site_name: "Kitchen Test".to_owned(),
        structured: true,
        body_xhtml: None,
    }
}

fn png(w: u32, h: u32) -> Vec<u8> {
    let img = image::RgbImage::from_fn(w, h, |x, y| {
        image::Rgb([(x % 256) as u8, (y % 256) as u8, 90])
    });
    let mut out = Vec::new();
    image::DynamicImage::ImageRgb8(img)
        .write_to(&mut std::io::Cursor::new(&mut out), image::ImageFormat::Png)
        .unwrap();
    out
}

/// What an image endpoint answers: `{"data":[{"b64_json": …}]}`.
fn reply(bytes: &[u8]) -> String {
    let b64 = base64::engine::general_purpose::STANDARD.encode(bytes);
    format!(r#"{{"data":[{{"b64_json":"{b64}"}}]}}"#)
}

fn jpeg() -> Vec<u8> {
    let img = image::GrayImage::from_pixel(300, 200, image::Luma([120]));
    let mut out = Vec::new();
    image::DynamicImage::ImageLuma8(img)
        .write_to(
            &mut std::io::Cursor::new(&mut out),
            image::ImageFormat::Jpeg,
        )
        .unwrap();
    out
}

struct Fixture {
    _dir: tempfile::TempDir,
    store: Store,
    saved: Saved,
    cfg: IllustrationsConfig,
}

fn fixture() -> Fixture {
    let dir = tempfile::tempdir().unwrap();
    let store = Store::new(dir.path().join("cookbook"));
    let now = Utc.with_ymd_and_hms(2026, 10, 25, 12, 0, 0).unwrap();
    store
        .put(
            blank("Roasted Squash Soup"),
            "https://kitchen.test/secret-url",
            Some("soups"),
            Some("secret-note"),
            now,
        )
        .unwrap();
    let saved = store
        .find("https://kitchen.test/secret-url")
        .unwrap()
        .unwrap();
    let printed = store.image_path(&saved.id, ImageKind::Printed);
    std::fs::create_dir_all(printed.parent().unwrap()).unwrap();
    std::fs::write(&printed, jpeg()).unwrap();
    store.clear_dirty().unwrap();
    Fixture {
        _dir: dir,
        store,
        saved,
        cfg: IllustrationsConfig {
            enabled: true,
            endpoint: "https://images.test/v1".to_owned(),
            model: "engraver-1".to_owned(),
        },
    }
}

/// Runs `illustrate` with a transport that records its calls and answers `answer`.
fn run(
    f: &Fixture,
    cfg: &IllustrationsConfig,
    key: Option<&str>,
    answer: Result<String, String>,
) -> (Result<(), &'static str>, Vec<Call>) {
    let calls: RefCell<Vec<Call>> = RefCell::new(Vec::new());
    let post = |url: &str, headers: &[(String, String)], body: &str| {
        calls
            .borrow_mut()
            .push((url.to_owned(), headers.to_vec(), body.to_owned()));
        answer.clone()
    };
    let result = illustrate(&f.store, &f.saved, cfg, key, &post as Post);
    (result, calls.into_inner())
}

fn ill(f: &Fixture) -> std::path::PathBuf {
    f.store.image_path(&f.saved.id, ImageKind::Ill)
}

#[test]
fn disabled_never_posts() {
    let f = fixture();
    let cfg = IllustrationsConfig {
        enabled: false,
        ..f.cfg.clone()
    };
    let post = |_: &str, _: &[(String, String)], _: &str| -> Result<String, String> {
        panic!("a disabled book must not post")
    };
    assert!(illustrate(&f.store, &f.saved, &cfg, Some("k"), &post as Post).is_err());
    assert!(!ill(&f).exists());
    assert!(!f.store.is_dirty());
}

#[test]
fn body_has_only_title_chapter_keys() {
    let f = fixture();
    let (result, calls) = run(&f, &f.cfg, Some("sk-test"), Ok(reply(&png(300, 200))));
    assert_eq!(result, Ok(()));
    assert_eq!(calls.len(), 1, "one request per action");
    let (url, headers, body) = &calls[0];
    assert_eq!(url, "https://images.test/v1/images/generations");
    let header = |name: &str| {
        headers
            .iter()
            .find(|(k, _)| k.eq_ignore_ascii_case(name))
            .map(|(_, v)| v.as_str())
    };
    assert_eq!(header("authorization"), Some("Bearer sk-test"));
    assert_eq!(header("content-type"), Some("application/json"));

    let v: serde_json::Value = serde_json::from_str(body).unwrap();
    assert_eq!(v["model"], "engraver-1");
    assert_eq!(v["size"], "1536x1024");
    assert_eq!(v["response_format"], "b64_json");
    assert_eq!(v["n"], 1);
    assert_eq!(
        v.as_object().unwrap().len(),
        5,
        "nothing else is sent: {body}"
    );
    let p = v["prompt"].as_str().unwrap();
    assert!(p.contains("Roasted Squash Soup"), "{p}");
    assert!(p.contains("Soups and stews"), "the chapter's name: {p}");
    for key in ["butternut squash", "olive oil", "pumpkin seed", "onion"] {
        assert!(p.contains(key), "{key} in {p}");
    }
    for secret in [
        "secret-url",
        "kitchen.test",
        "secret-note",
        "secret-description",
    ] {
        assert!(!body.contains(secret), "{secret} must not be sent: {body}");
    }
}

#[test]
fn prompt_caps_eight_keys() {
    let keys: Vec<String> = (1..=12).map(|n| format!("ingredient{n:02}")).collect();
    let p = prompt("Soup", "Soups", &keys);
    assert!(p.contains("ingredient08"), "{p}");
    assert!(!p.contains("ingredient09"), "{p}");
    assert!(p.contains("copperplate engraving"), "{p}");
    // No ingredients, no ingredients sentence.
    assert!(!prompt("Soup", "Soups", &[]).contains("Visible ingredients"));
}

#[test]
fn missing_key_keeps_photo() {
    let f = fixture();
    let before = std::fs::read(f.store.image_path(&f.saved.id, ImageKind::Printed)).unwrap();
    for key in [None, Some(""), Some("  ")] {
        let (result, calls) = run(&f, &f.cfg, key, Ok(reply(&png(300, 200))));
        assert_eq!(result, Err(FAILED));
        assert!(calls.is_empty(), "no key, no request");
    }
    assert!(!ill(&f).exists());
    assert!(!f.store.is_dirty());
    let after = std::fs::read(f.store.image_path(&f.saved.id, ImageKind::Printed)).unwrap();
    assert_eq!(before, after);
}

#[test]
fn http_error_keeps_photo_and_returns_generic_line() {
    let f = fixture();
    let before = std::fs::read(f.store.image_path(&f.saved.id, ImageKind::Printed)).unwrap();
    let (result, calls) = run(
        &f,
        &f.cfg,
        Some("sk-test"),
        Err("status: 401 invalid api key sk-test-leak".to_owned()),
    );
    assert_eq!(calls.len(), 1, "no retry loop");
    let line = result.unwrap_err();
    assert_eq!(line, "Couldn't draw this one; kept the photo");
    assert!(!line.contains("401") && !line.contains("sk-"));
    assert!(!ill(&f).exists());
    assert!(!f.store.is_dirty());
    let after = std::fs::read(f.store.image_path(&f.saved.id, ImageKind::Printed)).unwrap();
    assert_eq!(before, after);
}

#[test]
fn bad_base64_keeps_photo() {
    let f = fixture();
    let not_an_image =
        base64::engine::general_purpose::STANDARD.encode(b"definitely not a picture");
    let too_small = reply(&png(40, 40));
    for answer in [
        r#"{"data":[{"b64_json":"!!! not base64 !!!"}]}"#.to_owned(),
        format!(r#"{{"data":[{{"b64_json":"{not_an_image}"}}]}}"#),
        too_small,
        r#"{"data":[]}"#.to_owned(),
        r#"{"error":{"message":"raw provider words"}}"#.to_owned(),
        "<html>gateway</html>".to_owned(),
    ] {
        let (result, _) = run(&f, &f.cfg, Some("k"), Ok(answer.clone()));
        assert_eq!(result, Err(FAILED), "{answer}");
    }
    assert!(!ill(&f).exists());
    assert!(!f.store.is_dirty());
}

#[test]
fn failure_keeps_earlier_illustration() {
    let f = fixture();
    std::fs::write(ill(&f), jpeg()).unwrap();
    let before = std::fs::read(ill(&f)).unwrap();
    let (result, _) = run(&f, &f.cfg, Some("k"), Err("timeout".to_owned()));
    assert_eq!(result, Err(FAILED));
    assert_eq!(std::fs::read(ill(&f)).unwrap(), before);
}

#[test]
fn success_writes_ill_only() {
    let f = fixture();
    let printed = f.store.image_path(&f.saved.id, ImageKind::Printed);
    let site = std::fs::read(&printed).unwrap();
    let (result, _) = run(&f, &f.cfg, Some("k"), Ok(reply(&png(1536, 1024))));
    assert_eq!(result, Ok(()));

    // The illustration is a grayscale JPEG at most 1000 px wide.
    let bytes = std::fs::read(ill(&f)).unwrap();
    assert_eq!(&bytes[..2], &[0xff, 0xd8], "a JPEG");
    let img = image::load_from_memory(&bytes).unwrap();
    assert_eq!(img.color(), image::ColorType::L8);
    assert!(img.width() <= 1000 && img.width() > 900, "{}", img.width());

    // The site picture is untouched, and the book is due a rebuild.
    assert_eq!(std::fs::read(&printed).unwrap(), site);
    assert!(f.store.is_dirty());

    // The recipe prints it when its picture is Illustration.
    let cfg = BookConfig::default();
    let mut saved = f.saved.clone();
    assert_eq!(printed_picture(&f.store, &saved, &cfg), Picture::Site);
    saved.picture = Picture::Illustration;
    assert_eq!(
        printed_picture(&f.store, &saved, &cfg),
        Picture::Illustration
    );
}

#[test]
fn empty_endpoint_and_model_use_the_defaults() {
    let f = fixture();
    let cfg = IllustrationsConfig {
        enabled: true,
        endpoint: String::new(),
        model: String::new(),
    };
    let (result, calls) = run(&f, &cfg, Some("k"), Ok(reply(&png(300, 200))));
    assert_eq!(result, Ok(()));
    assert_eq!(calls[0].0, "https://api.openai.com/v1/images/generations");
    let v: serde_json::Value = serde_json::from_str(&calls[0].2).unwrap();
    assert_eq!(v["model"], illustrate::DEFAULT_MODEL);

    // A trailing slash on the endpoint doesn't double up.
    let cfg = IllustrationsConfig {
        endpoint: "http://localhost:8080/v1/".to_owned(),
        ..cfg
    };
    let (_, calls) = run(&f, &cfg, Some("k"), Ok(reply(&png(300, 200))));
    assert_eq!(calls[0].0, "http://localhost:8080/v1/images/generations");
}

#[test]
fn rebuild_never_regenerates() {
    let f = fixture();
    let mut saved = f.saved.clone();
    saved.picture = Picture::Illustration;
    f.store.update(&saved).unwrap();

    let calls = RefCell::new(0);
    let post = |_: &str, _: &[(String, String)], _: &str| {
        *calls.borrow_mut() += 1;
        Ok(reply(&png(300, 200)))
    };
    illustrate(&f.store, &saved, &f.cfg, Some("k"), &post as Post).unwrap();
    assert_eq!(*calls.borrow(), 1);

    // `build` has no transport at all; it prints what is there, and the same each time.
    let out = tempfile::tempdir().unwrap();
    let first = build(&f.store, &out.path().join("a.epub")).unwrap();
    let second = build(&f.store, &out.path().join("b.epub")).unwrap();
    assert_eq!(first.build_id, second.build_id);
    assert_eq!(*calls.borrow(), 1);

    let mut zip = zip::ZipArchive::new(std::fs::File::open(&first.path).unwrap()).unwrap();
    let mut files = std::collections::BTreeMap::new();
    for i in 0..zip.len() {
        let mut file = zip.by_index(i).unwrap();
        let mut bytes = Vec::new();
        std::io::Read::read_to_end(&mut file, &mut bytes).unwrap();
        files.insert(file.name().unwrap().into_owned(), bytes);
    }
    let name = format!("OEBPS/images/{}.ill.jpg", saved.id);
    assert!(files.contains_key(&name), "{:?}", files.keys());
    let page = String::from_utf8(files["OEBPS/chapter-soups.xhtml"].clone()).unwrap();
    assert!(page.contains("Illustration generated"), "{page}");
}
