// SPDX-License-Identifier: GPL-3.0-or-later
//! The reading queue: a JSONL file of saved articles (save-to-paper design §3).

use chrono::{DateTime, NaiveDate, Utc};
use deckle_digest::reading::{Added, MAX_WAITS, Outcome, ReadingQueue, State};

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
fn a_withheld_entry_waits_with_its_reason_and_keeps_its_tries() {
    let dir = tempfile::tempdir().unwrap();
    let q = queue(&dir);
    let d = NaiveDate::from_ymd_opt(2026, 10, 20).unwrap();
    let Added::New(id) = q
        .add_url("https://www.thepilot.com/a", None, "cli", now())
        .unwrap()
    else {
        panic!()
    };
    for day in 0..5 {
        q.commit(
            d + chrono::Days::new(day),
            &[(id.clone(), Outcome::Waiting("subscriber story".into()))],
        )
        .unwrap();
    }
    let e = &q.load()[0];
    assert_eq!(e.state, State::Pending);
    assert_eq!(e.tries, 0);
    assert_eq!(e.tried_on, None);
    assert_eq!(e.note.as_deref(), Some("subscriber story"));
    assert_eq!(e.waits, 5);
}

/// Saves `n` URLs a second apart, oldest first; returns their ids.
fn saved(q: &ReadingQueue, n: usize) -> Vec<String> {
    (0..n)
        .map(|i| {
            let at = now() + chrono::Duration::seconds(i as i64);
            match q
                .add_url(&format!("https://ex.com/{i}"), None, "cli", at)
                .unwrap()
            {
                Added::New(id) | Added::Duplicate(id) => id,
            }
        })
        .collect()
}

fn wait(q: &ReadingQueue, id: &str, d: NaiveDate) {
    q.commit(d, &[(id.to_owned(), Outcome::Waiting("withheld".into()))])
        .unwrap();
}

#[test]
fn a_waiting_entry_never_displaces_a_fresh_save() {
    let dir = tempfile::tempdir().unwrap();
    let q = queue(&dir);
    let d = NaiveDate::from_ymd_opt(2026, 10, 20).unwrap();
    let ids = saved(&q, 3);
    // The oldest is waiting on a withheld page.
    wait(&q, &ids[0], d);
    let got: Vec<String> = q.for_edition(d, 2).into_iter().map(|e| e.id).collect();
    assert_eq!(got, [ids[1].clone(), ids[2].clone()]);
}

#[test]
fn a_waiting_entry_fills_a_free_slot() {
    let dir = tempfile::tempdir().unwrap();
    let q = queue(&dir);
    let d = NaiveDate::from_ymd_opt(2026, 10, 20).unwrap();
    let ids = saved(&q, 2);
    wait(&q, &ids[0], d);
    let got: Vec<String> = q.for_edition(d, 3).into_iter().map(|e| e.id).collect();
    assert_eq!(got, [ids[1].clone(), ids[0].clone()]);
}

#[test]
fn a_waiting_entry_fails_after_max_waits() {
    let dir = tempfile::tempdir().unwrap();
    let q = queue(&dir);
    let d = NaiveDate::from_ymd_opt(2026, 10, 20).unwrap();
    let id = saved(&q, 1).remove(0);
    for day in 0..u64::from(MAX_WAITS) {
        wait(&q, &id, d + chrono::Days::new(day));
    }
    let e = &q.load()[0];
    assert_eq!(
        (e.waits, e.tries, &e.state),
        (MAX_WAITS, 0, &State::Pending)
    );
    // The 15th counts as a failed try, with the same note.
    let next = d + chrono::Days::new(u64::from(MAX_WAITS));
    wait(&q, &id, next);
    let e = &q.load()[0];
    assert_eq!(e.tries, 1);
    assert_eq!(e.tried_on, Some(next));
    assert_eq!(e.note.as_deref(), Some("withheld"));
    assert_eq!(e.state, State::Pending);
    // And it fails for good after the usual tries.
    for day in 1..3 {
        wait(&q, &id, next + chrono::Days::new(day));
    }
    assert_eq!(q.load()[0].state, State::Failed);
}

#[test]
fn an_old_line_without_waits_still_loads() {
    let dir = tempfile::tempdir().unwrap();
    let q = queue(&dir);
    std::fs::write(
        &q.path,
        r#"{"id":"r-20261020-abc","url":"https://ex.com/a","file":null,"title":null,"saved_at":"2026-10-20T14:03:11Z","source":"cli","state":"pending","tries":1,"printed_in":null,"note":"paywall","tried_on":"2026-10-20"}
"#,
    )
    .unwrap();
    let e = &q.load()[0];
    assert_eq!(e.id, "r-20261020-abc");
    assert_eq!((e.tries, e.waits), (1, 0));
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
