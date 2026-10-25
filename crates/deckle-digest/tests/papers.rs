// SPDX-License-Identifier: GPL-3.0-or-later
//! One Mac, many papers (many papers design §3): `digest.toml` plus `papers/<slug>.toml`.

use std::path::{Path, PathBuf};

use deckle_digest::config::{Config, Day, STARTER, save};
use deckle_digest::papers::{self, MAIN, load_all, load_one, slug_from_title, valid_slug};

fn home() -> (tempfile::TempDir, PathBuf) {
    let dir = tempfile::tempdir().unwrap();
    let main = dir.path().join("digest.toml");
    std::fs::write(
        &main,
        format!(
            "{STARTER}\n[email]\nenabled = true\nto = \"me@kindle.com\"\nfrom = \"me@example.com\"\n\
             smtp_host = \"smtp.example.com\"\nusername = \"me@example.com\"\n"
        ),
    )
    .unwrap();
    (dir, main)
}

fn paper(main: &Path, slug: &str, body: &str) {
    let dir = papers::papers_dir(main);
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(dir.join(format!("{slug}.toml")), body).unwrap();
}

#[test]
fn no_papers_folder_is_just_main() {
    let (_d, main) = home();
    let all = load_all(&main).unwrap();
    assert_eq!(all.papers.len(), 1);
    assert_eq!(all.papers[0], Config::load(&main).unwrap());
    assert_eq!(all.papers[0].paper, None);
    assert_eq!(all.papers[0].slug(), MAIN);
    assert!(all.notes.is_empty());
}

#[test]
fn extras_load_in_slug_order_with_state_dirs() {
    let (_d, main) = home();
    paper(&main, "b", "title = \"B\"\n");
    paper(&main, "a", "title = \"A\"\n");
    let all = load_all(&main).unwrap();
    let slugs: Vec<&str> = all.papers.iter().map(|p| p.slug()).collect();
    assert_eq!(slugs, ["main", "a", "b"]);
    let a = all.papers[1].paper.as_ref().unwrap();
    assert_eq!(a.state_dir, papers::papers_dir(&main).join("a"));
    assert_eq!(
        all.papers[1].puzzle_store(),
        papers::papers_dir(&main).join("a/puzzles.json")
    );
}

#[test]
fn bad_names_are_skipped_with_a_note() {
    let (_d, main) = home();
    paper(&main, "Kid Courier", "title = \"K\"\n");
    paper(&main, "main", "title = \"M\"\n");
    std::fs::write(papers::papers_dir(&main).join("notes.txt"), "hi").unwrap();
    let all = load_all(&main).unwrap();
    assert_eq!(all.papers.len(), 1);
    assert_eq!(all.notes.len(), 2, "{:?}", all.notes);
    assert!(all.notes.iter().all(|n| n.contains("not a paper name")));
}

#[test]
fn household_keys_are_refused() {
    let (_d, main) = home();
    for (body, key) in [
        ("[sync]\npush = false\n", "[sync]"),
        ("[reading]\nmax_items = 2\n", "[reading]"),
        ("[email]\nsmtp_host = \"x\"\n", "[email] smtp_host"),
    ] {
        paper(&main, "x", body);
        let err = load_one(&main, "x").unwrap_err();
        assert_eq!(
            err,
            format!("papers/x.toml: {key} is set in digest.toml for every paper")
        );
        // In a load of every paper it's a note, not a failure.
        let all = load_all(&main).unwrap();
        assert_eq!(all.papers.len(), 1);
        assert!(all.notes[0].contains(key));
    }
}

#[test]
fn household_keys_are_inherited() {
    let (_d, main) = home();
    paper(
        &main,
        "lakeside",
        "title = \"The Lakeside Gazette\"\n[email]\nenabled = true\nto = \"dad@kindle.com\"\nusb_too = true\n",
    );
    let first = Config::load(&main).unwrap();
    let extra = load_one(&main, "lakeside").unwrap();
    assert_eq!(extra.sync, first.sync);
    assert_eq!(extra.reading, first.reading);
    assert_eq!(extra.email.from, "me@example.com");
    assert_eq!(extra.email.smtp_host, "smtp.example.com");
    assert_eq!(extra.email.smtp_port, first.email.smtp_port);
    assert_eq!(extra.email.username, "me@example.com");
    assert_eq!(extra.email.to, "dad@kindle.com");
    assert!(extra.email.enabled && extra.email.usb_too);
}

#[test]
fn new_keys_round_trip_and_hide_at_defaults() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("digest.toml");
    std::fs::write(&path, STARTER).unwrap();
    // Saved untouched: no new line appears.
    save(&path, &Config::load(&path).unwrap()).unwrap();
    let text = std::fs::read_to_string(&path).unwrap();
    for key in [
        "\nreader =",
        "\nkindle =",
        "\nlarge_print =",
        "\n[schedule]",
    ] {
        assert!(!text.contains(key), "{key} written at its default");
    }
    let mut c = Config::load(&path).unwrap();
    c.reader = "Dad, in Ohio".into();
    c.kindle = "Dad's Kindle".into();
    c.large_print = true;
    c.schedule.at = Some("07:00".into());
    c.schedule.days = vec![Day::Sat];
    save(&path, &c).unwrap();
    let back = Config::load(&path).unwrap();
    assert_eq!(back.reader, "Dad, in Ohio");
    assert_eq!(back.kindle, "Dad's Kindle");
    assert!(back.large_print);
    assert_eq!(back.schedule.at.as_deref(), Some("07:00"));
    assert_eq!(back.schedule.days, [Day::Sat]);
    assert!(
        std::fs::read_to_string(&path)
            .unwrap()
            .starts_with("# Deckle Daily")
    );
}

#[test]
fn bad_schedule_is_an_error() {
    assert!(Config::parse("[schedule]\nat = \"25:00\"\n").is_err());
    assert!(Config::parse("[schedule]\ndays = [\"saturday\"]\n").is_err());
    assert!(Config::parse("[schedule]\nat = \"06:00\"\ndays = [\"mon\", \"wed\"]\n").is_ok());
}

#[test]
fn slug_rules() {
    assert!(valid_slug("kid-courier"));
    assert!(valid_slug("a"));
    for bad in ["Main", "main", "1paper", "kid courier", "-x", ""] {
        assert!(!valid_slug(bad), "{bad}");
    }
    assert!(!valid_slug(&"a".repeat(33)));
    assert!(valid_slug(&"a".repeat(32)));
    assert_eq!(slug_from_title("The Kid Courier"), "kid-courier");
    assert_eq!(slug_from_title("The Lakeside Gazette!"), "lakeside-gazette");
    assert_eq!(slug_from_title("The"), "the");
}

#[test]
fn create_writes_a_parseable_paper() {
    let (_d, main) = home();
    let path = papers::create(&main, "kid-courier", "The \"Kid\" Courier", "Maya").unwrap();
    assert_eq!(path, papers::paper_path(&main, "kid-courier"));
    let c = load_one(&main, "kid-courier").unwrap();
    assert_eq!(c.title, "The \"Kid\" Courier");
    assert_eq!(c.reader, "Maya");
    assert_eq!(c.place, Config::load(&main).unwrap().place);
    assert_eq!(c.latitude, Config::load(&main).unwrap().latitude);
    let err = papers::create(&main, "kid-courier", "Again", "x").unwrap_err();
    assert!(err.contains("already exists"), "{err}");
    assert!(papers::create(&main, "Bad Name", "x", "y").is_err());
}
