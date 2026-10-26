// SPDX-License-Identifier: GPL-3.0-or-later
//! Learning on a connection (interests design §5): a push to a fake Kindle reads the edition's
//! `.yjf` and the clippings, and `interests.json` gains the hits. Unix only: the fake Previewer
//! is a shell script.
#![cfg(unix)]

use std::path::{Path, PathBuf};

use assert_cmd::Command;
use deckle_readership::layout::{Layout, SectionStart, StoryStart};
use deckle_readership::state::State;

fn config_dir(home: &Path) -> PathBuf {
    if cfg!(target_os = "macos") {
        home.join("Library/Application Support/deckle")
    } else {
        home.join("deckle")
    }
}

fn kindle(root: &Path) {
    std::fs::create_dir_all(root.join("documents")).unwrap();
    std::fs::create_dir_all(root.join("system")).unwrap();
    std::fs::write(
        root.join("system/version.txt"),
        "Kindle 5.16.2.1.1 (4184940002)\n",
    )
    .unwrap();
}

const STEM: &str = "deckle-daily-2026-10-20";

/// A home with one paper and one recorded edition on a fake Kindle that was read: the `.yjf`
/// is the Scribe's (jumps from 821, 4 and 1015) and the clippings hold one paper highlight.
fn setup(learning: &str) -> (tempfile::TempDir, PathBuf, PathBuf) {
    let tmp = tempfile::tempdir().unwrap();
    let home = tmp.path().join("home");
    let dir = config_dir(&home);
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(
        dir.join("digest.toml"),
        format!(
            "title = \"The Deckle Daily\"\nsections = [\"horoscope\"]\n\n[sync]\nreadership = true\n\n{learning}"
        ),
    )
    .unwrap();
    let mut state = State::default();
    let sec = |key: &str, name: &str, start, stories: Vec<(&str, i64)>| SectionStart {
        key: key.into(),
        name: name.into(),
        start,
        items: None,
        stories: stories
            .into_iter()
            .map(|(t, s)| StoryStart {
                title: t.into(),
                start: s,
            })
            .collect(),
    };
    state.record_layout(Layout {
        date: "2026-10-20".parse().unwrap(),
        title: "The Deckle Daily".into(),
        file: STEM.into(),
        end: 200_000,
        sections: vec![
            sec("front", "Front page", 0, vec![]),
            sec(
                "interests:Panthers",
                "Panthers",
                500,
                vec![("Bryce Young benched", 800), ("Panthers trade talk", 1010)],
            ),
            sec("horoscope", "Horoscope", 100_000, vec![]),
        ],
    });
    state.save(&dir.join("readership.json")).unwrap();
    let mount = tmp.path().join("mounts/Scribe");
    kindle(&mount);
    let docs = mount.join("documents");
    std::fs::write(docs.join(format!("{STEM}.kfx")), b"not really a kfx").unwrap();
    std::fs::create_dir_all(docs.join(format!("{STEM}.sdr"))).unwrap();
    let yjf = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../fixtures/yjf/daily-2026-10-21.yjf");
    std::fs::copy(&yjf, docs.join(format!("{STEM}.sdr/{STEM}.yjf"))).unwrap();
    std::fs::write(
        docs.join("My Clippings.txt"),
        "The Deckle Daily — Tuesday, October 20, 2026 (Deckle)\n- Your Highlight on page 3 | Location 40-41 | Added on Tuesday, October 20, 2026 7:14:22 AM\n\nBryce Young looked lost against the Bucs\n==========\n",
    )
    .unwrap();
    (tmp, home, mount)
}

fn push(home: &Path, mount: &Path) -> Command {
    let mut c = Command::cargo_bin("deckle").unwrap();
    c.env("HOME", home)
        .env("XDG_CONFIG_HOME", home)
        .env("APPDATA", home)
        .env("LOCALAPPDATA", home)
        .env("XDG_CACHE_HOME", home.join("cache"))
        .env("DECKLE_MOUNTS", mount)
        .args(["digest", "push"]);
    c
}

#[test]
fn a_push_records_taps_and_paper_highlights_into_interests_json() {
    let (_tmp, home, mount) = setup("");
    let out = push(&home, &mount).output().unwrap();
    assert!(
        out.status.success(),
        "{}{}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );
    let file = config_dir(&home).join("interests.json");
    let l = deckle_readership::Learning::load(&file);
    assert!(file.is_file(), "interests.json written");
    let young = l
        .candidates
        .get("bryce young")
        .unwrap_or_else(|| panic!("{:?}", l.candidates.keys()));
    let signals: Vec<_> = young.hits.iter().map(|h| h.signal).collect();
    assert!(
        signals.contains(&deckle_readership::learn::Signal::Tap),
        "a jump from 821 lands in \"Bryce Young benched\": {signals:?}"
    );
    assert!(
        signals.contains(&deckle_readership::learn::Signal::Highlight),
        "the clipping: {signals:?}"
    );
    assert!(
        l.candidates.contains_key("panthers"),
        "{:?}",
        l.candidates.keys()
    );
}

#[test]
fn with_learning_off_nothing_is_written() {
    let (_tmp, home, mount) = setup("[learning]\nenabled = false\n");
    push(&home, &mount).assert().success();
    assert!(!config_dir(&home).join("interests.json").exists());
}
