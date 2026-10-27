// SPDX-License-Identifier: GPL-3.0-or-later
//! A PocketBook (other readers design, phase 3): found through `DECKLE_MOUNTS`, sent the paper and
//! books as EPUB with no Kindle Previewer, and pruned.

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
        .env("DECKLE_PREVIEWER", home.join("no-previewer"))
        .env("PATH", "/nonexistent");
    c
}

fn setup() -> (tempfile::TempDir, PathBuf, PathBuf) {
    let tmp = tempfile::tempdir().unwrap();
    let home = tmp.path().join("home");
    let dir = if cfg!(target_os = "macos") {
        home.join("Library/Application Support/deckle")
    } else {
        home.join("deckle")
    };
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(
        dir.join("digest.toml"),
        "sections = [\"horoscope\"]\nkeep = 1\n",
    )
    .unwrap();
    let root = tmp.path().join("PB632");
    std::fs::create_dir_all(root.join("system/explorer-3")).unwrap();
    std::fs::create_dir_all(root.join("system/config")).unwrap();
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
fn the_morning_paper_goes_to_a_pocketbook_as_an_epub() {
    let (_tmp, home, root) = setup();
    std::fs::create_dir_all(root.join("Deckle")).unwrap();
    std::fs::write(root.join("Deckle/deckle-daily-2026-10-01.epub"), b"old").unwrap();
    let o = deckle(&home, &root)
        .args(["digest", "run", "--date", "2026-10-23"])
        .output()
        .unwrap();
    assert!(o.status.success(), "{}", String::from_utf8_lossy(&o.stderr));
    let text = String::from_utf8_lossy(&o.stdout);
    assert!(text.contains("PocketBook:"), "{text}");
    assert_eq!(
        names(&root.join("Deckle")),
        ["deckle-daily-2026-10-23.epub"]
    );
    let sent = std::fs::read(root.join("Deckle/deckle-daily-2026-10-23.epub")).unwrap();
    let mut zip = zip::ZipArchive::new(std::io::Cursor::new(sent)).unwrap();
    for i in 0..zip.len() {
        let mut f = zip.by_index(i).unwrap();
        let mut s = String::new();
        if f.name().unwrap().ends_with(".xhtml") {
            std::io::Read::read_to_string(&mut f, &mut s).unwrap();
            assert!(!s.contains("koboSpan"), "a PocketBook gets plain EPUB");
        }
    }
}

#[test]
fn a_book_goes_to_a_pocketbook_when_no_kindle_is_plugged_in() {
    let (_tmp, home, root) = setup();
    let book = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../fixtures/mini/deckle-mini.epub");
    let o = deckle(&home, &root).arg("run").arg(&book).output().unwrap();
    assert!(o.status.success(), "{}", String::from_utf8_lossy(&o.stderr));
    assert_eq!(names(&root.join("Deckle")), ["deckle-mini.epub"]);
}

#[test]
fn highlights_say_a_pocketbook_has_none_to_read() {
    let (tmp, home, root) = setup();
    // No Kindle and no Kobo: there's nothing to read highlights from.
    let o = deckle(&home, &root)
        .args(["highlights", "export", "--to"])
        .arg(tmp.path().join("out"))
        .output()
        .unwrap();
    assert!(!o.status.success());
}
