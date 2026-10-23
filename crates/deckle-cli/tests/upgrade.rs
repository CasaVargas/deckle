// SPDX-License-Identifier: GPL-3.0-or-later
//! `deckle kindle upgrade` and `put-back` against a folder standing in for a Kindle's
//! documents/ (Books and Kindle design §13).

use std::path::{Path, PathBuf};

use assert_cmd::Command;

fn deckle_at(home: &Path) -> Command {
    let mut c = Command::cargo_bin("deckle").unwrap();
    c.env("HOME", home)
        .env("XDG_CACHE_HOME", home.join(".cache"))
        .env("XDG_CONFIG_HOME", home.join(".config"))
        .env("APPDATA", home.join("AppData"));
    c
}

/// Deckle's config folder for `home`, as `deckle_at` sets it up on each OS.
fn config_dir(home: &Path) -> PathBuf {
    if cfg!(target_os = "macos") {
        home.join("Library/Application Support/deckle")
    } else if cfg!(target_os = "windows") {
        home.join("AppData/deckle")
    } else {
        home.join(".config/deckle")
    }
}

fn fake_previewer() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../deckle-core/tests/fake_previewer/ok.sh")
}

/// A calibre library under `home` holding the fixture EPUB as calibre book `uuid-mini`.
fn calibre_with_fixture(home: &Path) -> (String, String) {
    let epub = deckle_kfx::fixtures::path("deckle-mini.epub");
    let info = deckle_core::inspect::inspect(&epub).unwrap();
    let author = info.creators[0].name.clone();
    let root = home.join("Calibre Library");
    let dir = root.join("Author/Deckle Mini (1)");
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::copy(&epub, dir.join("Deckle Mini - Author.epub")).unwrap();
    let db = rusqlite::Connection::open(root.join("metadata.db")).unwrap();
    db.execute_batch(&format!(
        "CREATE TABLE books (id INTEGER PRIMARY KEY, title TEXT, path TEXT, uuid TEXT);
         CREATE TABLE authors (id INTEGER PRIMARY KEY, name TEXT);
         CREATE TABLE books_authors_link (id INTEGER PRIMARY KEY, book INTEGER, author INTEGER);
         CREATE TABLE identifiers (id INTEGER PRIMARY KEY, book INTEGER, type TEXT, val TEXT);
         CREATE TABLE data (id INTEGER PRIMARY KEY, book INTEGER, format TEXT, name TEXT);
         INSERT INTO books VALUES (1, '{title}', 'Author/Deckle Mini (1)', 'uuid-mini');
         INSERT INTO authors VALUES (1, '{author}');
         INSERT INTO books_authors_link VALUES (1, 1, 1);
         INSERT INTO data VALUES (1, 1, 'EPUB', 'Deckle Mini - Author');",
        title = info.title.replace('\'', "''"),
        author = author.replace('\'', "''"),
    ))
    .unwrap();
    (info.title, author)
}

fn names(dir: &Path) -> Vec<String> {
    let mut v: Vec<String> = std::fs::read_dir(dir)
        .unwrap()
        .flatten()
        .map(|e| e.file_name().to_string_lossy().into_owned())
        .collect();
    v.sort();
    v
}

#[test]
#[cfg(unix)] // the fake Previewer is a shell script
fn a_calibre_book_is_upgraded_kept_and_put_back() {
    let tmp = tempfile::tempdir().unwrap();
    let home = tmp.path().join("home");
    let out = tmp.path().join("Kindle");
    std::fs::create_dir_all(&out).unwrap();
    let (title, author) = calibre_with_fixture(&home);

    // What calibre sent once: an AZW3 that names its calibre book, with reading data.
    let old = "Deckle Mini.azw3";
    let azw3 = deckle_mobi::fixture::build(&deckle_mobi::fixture::Book {
        title: &title,
        authors: &[&author],
        calibre_id: Some("uuid-mini"),
        cde: Some("PDOC"),
        kf8: true,
        ..Default::default()
    });
    std::fs::write(out.join(old), &azw3).unwrap();
    std::fs::create_dir_all(out.join("Deckle Mini.sdr")).unwrap();
    std::fs::write(
        out.join("Deckle Mini.sdr/Deckle Mini.azw3f"),
        b"reading data",
    )
    .unwrap();
    // A store book and a locked loan, which are never touched.
    let store = deckle_mobi::fixture::build(&deckle_mobi::fixture::Book {
        title: "A Store Book",
        cde: Some("EBOK"),
        drm: true,
        ..Default::default()
    });
    std::fs::write(out.join("Store.azw"), &store).unwrap();
    let before_store = std::fs::read(out.join("Store.azw")).unwrap();

    let books = deckle_at(&home)
        .args(["kindle", "books", "--to"])
        .arg(&out)
        .output()
        .unwrap();
    let text = String::from_utf8_lossy(&books.stdout).into_owned();
    assert!(books.status.success(), "{text}");
    assert!(text.contains("original on this computer"), "{text}");
    assert!(text.contains("Amazon's own"), "{text}");

    let dry = deckle_at(&home)
        .args(["kindle", "upgrade", "--dry-run", "--to"])
        .arg(&out)
        .output()
        .unwrap();
    let text = String::from_utf8_lossy(&dry.stdout).into_owned();
    assert!(dry.status.success(), "{text}");
    assert!(text.contains("1 can be upgraded"), "{text}");
    assert!(text.contains("1 Amazon's own, left alone"), "{text}");
    assert!(out.join(old).exists(), "a dry run changes nothing");

    let done = deckle_at(&home)
        .args(["kindle", "upgrade", "--yes", "--previewer"])
        .arg(fake_previewer())
        .args(["--to"])
        .arg(&out)
        .output()
        .unwrap();
    let text = format!(
        "{}{}",
        String::from_utf8_lossy(&done.stdout),
        String::from_utf8_lossy(&done.stderr)
    );
    assert!(done.status.success(), "{text}");
    assert!(text.contains(&format!("replaced {old}")), "{text}");
    let now = names(&out);
    assert!(!now.contains(&old.to_owned()), "{now:?}");
    assert!(!now.contains(&"Deckle Mini.sdr".to_owned()), "{now:?}");
    let new = now
        .iter()
        .find(|n| n.ends_with(".kfx"))
        .expect("Deckle's copy")
        .clone();
    assert_eq!(std::fs::read(out.join("Store.azw")).unwrap(), before_store);

    // The old copy and its reading data are kept, and the upgrade is recorded.
    let config = config_dir(&home);
    let record: serde_json::Value =
        serde_json::from_slice(&std::fs::read(config.join("upgrades.json")).unwrap()).unwrap();
    let u = &record["upgrades"][0];
    assert_eq!(u["old_file"], old);
    assert_eq!(u["new_file"], new.as_str());
    assert_eq!(u["origin"], "calibre");
    let kept = PathBuf::from(u["kept"].as_str().unwrap());
    assert!(kept.starts_with(config.join("kindle-originals")));
    assert_eq!(std::fs::read(kept.join(old)).unwrap(), azw3);
    assert_eq!(
        std::fs::read(kept.join("Deckle Mini.sdr/Deckle Mini.azw3f")).unwrap(),
        b"reading data"
    );

    let back = deckle_at(&home)
        .args(["kindle", "put-back", old, "--to"])
        .arg(&out)
        .output()
        .unwrap();
    let text = String::from_utf8_lossy(&back.stdout).into_owned();
    assert!(
        back.status.success(),
        "{text}{}",
        String::from_utf8_lossy(&back.stderr)
    );
    assert_eq!(std::fs::read(out.join(old)).unwrap(), azw3);
    assert_eq!(
        std::fs::read(out.join("Deckle Mini.sdr/Deckle Mini.azw3f")).unwrap(),
        b"reading data"
    );
    assert!(!out.join(&new).exists(), "Deckle's copy goes");
    assert!(!kept.exists());
    let record: serde_json::Value =
        serde_json::from_slice(&std::fs::read(config.join("upgrades.json")).unwrap()).unwrap();
    assert_eq!(record["upgrades"].as_array().unwrap().len(), 0);
}

#[test]
fn a_book_partway_through_waits_to_be_named() {
    let tmp = tempfile::tempdir().unwrap();
    let home = tmp.path().join("home");
    let out = tmp.path().join("Kindle");
    std::fs::create_dir_all(&out).unwrap();
    let (title, author) = calibre_with_fixture(&home);
    let azw3 = deckle_mobi::fixture::build(&deckle_mobi::fixture::Book {
        title: &title,
        authors: &[&author],
        ..Default::default()
    });
    std::fs::write(out.join("Mini.azw3"), azw3).unwrap();
    let dry = || {
        let o = deckle_at(&home)
            .args(["kindle", "upgrade", "--dry-run", "--to"])
            .arg(&out)
            .output()
            .unwrap();
        String::from_utf8_lossy(&o.stdout).into_owned()
    };
    // Not opened yet: in the plan.
    assert!(dry().contains("1 can be upgraded"));
    // 15% in (the Kindle's own count in its .yjf): skipped until it's named.
    std::fs::create_dir_all(out.join("Mini.sdr")).unwrap();
    let yjf =
        std::fs::read(Path::new(env!("CARGO_MANIFEST_DIR")).join("../../fixtures/yjf/book.yjf"))
            .unwrap();
    std::fs::write(out.join("Mini.sdr/Mini.yjf"), yjf).unwrap();
    let text = dry();
    assert!(text.contains("skipped: 15% read"), "{text}");
    assert!(text.contains("nothing to upgrade"), "{text}");
    let named = deckle_at(&home)
        .args(["kindle", "upgrade", "--dry-run", "Mini.azw3", "--to"])
        .arg(&out)
        .output()
        .unwrap();
    assert!(String::from_utf8_lossy(&named.stdout).contains("1 can be upgraded"));
    // Naming a book that has no original is a warning, and nothing is planned.
    let other = deckle_at(&home)
        .args(["kindle", "upgrade", "--dry-run", "Other.azw3", "--to"])
        .arg(&out)
        .output()
        .unwrap();
    assert!(
        String::from_utf8_lossy(&other.stderr).contains("Other.azw3: not a book with an original")
    );
    assert!(String::from_utf8_lossy(&other.stdout).contains("nothing to upgrade"));
}
