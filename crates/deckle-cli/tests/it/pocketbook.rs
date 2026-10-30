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
fn koreaders_place_in_the_paper_on_a_pocketbook_is_its_readership() {
    let (_tmp, home, root) = setup();
    let o = deckle(&home, &root)
        .args(["digest", "run", "--date", "2026-10-23"])
        .output()
        .unwrap();
    assert!(o.status.success(), "{}", String::from_utf8_lossy(&o.stderr));
    // no Kindle, no KFX: the layout is the EPUB's (KOReader hub design §C)
    let dir = if cfg!(target_os = "macos") {
        home.join("Library/Application Support/deckle")
    } else {
        home.join("deckle")
    };
    let file = dir.join("readership.json");
    let state = deckle_readership::State::load(&file);
    let layout = &state.editions[&"2026-10-23".parse().unwrap()];
    assert!(!layout.is_kfx(), "{layout:?}");
    let epub = layout.epub.as_ref().unwrap();
    assert_eq!(epub.files[..2], ["front.xhtml", "s1.xhtml"]);
    // KOReader opens it and reads into the horoscope (s1.xhtml, the second file)
    std::fs::create_dir_all(root.join("Deckle/deckle-daily-2026-10-23.sdr")).unwrap();
    std::fs::write(
        root.join("Deckle/deckle-daily-2026-10-23.sdr/metadata.epub.lua"),
        "return {\n    [\"last_xpointer\"] = \"/body/DocFragment[2]/body/div/p[3]/text().4\",\n    \
         [\"percent_finished\"] = 0.8,\n}\n",
    )
    .unwrap();
    let o = deckle(&home, &root)
        .args(["digest", "push"])
        .output()
        .unwrap();
    assert!(o.status.success(), "{}", String::from_utf8_lossy(&o.stderr));
    let text = String::from_utf8_lossy(&o.stdout);
    assert!(text.contains("readership: 1 edition read"), "{text}");
    let state = deckle_readership::State::load(&file);
    let reads = &state.reads[&"2026-10-23".parse().unwrap()];
    let read = &reads["koreader/pocketbook/PB632"];
    assert!(read.opened);
    let horoscope = layout
        .sections
        .iter()
        .find(|s| s.key == "horoscope")
        .unwrap();
    assert_eq!(read.furthest, Some(horoscope.start + 1));
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
fn a_pocketbook_without_koreader_has_no_highlights_yet() {
    let (tmp, home, root) = setup();
    // Only KOReader's sidecars are read on a PocketBook (KOReader reads design §6).
    let o = deckle(&home, &root)
        .args(["highlights", "export", "--to"])
        .arg(tmp.path().join("out"))
        .output()
        .unwrap();
    assert!(o.status.success(), "{}", String::from_utf8_lossy(&o.stderr));
    assert!(String::from_utf8_lossy(&o.stdout).contains("no highlights on"));
    assert!(!tmp.path().join("out").exists());
}

/// A KOReader sidecar with one highlight and a note (KOReader reads design §4).
const SIDECAR: &str = r#"return {
    ["annotations"] = {
        [1] = {
            ["datetime"] = "2026-10-21 08:15:03",
            ["drawer"] = "lighten",
            ["note"] = "Thoreau at his plainest.",
            ["pageno"] = 12,
            ["pos0"] = "a",
            ["pos1"] = "b",
            ["text"] = "The mass of men lead lives of quiet desperation.",
        },
    },
    ["doc_props"] = { ["authors"] = "Henry David Thoreau", ["title"] = "Walden" },
    ["percent_finished"] = 0.42,
}
"#;

#[test]
fn highlights_export_reads_koreader_on_a_pocketbook() {
    let (tmp, home, root) = setup();
    std::fs::create_dir_all(root.join("Books/Walden.sdr")).unwrap();
    std::fs::write(root.join("Books/Walden.sdr/metadata.epub.lua"), SIDECAR).unwrap();
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
    assert!(md.contains("quiet desperation"), "{md}");
    assert!(md.contains("Thoreau at his plainest."), "{md}");
}

#[test]
fn highlights_from_a_folder_read_koreader_sidecars_in_it() {
    // A Boox's folder, synced back to the computer by a two-way sync app.
    let (tmp, home, _root) = setup();
    let folder = tmp.path().join("Boox");
    std::fs::create_dir_all(folder.join("Walden.sdr")).unwrap();
    std::fs::write(folder.join("Walden.sdr/metadata.epub.lua"), SIDECAR).unwrap();
    let o = deckle(&home, Path::new("/nonexistent-mounts"))
        .args(["--json", "highlights", "--from"])
        .arg(&folder)
        .arg("list")
        .output()
        .unwrap();
    assert!(o.status.success(), "{}", String::from_utf8_lossy(&o.stderr));
    let rows: serde_json::Value = serde_json::from_slice(&o.stdout).unwrap();
    assert_eq!(rows[0]["title"], "Walden", "{rows}");
    assert_eq!(rows[0]["total"], 2, "a highlight and its note: {rows}");
}
