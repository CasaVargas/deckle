// SPDX-License-Identifier: GPL-3.0-or-later
//! The Inbox folder: files dropped in become queue entries (save-to-paper design §4).

use std::path::Path;
use std::time::{Duration, SystemTime};

use deckle_digest::inbox::{Skip, scan, scan_with_books};
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
