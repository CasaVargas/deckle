// SPDX-License-Identifier: GPL-3.0-or-later
//! The Inbox folder: files dropped in become queue entries (save-to-paper design §4).

use std::path::Path;
use std::time::{Duration, SystemTime};

use deckle_digest::inbox::scan;
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
