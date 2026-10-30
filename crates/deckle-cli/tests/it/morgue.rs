// SPDX-License-Identifier: GPL-3.0-or-later
//! The Morgue from the CLI (Morgue design §5), with Deckle's config and cache folders moved
//! into a temp dir.

use std::path::{Path, PathBuf};

use assert_cmd::Command;

fn deckle(home: &Path) -> Command {
    let mut c = Command::cargo_bin("deckle").unwrap();
    c.env("HOME", home)
        .env("XDG_CONFIG_HOME", home)
        .env("XDG_CACHE_HOME", home.join("cache"))
        .env("APPDATA", home)
        .env("LOCALAPPDATA", home);
    c
}

fn config_dir(home: &Path) -> PathBuf {
    if cfg!(target_os = "macos") {
        home.join("Library/Application Support/deckle")
    } else {
        home.join("deckle")
    }
}

/// Deckle's cache folder under `home`, as `default_cache_dir` finds it on this OS.
fn cache_dir(home: &Path) -> PathBuf {
    if cfg!(target_os = "macos") {
        home.join("Library/Caches/deckle")
    } else if cfg!(target_os = "windows") {
        home.join("deckle")
    } else {
        home.join("cache/deckle")
    }
}

/// A home whose paper is the horoscope alone (no network) for Leo.
fn setup(extra: &str) -> tempfile::TempDir {
    let home = tempfile::tempdir().unwrap();
    let dir = config_dir(home.path());
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(
        dir.join("digest.toml"),
        format!("sections = [\"horoscope\"]\n\n[horoscope]\nsigns = [\"leo\"]\n\n{extra}"),
    )
    .unwrap();
    home
}

fn today() -> deckle_digest::chrono::NaiveDate {
    deckle_digest::chrono::Local::now().date_naive()
}

fn build(home: &Path, date: &str) -> assert_cmd::assert::Assert {
    let out = home.join("out");
    deckle(home)
        .args(["digest", "build", "--date", date, "-o"])
        .arg(&out)
        .assert()
        .success()
}

fn morgue(home: &Path) -> PathBuf {
    home.join("Deckle/Morgue")
}

/// Puts an edition for `date` in the Morgue the way an earlier build would have, with one story.
fn seed(home: &Path, date: &str, paper: &str, story: &str) {
    use deckle_digest::chrono::Datelike;
    use deckle_morgue::sidecar::{Files, SectionText, Sidecar, Story};
    let date: deckle_digest::chrono::NaiveDate = date.parse().unwrap();
    let m = deckle_morgue::Morgue {
        root: morgue(home),
        keep_days: 0,
    };
    let epub = home.join("seed.epub");
    std::fs::write(&epub, b"EPUB").unwrap();
    let sidecar = Sidecar {
        date,
        number: date.ordinal(),
        paper: paper.into(),
        place: String::new(),
        weather: None,
        lead: None,
        sections: vec![SectionText {
            key: "local".into(),
            title: "Local".into(),
            stories: vec![Story {
                title: story.into(),
                teaser: None,
                text: format!("{story} in full"),
            }],
        }],
        files: Files {
            epub: format!("{date}.epub"),
            kfx: None,
        },
        stem: format!("deckle-daily-{date}"),
    };
    m.archive_edition_on(&epub, &sidecar, date).unwrap();
}

#[test]
fn build_archives() {
    let home = setup("");
    let date = today().to_string();
    build(home.path(), &date);
    let year = morgue(home.path()).join(&date[..4]);
    assert!(year.join(format!("{date}.epub")).exists());
    let json: serde_json::Value =
        serde_json::from_slice(&std::fs::read(year.join(format!("{date}.json"))).unwrap()).unwrap();
    assert_eq!(json["date"], date.as_str());
    assert!(json["sections"][0]["stories"].is_array());
}

#[test]
fn a_build_for_another_date_isnt_archived() {
    // A preview for a past date gathers today's news: it must not replace that day's real paper.
    let home = setup("");
    seed(home.path(), "2026-10-01", "THE REAL ONE", "Original story");
    let out = deckle(home.path())
        .args(["--json", "digest", "build", "--date", "2026-10-01", "-o"])
        .arg(home.path().join("out"))
        .assert()
        .success();
    let v: serde_json::Value = serde_json::from_slice(&out.get_output().stdout).unwrap();
    assert!(
        v["notes"]
            .as_array()
            .unwrap()
            .iter()
            .any(|n| n.as_str().unwrap().contains("not kept")),
        "{v}"
    );
    let json: serde_json::Value = serde_json::from_slice(
        &std::fs::read(morgue(home.path()).join("2026/2026-10-01.json")).unwrap(),
    )
    .unwrap();
    assert_eq!(json["paper"], "THE REAL ONE");
}

#[test]
fn a_build_for_a_future_date_leaves_the_archive_alone() {
    let home = setup("");
    seed(home.path(), "2026-01-05", "OLD", "Old story");
    deckle(home.path())
        .args(["digest", "build", "--date", "2031-12-01", "-o"])
        .arg(home.path().join("out"))
        .assert()
        .success();
    assert!(morgue(home.path()).join("2026/2026-01-05.json").exists());
}

#[test]
fn archive_failure_is_a_note() {
    let home = setup("[morgue]\nfolder = \"~/not-a-folder\"\n");
    std::fs::write(home.path().join("not-a-folder"), b"a file").unwrap();
    let out = deckle(home.path())
        .args([
            "--json",
            "digest",
            "build",
            "--date",
            &today().to_string(),
            "-o",
        ])
        .arg(home.path().join("out"))
        .assert()
        .success();
    let v: serde_json::Value = serde_json::from_slice(&out.get_output().stdout).unwrap();
    let notes = v["notes"].as_array().unwrap();
    assert!(
        notes.iter().any(|n| n
            .as_str()
            .unwrap()
            .starts_with("Morgue: couldn't save the edition")),
        "{notes:?}"
    );
    assert!(
        home.path()
            .join(format!("out/deckle-daily-{}.epub", today()))
            .exists(),
        "the build itself stands"
    );
}

#[test]
fn list_search_show() {
    use deckle_digest::chrono::{Datelike, Days};
    let home = setup("");
    let now = today();
    let older = now - Days::new(40);
    build(home.path(), &now.to_string());
    seed(
        home.path(),
        &older.to_string(),
        "The Deckle Daily",
        "Mill sold to a brewer",
    );
    let month_of = |d: deckle_digest::chrono::NaiveDate| format!("{}-{:02}", d.year(), d.month());

    let list = deckle(home.path())
        .args(["digest", "morgue", "list", "--month", &month_of(now)])
        .assert()
        .success();
    let text = String::from_utf8(list.get_output().stdout.clone()).unwrap();
    assert!(
        text.contains(&now.to_string()) && text.contains("Morgue: 2 editions"),
        "{text}"
    );
    let list = deckle(home.path())
        .args(["digest", "morgue", "list", "--month", &month_of(older)])
        .assert()
        .success();
    assert!(
        String::from_utf8(list.get_output().stdout.clone())
            .unwrap()
            .contains(&older.to_string())
    );

    let found = deckle(home.path())
        .args(["digest", "morgue", "search", "mill", "brewer"])
        .assert()
        .success();
    let text = String::from_utf8(found.get_output().stdout.clone()).unwrap();
    assert!(
        text.contains(&older.to_string())
            && text.contains("Local")
            && !text.contains(&now.to_string()),
        "{text}"
    );
    let leo = deckle(home.path())
        .args(["digest", "morgue", "search", "leo"])
        .assert()
        .success();
    assert!(
        String::from_utf8(leo.get_output().stdout.clone())
            .unwrap()
            .contains("Horoscope")
    );
    let none = deckle(home.path())
        .args(["digest", "morgue", "search", "zzzzqx"])
        .assert()
        .success();
    assert!(
        String::from_utf8(none.get_output().stdout.clone())
            .unwrap()
            .contains("Nothing")
    );

    let show = deckle(home.path())
        .args(["--json", "digest", "morgue", "show", &now.to_string()])
        .assert()
        .success();
    let v: serde_json::Value = serde_json::from_slice(&show.get_output().stdout).unwrap();
    assert_eq!(
        v["heading"],
        format!("{} · No. {}", now.format("%A, %B %-d"), now.ordinal())
    );
}

#[test]
fn show_of_a_missing_edition_says_so() {
    let home = setup("");
    deckle(home.path())
        .args(["digest", "morgue", "show", "2026-01-01"])
        .assert()
        .failure()
        .stderr(predicates::str::contains("no edition"));
}

#[test]
fn send_queues_kept_kfx() {
    let home = setup("");
    seed(home.path(), "2026-10-21", "The Deckle Daily", "A story");
    std::fs::write(
        morgue(home.path()).join("2026/2026-10-21.kfx"),
        b"KFX BYTES",
    )
    .unwrap();
    let out = deckle(home.path())
        .args(["--json", "digest", "morgue", "send", "2026-10-21"])
        .assert()
        .success();
    let v: serde_json::Value = serde_json::from_slice(&out.get_output().stdout).unwrap();
    assert_eq!(v["queued"], "deckle-daily-2026-10-21.kfx");
    let queued = cache_dir(home.path()).join("outbox/deckle-daily-2026-10-21.kfx");
    assert_eq!(std::fs::read(queued).unwrap(), b"KFX BYTES");
}

// ---- sent again, and what a connection teaches the Morgue (Task 6) ----

use deckle_morgue::state::{MorgueState, Resent};

fn state_file(home: &Path) -> PathBuf {
    config_dir(home).join("morgue.json")
}

fn load_state(home: &Path) -> MorgueState {
    MorgueState::load(&state_file(home))
}

/// A Kindle folder holding the given editions.
fn kindle_with(home: &Path, days: &[&str]) -> PathBuf {
    let k = home.join("kindle");
    std::fs::create_dir_all(&k).unwrap();
    for d in days {
        std::fs::write(k.join(format!("deckle-daily-{d}.kfx")), b"K").unwrap();
    }
    k
}

/// An old edition archived with a kept KFX, then queued with `morgue send`.
fn sent_again(home: &Path, date: &str) {
    seed(home, date, "The Deckle Daily", "An old story");
    std::fs::write(
        morgue(home).join(&date[..4]).join(format!("{date}.kfx")),
        b"OLD KFX",
    )
    .unwrap();
    deckle(home)
        .args(["digest", "morgue", "send", date])
        .assert()
        .success();
}

fn push(home: &Path, to: &Path) {
    deckle(home)
        .args(["digest", "push", "--to"])
        .arg(to)
        .assert()
        .success();
}

#[test]
fn resent_survives_seven_days() {
    let home = setup("");
    sent_again(home.path(), "2026-10-01");
    let kindle = kindle_with(home.path(), &["2026-10-18", "2026-10-19", "2026-10-20"]);
    push(home.path(), &kindle);
    assert!(
        kindle.join("deckle-daily-2026-10-01.kfx").exists(),
        "delivered and kept past keep = 3"
    );
    assert!(
        load_state(home.path()).resent[&"2026-10-01".parse().unwrap()]
            .delivered
            .is_some()
    );

    // Eight days after delivery it is just an old edition again.
    let mut state = load_state(home.path());
    let long_ago =
        (deckle_digest::chrono::Utc::now() - deckle_digest::chrono::Duration::days(8)).to_rfc3339();
    state
        .resent
        .get_mut(&"2026-10-01".parse().unwrap())
        .unwrap()
        .delivered = Some(long_ago);
    state.save(&state_file(home.path())).unwrap();
    let outbox = cache_dir(home.path()).join("outbox");
    std::fs::create_dir_all(&outbox).unwrap();
    std::fs::write(outbox.join("deckle-daily-2026-10-21.kfx"), b"NEW").unwrap();
    push(home.path(), &kindle);
    assert!(
        !kindle.join("deckle-daily-2026-10-01.kfx").exists(),
        "pruned after a week"
    );
    assert!(kindle.join("deckle-daily-2026-10-21.kfx").exists());
}

#[test]
fn send_without_kindle_waits() {
    let home = setup("");
    sent_again(home.path(), "2026-10-01");
    let key: deckle_digest::chrono::NaiveDate = "2026-10-01".parse().unwrap();
    // Queued, and no Kindle has taken it: the clock hasn't started. (Never `--to device` in a
    // test: a real Kindle may be plugged in.)
    let r: &Resent = &load_state(home.path()).resent[&key];
    assert!(r.delivered.is_none() && !r.queued.is_empty());
    assert!(
        cache_dir(home.path())
            .join("outbox/deckle-daily-2026-10-01.kfx")
            .exists()
    );
    // It was queued a month ago; the week counts from delivery, not from then.
    let mut state = load_state(home.path());
    let month_ago = (deckle_digest::chrono::Utc::now() - deckle_digest::chrono::Duration::days(30))
        .to_rfc3339();
    state.resent.get_mut(&key).unwrap().queued = month_ago;
    state.save(&state_file(home.path())).unwrap();
    // A Kindle with three newer editions would prune it; it arrives, survives, and starts its week.
    let kindle = kindle_with(home.path(), &["2026-10-18", "2026-10-19", "2026-10-20"]);
    push(home.path(), &kindle);
    assert!(kindle.join("deckle-daily-2026-10-01.kfx").exists());
    let delivered = load_state(home.path()).resent[&key]
        .delivered
        .clone()
        .expect("stamped on delivery");
    let at = deckle_digest::chrono::DateTime::parse_from_rfc3339(&delivered).unwrap();
    assert!(
        (deckle_digest::chrono::Utc::now() - at.with_timezone(&deckle_digest::chrono::Utc))
            .num_minutes()
            < 5
    );
}

#[test]
fn connection_records_highlights_and_presence() {
    use deckle_readership::layout::{Layout, SectionStart};
    use deckle_readership::state::{Read, State};
    let home = setup("[sync]\nclippings = true\nreadership = true\n");
    let date: deckle_digest::chrono::NaiveDate = "2026-10-21".parse().unwrap();
    let mut readership = State::default();
    readership.record_layout(Layout {
        date,
        title: "The Deckle Daily".into(),
        file: "deckle-daily-2026-10-21".into(),
        end: 10_000,
        epub: None,
        sections: vec![SectionStart {
            key: "front".into(),
            name: "Front page".into(),
            start: 0,
            items: None,
            stories: Vec::new(),
        }],
    });
    readership.reads.entry(date).or_default().insert(
        "seed".into(),
        Read {
            opened: false,
            furthest: None,
            last: None,
            last_read: None,
            seen: "x".into(),
            history: Vec::new(),
        },
    );
    readership
        .save(&config_dir(home.path()).join("readership.json"))
        .unwrap();

    let kindle = kindle_with(home.path(), &[]);
    let title = format!(
        "The Deckle Daily \u{2014} {} (Deckle)",
        deckle_digest::long_date(date)
    );
    let clippings = format!(
        "{title}\r\n- Your Highlight on Location 10-11 | Added on Wednesday, October 21, 2026 8:00:00 AM\r\n\r\nOne.\r\n==========\r\n\
         {title}\r\n- Your Highlight on Location 30-31 | Added on Wednesday, October 21, 2026 8:05:00 AM\r\n\r\nTwo.\r\n==========\r\n"
    );
    std::fs::write(kindle.join("My Clippings.txt"), clippings).unwrap();
    push(home.path(), &kindle);
    let state = load_state(home.path());
    assert_eq!(state.highlights[&date], 2);
    assert_eq!(
        state.on_kindle.get("deckle-daily-2026-10-21"),
        Some(&false),
        "read, and not on the Kindle"
    );
}

#[test]
fn a_sent_again_edition_not_in_the_outbox_any_more_isnt_exempt() {
    // Queued, then lost (the morning's paper superseded it, or the file was deleted): it must not
    // stay exempt from prune for ever.
    let home = setup("");
    sent_again(home.path(), "2026-10-01");
    std::fs::remove_file(cache_dir(home.path()).join("outbox/deckle-daily-2026-10-01.kfx"))
        .unwrap();
    let kindle = kindle_with(
        home.path(),
        &["2026-10-01", "2026-10-18", "2026-10-19", "2026-10-20"],
    );
    let outbox = cache_dir(home.path()).join("outbox");
    std::fs::write(outbox.join("deckle-daily-2026-10-21.kfx"), b"NEW").unwrap();
    push(home.path(), &kindle);
    assert!(
        !kindle.join("deckle-daily-2026-10-01.kfx").exists(),
        "pruned like any old edition"
    );
}

#[test]
fn pruned_editions_are_no_longer_on_the_kindle() {
    use deckle_readership::layout::{Layout, SectionStart};
    use deckle_readership::state::{Read, State};
    let home = setup("[sync]\nreadership = true\n");
    let mut readership = State::default();
    for day in ["2026-10-17", "2026-10-18", "2026-10-19", "2026-10-20"] {
        let date: deckle_digest::chrono::NaiveDate = day.parse().unwrap();
        readership.record_layout(Layout {
            date,
            title: "The Deckle Daily".into(),
            file: format!("deckle-daily-{day}"),
            end: 10_000,
            epub: None,
            sections: vec![SectionStart {
                key: "front".into(),
                name: "Front page".into(),
                start: 0,
                items: None,
                stories: Vec::new(),
            }],
        });
        readership.reads.entry(date).or_default().insert(
            "seed".into(),
            Read {
                opened: false,
                furthest: None,
                last: None,
                last_read: None,
                seen: "x".into(),
                history: Vec::new(),
            },
        );
    }
    readership
        .save(&config_dir(home.path()).join("readership.json"))
        .unwrap();
    // Four on the Kindle, keep = 3: the oldest is pruned in this very session, after the read.
    let kindle = kindle_with(
        home.path(),
        &["2026-10-17", "2026-10-18", "2026-10-19", "2026-10-20"],
    );
    let outbox = cache_dir(home.path()).join("outbox");
    std::fs::create_dir_all(&outbox).unwrap();
    std::fs::write(outbox.join("deckle-daily-2026-10-21.kfx"), b"NEW").unwrap();
    push(home.path(), &kindle);
    assert!(!kindle.join("deckle-daily-2026-10-17.kfx").exists());
    let state = load_state(home.path());
    assert_eq!(
        state.on_kindle.get("deckle-daily-2026-10-17"),
        Some(&false),
        "gone by the time we say"
    );
    assert_eq!(state.on_kindle.get("deckle-daily-2026-10-19"), Some(&true));
}
