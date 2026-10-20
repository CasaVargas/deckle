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
            "Weather",
            "Local",
            "Hacker News",
            "Lottery",
            "Horoscope",
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
    let weather = r.sections.iter().find(|s| s.title == "Weather").unwrap();
    assert!(weather.unavailable.as_deref().unwrap().contains("offline"));
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
