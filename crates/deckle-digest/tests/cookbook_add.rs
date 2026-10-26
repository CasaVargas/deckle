// SPDX-License-Identifier: GPL-3.0-or-later
//! Adding a recipe from a link, a link file or a saved page, with its site photo
//! (cookbook design §4.3, §7.1). A fixture `Net` maps URLs to bytes; nothing touches the network.

use std::collections::HashMap;
use std::sync::Mutex;

use chrono::{DateTime, TimeZone, Utc};
use deckle_digest::Net;
use deckle_digest::cookbook::add::{AddOptions, add, preview, site_picture};
use deckle_digest::cookbook::store::{ImageKind, Store, Stored, recipe_id};

const PAGE_URL: &str = "https://fetch.test/recipes/oat-cookies?utm_source=feed";
const KEY: &str = "https://fetch.test/recipes/oat-cookies";
const IMG_URL: &str = "https://img.test/cookies.png";

fn at(day: u32) -> DateTime<Utc> {
    Utc.with_ymd_and_hms(2026, 10, day, 12, 0, 0).unwrap()
}

fn png(w: u32, h: u32) -> Vec<u8> {
    let mut out = Vec::new();
    image::DynamicImage::ImageRgb8(image::RgbImage::from_fn(w, h, |x, y| {
        image::Rgb([(x % 256) as u8, (y % 256) as u8, ((x + y) % 256) as u8])
    }))
    .write_to(&mut std::io::Cursor::new(&mut out), image::ImageFormat::Png)
    .unwrap();
    out
}

fn recipe_page(image: &str) -> String {
    format!(
        r#"<!doctype html><html><head><title>Oat Cookies</title>
<script type="application/ld+json">
{{"@context": "https://schema.org", "@type": "Recipe", "name": "Oat Cookies",
  "recipeCategory": "Dessert", "image": "{image}",
  "recipeIngredient": ["1 cup oats"], "recipeInstructions": ["Bake them."]}}
</script></head><body><h1>Oat Cookies</h1></body></html>"#
    )
}

/// Serves what it is given, records each URL asked for, and fails everything else.
struct Fixture {
    files: HashMap<String, Vec<u8>>,
    asked: Mutex<Vec<String>>,
}

impl Fixture {
    fn new(files: &[(&str, Vec<u8>)]) -> Self {
        Fixture {
            files: files
                .iter()
                .map(|(u, b)| ((*u).to_owned(), b.clone()))
                .collect(),
            asked: Mutex::new(Vec::new()),
        }
    }
}

impl Net for Fixture {
    fn get(&self, url: &str) -> Result<Vec<u8>, String> {
        self.asked.lock().unwrap().push(url.to_owned());
        self.files
            .get(url)
            .cloned()
            .ok_or_else(|| format!("offline: {url}"))
    }
}

fn opts() -> AddOptions<'static> {
    AddOptions {
        chapter: None,
        note: None,
    }
}

fn store() -> (tempfile::TempDir, Store) {
    let dir = tempfile::tempdir().unwrap();
    let store = Store::new(dir.path().join("cookbook"));
    (dir, store)
}

fn dims(bytes: &[u8]) -> (u32, u32) {
    let img = image::load_from_memory(bytes).unwrap();
    (img.width(), img.height())
}

fn saved(stored: &Stored) -> &deckle_digest::cookbook::store::Saved {
    let (Stored::Added(s) | Stored::Updated(s)) = stored;
    s
}

#[test]
fn add_url_stores_recipe_and_photo() {
    let (_d, store) = store();
    let source = png(1600, 1200);
    let net = Fixture::new(&[
        (PAGE_URL, recipe_page(IMG_URL).into_bytes()),
        (IMG_URL, source.clone()),
    ]);
    let out = add(&store, PAGE_URL, &net, &opts(), at(26)).unwrap();
    assert_eq!(out.picture_note, None);
    let Stored::Added(s) = &out.stored else {
        panic!("a new recipe is added: {:?}", out.stored)
    };
    assert_eq!(s.id, recipe_id(KEY));
    assert_eq!(s.recipe.title, "Oat Cookies");
    assert_eq!(s.chapter, "desserts");
    assert_eq!(store.find(&s.id).unwrap().unwrap(), *s);

    // As fetched, kept beside the printed picture.
    let orig = std::fs::read(store.image_path(&s.id, ImageKind::Orig)).unwrap();
    assert_eq!(orig, source);
    let printed = std::fs::read(store.image_path(&s.id, ImageKind::Printed)).unwrap();
    assert_eq!(dims(&printed), (1000, 667));
    assert_eq!(
        image::guess_format(&printed).unwrap(),
        image::ImageFormat::Jpeg
    );
    assert_eq!(*net.asked.lock().unwrap(), vec![PAGE_URL, IMG_URL]);
}

#[test]
fn add_webloc_follows_link() {
    let (d, store) = store();
    let link = d.path().join("Oat Cookies.webloc");
    std::fs::write(
        &link,
        format!(
            "<?xml version=\"1.0\"?><plist><dict><key>URL</key><string>{KEY}</string></dict></plist>"
        ),
    )
    .unwrap();
    let net = Fixture::new(&[
        (KEY, recipe_page(IMG_URL).into_bytes()),
        (PAGE_URL, recipe_page(IMG_URL).into_bytes()),
        (IMG_URL, png(1200, 800)),
    ]);
    let out = add(&store, &link.to_string_lossy(), &net, &opts(), at(26)).unwrap();
    assert_eq!(saved(&out.stored).id, recipe_id(KEY));
    assert!(
        store
            .image_path(&recipe_id(KEY), ImageKind::Printed)
            .exists()
    );

    // The same link as a `.url` file is the same recipe, so it is updated.
    let url_file = d.path().join("Oat Cookies.url");
    std::fs::write(&url_file, format!("[InternetShortcut]\nURL={PAGE_URL}\n")).unwrap();
    let again = add(&store, &url_file.to_string_lossy(), &net, &opts(), at(27)).unwrap();
    assert!(
        matches!(again.stored, Stored::Updated(_)),
        "{:?}",
        again.stored
    );
}

#[test]
fn saved_page_never_fetches() {
    struct Never;
    impl Net for Never {
        fn get(&self, url: &str) -> Result<Vec<u8>, String> {
            panic!("a saved page is never fetched: {url}")
        }
    }
    let (d, store) = store();
    let site = d.path().join("site");
    std::fs::create_dir_all(site.join("page_files")).unwrap();
    std::fs::write(site.join("page_files/c.png"), png(900, 600)).unwrap();
    let file = site.join("Oat Cookies.html");
    std::fs::write(&file, recipe_page("page_files/c.png")).unwrap();

    let out = add(&store, &file.to_string_lossy(), &Never, &opts(), at(26)).unwrap();
    let s = saved(&out.stored);
    assert_eq!(out.picture_note, None);
    // Keyed by the page's canonical absolute path.
    let key = std::fs::canonicalize(&file).unwrap();
    assert_eq!(s.id, recipe_id(&key.to_string_lossy()));
    let printed = std::fs::read(store.image_path(&s.id, ImageKind::Printed)).unwrap();
    assert_eq!(dims(&printed), (900, 600));
}

#[test]
fn photo_failure_saves_without_picture() {
    let (_d, store) = store();
    // The image is not served.
    let net = Fixture::new(&[(PAGE_URL, recipe_page(IMG_URL).into_bytes())]);
    let out = add(&store, PAGE_URL, &net, &opts(), at(26)).unwrap();
    assert_eq!(out.picture_note, Some("No picture on the page"));
    let id = &saved(&out.stored).id;
    assert!(store.find(id).unwrap().is_some());
    assert!(!store.image_path(id, ImageKind::Printed).exists());
    assert!(!store.image_path(id, ImageKind::Orig).exists());

    // A page that names no image at all says the same.
    let bare = recipe_page("").replace(r#""image": "","#, "");
    let net = Fixture::new(&[("https://fetch.test/bare", bare.into_bytes())]);
    let out = add(&store, "https://fetch.test/bare", &net, &opts(), at(26)).unwrap();
    assert_eq!(out.picture_note, Some("No picture on the page"));
}

#[test]
fn tiny_image_is_no_picture() {
    let (_d, store) = store();
    let net = Fixture::new(&[
        (PAGE_URL, recipe_page(IMG_URL).into_bytes()),
        (IMG_URL, png(40, 40)),
    ]);
    let out = add(&store, PAGE_URL, &net, &opts(), at(26)).unwrap();
    assert_eq!(out.picture_note, Some("No picture on the page"));
    let id = &saved(&out.stored).id;
    assert!(store.find(id).unwrap().is_some());
    assert!(!store.image_path(id, ImageKind::Printed).exists());

    // Bytes that are no image at all.
    let net = Fixture::new(&[
        (PAGE_URL, recipe_page(IMG_URL).into_bytes()),
        (IMG_URL, b"<html>not a picture</html>".to_vec()),
    ]);
    let out = add(&store, PAGE_URL, &net, &opts(), at(27)).unwrap();
    assert_eq!(out.picture_note, Some("No picture on the page"));
}

#[test]
fn preview_writes_nothing() {
    let (_d, store) = store();
    let net = Fixture::new(&[
        (PAGE_URL, recipe_page(IMG_URL).into_bytes()),
        (IMG_URL, png(1600, 1200)),
    ]);
    let p = preview(PAGE_URL, &net, &store).unwrap();
    assert_eq!(p.recipe.title, "Oat Cookies");
    assert_eq!(p.chapter, "desserts");
    assert!(!p.exists);
    assert_eq!(dims(&p.picture_jpeg.unwrap()), (1000, 667));
    assert!(!store.dir.exists(), "the store folder is not even made");

    // Once saved, a preview says so, and shows the chapter it was moved to.
    let out = add(
        &store,
        PAGE_URL,
        &net,
        &AddOptions {
            chapter: Some("breakfast"),
            note: None,
        },
        at(26),
    )
    .unwrap();
    let p = preview(PAGE_URL, &net, &store).unwrap();
    assert!(p.exists);
    assert_eq!(p.chapter, "breakfast");
    assert_eq!(store.all().unwrap().len(), 1);
    assert_eq!(saved(&out.stored).chapter, "breakfast");
}

#[test]
fn text_fallback_uses_lead_photo() {
    let para = "The first frost came early this year, and with it the urge to stand over a pot of something slow, so we softened the onions and let the squash collapse into the broth. ";
    let body: String = (0..12).map(|_| format!("<p>{para}</p>")).collect();
    let html = format!(
        "<html><head><title>Squash Stew</title>\
         <meta property=\"og:image\" content=\"https://img.test/stew.png\"></head>\
         <body><article><h1>Squash Stew</h1>{body}</article></body></html>"
    );
    let (_d, store) = store();
    let net = Fixture::new(&[
        ("https://fetch.test/stew", html.into_bytes()),
        ("https://img.test/stew.png", png(800, 600)),
    ]);
    let out = add(&store, "https://fetch.test/stew", &net, &opts(), at(26)).unwrap();
    let s = saved(&out.stored);
    assert!(!s.recipe.structured);
    assert_eq!(out.picture_note, None);
    let printed = std::fs::read(store.image_path(&s.id, ImageKind::Printed)).unwrap();
    assert_eq!(dims(&printed), (800, 533));
}

#[test]
fn resaving_keeps_the_owners_fields_and_refreshes_the_photo() {
    let (_d, store) = store();
    let net = Fixture::new(&[
        (PAGE_URL, recipe_page(IMG_URL).into_bytes()),
        (IMG_URL, png(1600, 1200)),
    ]);
    let first = add(
        &store,
        PAGE_URL,
        &net,
        &AddOptions {
            chapter: Some("breakfast"),
            note: Some("Less sugar"),
        },
        at(26),
    )
    .unwrap();
    let net = Fixture::new(&[
        (PAGE_URL, recipe_page(IMG_URL).into_bytes()),
        (IMG_URL, png(1200, 1200)),
    ]);
    let again = add(&store, PAGE_URL, &net, &opts(), at(27)).unwrap();
    let Stored::Updated(s) = &again.stored else {
        panic!("{:?}", again.stored)
    };
    assert_eq!(s.chapter, "breakfast");
    assert_eq!(s.note.as_deref(), Some("Less sugar"));
    assert_eq!(s.added, saved(&first.stored).added);
    let orig = std::fs::read(store.image_path(&s.id, ImageKind::Orig)).unwrap();
    assert_eq!(dims(&orig), (1200, 1200));
}

#[test]
fn site_picture_crops_to_three_by_two() {
    // Taller than 3:2 loses top and bottom; wider loses the sides.
    assert_eq!(dims(&site_picture(&png(600, 600)).unwrap()), (600, 400));
    assert_eq!(dims(&site_picture(&png(1200, 400)).unwrap()), (600, 400));
    assert_eq!(dims(&site_picture(&png(2400, 1600)).unwrap()), (1000, 667));
    assert!(site_picture(&png(50, 50)).is_none());
    assert!(site_picture(b"nope").is_none());
}

#[test]
fn a_target_that_is_nothing_is_refused() {
    let (_d, store) = store();
    let net = Fixture::new(&[]);
    let err = add(&store, "notes.docx", &net, &opts(), at(26)).unwrap_err();
    assert!(err.contains("saved page"), "{err}");
    let err = add(&store, "https://fetch.test/gone", &net, &opts(), at(26)).unwrap_err();
    assert!(err.contains("offline"), "{err}");
    assert!(!store.dir.exists());
}
