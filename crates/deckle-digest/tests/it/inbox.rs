// SPDX-License-Identifier: GPL-3.0-or-later
//! The Inbox folder: files dropped in become queue entries (save-to-paper design §4).

use std::path::Path;
use std::time::{Duration, SystemTime};

use deckle_digest::Net;
use deckle_digest::cookbook::store::Store;
use deckle_digest::inbox::{Skip, scan, scan_for_kindles, scan_recipes, scan_with_books};
use deckle_digest::reading::ReadingQueue;

fn later() -> SystemTime {
    SystemTime::now() + Duration::from_secs(60)
}

fn setup() -> (tempfile::TempDir, ReadingQueue, std::path::PathBuf) {
    let dir = tempfile::tempdir().unwrap();
    let q = ReadingQueue::new(dir.path().join("data/reading.jsonl"));
    let inbox = dir.path().join("Inbox");
    std::fs::create_dir_all(&inbox).unwrap();
    (dir, q, inbox)
}

fn write(p: &Path, s: &str) {
    std::fs::write(p, s).unwrap();
}

#[test]
fn webloc_url_txt_and_html_are_queued() {
    let (_d, q, inbox) = setup();
    write(
        &inbox.join("a.webloc"),
        r#"<?xml version="1.0" encoding="UTF-8"?><plist version="1.0"><dict><key>URL</key><string>https://ex.com/webloc</string></dict></plist>"#,
    );
    write(
        &inbox.join("b.url"),
        "[InternetShortcut]\r\nURL=https://ex.com/url\r\n",
    );
    write(
        &inbox.join("c.txt"),
        "https://ex.com/one\n\nhello\nhttps://ex.com/two and more\n",
    );
    write(
        &inbox.join("d.html"),
        "<html><head><title>d</title></head><body><p>page</p></body></html>",
    );
    let r = scan(&inbox, &q, later());
    assert_eq!(r.queued, 5, "{:?}", r.unreadable);
    assert!(r.unreadable.is_empty());
    for f in ["a.webloc", "b.url", "c.txt", "d.html"] {
        assert!(inbox.join("Saved").join(f).exists(), "{f} moved to Saved");
        assert!(!inbox.join(f).exists());
    }
    let urls: Vec<_> = q.load().into_iter().filter_map(|e| e.url).collect();
    for u in ["webloc", "url", "one", "two"] {
        assert!(
            urls.contains(&format!("https://ex.com/{u}")),
            "{u} queued: {urls:?}"
        );
    }
}

#[test]
fn fresh_files_wait() {
    let (_d, q, inbox) = setup();
    write(&inbox.join("new.txt"), "https://ex.com/new\n");
    let r = scan(&inbox, &q, SystemTime::now());
    assert_eq!(r.queued, 0);
    assert!(inbox.join("new.txt").exists());
}

#[test]
fn unreadable_goes_aside_with_reason() {
    let (_d, q, inbox) = setup();
    std::fs::write(inbox.join("photo.png"), [0u8, 1, 2]).unwrap();
    write(&inbox.join("empty.txt"), "nothing to see\n");
    let r = scan(&inbox, &q, later());
    assert_eq!(r.queued, 0);
    assert_eq!(r.unreadable.len(), 2);
    assert!(inbox.join("Unreadable/photo.png").exists());
    assert!(inbox.join("Unreadable/photo.png.txt").exists());
    assert!(inbox.join("Unreadable/empty.txt").exists());
}

#[test]
fn missing_inbox_is_created() {
    let dir = tempfile::tempdir().unwrap();
    let q = ReadingQueue::new(dir.path().join("reading.jsonl"));
    let inbox = dir.path().join("Deckle/Inbox");
    let r = scan(&inbox, &q, later());
    assert_eq!(r.queued, 0);
    assert!(inbox.is_dir());
}

#[test]
fn a_busy_queue_leaves_the_file_for_the_next_scan() {
    let (_d, q, inbox) = setup();
    write(&inbox.join("later.txt"), "https://ex.com/later\n");
    std::fs::create_dir_all(q.path.parent().unwrap()).unwrap();
    std::fs::write(q.path.with_extension("lock"), b"").unwrap(); // another writer holds it
    let r = scan(&inbox, &q, later());
    assert_eq!(r.queued, 0);
    assert!(r.unreadable.is_empty(), "{:?}", r.unreadable);
    assert_eq!(r.retry.len(), 1);
    assert!(inbox.join("later.txt").exists(), "still in the Inbox");
}

#[test]
fn a_binary_webloc_is_read() {
    let (_d, q, inbox) = setup();
    // What Finder writes when a link is dragged out of a browser (binary property list).
    let mut b = b"bplist00\xd1\x01\x02SURL_\x10\x15".to_vec();
    b.extend_from_slice(b"https://ex.com/binary");
    b.extend_from_slice(b"\x08\x0b\x0f\x00\x00\x00\x00\x00\x00\x01\x01");
    std::fs::write(inbox.join("drag.webloc"), &b).unwrap();
    let r = scan(&inbox, &q, later());
    assert_eq!(r.queued, 1, "{:?}", r.unreadable);
    assert_eq!(q.load()[0].url.as_deref(), Some("https://ex.com/binary"));
}

#[test]
fn books_go_to_the_handler_then_to_sent() {
    let (_d, q, inbox) = setup();
    for f in ["a.epub", "b.DOCX", "c.kfx"] {
        std::fs::write(inbox.join(f), b"book").unwrap();
    }
    write(&inbox.join("d.txt"), "https://ex.com/article\n");
    let mut seen = Vec::new();
    let r = scan_with_books(&inbox, &q, later(), &mut |p: &Path| {
        seen.push(p.file_name().unwrap().to_string_lossy().into_owned());
        Ok(())
    });
    assert_eq!(seen, ["a.epub", "b.DOCX", "c.kfx"]);
    assert_eq!(r.sent.len(), 3);
    assert_eq!(r.queued, 1, "the article is still an article");
    for f in ["a.epub", "b.DOCX", "c.kfx"] {
        assert!(inbox.join("Sent").join(f).exists(), "{f} moved to Sent");
    }
    assert!(inbox.join("Saved/d.txt").exists());
}

#[test]
fn a_book_that_fails_goes_to_unreadable_with_the_reason() {
    let (_d, q, inbox) = setup();
    std::fs::write(inbox.join("bad.epub"), b"not a zip").unwrap();
    let r = scan_with_books(&inbox, &q, later(), &mut |_: &Path| {
        Err(Skip::Unreadable("inspect: not an EPUB".into()))
    });
    assert!(r.sent.is_empty());
    assert_eq!(r.unreadable.len(), 1);
    assert!(inbox.join("Unreadable/bad.epub").exists());
    let why = std::fs::read_to_string(inbox.join("Unreadable/bad.epub.txt")).unwrap();
    assert!(why.contains("not an EPUB"), "{why}");
}

#[test]
fn a_book_that_cant_be_handled_yet_stays() {
    let (_d, q, inbox) = setup();
    std::fs::write(inbox.join("wait.epub"), b"book").unwrap();
    let r = scan_with_books(&inbox, &q, later(), &mut |_: &Path| {
        Err(Skip::Retry("Kindle Previewer not found".into()))
    });
    assert_eq!(r.retry.len(), 1);
    assert!(inbox.join("wait.epub").exists());
}

#[test]
fn a_fresh_book_waits() {
    let (_d, q, inbox) = setup();
    std::fs::write(inbox.join("new.epub"), b"book").unwrap();
    let mut called = false;
    let r = scan_with_books(&inbox, &q, SystemTime::now(), &mut |_: &Path| {
        called = true;
        Ok(())
    });
    assert!(!called);
    assert!(r.sent.is_empty());
    assert!(inbox.join("new.epub").exists());
}

#[test]
fn plain_scan_leaves_books_for_a_scan_that_can_send_them() {
    let (_d, q, inbox) = setup();
    std::fs::write(inbox.join("book.epub"), b"book").unwrap();
    let r = scan(&inbox, &q, later());
    assert!(r.unreadable.is_empty());
    assert_eq!(r.retry.len(), 1);
    assert!(inbox.join("book.epub").exists());
}

#[test]
fn a_book_in_a_kindles_folder_is_meant_for_that_kindle() {
    let (_d, q, inbox) = setup();
    let mom = inbox.join("mom's kindle");
    std::fs::create_dir_all(&mom).unwrap();
    std::fs::create_dir_all(inbox.join("Holiday")).unwrap();
    std::fs::create_dir_all(inbox.join("Sent")).unwrap();
    std::fs::write(mom.join("Dune.epub"), b"book").unwrap();
    std::fs::write(inbox.join("Holiday/Left.epub"), b"book").unwrap();
    std::fs::write(inbox.join("Anyone.epub"), b"book").unwrap();
    write(&mom.join("a.txt"), "https://ex.com/for-the-paper\n");
    let kindles = [("Mom's Kindle".to_owned(), "G000AAAA".to_owned())];
    let mut seen = Vec::new();
    let r = scan_for_kindles(&inbox, &q, later(), &kindles, &mut |p, owner| {
        seen.push((
            p.file_name().unwrap().to_string_lossy().into_owned(),
            owner.map(str::to_owned),
        ));
        Ok(())
    });
    assert_eq!(
        seen,
        [
            ("Anyone.epub".to_owned(), None),
            ("Dune.epub".to_owned(), Some("G000AAAA".to_owned())),
        ]
    );
    assert!(mom.join("Sent/Dune.epub").exists());
    // Articles in a Kindle's folder still go to the paper.
    assert_eq!(r.queued, 1);
    assert_eq!(r.other_folders, [inbox.join("Holiday")]);
    assert!(
        inbox.join("Holiday/Left.epub").exists(),
        "other folders are left alone"
    );
}

// The Recipes folder (cookbook design §8.3).

const RECIPE_URL: &str = "https://fetch.test/recipes/oat-cookies";

fn recipe_page() -> String {
    r#"<!doctype html><html><head><title>Oat Cookies</title>
<script type="application/ld+json">
{"@context": "https://schema.org", "@type": "Recipe", "name": "Oat Cookies",
  "recipeCategory": "Dessert",
  "recipeIngredient": ["1 cup oats"], "recipeInstructions": ["Bake them."]}
</script></head><body><h1>Oat Cookies</h1></body></html>"#
        .to_owned()
}

/// Serves the one recipe page, and records each URL asked for.
struct Web(std::sync::Mutex<Vec<String>>);

impl Web {
    fn new() -> Self {
        Web(std::sync::Mutex::new(Vec::new()))
    }
}

impl Net for Web {
    fn get(&self, url: &str) -> Result<Vec<u8>, String> {
        self.0.lock().unwrap().push(url.to_owned());
        if url == RECIPE_URL {
            Ok(recipe_page().into_bytes())
        } else {
            Err(format!("offline: {url}"))
        }
    }
}

fn recipes_setup() -> (tempfile::TempDir, ReadingQueue, std::path::PathBuf, Store) {
    let (d, q, inbox) = setup();
    std::fs::create_dir_all(inbox.join("Recipes")).unwrap();
    let store = Store::new(d.path().join("cookbook"));
    (d, q, inbox, store)
}

#[test]
fn recipe_link_goes_to_cookbook_not_reading() {
    let (_d, q, inbox, store) = recipes_setup();
    write(
        &inbox.join("Recipes/cookies.url"),
        &format!("[InternetShortcut]\r\nURL={RECIPE_URL}\r\n"),
    );
    write(
        &inbox.join("Recipes/more.txt"),
        &format!("{RECIPE_URL}\nhttps://nowhere.test/x\n"),
    );
    // The article scan leaves the folder alone.
    let r = scan(&inbox, &q, later());
    assert_eq!(r.queued, 0);
    assert!(r.other_folders.is_empty(), "{:?}", r.other_folders);
    assert!(inbox.join("Recipes/cookies.url").exists());

    let net = Web::new();
    let r = scan_recipes(&inbox, &store, &net, later());
    // cookies.url, and the first address in more.txt (the second is nowhere).
    assert_eq!(r.recipes, 2, "{:?} {:?}", r.unreadable, r.retry);
    assert_eq!(r.unreadable.len(), 1, "more.txt names a page that is gone");
    assert!(inbox.join("Recipes/Unreadable/more.txt").exists());
    assert_eq!(store.all().unwrap().len(), 1, "both were the same recipe");
    assert_eq!(store.all().unwrap()[0].recipe.title, "Oat Cookies");
    assert!(q.load().is_empty(), "recipes never land in Your Reading");
    assert!(inbox.join("Recipes/Sent/cookies.url").exists());
}

#[test]
fn saved_page_in_recipes_added() {
    let (_d, q, inbox, store) = recipes_setup();
    write(&inbox.join("Recipes/page.html"), &recipe_page());
    write(&inbox.join("Recipes/page.HTM"), &recipe_page());
    let net = Web::new();
    let r = scan_recipes(&inbox, &store, &net, later());
    assert_eq!(r.recipes, 2, "{:?}", r.unreadable);
    assert_eq!(store.all().unwrap().len(), 2, "a saved page is its own key");
    assert!(
        net.0.lock().unwrap().is_empty(),
        "a saved page is not fetched"
    );
    assert!(inbox.join("Recipes/Sent/page.html").exists());
    assert!(q.load().is_empty());
}

#[test]
fn unreadable_moves_with_reason_txt() {
    let (_d, _q, inbox, store) = recipes_setup();
    write(
        &inbox.join("Recipes/gone.txt"),
        "https://nowhere.test/gone\n",
    );
    write(&inbox.join("Recipes/empty.txt"), "no link in here\n");
    std::fs::write(inbox.join("Recipes/photo.png"), [0u8, 1, 2]).unwrap();
    let r = scan_recipes(&inbox, &store, &Web::new(), later());
    assert_eq!(r.recipes, 0);
    assert_eq!(r.unreadable.len(), 3, "{:?}", r.unreadable);
    assert!(store.all().unwrap().is_empty());
    for f in ["gone.txt", "empty.txt", "photo.png"] {
        assert!(inbox.join("Recipes/Unreadable").join(f).exists(), "{f}");
        assert!(
            inbox
                .join("Recipes/Unreadable")
                .join(format!("{f}.txt"))
                .exists(),
            "{f} has a reason"
        );
    }
    let why = std::fs::read_to_string(inbox.join("Recipes/Unreadable/gone.txt.txt")).unwrap();
    assert!(why.contains("offline"), "{why}");
}

#[test]
fn sent_folder_used_for_added() {
    let (_d, _q, inbox, store) = recipes_setup();
    write(&inbox.join("Recipes/a.url"), &format!("URL={RECIPE_URL}\n"));
    let net = Web::new();
    scan_recipes(&inbox, &store, &net, later());
    // The same name again lands beside the first, never over it.
    write(&inbox.join("Recipes/a.url"), &format!("URL={RECIPE_URL}\n"));
    let r = scan_recipes(&inbox, &store, &net, later());
    assert_eq!(r.recipes, 1, "adding again updates the recipe");
    assert!(inbox.join("Recipes/Sent/a.url").exists());
    assert!(inbox.join("Recipes/Sent/a (2).url").exists());
    assert!(
        !inbox.join("Recipes/Saved").exists(),
        "Saved/ is for articles"
    );
    assert_eq!(store.all().unwrap().len(), 1);
}

#[test]
fn kindle_named_recipes_is_skipped_with_warning() {
    let (_d, q, inbox, _store) = recipes_setup();
    write(&inbox.join("Recipes/book.epub"), "book");
    let kindles = [("Recipes".to_owned(), "G000AAAA".to_owned())];
    let mut seen = 0;
    let r = scan_for_kindles(&inbox, &q, later(), &kindles, &mut |_, _| {
        seen += 1;
        Ok(())
    });
    assert_eq!(seen, 0, "Recipes/ is never a Kindle's folder");
    assert!(inbox.join("Recipes/book.epub").exists());
    assert_eq!(
        r.warnings,
        ["A Kindle named Recipes can't have an Inbox folder: Recipes/ is for the cookbook"]
    );
    // Any case, and no warning for other names.
    let kindles = [("recipes".to_owned(), "G000AAAA".to_owned())];
    let r = scan_for_kindles(&inbox, &q, later(), &kindles, &mut |_, _| Ok(()));
    assert_eq!(r.warnings.len(), 1);
    let r = scan_for_kindles(&inbox, &q, later(), &[], &mut |_, _| Ok(()));
    assert!(r.warnings.is_empty());
}

#[test]
fn unsettled_files_wait() {
    let (_d, _q, inbox, store) = recipes_setup();
    write(
        &inbox.join("Recipes/new.url"),
        &format!("URL={RECIPE_URL}\n"),
    );
    let net = Web::new();
    let r = scan_recipes(&inbox, &store, &net, SystemTime::now());
    assert_eq!(r.recipes, 0);
    assert!(inbox.join("Recipes/new.url").exists());
    assert!(net.0.lock().unwrap().is_empty(), "nothing fetched yet");
}

#[test]
fn an_empty_or_missing_recipes_folder_costs_nothing() {
    let (d, _q, inbox, store) = recipes_setup();
    let net = Web::new();
    let r = scan_recipes(&inbox, &store, &net, later());
    assert_eq!(r.recipes, 0);
    assert!(r.unreadable.is_empty() && r.retry.is_empty());
    assert!(!inbox.join("Recipes/Sent").exists());
    assert!(!inbox.join("Recipes/Unreadable").exists());
    std::fs::remove_dir(inbox.join("Recipes")).unwrap();
    let r = scan_recipes(&inbox, &store, &net, later());
    assert_eq!(r.recipes, 0);
    assert!(!d.path().join("cookbook").exists(), "the book is untouched");
}

#[test]
fn the_article_scan_makes_the_recipes_folder() {
    let dir = tempfile::tempdir().unwrap();
    let q = ReadingQueue::new(dir.path().join("reading.jsonl"));
    let inbox = dir.path().join("Inbox");
    scan(&inbox, &q, later());
    assert!(inbox.join("Recipes").is_dir());
}

#[test]
fn a_bookmarklet_link_is_added_by_the_next_scan() {
    let (d, q, inbox, store) = recipes_setup();
    let line = format!(
        "GET /save?token=t&kind=recipe&url={}&title=Oat%20Cookies HTTP/1.1",
        deckle_digest::fetch::encode(RECIPE_URL)
    );
    let (status, _) = deckle_digest::bookmarklet::handle(
        &line,
        "t",
        &q,
        &inbox.join("Recipes"),
        chrono::Utc::now(),
    );
    assert_eq!(status, 200);
    let net = Web::new();
    assert!(
        net.0.lock().unwrap().is_empty(),
        "the handler fetched nothing"
    );
    let r = scan_recipes(&inbox, &store, &net, later());
    assert_eq!(r.recipes, 1, "{:?}", r.unreadable);
    assert_eq!(store.all().unwrap()[0].recipe.title, "Oat Cookies");
    assert!(inbox.join("Recipes/Sent/oat-cookies.url").exists());
    assert!(q.load().is_empty());
    drop(d);
}
