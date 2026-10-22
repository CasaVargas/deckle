// SPDX-License-Identifier: GPL-3.0-or-later
//! A whole edition from saved responses: no network.

use std::io::Read;
use std::path::{Path, PathBuf};

use chrono::{DateTime, NaiveDate, Utc};
use deckle_digest::{Config, build, gather};

fn fixture(name: &str) -> Vec<u8> {
    std::fs::read(
        Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("tests/fixtures")
            .join(name),
    )
    .unwrap()
}

/// Serves fixtures by URL; anything else fails like a dead network.
fn fake(url: &str) -> Result<Vec<u8>, String> {
    let name = if url.starts_with("https://api.open-meteo.com/") {
        "weather.json"
    } else if url.starts_with("https://news.google.com/") {
        "googlenews.xml"
    } else if url.starts_with("https://hnrss.org/") {
        "hn.xml"
    } else if url.contains("d6yy-54nr") {
        "powerball.json"
    } else if url.contains("5xaw-6ayf") {
        "megamillions.json"
    } else if url.contains("onthisday") {
        "onthisday.json"
    } else {
        return Err(format!("offline: {url}"));
    };
    Ok(fixture(name))
}

fn config() -> Config {
    let mut c = Config::parse(deckle_digest::config::STARTER).unwrap();
    // Never the owner's real queue: a path that doesn't exist reads as empty.
    c.reading.queue_path = Some(
        std::env::temp_dir().join(format!("deckle-test-no-queue-{}.jsonl", std::process::id())),
    );
    // Nor the owner's frost cache; the archive is offline here, so nothing is written.
    c.almanac_path =
        Some(std::env::temp_dir().join(format!("deckle-test-almanac-{}.json", std::process::id())));
    // Nor the owner's puzzles: a fresh store per test run.
    c.puzzle_path =
        Some(std::env::temp_dir().join(format!("deckle-test-puzzles-{}.json", std::process::id())));
    c.horoscope.signs = vec!["leo".into(), "virgo".into()];
    // The fixtures were saved on 2026-10-20; keep a week of stories.
    c.max_age_hours = 24 * 7;
    c
}

fn date() -> NaiveDate {
    NaiveDate::from_ymd_opt(2026, 10, 20).unwrap()
}

fn now() -> DateTime<Utc> {
    "2026-10-20T22:00:00Z".parse().unwrap()
}

fn read_zip(path: &PathBuf) -> zip::ZipArchive<std::fs::File> {
    zip::ZipArchive::new(std::fs::File::open(path).unwrap()).unwrap()
}

#[test]
fn every_section_fills_from_fixtures() {
    let g = gather(&config(), date(), now(), &fake);
    let titles: Vec<_> = g
        .edition
        .sections
        .iter()
        .map(|s| s.title.as_str())
        .collect();
    assert_eq!(
        titles,
        [
            "Weather & Almanac",
            "Local",
            "Hacker News",
            "Lottery",
            "Horoscope",
            "Puzzle",
            "On This Day"
        ]
    );
    for s in &g.edition.sections {
        assert!(s.unavailable.is_none(), "{}: {:?}", s.title, s.unavailable);
        assert!(!s.articles.is_empty(), "{} is empty", s.title);
    }
    assert!(g.edition.weather_line.as_deref().unwrap().contains("°F"));
    let lottery = &g.edition.sections[3];
    assert!(lottery.articles[0].body.contains("ball special"));
    assert_eq!(g.news, [1, 2]);
}

#[test]
fn dead_network_still_builds() {
    let tmp = tempfile::tempdir().unwrap();
    let offline = |url: &str| -> Result<Vec<u8>, String> { Err(format!("offline: {url}")) };
    let r = build(&config(), date(), now(), tmp.path(), &offline).unwrap();
    assert!(r.epub.exists());
    let horoscope = r.sections.iter().find(|s| s.title == "Horoscope").unwrap();
    assert_eq!(horoscope.items, 2);
    // Offline, the forecast is missing but the almanac still prints (almanac design §2.7).
    let weather = r.sections.iter().find(|s| s.key == "weather").unwrap();
    assert!(weather.unavailable.is_none());
    assert!(weather.items >= 2, "{weather:?}");
}

#[test]
fn epub_is_well_formed() {
    let tmp = tempfile::tempdir().unwrap();
    let r = build(&config(), date(), now(), tmp.path(), &fake).unwrap();
    assert_eq!(r.title, "The Deckle Daily — Tuesday, October 20, 2026");
    let mut zip = read_zip(&r.epub);
    assert_eq!(zip.by_index(0).unwrap().name().unwrap(), "mimetype");
    let names: Vec<String> = zip.file_names().map(|n| n.unwrap().into_owned()).collect();
    for name in &names {
        if name.ends_with(".xhtml") || name.ends_with(".opf") || name.ends_with(".ncx") {
            let mut s = String::new();
            zip.by_name(name).unwrap().read_to_string(&mut s).unwrap();
            roxmltree::Document::parse(&s).unwrap_or_else(|e| panic!("{name}: {e}"));
        }
    }
    let mut cover = Vec::new();
    zip.by_name("OEBPS/cover.jpg")
        .unwrap()
        .read_to_end(&mut cover)
        .unwrap();
    let img = image::load_from_memory(&cover).unwrap();
    assert_eq!((img.width(), img.height()), (1264, 1680));
}

// ---- Your Reading (save-to-paper design §5) ----

use deckle_digest::reading::{Added, Outcome, ReadingQueue};

const WXII: &str = "https://www.wxii12.com/article/candor-peach-street-standoff/74194295";

fn long_page(words: usize) -> String {
    let para = "The council met again tonight to argue about the new library and its long overdue roof repairs. ";
    let per = para.split_whitespace().count();
    let body: String = (0..words / per).map(|_| format!("<p>{para}</p>")).collect();
    format!(
        "<html><head><title>The Long One</title></head><body><article><h1>The Long One</h1>{body}</article></body></html>"
    )
}

/// Fixtures plus the WXII story and a 5,000-word page at ex.com/long.
fn net_with_pages(url: &str) -> Result<Vec<u8>, String> {
    if url == WXII {
        return Ok(fixture("article-wxii.html"));
    }
    if url == "https://ex.com/long" {
        return Ok(long_page(5000).into_bytes());
    }
    fake(url)
}

fn reading_setup(urls: &[&str]) -> (tempfile::TempDir, Config, ReadingQueue, Vec<String>) {
    let dir = tempfile::tempdir().unwrap();
    let q = ReadingQueue::new(dir.path().join("reading.jsonl"));
    let ids = urls
        .iter()
        .map(|u| match q.add_url(u, None, "cli", now()).unwrap() {
            Added::New(i) | Added::Duplicate(i) => i,
        })
        .collect();
    let mut c = config();
    c.reading.queue_path = Some(q.path.clone());
    (dir, c, q, ids)
}

#[test]
fn saved_article_prints_first_as_your_reading() {
    let (_d, c, _q, ids) = reading_setup(&[WXII]);
    let g = gather(&c, date(), now(), &net_with_pages);
    let first = &g.edition.sections[0];
    assert_eq!(first.title, "Your Reading");
    assert_eq!(g.reading, Some(0));
    let meta = first.articles[0].meta.as_deref().unwrap();
    assert!(meta.starts_with("Saved Tue"), "{meta}");
    assert!(meta.contains("about"), "{meta}");
    assert_eq!(g.reading_outcomes, [(ids[0].clone(), Outcome::Printed)]);
}

#[test]
fn long_read_becomes_its_own_book() {
    let (_d, c, _q, ids) = reading_setup(&["https://ex.com/long"]);
    let out = tempfile::tempdir().unwrap();
    let g = gather(&c, date(), now(), &net_with_pages);
    assert_eq!(g.long_reads.len(), 1);
    assert!(
        g.long_reads[0]
            .file_stem
            .starts_with("deckle-reads-2026-10-20-the-long-one")
    );
    assert!(
        g.edition.sections[0].articles[0]
            .body
            .contains("In your library")
    );
    assert_eq!(g.reading_outcomes, [(ids[0].clone(), Outcome::Book)]);
    let r = build(&c, date(), now(), out.path(), &net_with_pages).unwrap();
    assert_eq!(r.long_reads.len(), 1);
    let book = &r.long_reads[0];
    assert!(
        book.file_name()
            .unwrap()
            .to_string_lossy()
            .starts_with("deckle-reads-2026-10-20-")
    );
    let mut zip = read_zip(book);
    assert_eq!(zip.by_index(0).unwrap().name().unwrap(), "mimetype");
    let names: Vec<String> = zip.file_names().map(|n| n.unwrap().into_owned()).collect();
    for name in &names {
        if name.ends_with(".xhtml") || name.ends_with(".opf") {
            let mut s = String::new();
            zip.by_name(name).unwrap().read_to_string(&mut s).unwrap();
            roxmltree::Document::parse(&s).unwrap_or_else(|e| panic!("{name}: {e}"));
        }
    }
}

#[test]
fn failing_save_stays_pending_with_a_try() {
    let (_d, c, _q, ids) = reading_setup(&["https://ex.com/offline"]);
    let g = gather(&c, date(), now(), &net_with_pages);
    assert!(g.edition.sections.iter().all(|s| s.title != "Your Reading"));
    assert!(matches!(&g.reading_outcomes[..], [(id, Outcome::Failed(_))] if *id == ids[0]));
}

#[test]
fn reading_section_inserted_when_not_configured() {
    let (_d, mut c, _q, _ids) = reading_setup(&[WXII]);
    c.sections
        .retain(|k| *k != deckle_digest::SectionKind::Reading);
    let g = gather(&c, date(), now(), &net_with_pages);
    assert_eq!(g.edition.sections[0].title, "Your Reading");
    assert_eq!(g.notes.len(), 1, "{:?}", g.notes);
}

#[test]
fn cover_lead_stays_news() {
    let (_d, c, _q, _ids) = reading_setup(&[WXII]);
    let g = gather(&c, date(), now(), &net_with_pages);
    assert!(!g.news.contains(&0), "Your Reading is not a news section");
    deckle_digest::cover::render(&g).unwrap();
}

#[test]
fn a_long_read_that_must_print_inline_is_in_the_paper() {
    let (_d, mut c, _q, ids) = reading_setup(&["https://ex.com/long"]);
    c.reading.inline = ids.clone();
    let g = gather(&c, date(), now(), &net_with_pages);
    assert!(g.long_reads.is_empty());
    let story = &g.edition.sections[0].articles[0];
    assert!(!story.body.contains("In your library"));
    assert!(story.body.contains("roof repairs"));
    assert_eq!(g.reading_outcomes, [(ids[0].clone(), Outcome::Printed)]);
}

#[test]
fn gathered_sections_carry_their_keys() {
    let g = gather(&config(), date(), now(), &fake);
    let keys: Vec<_> = g.edition.sections.iter().map(|s| s.key.as_str()).collect();
    assert_eq!(
        keys,
        [
            "weather",
            "local",
            "feeds:Hacker News",
            "lottery",
            "horoscope",
            "puzzle",
            "onthisday"
        ]
    );
    let tmp = tempfile::tempdir().unwrap();
    let r = build(&config(), date(), now(), tmp.path(), &fake).unwrap();
    assert_eq!(r.sections[2].key, "feeds:Hacker News");
}

#[test]
fn your_reading_has_the_reading_key() {
    let (_tmp, c, _q, _urls) = reading_setup(&[WXII]);
    let g = gather(&c, date(), now(), &net_with_pages);
    assert_eq!(g.edition.sections[0].key, "reading");
}

#[test]
fn weather_page_keeps_its_key() {
    let mut c = config();
    c.sections = vec![deckle_digest::config::SectionKind::Weather];
    let g = gather(&c, date(), now(), &fake);
    assert_eq!(g.edition.sections.len(), 1);
    assert_eq!(g.edition.sections[0].key, "weather");
    assert_eq!(g.edition.sections[0].title, "Weather & Almanac");
}

#[test]
fn icons_ship_in_the_epub() {
    let dir = tempfile::tempdir().unwrap();
    let mut c = config();
    c.sections = vec![deckle_digest::config::SectionKind::Weather];
    let report = build(&c, date(), now(), dir.path(), &fake).unwrap();
    // The CLI hands these (key, title) pairs to the readership layout.
    assert_eq!(report.sections[0].key, "weather");
    assert_eq!(report.sections[0].title, "Weather & Almanac");
    let mut zip = read_zip(&report.epub);
    let names: Vec<String> = zip.file_names().map(|n| n.unwrap().into_owned()).collect();
    let mut opf = String::new();
    zip.by_name("OEBPS/content.opf")
        .unwrap()
        .read_to_string(&mut opf)
        .unwrap();
    let mut page = String::new();
    zip.by_name("OEBPS/s1.xhtml")
        .unwrap()
        .read_to_string(&mut page)
        .unwrap();
    let srcs: Vec<&str> = page
        .split("src=\"")
        .skip(1)
        .filter_map(|s| s.split('"').next())
        .collect();
    assert!(!srcs.is_empty(), "no pictures on the weather page");
    for src in srcs {
        assert!(
            names.contains(&format!("OEBPS/{src}")),
            "{src} missing from the EPUB"
        );
        assert!(
            opf.contains(&format!("href=\"{src}\"")),
            "{src} missing from the manifest"
        );
    }
}

#[test]
fn gather_records_headlines_for_tomorrows_puzzle() {
    let dir = tempfile::tempdir().unwrap();
    let mut c = config();
    c.puzzle_path = Some(dir.path().join("puzzles.json"));
    let g = gather(&c, date(), now(), &fake);
    let puzzle = g
        .edition
        .sections
        .iter()
        .find(|s| s.key == "puzzle")
        .unwrap();
    assert!(puzzle.unavailable.is_none());
    let store = deckle_digest::puzzle::store::Store::load(c.puzzle_path.as_ref().unwrap());
    let day = &store.days[&date()];
    assert!(!day.headlines.is_empty());
    assert!(
        day.headlines
            .iter()
            .all(|t| t.section == "Local" || t.section == "Hacker News")
    );
    // a rebuild prints the same puzzle
    let again = gather(&c, date(), now(), &fake);
    let p2 = again
        .edition
        .sections
        .iter()
        .find(|s| s.key == "puzzle")
        .unwrap();
    assert_eq!(puzzle.articles[0].body, p2.articles[0].body);
}

#[test]
fn puzzle_store_write_error_is_a_note() {
    let dir = tempfile::tempdir().unwrap();
    let mut c = config();
    // a directory where the file should be: the save fails, the paper doesn't
    let path = dir.path().join("puzzles.json");
    std::fs::create_dir(&path).unwrap();
    c.puzzle_path = Some(path);
    let g = gather(&c, date(), now(), &fake);
    assert!(
        g.notes.iter().any(|n| n.contains("puzzle")),
        "{:?}",
        g.notes
    );
    let puzzle = g
        .edition
        .sections
        .iter()
        .find(|s| s.key == "puzzle")
        .unwrap();
    assert!(!puzzle.articles.is_empty());
}
