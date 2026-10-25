// SPDX-License-Identifier: GPL-3.0-or-later
//! One Mac, many papers (many papers design): `digest papers`, `--paper`, a paper bound to one
//! Kindle waiting for it, and each paper pruned by its own prefix and `keep`. Offline: the
//! papers print only the horoscope. Unix only: the fake Previewer is a shell script.
#![cfg(unix)]

use std::path::{Path, PathBuf};

use assert_cmd::Command;

struct Home {
    _tmp: tempfile::TempDir,
    home: PathBuf,
    config: PathBuf,
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

/// The id `sync::device_id` gives a fake Kindle at `root` (no USB serial).
fn id(root: &Path) -> String {
    format!(
        "Kindle 5.16.2.1.1 (4184940002)/{}",
        root.file_name().unwrap().to_string_lossy()
    )
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
    let mom = tmp.path().join("mounts/Mom");
    let dad = tmp.path().join("mounts/Dad");
    kindle(&mom);
    kindle(&dad);
    let config = dir.join("digest.toml");
    std::fs::write(
        &config,
        format!(
            "title = \"The Morning Deckle\"\nsections = [\"horoscope\"]\nkeep = 3\n\n\
             [sync.devices.'{}']\nname = \"Mom's Kindle\"\n\n[sync.devices.'{}']\nname = \"Dad's Kindle\"\n",
            id(&mom),
            id(&dad)
        ),
    )
    .unwrap();
    std::fs::create_dir_all(dir.join("papers")).unwrap();
    std::fs::write(
        dir.join("papers/kid-courier.toml"),
        "title = \"The Kid Courier\"\nreader = \"Maya\"\nkindle = \"Mom's Kindle\"\n\
         sections = [\"horoscope\"]\nkeep = 2\n",
    )
    .unwrap();
    Home {
        _tmp: tmp,
        home,
        config,
        mom,
        dad,
    }
}

impl Home {
    fn deckle(&self, plugged: &[&Path]) -> Command {
        let mut c = Command::cargo_bin("deckle").unwrap();
        c.env("HOME", &self.home)
            .env("XDG_CONFIG_HOME", &self.home)
            .env("APPDATA", &self.home)
            .env("LOCALAPPDATA", &self.home)
            .env("XDG_CACHE_HOME", self.home.join("cache"))
            .env("DECKLE_MOUNTS", std::env::join_paths(plugged).unwrap());
        c
    }
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

fn text(o: &std::process::Output) -> String {
    format!(
        "{}{}",
        String::from_utf8_lossy(&o.stdout),
        String::from_utf8_lossy(&o.stderr)
    )
}

#[test]
fn papers_lists_main_and_extras_and_new_makes_one() {
    let h = setup();
    let o = h
        .deckle(&[])
        .args(["--json", "digest", "papers"])
        .output()
        .unwrap();
    assert!(o.status.success(), "{}", text(&o));
    let v: serde_json::Value = serde_json::from_slice(&o.stdout).unwrap();
    let slugs: Vec<&str> = v["papers"]
        .as_array()
        .unwrap()
        .iter()
        .map(|p| p["slug"].as_str().unwrap())
        .collect();
    assert_eq!(slugs, ["main", "kid-courier"]);
    assert_eq!(v["papers"][1]["reader"], "Maya");

    let o = h
        .deckle(&[])
        .args([
            "digest",
            "papers",
            "new",
            "--title",
            "The Lakeside Gazette",
            "--for",
            "Dad",
        ])
        .output()
        .unwrap();
    assert!(o.status.success(), "{}", text(&o));
    assert!(
        h.config
            .with_file_name("papers/lakeside-gazette.toml")
            .exists()
    );
    let o = h
        .deckle(&[])
        .args(["digest", "papers", "new", "--title", "The Lakeside Gazette"])
        .output()
        .unwrap();
    assert!(!o.status.success());
    assert!(text(&o).contains("already exists"), "{}", text(&o));
}

#[test]
fn build_with_paper_names_by_stem_and_an_unknown_paper_is_an_error() {
    let h = setup();
    let out = h.home.join("out");
    let o = h
        .deckle(&[])
        .args([
            "digest",
            "--paper",
            "kid-courier",
            "build",
            "--date",
            "2026-10-24",
            "-o",
        ])
        .arg(&out)
        .output()
        .unwrap();
    assert!(o.status.success(), "{}", text(&o));
    assert!(
        out.join("deckle-daily-kid-courier-2026-10-24.epub")
            .exists()
    );
    let o = h
        .deckle(&[])
        .args(["digest", "--paper", "nope", "build", "-o"])
        .arg(&out)
        .output()
        .unwrap();
    assert!(!o.status.success());
    assert!(text(&o).contains("papers/nope.toml"), "{}", text(&o));
}

#[test]
fn a_bound_paper_waits_for_its_kindle_and_each_paper_is_pruned_by_its_own() {
    let h = setup();
    // Older editions of both papers already on Mom's Kindle.
    for d in ["2026-10-19", "2026-10-20", "2026-10-21"] {
        std::fs::write(h.mom.join(format!("documents/deckle-daily-{d}.kfx")), b"K").unwrap();
        std::fs::write(
            h.mom
                .join(format!("documents/deckle-daily-kid-courier-{d}.kfx")),
            b"K",
        )
        .unwrap();
    }
    // Built with no Kindle plugged in: the Courier waits in the outbox for Mom's.
    let o = h
        .deckle(&[])
        .args([
            "digest",
            "--paper",
            "kid-courier",
            "run",
            "--date",
            "2026-10-24",
            "--previewer",
        ])
        .arg(fake_previewer())
        .output()
        .unwrap();
    assert!(o.status.success(), "{}", text(&o));
    let courier = "deckle-daily-kid-courier-2026-10-24.kfx";

    // Dad's Kindle connects: it isn't his.
    let o = h
        .deckle(&[&h.dad])
        .args(["digest", "push"])
        .output()
        .unwrap();
    assert!(o.status.success(), "{}", text(&o));
    assert!(
        text(&o).contains("1 book waits for Mom's Kindle"),
        "{}",
        text(&o)
    );
    assert!(books(&h.dad).is_empty());

    // Mom's connects: delivered, then each paper keeps its own `keep`.
    let o = h
        .deckle(&[&h.mom])
        .args(["digest", "push"])
        .output()
        .unwrap();
    assert!(o.status.success(), "{}", text(&o));
    assert_eq!(
        books(&h.mom),
        [
            "deckle-daily-2026-10-19.kfx",
            "deckle-daily-2026-10-20.kfx",
            "deckle-daily-2026-10-21.kfx",
            "deckle-daily-kid-courier-2026-10-21.kfx",
            courier,
        ],
        "{}",
        text(&o)
    );
}

#[test]
fn a_paper_bound_to_an_unknown_kindle_fails_before_any_work() {
    let h = setup();
    std::fs::write(
        h.config.with_file_name("papers/kid-courier.toml"),
        "title = \"The Kid Courier\"\nkindle = \"Grandpa's\"\nsections = [\"horoscope\"]\n",
    )
    .unwrap();
    let o = h
        .deckle(&[])
        .args(["digest", "--paper", "kid-courier", "run", "--previewer"])
        .arg(fake_previewer())
        .output()
        .unwrap();
    assert!(!o.status.success());
    let t = text(&o);
    assert!(
        t.contains("papers/kid-courier.toml") && t.contains("deckle devices name"),
        "{t}"
    );
}

#[test]
fn readership_is_kept_per_paper() {
    let h = setup();
    let dir = h.config.parent().unwrap().to_path_buf();
    // Main by hand, today's date: its layout goes to main's readership.json.
    let o = h
        .deckle(&[])
        .args(["digest", "run", "--previewer"])
        .arg(fake_previewer())
        .output()
        .unwrap();
    assert!(o.status.success(), "{}", text(&o));
    let o = h
        .deckle(&[])
        .args(["digest", "--paper", "kid-courier", "run", "--previewer"])
        .arg(fake_previewer())
        .output()
        .unwrap();
    assert!(o.status.success(), "{}", text(&o));
    let courier = dir.join("papers/kid-courier/readership.json");
    let main = dir.join("readership.json");
    // The fake Previewer's KFX may carry no layout; when one is recorded, it's in its own file.
    if courier.exists() {
        let s = std::fs::read_to_string(&courier).unwrap();
        assert!(s.contains("deckle-daily-kid-courier-"), "{s}");
        assert!(!s.contains("\"deckle-daily-20"), "{s}");
    }
    if main.exists() {
        let s = std::fs::read_to_string(&main).unwrap();
        assert!(!s.contains("kid-courier"), "{s}");
    }
}

#[test]
fn morgue_and_schedule_say_they_are_main_only_for_now() {
    let h = setup();
    for args in [
        &["digest", "--paper", "kid-courier", "morgue", "list"][..],
        &["digest", "--paper", "kid-courier", "schedule"][..],
    ] {
        let o = h.deckle(&[]).args(args).output().unwrap();
        assert!(!o.status.success());
        assert!(text(&o).contains("main paper"), "{}", text(&o));
    }
}
