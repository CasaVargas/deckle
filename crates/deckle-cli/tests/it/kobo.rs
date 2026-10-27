// SPDX-License-Identifier: GPL-3.0-or-later
//! A Kobo (other readers design §8): found through `DECKLE_MOUNTS`, sent KEPUBs with no Kindle
//! Previewer, pruned, and read for highlights.

use std::path::{Path, PathBuf};

use assert_cmd::Command;

fn deckle(home: &Path, mounts: &Path) -> Command {
    let mut c = Command::cargo_bin("deckle").unwrap();
    c.env("HOME", home)
        .env("XDG_CONFIG_HOME", home)
        .env("APPDATA", home)
        .env("LOCALAPPDATA", home)
        .env("XDG_CACHE_HOME", home.join("cache"))
        .env("DECKLE_MOUNTS", mounts)
        // Never a real Previewer: a Kobo doesn't need one.
        .env("DECKLE_PREVIEWER", home.join("no-previewer"))
        .env("PATH", "/nonexistent");
    c
}

fn config_dir(home: &Path) -> PathBuf {
    if cfg!(target_os = "macos") {
        home.join("Library/Application Support/deckle")
    } else {
        home.join("deckle")
    }
}

/// A Kobo's drive: `.kobo/version` and a database with one highlight in a book being read.
fn kobo(root: &Path) {
    std::fs::create_dir_all(root.join(".kobo")).unwrap();
    std::fs::write(
        root.join(".kobo/version"),
        "N4181,4.1.15,4.38.21908,4.1.15,4.1.15,00000000-0000-0000-0000-000000000390\n",
    )
    .unwrap();
    let db = rusqlite::Connection::open(root.join(".kobo/KoboReader.sqlite")).unwrap();
    db.execute_batch(
        "CREATE TABLE content (ContentID TEXT, ContentType TEXT, Title TEXT, Attribution TEXT, \
         ___PercentRead INTEGER, DateLastRead TEXT, ReadStatus INTEGER, BookID TEXT); \
         CREATE TABLE Bookmark (BookmarkID TEXT, VolumeID TEXT, ContentID TEXT, Text TEXT, \
         Annotation TEXT, DateCreated TEXT); \
         INSERT INTO content VALUES ('file:///mnt/onboard/mill.epub', '6', 'The Mill', 'Ann Lee', \
         30, '2026-10-22T20:00:00Z', 1, NULL); \
         INSERT INTO Bookmark VALUES ('b1', 'file:///mnt/onboard/mill.epub', 'x', \
         'The wheel turned.', NULL, '2026-10-22T20:00:00Z');",
    )
    .unwrap();
}

fn setup(digest: &str) -> (tempfile::TempDir, PathBuf, PathBuf) {
    let tmp = tempfile::tempdir().unwrap();
    let home = tmp.path().join("home");
    let dir = config_dir(&home);
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(dir.join("digest.toml"), digest).unwrap();
    let root = tmp.path().join("KOBOeReader");
    kobo(&root);
    (tmp, home, root)
}

fn names(dir: &Path) -> Vec<String> {
    let mut n: Vec<String> = std::fs::read_dir(dir)
        .map(|rd| {
            rd.map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
                .collect()
        })
        .unwrap_or_default();
    n.sort();
    n
}

#[test]
fn the_morning_paper_goes_to_a_kobo_with_no_previewer() {
    let (_tmp, home, root) = setup("sections = [\"horoscope\"]\nkeep = 2\n");
    std::fs::create_dir_all(root.join("Deckle")).unwrap();
    for d in ["2026-10-01", "2026-10-02"] {
        std::fs::write(
            root.join(format!("Deckle/deckle-daily-{d}.kepub.epub")),
            b"old",
        )
        .unwrap();
    }
    let o = deckle(&home, &root)
        .args(["--json", "digest", "run", "--date", "2026-10-23"])
        .output()
        .unwrap();
    assert!(o.status.success(), "{}", String::from_utf8_lossy(&o.stderr));
    let v: serde_json::Value = serde_json::from_slice(&o.stdout).unwrap();
    assert_eq!(v["run"], serde_json::Value::Null);
    assert_eq!(v["epub_queued"], true);
    let notes = v["build"]["notes"].to_string();
    assert!(notes.contains("no Kindle copy"), "{notes}");
    assert_eq!(
        names(&root.join("Deckle")),
        [
            "deckle-daily-2026-10-02.kepub.epub",
            "deckle-daily-2026-10-23.kepub.epub"
        ]
    );
    let sent = std::fs::read(root.join("Deckle/deckle-daily-2026-10-23.kepub.epub")).unwrap();
    let mut zip = zip::ZipArchive::new(std::io::Cursor::new(sent)).unwrap();
    let mut any_span = false;
    for i in 0..zip.len() {
        let mut f = zip.by_index(i).unwrap();
        let mut s = String::new();
        if f.name().unwrap().ends_with(".xhtml") {
            std::io::Read::read_to_string(&mut f, &mut s).unwrap();
            any_span |= s.contains("class=\"koboSpan\"");
        }
    }
    assert!(any_span, "the paper went over as a KEPUB");
    // Remembered: the next run queues the paper for it even when it's unplugged.
    let seen = std::fs::read_to_string(home.join(".config/com.casavargas.deckle/readers.json"))
        .or_else(|_| {
            std::fs::read_to_string(
                home.join("Library/Application Support/com.casavargas.deckle/readers.json"),
            )
        })
        .or_else(|_| std::fs::read_to_string(home.join("com.casavargas.deckle/readers.json")))
        .unwrap();
    assert!(seen.contains("kobo/N4181"), "{seen}");
}

#[test]
fn a_book_goes_to_a_kobo_when_no_kindle_is_plugged_in() {
    let (_tmp, home, root) = setup("");
    let book = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../fixtures/mini/deckle-mini.epub");
    let o = deckle(&home, &root)
        .args(["--json", "run"])
        .arg(&book)
        .output()
        .unwrap();
    assert!(o.status.success(), "{}", String::from_utf8_lossy(&o.stderr));
    let v: serde_json::Value = serde_json::from_slice(&o.stdout).unwrap();
    assert_eq!(v[0]["error"], serde_json::Value::Null);
    assert_eq!(names(&root.join("Deckle")), ["deckle-mini.kepub.epub"]);
}

#[test]
fn highlights_export_reads_a_kobo_when_no_kindle_is_plugged_in() {
    let (tmp, home, root) = setup("");
    let out = tmp.path().join("highlights");
    let o = deckle(&home, &root)
        .args(["highlights", "export", "--to"])
        .arg(&out)
        .output()
        .unwrap();
    assert!(o.status.success(), "{}", String::from_utf8_lossy(&o.stderr));
    let written = names(&out);
    assert_eq!(written.len(), 1, "{written:?}");
    let md = std::fs::read_to_string(out.join(&written[0])).unwrap();
    assert!(md.contains("The wheel turned."), "{md}");
}

#[test]
fn a_push_sends_what_waits_in_the_kobo_outbox() {
    let (_tmp, home, root) = setup("sections = [\"horoscope\"]\n");
    // Queued while the Kobo was away: today's paper, built and left in the outbox.
    let o = deckle(&home, Path::new("/nonexistent-mounts"))
        .args(["--json", "digest", "run", "--date", "2026-10-23"])
        .output()
        .unwrap();
    // No Kobo seen yet and no Previewer: nothing to send it to, as before.
    assert!(!o.status.success());
    // Plugged in once: from now on it's remembered.
    deckle(&home, &root)
        .args(["digest", "push"])
        .assert()
        .success();
    let o = deckle(&home, Path::new("/nonexistent-mounts"))
        .args(["--json", "digest", "run", "--date", "2026-10-23"])
        .output()
        .unwrap();
    assert!(o.status.success(), "{}", String::from_utf8_lossy(&o.stderr));
    assert!(names(&root.join("Deckle")).is_empty());
    let o = deckle(&home, &root)
        .args(["digest", "push"])
        .output()
        .unwrap();
    assert!(o.status.success(), "{}", String::from_utf8_lossy(&o.stderr));
    assert_eq!(
        names(&root.join("Deckle")),
        ["deckle-daily-2026-10-23.kepub.epub"],
        "{}",
        String::from_utf8_lossy(&o.stdout)
    );
}
