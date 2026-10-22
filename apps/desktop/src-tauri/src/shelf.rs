// SPDX-License-Identifier: GPL-3.0-or-later
//! From Your Shelf on the Daily tab (shelf design §7): what the last Kindle shared, the switch,
//! and forgetting it all.

use std::path::Path;

use deckle_digest::chrono::{DateTime, Utc};
use deckle_digest::shelf::store::Shelf;
use serde::Serialize;

/// What the Daily tab's status line needs.
#[derive(Clone, Debug, Default, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ShelfStatus {
    /// The Kindle that was plugged in last.
    pub kindle: Option<String>,
    /// When, as the snapshot recorded it (`YYYY-MM-DDTHH:MM:SSZ`).
    pub seen: Option<String>,
    pub in_progress: usize,
    pub highlights_shared: bool,
    pub words_shared: bool,
    /// Email delivery is on: the page then passes through Amazon.
    pub email_on: bool,
    /// `[sync] shelf`: the page reads the Kindle at all.
    pub enabled: bool,
}

fn load_config(digest: &Path) -> Result<deckle_digest::Config, String> {
    match digest.exists() {
        true => deckle_digest::Config::load(digest),
        false => deckle_digest::Config::parse(deckle_digest::config::STARTER),
    }
}

pub fn status_at(shelf: &Path, digest: &Path, now: DateTime<Utc>) -> ShelfStatus {
    let cfg = load_config(digest).unwrap_or_default();
    let shelf = Shelf::load(shelf);
    let newest = shelf.kindles.values().max_by(|a, b| a.seen.cmp(&b.seen));
    ShelfStatus {
        kindle: newest.map(|k| k.name.clone()),
        seen: newest.map(|k| k.seen.clone()),
        in_progress: newest.map_or(0, |k| deckle_shelf::in_progress(k, now)),
        highlights_shared: newest.is_some_and(|k| k.highlights_shared),
        words_shared: newest.is_some_and(|k| k.words_shared),
        email_on: cfg.email.enabled,
        enabled: cfg.sync.shelf,
    }
}

/// Deletes every snapshot: the books, highlights and words the Kindles shared.
pub fn forget_at(shelf: &Path, digest: &Path) -> Result<ShelfStatus, String> {
    Shelf::forget(shelf)?;
    Ok(status_at(shelf, digest, Utc::now()))
}

/// Writes `[sync] shelf`, keeping the file's comments. Returns the new value.
pub fn set_at(digest: &Path, on: bool) -> Result<bool, String> {
    let mut cfg = load_config(digest)?;
    if cfg.sync.shelf != on || !digest.exists() {
        cfg.sync.shelf = on;
        deckle_digest::config::save(digest, &cfg)?;
    }
    Ok(on)
}

// ---- Tauri commands -----------------------------------------------------------------

fn paths() -> (std::path::PathBuf, std::path::PathBuf) {
    let digest = deckle_digest::config::default_path();
    (deckle_digest::Config::default().shelf_store(), digest)
}

#[tauri::command]
pub fn shelf_status() -> ShelfStatus {
    let (s, d) = paths();
    status_at(&s, &d, Utc::now())
}

#[tauri::command]
pub fn shelf_forget() -> Result<ShelfStatus, String> {
    let (s, d) = paths();
    forget_at(&s, &d)
}

#[tauri::command]
pub fn shelf_set(on: bool) -> Result<ShelfStatus, String> {
    let (s, d) = paths();
    set_at(&d, on)?;
    Ok(status_at(&s, &d, Utc::now()))
}

#[cfg(test)]
mod tests {
    use std::path::Path;

    use deckle_digest::shelf::store::{BookSnap, KindleSnapshot, Shelf};

    use super::*;

    fn now() -> DateTime<Utc> {
        "2026-10-22T12:00:00Z".parse().unwrap()
    }

    fn book(title: &str, pct: i64) -> BookSnap {
        BookSnap {
            file: format!("{title}.kfx"),
            title: title.into(),
            last: Some(pct),
            end: Some(100),
            last_read: Some("2026-10-21T16:00:00Z".into()),
            ..Default::default()
        }
    }

    fn setup(dir: &Path, snap: KindleSnapshot) -> (std::path::PathBuf, std::path::PathBuf) {
        let (shelf, digest) = (dir.join("shelf.json"), dir.join("digest.toml"));
        let mut s = Shelf::default();
        s.put("S", snap);
        s.save(&shelf).unwrap();
        (shelf, digest)
    }

    fn scribe() -> KindleSnapshot {
        KindleSnapshot {
            name: "Kindle Scribe".into(),
            seen: "2026-10-21T20:00:00Z".into(),
            books: vec![book("Walden", 40), book("Done", 99)],
            ..Default::default()
        }
    }

    #[test]
    fn status_says_what_the_kindle_shares() {
        let tmp = tempfile::tempdir().unwrap();
        let (shelf, digest) = setup(tmp.path(), scribe());
        let s = status_at(&shelf, &digest, now());
        assert_eq!(s.kindle.as_deref(), Some("Kindle Scribe"));
        assert_eq!(s.seen.as_deref(), Some("2026-10-21T20:00:00Z"));
        assert_eq!(s.in_progress, 1);
        assert!(!s.highlights_shared && !s.words_shared);
        assert!(!s.email_on);
        assert!(s.enabled);
        assert_eq!(
            serde_json::to_value(&s).unwrap()["inProgress"],
            1,
            "camelCase over IPC"
        );
    }

    #[test]
    fn email_and_the_switch_come_from_digest_toml() {
        let tmp = tempfile::tempdir().unwrap();
        let (shelf, digest) = setup(tmp.path(), scribe());
        std::fs::write(
            &digest,
            "[sync]\nshelf = false\n\n[email]\nenabled = true\nto = \"a@kindle.com\"\n\
             from = \"me@example.com\"\nsmtp_host = \"smtp.example.com\"\nusername = \"me\"\n",
        )
        .unwrap();
        let s = status_at(&shelf, &digest, now());
        assert!(s.email_on);
        assert!(!s.enabled);
    }

    #[test]
    fn nothing_recorded_yet() {
        let tmp = tempfile::tempdir().unwrap();
        let s = status_at(
            &tmp.path().join("shelf.json"),
            &tmp.path().join("digest.toml"),
            now(),
        );
        assert_eq!((s.kindle, s.seen, s.in_progress), (None, None, 0));
    }

    #[test]
    fn the_newest_kindle_speaks() {
        let tmp = tempfile::tempdir().unwrap();
        let (shelf, digest) = setup(tmp.path(), scribe());
        let mut s = Shelf::load(&shelf);
        s.put(
            "P",
            KindleSnapshot {
                name: "Kindle Paperwhite".into(),
                seen: "2026-10-22T09:00:00Z".into(),
                highlights_shared: true,
                words_shared: true,
                ..Default::default()
            },
        );
        s.save(&shelf).unwrap();
        let st = status_at(&shelf, &digest, now());
        assert_eq!(st.kindle.as_deref(), Some("Kindle Paperwhite"));
        assert!(st.highlights_shared && st.words_shared);
    }

    #[test]
    fn forget_removes_every_snapshot_and_set_writes_the_switch() {
        let tmp = tempfile::tempdir().unwrap();
        let (shelf, digest) = setup(tmp.path(), scribe());
        let after = forget_at(&shelf, &digest).unwrap();
        assert!(!shelf.exists());
        assert_eq!(after.kindle, None);

        let after = set_at(&digest, false).unwrap_or_else(|e| panic!("{e}"));
        assert!(!after);
        let cfg = deckle_digest::Config::load(&digest).unwrap();
        assert!(!cfg.sync.shelf);
        assert!(set_at(&digest, true).unwrap());
        assert!(deckle_digest::Config::load(&digest).unwrap().sync.shelf);
    }
}
