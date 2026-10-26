// SPDX-License-Identifier: GPL-3.0-or-later
//! `deckle digest interests` (interests design §5.5): list, add and remove on the main paper or
//! another paper's own file.

use std::path::PathBuf;

use assert_cmd::Command;
use predicates::prelude::*;

struct Home {
    _tmp: tempfile::TempDir,
    home: PathBuf,
    config: PathBuf,
}

fn setup() -> Home {
    let tmp = tempfile::tempdir().unwrap();
    let home = tmp.path().join("home");
    let dir = if cfg!(target_os = "macos") {
        home.join("Library/Application Support/deckle")
    } else {
        home.join("deckle")
    };
    std::fs::create_dir_all(&dir).unwrap();
    let config = dir.join("digest.toml");
    std::fs::write(
        &config,
        "title = \"The Morning Deckle\"\n# the order\nsections = [\"horoscope\"]\nkeep = 3\n\n\
         [sync]\npush = true\n",
    )
    .unwrap();
    Home {
        _tmp: tmp,
        home,
        config,
    }
}

impl Home {
    fn deckle(&self) -> Command {
        let mut c = Command::cargo_bin("deckle").unwrap();
        c.env("HOME", &self.home)
            .env("XDG_CONFIG_HOME", &self.home)
            .env("APPDATA", &self.home)
            .env("LOCALAPPDATA", &self.home)
            .env("XDG_CACHE_HOME", self.home.join("cache"));
        c
    }
}

#[test]
fn add_list_remove_round_trip_on_the_main_paper() {
    let h = setup();
    h.deckle()
        .args([
            "digest",
            "interests",
            "add",
            "Rust",
            "--not",
            "Rust Belt",
            "--also",
            "Cargo",
        ])
        .assert()
        .success()
        .stdout(predicate::str::contains(
            "added Rust (section \"interests\" added to sections)",
        ));
    let text = std::fs::read_to_string(&h.config).unwrap();
    assert!(
        text.contains("interests")
            && text.contains("name = \"Rust\"")
            && text.contains("Rust Belt")
            && text.contains("# the order")
            && text.contains("sections = [\"horoscope\", \"interests\"]"),
        "{text}"
    );
    h.deckle()
        .args(["digest", "interests", "add", "rust"])
        .assert()
        .failure()
        .stderr(predicate::str::contains("already exists"));
    let o = h
        .deckle()
        .args(["--json", "digest", "interests"])
        .output()
        .unwrap();
    assert!(o.status.success());
    let v: serde_json::Value = serde_json::from_slice(&o.stdout).unwrap();
    assert_eq!(v["interests"][0]["name"], "Rust");
    assert_eq!(v["interests"][0]["also"][0], "Cargo");
    h.deckle()
        .args(["digest", "interests"])
        .assert()
        .success()
        .stdout(predicate::str::contains("Rust — \"Rust\" · 5 stories"));
    h.deckle()
        .args(["digest", "interests", "remove", "Rust"])
        .assert()
        .success()
        .stdout(predicate::str::contains("removed Rust"));
    h.deckle()
        .args(["digest", "interests", "remove", "Rust"])
        .assert()
        .failure()
        .stderr(predicate::str::contains("no interest named"));
}

#[test]
fn add_on_another_paper_writes_its_own_file_only() {
    let h = setup();
    h.deckle()
        .args(["digest", "papers", "new", "--title", "The Kid Courier"])
        .assert()
        .success();
    h.deckle()
        .args([
            "digest",
            "--paper",
            "kid-courier",
            "interests",
            "add",
            "Dinosaurs",
        ])
        .assert()
        .success()
        .stdout(predicate::str::contains("added Dinosaurs"));
    let kid = std::fs::read_to_string(h.config.with_file_name("papers/kid-courier.toml")).unwrap();
    assert!(
        kid.contains("name = \"Dinosaurs\"")
            && !kid.contains("\n[sync]")
            && !kid.contains("push ="),
        "{kid}"
    );
    assert!(
        !std::fs::read_to_string(&h.config)
            .unwrap()
            .contains("Dinosaurs")
    );
}
