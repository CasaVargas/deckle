// SPDX-License-Identifier: GPL-3.0-or-later
//! Several Kindles in one home (Kindle profiles design): two mass-storage Kindles found through
//! `DECKLE_MOUNTS`, named, sent to by name, and a book waiting in the outbox for the one that
//! isn't plugged in. Unix only: the fake Previewer is a shell script.

use std::path::{Path, PathBuf};

use assert_cmd::Command;

struct Home {
    home: tempfile::TempDir,
    mom: PathBuf,
    dad: PathBuf,
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

fn setup() -> Home {
    let home = tempfile::tempdir().unwrap();
    let mom = home.path().join("mounts/Mom");
    let dad = home.path().join("mounts/Dad");
    kindle(&mom);
    kindle(&dad);
    Home { home, mom, dad }
}

impl Home {
    /// `deckle` with these Kindles plugged in.
    fn deckle(&self, plugged: &[&Path]) -> Command {
        let mut c = Command::cargo_bin("deckle").unwrap();
        let h = self.home.path();
        c.env("HOME", h)
            .env("XDG_CONFIG_HOME", h)
            .env("APPDATA", h)
            .env("LOCALAPPDATA", h)
            .env("XDG_CACHE_HOME", h.join("cache"))
            .env("DECKLE_MOUNTS", std::env::join_paths(plugged).unwrap());
        c
    }

    fn name(&self, kindle: &Path, name: &str) {
        self.deckle(&[kindle])
            .args(["devices", "name", name])
            .assert()
            .success();
    }
}

fn mini() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../fixtures/mini/deckle-mini.epub")
}

fn fake_previewer() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../deckle-core/tests/fake_previewer/ok.sh")
}

fn books(kindle: &Path) -> Vec<String> {
    let mut names: Vec<String> = std::fs::read_dir(kindle.join("documents"))
        .unwrap()
        .flatten()
        .map(|e| e.file_name().to_string_lossy().into_owned())
        .filter(|n| n.ends_with(".kfx"))
        .collect();
    names.sort();
    names
}

fn text(out: &std::process::Output) -> String {
    format!(
        "{}{}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    )
}

#[test]
fn naming_shows_in_the_list_and_a_clash_is_refused() {
    let h = setup();
    h.name(&h.mom, "Mom's Kindle");
    let out = h.deckle(&[&h.mom, &h.dad]).arg("devices").output().unwrap();
    let listed = text(&out);
    assert!(listed.contains("Mom's Kindle  "), "{listed}");
    let out = h
        .deckle(&[&h.dad])
        .args(["devices", "name", "mom's kindle"])
        .output()
        .unwrap();
    assert!(!out.status.success());
    assert!(text(&out).contains("already called"), "{}", text(&out));
    // Two connected and no choice: refused, not guessed.
    let out = h
        .deckle(&[&h.mom, &h.dad])
        .args(["devices", "name", "Dad's Kindle"])
        .output()
        .unwrap();
    assert!(!out.status.success());
    assert!(text(&out).contains("--kindle"), "{}", text(&out));
}

#[test]
fn run_sends_to_the_named_kindle_of_two_connected() {
    let h = setup();
    h.name(&h.mom, "Mom's Kindle");
    h.name(&h.dad, "Dad's Kindle");
    let out = h
        .deckle(&[&h.mom, &h.dad])
        .args(["run", "--kindle", "dad's kindle", "--previewer"])
        .arg(fake_previewer())
        .arg(mini())
        .output()
        .unwrap();
    assert!(out.status.success(), "{}", text(&out));
    assert_eq!(books(&h.mom), Vec::<String>::new());
    assert_eq!(books(&h.dad).len(), 1, "{}", text(&out));
}

#[test]
fn a_book_for_an_absent_kindle_waits_until_it_connects() {
    let h = setup();
    h.name(&h.mom, "Mom's Kindle");
    h.name(&h.dad, "Dad's Kindle");
    let out = h
        .deckle(&[&h.dad])
        .args(["--json", "run", "--kindle", "Mom's Kindle", "--previewer"])
        .arg(fake_previewer())
        .arg(mini())
        .output()
        .unwrap();
    assert!(out.status.success(), "{}", text(&out));
    let queued: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(queued[0]["kindle"], "Mom's Kindle");
    assert!(queued[0]["queued"].as_str().unwrap().ends_with(".kfx"));
    assert_eq!(books(&h.dad), Vec::<String>::new(), "nothing went to Dad's");

    // Dad's Kindle connects: the book stays queued, and the push says for whom.
    let out = h
        .deckle(&[&h.dad])
        .args(["digest", "push"])
        .output()
        .unwrap();
    assert!(out.status.success(), "{}", text(&out));
    assert!(
        text(&out).contains("1 book waits for Mom's Kindle"),
        "{}",
        text(&out)
    );
    assert_eq!(books(&h.dad), Vec::<String>::new());

    // Mom's connects: it's delivered there.
    let out = h
        .deckle(&[&h.mom])
        .args(["digest", "push"])
        .output()
        .unwrap();
    assert!(out.status.success(), "{}", text(&out));
    assert_eq!(books(&h.mom).len(), 1, "{}", text(&out));
}

#[test]
fn an_unknown_kindle_or_a_folder_target_is_refused() {
    let h = setup();
    h.name(&h.mom, "Mom's Kindle");
    let out = h
        .deckle(&[&h.mom])
        .args(["run", "--kindle", "Grandpa's", "--previewer"])
        .arg(fake_previewer())
        .arg(mini())
        .output()
        .unwrap();
    assert!(!out.status.success());
    assert!(text(&out).contains("known: Mom's Kindle"), "{}", text(&out));
    let out = h
        .deckle(&[&h.mom])
        .args(["run", "--kindle", "Mom's Kindle", "--to"])
        .arg(h.home.path().join("out"))
        .arg(mini())
        .output()
        .unwrap();
    assert!(!out.status.success());
    assert!(
        text(&out).contains("--kindle sends to a Kindle"),
        "{}",
        text(&out)
    );
}
