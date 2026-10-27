// SPDX-License-Identifier: GPL-3.0-or-later
//! The Kitchen Book's standing delivery (cookbook design §9): `digest watch` rebuilds a changed
//! cookbook and every Kindle gets the newest build on its next plug-in, one file under a fixed
//! name and ASIN. Simulated mass-storage Kindles (`DECKLE_MOUNTS`) and the fake Previewer
//! (`DECKLE_PREVIEWER`); unix only, the fake Previewer is a shell script.

use std::collections::BTreeMap;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::{Duration, Instant, SystemTime};

use assert_cmd::Command;
use deckle_core::deliver::{Conn, Target};
use deckle_digest::chrono::{TimeZone, Utc};
use deckle_digest::cookbook::Recipe;
use deckle_digest::cookbook::book::DEVICE_NAME;
use deckle_digest::cookbook::store::Store;

struct Home {
    tmp: tempfile::TempDir,
    home: PathBuf,
    cookbook: PathBuf,
    mom: PathBuf,
    dad: PathBuf,
    /// Named in `digest.toml` with `cookbook = false`.
    attic: PathBuf,
    /// Never named: a Kindle Deckle hasn't seen.
    guest: PathBuf,
    runs: AtomicUsize,
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

fn fake_previewer() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../deckle-core/tests/fake_previewer/ok.sh")
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
    let mounts = tmp.path().join("mounts");
    let (mom, dad, attic, guest) = (
        mounts.join("Mom"),
        mounts.join("Dad"),
        mounts.join("Attic"),
        mounts.join("Guest"),
    );
    for k in [&mom, &dad, &attic, &guest] {
        kindle(k);
    }
    std::fs::write(
        dir.join("digest.toml"),
        format!(
            "title = \"The Morning Deckle\"\nsections = [\"horoscope\"]\nkeep = 1\n\n\
             [sync.devices.'{}']\nname = \"Mom's Kindle\"\n\n\
             [sync.devices.'{}']\nname = \"Dad's Kindle\"\n\n\
             [sync.devices.'{}']\nname = \"Attic Kindle\"\ncookbook = false\n",
            id(&mom),
            id(&dad),
            id(&attic),
        ),
    )
    .unwrap();
    Home {
        cookbook: tmp.path().join("cookbook"),
        tmp,
        home,
        mom,
        dad,
        attic,
        guest,
        runs: AtomicUsize::new(0),
    }
}

fn text(o: &std::process::Output) -> String {
    format!(
        "{}{}",
        String::from_utf8_lossy(&o.stdout),
        String::from_utf8_lossy(&o.stderr)
    )
}

impl Home {
    fn env(&self, plugged: &[&Path]) -> Vec<(&'static str, std::ffi::OsString)> {
        vec![
            ("HOME", self.home.clone().into()),
            ("XDG_CONFIG_HOME", self.home.clone().into()),
            ("APPDATA", self.home.clone().into()),
            ("LOCALAPPDATA", self.home.clone().into()),
            ("XDG_CACHE_HOME", self.home.join("cache").into()),
            ("DECKLE_MOUNTS", std::env::join_paths(plugged).unwrap()),
            ("DECKLE_PREVIEWER", fake_previewer().into()),
            ("DECKLE_COOKBOOK_DIR", self.cookbook.clone().into()),
        ]
    }

    fn deckle(&self, plugged: &[&Path]) -> Command {
        let mut c = Command::cargo_bin("deckle").unwrap();
        c.envs(self.env(plugged));
        c
    }

    /// `deckle --json digest push` with `plugged` connected.
    fn push(&self, plugged: &[&Path]) -> std::process::Output {
        self.deckle(plugged)
            .args(["--json", "digest", "push"])
            .output()
            .unwrap()
    }

    /// `deckle cookbook <args>` with `plugged` connected.
    fn cookbook_cmd(&self, plugged: &[&Path], args: &[&str]) -> std::process::Output {
        self.deckle(plugged)
            .arg("cookbook")
            .args(args)
            .output()
            .unwrap()
    }

    /// A mount that holds no Kindle: `DECKLE_MOUNTS` can't be empty (that looks at real drives).
    fn nothing(&self) -> PathBuf {
        let dir = self.tmp.path().join("mounts/empty");
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    /// Runs `digest watch` with `plugged` connected until `done` holds for its output, then
    /// `linger` more, and stops it. Its output.
    fn watch(
        &self,
        plugged: &[&Path],
        what: &str,
        linger: Duration,
        done: impl Fn(&str) -> bool,
    ) -> String {
        assert!(
            !plugged.is_empty(),
            "an empty DECKLE_MOUNTS looks at real drives"
        );
        let n = self.runs.fetch_add(1, Ordering::SeqCst);
        let log = self.tmp.path().join(format!("watch-{n}.log"));
        let out = std::fs::File::create(&log).unwrap();
        let mut child = std::process::Command::new(assert_cmd::cargo::cargo_bin("deckle"))
            .envs(self.env(plugged))
            .args(["digest", "watch", "--interval", "1"])
            .stdout(out.try_clone().unwrap())
            .stderr(out)
            .spawn()
            .unwrap();
        let read = || std::fs::read_to_string(&log).unwrap_or_default();
        let start = Instant::now();
        while !done(&read()) {
            if let Ok(Some(status)) = child.try_wait() {
                panic!("watch exited ({status}) before {what}:\n{}", read());
            }
            if start.elapsed() > Duration::from_secs(120) {
                let _ = child.kill();
                let _ = child.wait();
                panic!("timed out waiting for {what}:\n{}", read());
            }
            std::thread::sleep(Duration::from_millis(100));
        }
        std::thread::sleep(linger);
        let _ = child.kill();
        let _ = child.wait();
        read()
    }

    /// Watches with `kindle` plugged in until it has the newest build.
    fn plug_in(&self, kindle: &Path) {
        self.watch(
            &[kindle],
            "the cookbook on the Kindle",
            Duration::ZERO,
            |_| {
                let built = self.build_id();
                built.is_some() && self.delivered_to(kindle) == built
            },
        );
    }

    fn store(&self) -> Store {
        Store::new(self.cookbook.clone())
    }

    /// Saves a recipe (which marks the cookbook changed), added on 2026-10-`day`.
    fn add(&self, title: &str, day: u32) {
        let recipe = Recipe {
            title: title.to_owned(),
            description: None,
            yield_: None,
            prep: None,
            cook: None,
            total: None,
            ingredients: vec![],
            steps: vec![],
            category: vec![],
            cuisine: None,
            author: None,
            image_url: None,
            source_url: format!("https://example.com/{}", title.replace(' ', "-")),
            site_name: "example.com".to_owned(),
            structured: true,
            body_xhtml: None,
        };
        let key = recipe.source_url.clone();
        let at = Utc.with_ymd_and_hms(2026, 10, day, 12, 0, 0).unwrap();
        self.store().put(recipe, &key, None, None, at).unwrap();
    }

    fn build_id(&self) -> Option<String> {
        std::fs::read_to_string(self.cookbook.join("build/build_id"))
            .ok()
            .map(|s| s.trim().to_owned())
            .filter(|s| !s.is_empty())
    }

    fn delivered(&self) -> BTreeMap<String, String> {
        std::fs::read(self.cookbook.join("delivered.json"))
            .ok()
            .and_then(|b| serde_json::from_slice(&b).ok())
            .unwrap_or_default()
    }

    fn delivered_to(&self, kindle: &Path) -> Option<String> {
        self.delivered().get(&id(kindle)).cloned()
    }

    fn outbox(&self) -> PathBuf {
        let cache = if cfg!(target_os = "macos") {
            self.home.join("Library/Caches")
        } else {
            self.home.join("cache")
        };
        cache.join("deckle/outbox")
    }

    /// The cookbook's entry in the outbox, when one is queued.
    fn queued(&self) -> Option<PathBuf> {
        let stem = DEVICE_NAME.strip_suffix(".kfx").unwrap();
        Some(self.outbox().join(format!("{stem}.book"))).filter(|p| p.exists())
    }
}

/// The KFX files in a Kindle's `documents/`, sorted.
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

fn asin(kindle: &Path) -> String {
    let target = Target::Folder(kindle.join("documents"));
    let inv = deckle_core::inventory::list_on(&mut Conn::new(&target), None).unwrap();
    inv.books
        .iter()
        .find(|b| b.file == DEVICE_NAME)
        .and_then(|b| b.asin.clone())
        .expect("the cookbook on the Kindle has an ASIN")
}

fn modified(kindle: &Path) -> SystemTime {
    std::fs::metadata(kindle.join("documents").join(DEVICE_NAME))
        .unwrap()
        .modified()
        .unwrap()
}

fn delivered_json(o: &std::process::Output) -> Vec<serde_json::Value> {
    let v: serde_json::Value = serde_json::from_slice(&o.stdout).unwrap();
    v["flush"]["delivered"]
        .as_array()
        .cloned()
        .unwrap_or_default()
}

#[test]
fn two_builds_one_device_file_same_asin() {
    let h = setup();
    h.add("Lemon Bars", 1);
    h.plug_in(&h.mom);
    let first = h.build_id().unwrap();
    assert_eq!(books(&h.mom), [DEVICE_NAME]);
    let first_asin = asin(&h.mom);
    let first_written = modified(&h.mom);

    h.add("Plum Cake", 2);
    h.watch(&[&h.mom], "the second build", Duration::ZERO, |_| {
        h.delivered_to(&h.mom).is_some_and(|b| b != first)
    });
    let second = h.build_id().unwrap();
    assert_ne!(first, second);
    assert_eq!(h.delivered_to(&h.mom), Some(second));
    assert_eq!(books(&h.mom), [DEVICE_NAME], "one file, overwritten");
    assert!(
        modified(&h.mom) > first_written,
        "the second build was written"
    );
    assert_eq!(asin(&h.mom), first_asin);
}

#[test]
fn rename_keeps_one_device_file() {
    let h = setup();
    h.add("Lemon Bars", 1);
    h.plug_in(&h.mom);
    let first = h.build_id().unwrap();
    let first_asin = asin(&h.mom);

    // The owner renames the book by hand: `cookbook.toml` changes, and no `dirty` is written.
    std::fs::write(
        h.cookbook.join("cookbook.toml"),
        "title = \"Grandma's Recipes\"\n",
    )
    .unwrap();
    assert!(!h.cookbook.join("dirty").exists());
    h.watch(&[&h.mom], "the renamed build", Duration::ZERO, |_| {
        h.delivered_to(&h.mom).is_some_and(|b| b != first)
    });
    assert_eq!(books(&h.mom), [DEVICE_NAME]);
    assert_eq!(asin(&h.mom), first_asin);
}

#[test]
fn both_kindles_get_the_build() {
    let h = setup();
    h.add("Lemon Bars", 1);
    h.plug_in(&h.mom);
    let built = h.build_id().unwrap();
    // The outbox entry went to Mom's Kindle; Dad's still gets the same build.
    assert!(h.queued().is_none());
    h.plug_in(&h.dad);
    assert_eq!(h.build_id(), Some(built.clone()), "no rebuild in between");
    assert_eq!(books(&h.dad), [DEVICE_NAME]);
    assert_eq!(h.delivered_to(&h.mom), Some(built.clone()));
    assert_eq!(h.delivered_to(&h.dad), Some(built));
    assert_eq!(asin(&h.mom), asin(&h.dad));
}

#[test]
fn up_to_date_kindle_gets_no_push() {
    let h = setup();
    h.add("Lemon Bars", 1);
    h.plug_in(&h.mom);
    let written = modified(&h.mom);

    let o = h.push(&[&h.mom]);
    assert!(o.status.success(), "{}", text(&o));
    assert!(delivered_json(&o).is_empty(), "{}", text(&o));
    assert_eq!(modified(&h.mom), written);
    assert!(h.queued().is_none());
}

#[test]
fn kindle_with_cookbook_false_gets_nothing() {
    let h = setup();
    h.add("Lemon Bars", 1);
    h.plug_in(&h.mom);

    let o = h.push(&[&h.attic]);
    assert!(o.status.success(), "{}", text(&o));
    assert!(delivered_json(&o).is_empty(), "{}", text(&o));
    assert!(books(&h.attic).is_empty());
    assert_eq!(h.delivered_to(&h.attic), None);
    assert!(h.queued().is_none(), "nothing queued for it either");
}

#[test]
fn new_kindle_gets_it_on_first_plug_in() {
    let h = setup();
    h.add("Lemon Bars", 1);
    h.plug_in(&h.mom);
    // A Kindle `digest.toml` doesn't name gets the cookbook too (on by default).
    h.plug_in(&h.guest);
    assert_eq!(books(&h.guest), [DEVICE_NAME]);
    assert_eq!(h.delivered_to(&h.guest), h.build_id());
}

#[test]
fn interrupted_push_redelivers_later() {
    let h = setup();
    h.add("Lemon Bars", 1);
    // Built with only the Attic's Kindle plugged in, which doesn't take it.
    h.watch(&[&h.attic], "the first build", Duration::ZERO, |_| {
        h.build_id().is_some()
    });
    let built = h.build_id().unwrap();
    assert!(h.queued().is_none());

    // Mom's push fails partway: her Kindle won't take the file.
    let docs = h.mom.join("documents");
    std::fs::set_permissions(&docs, std::fs::Permissions::from_mode(0o555)).unwrap();
    let o = h.push(&[&h.mom]);
    assert!(!o.status.success(), "{}", text(&o));
    let entry = h.queued().expect("the cookbook stays queued for Mom");
    let owner = std::fs::read_to_string(format!("{}.for", entry.display())).unwrap();
    assert_eq!(owner.trim(), id(&h.mom));
    assert_eq!(h.delivered_to(&h.mom), None);

    // Dad plugs in: the queued entry is his now.
    let o = h.push(&[&h.dad]);
    assert!(o.status.success(), "{}", text(&o));
    assert_eq!(books(&h.dad), [DEVICE_NAME]);
    assert_eq!(h.delivered_to(&h.dad), Some(built.clone()));
    assert!(h.queued().is_none());

    // Mom's next connection still gets it: her record never moved.
    std::fs::set_permissions(&docs, std::fs::Permissions::from_mode(0o755)).unwrap();
    let o = h.push(&[&h.mom]);
    assert!(o.status.success(), "{}", text(&o));
    assert_eq!(books(&h.mom), [DEVICE_NAME]);
    assert_eq!(h.delivered_to(&h.mom), Some(built));
}

#[test]
fn export_folder_push_gets_no_cookbook() {
    let h = setup();
    h.add("Lemon Bars", 1);
    h.plug_in(&h.mom);

    // `push --to <folder>` is an export, not a Kindle: the cookbook stays out of it.
    let folder = h.tmp.path().join("export");
    let o = h
        .deckle(&[&h.mom])
        .args(["--json", "digest", "push", "--to"])
        .arg(&folder)
        .output()
        .unwrap();
    assert!(o.status.success(), "{}", text(&o));
    assert!(!folder.join(DEVICE_NAME).exists(), "{}", text(&o));
    assert_eq!(
        h.delivered().len(),
        1,
        "only Mom's Kindle: {:?}",
        h.delivered()
    );
    assert!(h.queued().is_none());
}

#[test]
fn broken_recipe_is_said_once() {
    let h = setup();
    h.add("Lemon Bars", 1);
    // A hand edit gone wrong: the book can't be built until it's fixed.
    std::fs::write(h.cookbook.join("recipes/0123456789ab.json"), "{ not json").unwrap();
    let log = h.watch(
        &[&h.mom],
        "the build error",
        Duration::from_millis(2500),
        |log| log.contains("0123456789ab.json"),
    );
    assert_eq!(
        log.lines()
            .filter(|l| l.contains("0123456789ab.json"))
            .count(),
        1,
        "{log}"
    );
    assert!(h.build_id().is_none());
    assert!(books(&h.mom).is_empty());
    assert!(h.cookbook.join("dirty").exists(), "tried again once fixed");
}

#[test]
fn empty_cookbook_sends_nothing() {
    let h = setup();
    std::fs::create_dir_all(&h.cookbook).unwrap();
    h.store().mark_dirty().unwrap();
    h.watch(&[&h.mom], "a tick", Duration::from_millis(1500), |log| {
        log.contains("watching")
    });
    assert!(h.build_id().is_none());
    assert!(books(&h.mom).is_empty());
    assert!(h.queued().is_none());
}

fn stdout(o: &std::process::Output) -> String {
    String::from_utf8_lossy(&o.stdout).into_owned()
}

#[test]
fn send_builds_and_pushes_to_connected_kindle() {
    let h = setup();
    h.add("Lemon Bars", 1);
    assert!(h.build_id().is_none());

    let o = h.cookbook_cmd(&[&h.mom], &["send"]);
    assert!(o.status.success(), "{}", text(&o));
    let built = h.build_id().expect("send builds when there's no build yet");
    assert_eq!(books(&h.mom), [DEVICE_NAME]);
    assert_eq!(h.delivered_to(&h.mom), Some(built));
    assert!(stdout(&o).contains("Sent to Mom's Kindle"), "{}", text(&o));
    assert!(h.queued().is_none());
}

#[test]
fn send_reuses_the_current_build() {
    let h = setup();
    h.add("Lemon Bars", 1);
    h.plug_in(&h.mom);
    let built = h.build_id().unwrap();
    let stamp = std::fs::metadata(h.cookbook.join("build/build_id"))
        .unwrap()
        .modified()
        .unwrap();

    // Nothing changed: no rebuild, and the Kindle that hasn't the book gets this build.
    let o = h.cookbook_cmd(&[&h.dad], &["send"]);
    assert!(o.status.success(), "{}", text(&o));
    assert_eq!(h.build_id(), Some(built.clone()));
    assert_eq!(
        std::fs::metadata(h.cookbook.join("build/build_id"))
            .unwrap()
            .modified()
            .unwrap(),
        stamp,
        "not rebuilt"
    );
    assert_eq!(h.delivered_to(&h.dad), Some(built));

    // A Kindle that's up to date says so and is left alone.
    let written = modified(&h.dad);
    let o = h.cookbook_cmd(&[&h.dad], &["send"]);
    assert!(o.status.success(), "{}", text(&o));
    assert!(stdout(&o).contains("up to date"), "{}", text(&o));
    assert_eq!(modified(&h.dad), written);
}

#[test]
fn send_rebuilds_a_changed_cookbook() {
    let h = setup();
    h.add("Lemon Bars", 1);
    h.plug_in(&h.mom);
    let first = h.build_id().unwrap();
    h.add("Plum Cake", 2);

    let o = h.cookbook_cmd(&[&h.mom], &["send"]);
    assert!(o.status.success(), "{}", text(&o));
    let second = h.build_id().unwrap();
    assert_ne!(first, second);
    assert_eq!(h.delivered_to(&h.mom), Some(second));
}

#[test]
fn send_without_kindle_says_it_waits() {
    let h = setup();
    h.add("Lemon Bars", 1);
    let none = h.nothing();

    for args in [&["send"][..], &["send", "--to", "device"][..]] {
        let o = h.cookbook_cmd(&[&none], args);
        assert!(o.status.success(), "{}", text(&o));
        assert!(
            stdout(&o)
                .lines()
                .any(|l| l == "Queued: goes to each Kindle on its next plug-in"),
            "{}",
            text(&o)
        );
    }
    assert!(h.build_id().is_some(), "it still builds");
    assert!(h.delivered().is_empty());
}

#[test]
fn send_with_no_recipes_says_so() {
    let h = setup();
    let o = h.cookbook_cmd(&[&h.mom], &["send"]);
    assert!(!o.status.success(), "{}", text(&o));
    assert!(books(&h.mom).is_empty());
}

#[test]
fn send_to_a_folder_exports_without_recording() {
    let h = setup();
    h.add("Lemon Bars", 1);
    let folder = h.tmp.path().join("export");

    let o = h
        .deckle(&[&h.mom])
        .args(["cookbook", "send", "--to"])
        .arg(&folder)
        .output()
        .unwrap();
    assert!(o.status.success(), "{}", text(&o));
    assert!(folder.join("cookbook.epub").is_file(), "{}", text(&o));
    assert!(h.delivered().is_empty(), "a folder isn't a Kindle");
    assert!(books(&h.mom).is_empty(), "nor did Mom's Kindle get it");
    assert!(h.queued().is_none());
}

#[test]
fn send_leaves_out_a_kindle_with_cookbook_false() {
    let h = setup();
    h.add("Lemon Bars", 1);
    let o = h.cookbook_cmd(&[&h.attic], &["send"]);
    assert!(o.status.success(), "{}", text(&o));
    assert!(books(&h.attic).is_empty());
    assert_eq!(h.delivered_to(&h.attic), None);
    assert!(stdout(&o).contains("cookbook = false"), "{}", text(&o));
}

#[test]
fn list_footer_shows_each_kindle_status() {
    let h = setup();
    h.add("Lemon Bars", 1);
    h.plug_in(&h.mom);
    h.add("Plum Cake", 2);
    let o = h.cookbook_cmd(&[&h.dad], &["send"]);
    assert!(o.status.success(), "{}", text(&o));

    let none = h.nothing();
    let o = h.cookbook_cmd(&[&none], &["list"]);
    assert!(o.status.success(), "{}", text(&o));
    let out = stdout(&o);
    let footer: Vec<&str> = out
        .lines()
        .skip_while(|l| !l.starts_with("Attic Kindle"))
        .collect();
    assert_eq!(
        footer,
        [
            "Attic Kindle: Off",
            "Dad's Kindle: Up to date",
            "Mom's Kindle: 1 change behind"
        ],
        "{out}"
    );

    let o = h.cookbook_cmd(&[&none], &["list", "--json"]);
    let v: serde_json::Value = serde_json::from_slice(&o.stdout).unwrap();
    assert_eq!(v["recipes"].as_array().unwrap().len(), 2);
    let kindles = v["kindles"].as_array().unwrap();
    assert_eq!(kindles.len(), 3);
    let mom = kindles
        .iter()
        .find(|k| k["name"] == "Mom's Kindle")
        .unwrap();
    assert_eq!(mom["id"], id(&h.mom));
    assert_eq!(mom["status"], "1 change behind");
}

/// A fresh home: no `digest.toml`.
fn forget_config(h: &Home) {
    let dir = if cfg!(target_os = "macos") {
        "Library/Application Support/deckle"
    } else {
        "deckle"
    };
    std::fs::remove_file(h.home.join(dir).join("digest.toml")).unwrap();
}

#[test]
fn send_works_without_a_digest_config() {
    let h = setup();
    forget_config(&h);
    h.add("Lemon Bars", 1);
    let o = h.cookbook_cmd(&[&h.mom], &["send"]);
    assert!(o.status.success(), "{}", text(&o));
    assert_eq!(books(&h.mom), [DEVICE_NAME]);
    assert_eq!(h.delivered_to(&h.mom), h.build_id());
}

#[test]
fn list_has_no_footer_when_no_kindle_is_known() {
    let h = setup();
    forget_config(&h);
    h.add("Lemon Bars", 1);
    let none = h.nothing();
    let o = h.cookbook_cmd(&[&none], &["list"]);
    assert!(o.status.success(), "{}", text(&o));
    // The recipe, then only what's not built yet: no Kindle line.
    let out = stdout(&o);
    let lines: Vec<&str> = out.lines().collect();
    assert_eq!(lines.len(), 3, "{out}");
    assert_eq!(
        lines[2], "Changes not built yet: waiting for the next build",
        "{out}"
    );
}

#[test]
fn send_reports_a_failed_offer() {
    let h = setup();
    h.add("Lemon Bars", 1);
    h.plug_in(&h.mom);
    // The record is damaged: the cookbook can't be offered to anyone.
    std::fs::write(h.cookbook.join("delivered.json"), "{ nope").unwrap();

    let o = h.cookbook_cmd(&[&h.dad], &["send"]);
    assert!(!o.status.success(), "{}", text(&o));
    let err = String::from_utf8_lossy(&o.stderr);
    assert!(err.contains("not sent to Dad's Kindle"), "{err}");
    assert!(err.contains("delivered.json"), "{err}");
    assert!(!text(&o).contains("up to date"), "{}", text(&o));
    assert!(books(&h.dad).is_empty());
}

#[test]
fn list_shows_changes_not_built_after_a_failed_build() {
    let h = setup();
    h.add("Lemon Bars", 1);
    let none = h.nothing();
    // No Previewer to build with.
    let o = h
        .deckle(&[&none])
        .env("DECKLE_PREVIEWER", h.tmp.path().join("no-previewer"))
        .args(["cookbook", "send"])
        .output()
        .unwrap();
    assert!(!o.status.success(), "{}", text(&o));

    let o = h.cookbook_cmd(&[&none], &["list"]);
    let out = stdout(&o);
    let line = out
        .lines()
        .find(|l| l.starts_with("Changes not built yet: "))
        .unwrap_or_else(|| panic!("{out}"));
    assert!(line.contains("can't build the cookbook yet"), "{line}");
    let o = h.cookbook_cmd(&[&none], &["list", "--json"]);
    let v: serde_json::Value = serde_json::from_slice(&o.stdout).unwrap();
    assert_eq!(v["pending_build"], true);
    assert!(
        v["build_error"]
            .as_str()
            .unwrap()
            .contains("can't build the cookbook yet")
    );

    // A good build clears both.
    let o = h.cookbook_cmd(&[&none], &["send"]);
    assert!(o.status.success(), "{}", text(&o));
    let out = stdout(&h.cookbook_cmd(&[&none], &["list"]));
    assert!(!out.contains("Changes not built yet"), "{out}");
    let v: serde_json::Value =
        serde_json::from_slice(&h.cookbook_cmd(&[&none], &["list", "--json"]).stdout).unwrap();
    assert_eq!(v["pending_build"], false);
    assert!(v["build_error"].is_null());
}

#[test]
fn send_with_every_kindle_current_says_so() {
    let h = setup();
    h.add("Lemon Bars", 1);
    h.plug_in(&h.mom);
    h.plug_in(&h.dad);
    h.plug_in(&h.guest);
    // Mom, Dad and the Guest have it; the Attic's Kindle is left out. Nobody is waiting.
    let none = h.nothing();
    let o = h.cookbook_cmd(&[&none], &["send"]);
    assert!(o.status.success(), "{}", text(&o));
    assert_eq!(stdout(&o).trim(), "Every Kindle already has this build");
    let o = h.cookbook_cmd(&[&none], &["--json", "send"]);
    let v: serde_json::Value = serde_json::from_slice(&o.stdout).unwrap();
    assert_eq!(v["result"], "all_current");

    // A change makes someone wait again.
    h.add("Plum Cake", 2);
    let o = h.cookbook_cmd(&[&none], &["send"]);
    assert!(
        stdout(&o).contains("Queued: goes to each Kindle on its next plug-in"),
        "{}",
        text(&o)
    );
}
