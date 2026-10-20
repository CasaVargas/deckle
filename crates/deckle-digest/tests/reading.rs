// SPDX-License-Identifier: GPL-3.0-or-later
//! The reading queue: a JSONL file of saved articles (save-to-paper design §3).

use chrono::{DateTime, NaiveDate, Utc};
use deckle_digest::reading::{Added, Outcome, ReadingQueue, State};

fn now() -> DateTime<Utc> {
    "2026-10-20T14:03:11Z".parse().unwrap()
}

fn queue(dir: &tempfile::TempDir) -> ReadingQueue {
    ReadingQueue::new(dir.path().join("reading.jsonl"))
}

#[test]
fn add_then_duplicate() {
    let dir = tempfile::tempdir().unwrap();
    let q = queue(&dir);
    let Added::New(id) = q
        .add_url("https://ex.com/a?utm_source=x#top", None, "cli", now())
        .unwrap()
    else {
        panic!("first save is new")
    };
    assert!(id.starts_with("r-20261020-"));
    assert_eq!(
        q.add_url("https://ex.com/a", Some("A"), "bookmarklet", now())
            .unwrap(),
        Added::Duplicate(id)
    );
    assert_eq!(q.load().len(), 1);
}

#[test]
fn for_edition_takes_pending_and_same_day_reprints() {
    let dir = tempfile::tempdir().unwrap();
    let q = queue(&dir);
    let d = NaiveDate::from_ymd_opt(2026, 10, 20).unwrap();
    let id = |a: Added| match a {
        Added::New(i) | Added::Duplicate(i) => i,
    };
    let today = id(q
        .add_url("https://ex.com/today", None, "cli", now())
        .unwrap());
    let yesterday = id(q
        .add_url("https://ex.com/yesterday", None, "cli", now())
        .unwrap());
    let pending = id(q
        .add_url("https://ex.com/pending", None, "cli", now())
        .unwrap());
    q.commit(d, &[(today.clone(), Outcome::Printed)]).unwrap();
    q.commit(d.pred_opt().unwrap(), &[(yesterday, Outcome::Printed)])
        .unwrap();
    let ids: Vec<String> = q.for_edition(d, 10).into_iter().map(|e| e.id).collect();
    assert_eq!(ids, [today, pending]);
}

#[test]
fn three_failures_mark_failed() {
    let dir = tempfile::tempdir().unwrap();
    let q = queue(&dir);
    let d = NaiveDate::from_ymd_opt(2026, 10, 20).unwrap();
    let Added::New(id) = q
        .add_url("https://ex.com/wall", None, "cli", now())
        .unwrap()
    else {
        panic!()
    };
    for day in 0..3 {
        let e = &q.load()[0];
        assert_eq!(e.state, State::Pending, "still pending before try {day}");
        q.commit(
            d + chrono::Days::new(day),
            &[(id.clone(), Outcome::Failed("paywall".into()))],
        )
        .unwrap();
    }
    let e = &q.load()[0];
    assert_eq!(e.state, State::Failed);
    assert_eq!(e.tries, 3);
    assert_eq!(e.note.as_deref(), Some("paywall"));
}

#[test]
fn malformed_lines_survive_rewrite() {
    let dir = tempfile::tempdir().unwrap();
    let q = queue(&dir);
    let Added::New(id) = q.add_url("https://ex.com/a", None, "cli", now()).unwrap() else {
        panic!()
    };
    let text = std::fs::read_to_string(&q.path).unwrap();
    std::fs::write(&q.path, format!("not json\n{text}")).unwrap();
    q.add_url("https://ex.com/b", None, "cli", now()).unwrap();
    assert!(q.remove(&id).unwrap());
    let text = std::fs::read_to_string(&q.path).unwrap();
    assert!(text.starts_with("not json\n"));
    assert_eq!(q.load().len(), 1);
}

#[test]
fn add_page_copies_files_folder() {
    let dir = tempfile::tempdir().unwrap();
    let q = queue(&dir);
    let src = tempfile::tempdir().unwrap();
    std::fs::write(
        src.path().join("a.html"),
        r#"<html><head><link rel="canonical" href="https://ex.com/story"></head><body>x</body></html>"#,
    )
    .unwrap();
    std::fs::create_dir_all(src.path().join("a_files")).unwrap();
    std::fs::write(src.path().join("a_files/p.jpg"), b"jpg").unwrap();
    let Added::New(id) = q
        .add_page(&src.path().join("a.html"), "inbox", now())
        .unwrap()
    else {
        panic!()
    };
    let e = &q.load()[0];
    assert_eq!(e.url.as_deref(), Some("https://ex.com/story"));
    let saved = dir.path().join("reading").join(&id);
    assert_eq!(e.file.as_deref(), Some(saved.join("a.html").as_path()));
    assert!(saved.join("a.html").exists());
    assert!(saved.join("a_files/p.jpg").exists());
}

#[test]
fn a_non_utf8_line_never_wipes_the_queue() {
    let dir = tempfile::tempdir().unwrap();
    let q = queue(&dir);
    q.add_url("https://ex.com/a", None, "cli", now()).unwrap();
    // A hand edit saved as Latin-1: "café" with a raw 0xE9 byte.
    let mut bytes = b"{\"note\":\"caf\xe9\"}\n".to_vec();
    bytes.extend(std::fs::read(&q.path).unwrap());
    std::fs::write(&q.path, &bytes).unwrap();
    q.add_url("https://ex.com/b", None, "cli", now()).unwrap();
    let after = std::fs::read(&q.path).unwrap();
    assert!(
        after.starts_with(b"{\"note\":\"caf\xe9\"}\n"),
        "the odd line is kept byte for byte"
    );
    assert_eq!(q.load().len(), 2, "both entries survive");
}

#[test]
fn an_unreadable_queue_refuses_to_write() {
    let dir = tempfile::tempdir().unwrap();
    let q = ReadingQueue::new(dir.path().join("reading.jsonl"));
    std::fs::create_dir_all(&q.path).unwrap(); // a directory where the file should be
    assert!(q.add_url("https://ex.com/a", None, "cli", now()).is_err());
    assert!(q.path.is_dir(), "nothing was written over it");
}

#[test]
fn a_saved_page_attaches_to_its_pending_url() {
    let dir = tempfile::tempdir().unwrap();
    let q = queue(&dir);
    let d = NaiveDate::from_ymd_opt(2026, 10, 20).unwrap();
    let Added::New(id) = q
        .add_url("https://ex.com/story", None, "cli", now())
        .unwrap()
    else {
        panic!()
    };
    q.commit(d, &[(id.clone(), Outcome::Failed("paywall".into()))])
        .unwrap();
    let src = tempfile::tempdir().unwrap();
    std::fs::write(
        src.path().join("s.html"),
        r#"<link rel="canonical" href="https://ex.com/story"><p>x</p>"#,
    )
    .unwrap();
    assert_eq!(
        q.add_page(&src.path().join("s.html"), "inbox", now())
            .unwrap(),
        Added::New(id.clone()),
        "the page is used, under the same entry"
    );
    let e = &q.load()[0];
    assert_eq!(q.load().len(), 1);
    assert!(e.file.as_ref().unwrap().ends_with("s.html"));
    assert_eq!((e.tries, e.note.as_deref()), (0, None), "a fresh start");
}
