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
    // Nor the owner's puzzles: a fresh store per call, so parallel tests don't share one.
    static N: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);
    let n = N.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    c.puzzle_path = Some(std::env::temp_dir().join(format!(
        "deckle-test-puzzles-{}-{n}.json",
        std::process::id()
    )));
    // Nor the owner's Kindle snapshots; no Kindle is plugged in here, so the page stays out.
    c.shelf_path = Some(
        std::env::temp_dir().join(format!("deckle-test-no-shelf-{}.json", std::process::id())),
    );
    c.sections
        .retain(|k| *k != deckle_digest::SectionKind::Shelf);
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

#[test]
fn building_another_date_leaves_the_puzzle_store_alone() {
    let dir = tempfile::tempdir().unwrap();
    let mut c = config();
    c.puzzle_path = Some(dir.path().join("puzzles.json"));
    gather(&c, date(), now(), &fake);
    let before = std::fs::read(c.puzzle_path.as_ref().unwrap()).unwrap();
    // a preview of next year's paper: printed, but neither kept nor pruning today's
    let later = date() + chrono::Days::new(400);
    let g = gather(&c, later, now(), &fake);
    let puzzle = g
        .edition
        .sections
        .iter()
        .find(|s| s.key == "puzzle")
        .unwrap();
    assert!(!puzzle.articles.is_empty());
    assert_eq!(
        std::fs::read(c.puzzle_path.as_ref().unwrap()).unwrap(),
        before
    );
}

// --- local news for more places (#102) ---

use deckle_digest::config::PlaceNews;

fn place(name: &str, search: &str) -> PlaceNews {
    PlaceNews {
        name: name.into(),
        google_news: search.into(),
        feeds: Vec::new(),
        max_items: None,
    }
}

#[test]
fn places_print_after_local_each_from_its_own_search() {
    let mut c = config();
    c.local.places = vec![
        place("Asheville, NC", "Asheville NC"),
        place("Boone, NC", ""),
    ];
    c.local.places[1].feeds = vec!["https://boone.example/rss".into()]; // offline in the fake
    let asked = std::sync::Mutex::new(Vec::<String>::new());
    let net = |url: &str| {
        asked.lock().unwrap().push(url.to_owned());
        fake(url)
    };
    let g = gather(&c, date(), now(), &net);
    let keys: Vec<&str> = g.edition.sections.iter().map(|s| s.key.as_str()).collect();
    let at = keys.iter().position(|k| *k == "local").unwrap();
    assert_eq!(
        &keys[at..at + 3],
        ["local", "local:Asheville, NC", "local:Boone, NC"]
    );
    let asheville = &g.edition.sections[at + 1];
    assert_eq!(asheville.title, "Asheville, NC");
    assert_eq!(
        asheville.kicker.as_deref(),
        Some("News from around Asheville, NC")
    );
    assert!(!asheville.articles.is_empty());
    // its own search, and only Local and Asheville search Google News
    let asked = asked.lock().unwrap();
    assert!(
        asked.iter().any(|u| u.contains("q=Asheville+NC")),
        "{asked:?}"
    );
    let searches = asked
        .iter()
        .filter(|u| u.starts_with("https://news.google.com/rss/search"))
        .count();
    assert_eq!(searches, 2, "{asked:?}");
    // Boone's feed is down: its section says so, and nothing else is touched
    let boone = &g.edition.sections[at + 2];
    assert!(
        boone
            .unavailable
            .as_deref()
            .unwrap()
            .contains("boone.example"),
        "{boone:?}"
    );
    assert!(g.edition.sections[at].unavailable.is_none());
    // all three are news, but only Local is the local tier
    assert!(g.news.contains(&(at + 1)) && g.news.contains(&(at + 2)));
}

#[test]
fn a_place_with_nothing_to_read_says_so() {
    let mut c = config();
    c.local.places = vec![place("Nowhere", "")];
    let g = gather(&c, date(), now(), &fake);
    let s = g
        .edition
        .sections
        .iter()
        .find(|s| s.key == "local:Nowhere")
        .unwrap();
    assert!(s.unavailable.as_deref().unwrap().contains("for Nowhere"));
}

#[test]
fn places_follow_local_off() {
    let mut c = config();
    c.sections
        .retain(|k| *k != deckle_digest::SectionKind::Local);
    c.local.places = vec![place("Asheville, NC", "Asheville NC")];
    let g = gather(&c, date(), now(), &fake);
    assert!(
        g.edition
            .sections
            .iter()
            .all(|s| !s.key.starts_with("local"))
    );
}

#[test]
fn place_budget_defaults_to_locals() {
    let mut c = config();
    c.local.max_items = 2;
    c.local.places = vec![place("A", "Asheville NC"), place("B", "Boone NC")];
    c.local.places[1].max_items = Some(1);
    c.full_text = false;
    let g = gather(&c, date(), now(), &fake);
    let n = |k: &str| {
        g.edition
            .sections
            .iter()
            .find(|s| s.key == k)
            .unwrap()
            .articles
            .len()
    };
    assert_eq!((n("local:A"), n("local:B")), (2, 1));
}

#[test]
fn places_round_trip_and_stay_out_of_an_untouched_save() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("digest.toml");
    let mut c = Config::parse(deckle_digest::config::STARTER).unwrap();
    deckle_digest::config::save(&path, &c).unwrap();
    assert!(!std::fs::read_to_string(&path).unwrap().contains("places ="));
    c.local.places = vec![place("Asheville, NC", "Asheville NC")];
    deckle_digest::config::save(&path, &c).unwrap();
    let back = Config::load(&path).unwrap();
    assert_eq!(back.local.places, c.local.places);
    // an old file without places still reads
    assert!(
        Config::parse("[local]\ngoogle_news = \"x\"\n")
            .unwrap()
            .local
            .places
            .is_empty()
    );
}

// ---- The front page ----

mod front {
    use deckle_digest::chrono::NaiveDate;
    use deckle_digest::front::front_plan;
    use deckle_digest::{Article, Edition, Gathered, Section};

    fn paper(sections: &[(&str, &[&str])]) -> Gathered {
        let sections: Vec<Section> = sections
            .iter()
            .map(|(key, titles)| Section {
                brief: Vec::new(),
                key: (*key).into(),
                title: (*key).into(),
                articles: titles
                    .iter()
                    .map(|t| Article {
                        title: (*t).into(),
                        meta: Some("meta".into()),
                        ..Default::default()
                    })
                    .collect(),
                ..Default::default()
            })
            .collect();
        Gathered {
            news: (0..sections.len()).collect(),
            reading: None,
            long_reads: Vec::new(),
            reading_outcomes: Vec::new(),
            notes: Vec::new(),
            edition: Edition {
                large_print: false,
                paper: "The Deckle Daily".into(),
                date: NaiveDate::from_ymd_opt(2026, 10, 23).unwrap(),
                place: String::new(),
                weather_line: None,
                masthead: Default::default(),
                sections,
                assets: Vec::new(),
            },
        }
    }

    fn front_xhtml(g: &Gathered) -> String {
        let files = deckle_digest::render::render(g, Vec::new());
        let (_, bytes) = files
            .iter()
            .find(|(p, _)| p == "OEBPS/front.xhtml")
            .unwrap();
        String::from_utf8(bytes.clone()).unwrap()
    }

    fn named(n: usize, prefix: &str) -> Vec<String> {
        (0..n).map(|i| format!("{prefix}{i}")).collect()
    }

    fn with(local: usize, feed: usize) -> Gathered {
        let (l, f) = (named(local, "L"), named(feed, "F"));
        let l: Vec<&str> = l.iter().map(String::as_str).collect();
        let f: Vec<&str> = f.iter().map(String::as_str).collect();
        paper(&[("local", &l), ("feeds:A", &f)])
    }

    #[test]
    fn front_page_has_lead_table_and_inside_index() {
        let g = with(4, 4);
        let x = front_xhtml(&g);
        roxmltree::Document::parse(&x).unwrap();
        let plan = front_plan(&g);
        let lead = x.find("class=\"lead\"").unwrap();
        assert!(x.find("L0").unwrap() > lead - 40);
        assert!(lead < x.find("table class=\"cols\"").unwrap());
        let doc = roxmltree::Document::parse(&x).unwrap();
        let tiles: Vec<_> = doc
            .descendants()
            .filter(|n| n.has_tag_name("td"))
            .flat_map(|td| td.children().filter(|c| c.has_tag_name("a")))
            .collect();
        assert_eq!(tiles.len(), plan.tiles.len());
        let mut ids: Vec<_> = doc
            .descendants()
            .filter_map(|n| n.attribute("id"))
            .collect();
        let n = ids.len();
        ids.sort_unstable();
        ids.dedup();
        assert_eq!(ids.len(), n, "ids are unique");
        assert!(x.contains("Inside today"));
        assert!(!x.contains("Top stories"));
    }

    #[test]
    fn tile_meta_leaves_out_the_section_name() {
        let g = with(4, 4);
        let x = front_xhtml(&g);
        let plan = front_plan(&g);
        let doc = roxmltree::Document::parse(&x).unwrap();
        let metas: Vec<String> = doc
            .descendants()
            .filter(|n| n.has_tag_name("td"))
            .flat_map(|td| {
                td.descendants()
                    .filter(|c| c.attribute("class") == Some("meta"))
            })
            .filter_map(|m| m.text().map(String::from))
            .collect();
        assert_eq!(metas.len(), plan.tiles.len());
        assert!(metas.iter().all(|m| m == "meta"), "{metas:?}");
        // the lead keeps the section name
        assert!(x.contains("local · meta"));
    }

    #[test]
    fn inside_today_tells_empty_from_unavailable() {
        let mut g = paper(&[("local", &["One"]), ("feeds:A", &[]), ("feeds:B", &[])]);
        g.edition.sections[1].unavailable = Some("No new stories today.".into());
        g.edition.sections[2].unavailable = Some("Unavailable today (offline).".into());
        let x = front_xhtml(&g);
        assert!(x.contains("no new stories"), "{x}");
        assert_eq!(x.matches(">unavailable<").count(), 1);
    }

    #[test]
    fn odd_tiles_pad_the_last_row() {
        // lead + 3 tiles
        let x = front_xhtml(&with(2, 2));
        let doc = roxmltree::Document::parse(&x).unwrap();
        let tds: Vec<_> = doc.descendants().filter(|n| n.has_tag_name("td")).collect();
        assert_eq!(tds.len(), 4);
        assert!(tds[3].children().next().is_none());
    }

    #[test]
    fn no_tiles_no_table() {
        let x = front_xhtml(&with(1, 0));
        assert!(x.contains("class=\"lead\""));
        assert!(!x.contains("class=\"cols\""));
    }

    #[test]
    fn headlines_are_escaped() {
        let g = paper(&[("local", &["A & B <c>", "x"])]);
        let x = front_xhtml(&g);
        assert!(x.contains("A &amp; B &lt;c&gt;"));
        roxmltree::Document::parse(&x).unwrap();
    }

    fn section_xhtml(g: &Gathered, n: usize) -> String {
        let files = deckle_digest::render::render(g, Vec::new());
        let want = format!("OEBPS/s{n}.xhtml");
        let (_, bytes) = files.iter().find(|(p, _)| *p == want).unwrap();
        String::from_utf8(bytes.clone()).unwrap()
    }

    fn hrefs(x: &str, class: &str) -> Vec<String> {
        let doc = roxmltree::Document::parse(x).unwrap();
        doc.descendants()
            .filter(|n| n.attribute("class") == Some(class))
            .flat_map(|n| n.descendants().filter(|c| c.has_tag_name("a")))
            .filter_map(|a| a.attribute("href").map(String::from))
            .collect()
    }

    #[test]
    fn section_with_two_stories_has_headline_list() {
        let g = paper(&[("local", &["One", "Two"])]);
        let x = section_xhtml(&g, 1);
        roxmltree::Document::parse(&x).unwrap();
        assert!(x.contains("id=\"top\""));
        assert!(x.contains("In this section"));
        assert_eq!(hrefs(&x, "toc"), vec!["#a1", "#a2"]);
    }

    #[test]
    fn one_story_section_has_no_list() {
        let x = section_xhtml(&paper(&[("local", &["Only"])]), 1);
        assert!(!x.contains("class=\"toc\""));
        assert!(!x.contains("In this section"));
        assert!(x.contains("class=\"back\""));
    }

    #[test]
    fn back_links_target_the_tile_when_on_the_front_page() {
        let g = with(4, 4);
        let plan = front_plan(&g);
        let x = section_xhtml(&g, 1);
        let back = hrefs(&x, "back");
        assert_eq!(back.len(), 8);
        for j in 0..4 {
            let want = if plan.contains(0, j) {
                format!("front.xhtml#t0-{j}")
            } else {
                "front.xhtml".to_string()
            };
            assert_eq!(back[j * 2], "#top");
            assert_eq!(back[j * 2 + 1], want);
        }
        assert!(back.iter().any(|h| h.starts_with("front.xhtml#t0-")));
        assert!(x.contains("\u{21a9} local"));
        assert!(x.contains("\u{21a9} Front page"));
    }

    #[test]
    fn back_links_fall_back_to_front_xhtml() {
        let g = with(1, 12);
        let plan = front_plan(&g);
        let j = (0..12)
            .find(|&j| !plan.contains(1, j))
            .expect("one is off the front");
        let x = section_xhtml(&g, 2);
        let back = hrefs(&x, "back");
        assert_eq!(back[j * 2 + 1], "front.xhtml");
    }

    #[test]
    fn unavailable_section_has_no_back_links() {
        let mut g = paper(&[("local", &["One"]), ("feeds:A", &[])]);
        g.edition.sections[1].unavailable = Some("Down".into());
        let x = section_xhtml(&g, 2);
        roxmltree::Document::parse(&x).unwrap();
        assert!(x.contains("Down"));
        assert!(!x.contains("class=\"back\""));
        assert!(!x.contains("class=\"toc\""));
    }

    #[test]
    fn duplicate_titles_keep_anchors_unique() {
        let g = paper(&[("local", &["Same", "Same", "Same"])]);
        let x = section_xhtml(&g, 1);
        let doc = roxmltree::Document::parse(&x).unwrap();
        let mut ids: Vec<_> = doc
            .descendants()
            .filter_map(|n| n.attribute("id"))
            .collect();
        let n = ids.len();
        ids.sort_unstable();
        ids.dedup();
        assert_eq!(ids.len(), n);
        assert_eq!(hrefs(&x, "toc"), vec!["#a1", "#a2", "#a3"]);
    }

    #[test]
    fn titles_are_escaped_in_the_list() {
        let g = paper(&[("local", &["A & B <c>", "x"])]);
        let x = section_xhtml(&g, 1);
        roxmltree::Document::parse(&x).unwrap();
        assert!(x.contains("A &amp; B &lt;c&gt;"));
    }
}

// ---- Export folders (other readers design §7) ----

use deckle_digest::config::{ExportConfig, Photos, STARTER, save, validate_exports};

fn exporting(c: &mut Config, photos: Photos) {
    c.export = vec![ExportConfig {
        name: "iPad".into(),
        dir: "~/Deckle/iPad".into(),
        photos,
        ..Default::default()
    }];
}

#[test]
fn a_color_export_builds_a_color_edition_beside_the_paper() {
    let (_d, mut c, _q, _) = reading_setup(&["https://ex.com/long"]);
    exporting(&mut c, Photos::Color);
    let out = tempfile::tempdir().unwrap();
    let r = build(&c, date(), now(), out.path(), &net_with_pages).unwrap();
    let color = r.color_epub.as_ref().unwrap();
    assert_eq!(
        color,
        &out.path().join("color/deckle-daily-2026-10-20.epub")
    );
    assert_eq!(r.color_long_reads.len(), r.long_reads.len());
    assert_eq!(
        r.color_long_reads[0].file_name(),
        r.long_reads[0].file_name()
    );
    // Same pages, same files: only the photos may differ.
    let names = |p: &PathBuf| {
        let mut z = read_zip(p);
        let mut n: Vec<String> = z.file_names().map(|n| n.unwrap().into_owned()).collect();
        n.sort();
        let mut text = String::new();
        z.by_name("OEBPS/section-1.xhtml")
            .map(|mut f| f.read_to_string(&mut text).unwrap())
            .ok();
        (n, text)
    };
    assert_eq!(names(&r.epub), names(color));
}

#[test]
fn a_gray_export_or_none_builds_only_the_paper() {
    for photos in [None, Some(Photos::Gray)] {
        let mut c = config();
        if let Some(p) = photos {
            exporting(&mut c, p);
        }
        let out = tempfile::tempdir().unwrap();
        let r = build(&c, date(), now(), out.path(), &fake).unwrap();
        assert_eq!(r.color_epub, None);
        assert!(!out.path().join("color").exists());
    }
}

#[test]
fn the_starter_export_block_parses_once_uncommented() {
    let start = STARTER.find("# [[export]]").unwrap();
    let block: String = STARTER[start..]
        .lines()
        .take_while(|l| l.starts_with("# "))
        .map(|l| format!("{}\n", &l[2..]))
        .collect();
    let c = Config::parse(&format!("{STARTER}\n{block}")).unwrap();
    assert_eq!(c.export.len(), 1);
    let e = &c.export[0];
    assert_eq!(e.name, "iPad");
    assert!(e.dir.ends_with("Deckle Daily"));
    assert_eq!((e.photos, e.keep, e.books), (Photos::Color, 7, false));
    assert!(c.wants_color());
    assert!(Config::parse(STARTER).unwrap().export.is_empty());
}

#[test]
fn exports_need_a_unique_name_and_a_folder() {
    let one = |name: &str, dir: &str| ExportConfig {
        name: name.into(),
        dir: dir.into(),
        ..Default::default()
    };
    assert!(validate_exports(&[one("iPad", "~/a"), one("Boox", "~/b")]).is_ok());
    assert!(validate_exports(&[one("", "~/a")]).is_err());
    assert!(validate_exports(&[one("iPad", " ")]).is_err());
    assert!(validate_exports(&[one("iPad", "~/a"), one("iPad", "~/b")]).is_err());
    let err = Config::parse("[[export]]\nname = \"iPad\"\ndir = \"\"\n").unwrap_err();
    assert!(err.contains("dir is empty"), "{err}");
    // A Kobo's KEPUB isn't an export format: a Kobo takes it from the outbox.
    assert!(
        Config::parse("[[export]]\nname = \"x\"\ndir = \"~/x\"\nformat = \"kepub\"\n").is_err()
    );
}

#[test]
fn a_save_writes_exports_only_when_there_are_some() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("digest.toml");
    let mut c = Config::parse(STARTER).unwrap();
    save(&path, &c).unwrap();
    let text = std::fs::read_to_string(&path).unwrap();
    assert!(
        !text
            .lines()
            .any(|l| l.starts_with("[[export]]") || l.starts_with("export"))
    );
    exporting(&mut c, Photos::Color);
    save(&path, &c).unwrap();
    let back = Config::load(&path).unwrap();
    assert_eq!(back.export, c.export);
    c.export.clear();
    save(&path, &c).unwrap();
    assert!(Config::load(&path).unwrap().export.is_empty());
    let mut bad = c.clone();
    exporting(&mut bad, Photos::Gray);
    bad.export[0].name.clear();
    assert!(save(&path, &bad).is_err());
}

// ---- The PDF edition (pdf edition design §8) ----

#[test]
fn the_pdf_edition_reads_back_with_its_titles_and_outline() {
    use deckle_digest::config::Photos;
    use deckle_digest::pdf::{Screen, render};
    let g = gather(&config(), date(), now(), &fake);
    let bytes = render(&g, Screen::Remarkable2, Photos::Gray);
    assert!(bytes.starts_with(b"%PDF-"));
    assert!(bytes.trim_ascii_end().ends_with(b"%%EOF"));
    let pdfium = deckle_pdf::engine::pdfium().expect("PDFium (tools/fetch-pdfium.sh)");
    let doc = pdfium.load_pdf_from_byte_slice(&bytes, None).unwrap();
    let pages = doc.pages();
    assert!(
        pages.len() as usize > g.edition.sections.len(),
        "a front page and a page per section at least"
    );
    let page_w = pages.get(0).unwrap().width().value;
    assert!((page_w - 447.3).abs() < 1.0, "{page_w}");
    let text: String = pages
        .iter()
        .map(|p| p.text().unwrap().all())
        .collect::<Vec<_>>()
        .join("\n");
    // Ligatures and line breaks aside, the words come back.
    let flat = text.split_whitespace().collect::<Vec<_>>().join(" ");
    assert!(flat.contains("Inside today"), "{flat}");
    for s in &g.edition.sections {
        assert!(flat.contains(&s.title), "{} missing", s.title);
    }
    let story = &g.edition.sections[2].articles[0].title;
    let first_words: String = story
        .split_whitespace()
        .take(3)
        .collect::<Vec<_>>()
        .join(" ");
    assert!(flat.contains(&first_words), "{first_words}");
    // The outline: the front page, each section, and each story under its section.
    let marks = doc.bookmarks().iter().count();
    let stories: usize = g.edition.sections.iter().map(|s| s.articles.len()).sum();
    assert_eq!(marks, 1 + g.edition.sections.len() + stories);
}

#[test]
fn every_screen_lays_the_paper_out() {
    use deckle_digest::config::Photos;
    use deckle_digest::pdf::{Screen, render};
    let g = gather(&config(), date(), now(), &fake);
    for screen in [
        Screen::Remarkable2,
        Screen::RemarkablePaperPro,
        Screen::RemarkablePaperProMove,
        Screen::Ipad,
        Screen::A5,
    ] {
        let bytes = render(&g, screen, Photos::Color);
        let doc = deckle_pdf::engine::pdfium()
            .unwrap()
            .load_pdf_from_byte_slice(&bytes, None)
            .unwrap();
        let p = doc.pages().get(0).unwrap();
        let (w, h) = screen.page();
        assert!((p.width().value - w).abs() < 1.0 && (p.height().value - h).abs() < 1.0);
    }
}

#[test]
fn a_pdf_export_or_a_remarkable_builds_its_pdf_edition() {
    use deckle_digest::pdf::Screen;
    let mut c = config();
    exporting(&mut c, Photos::Gray);
    c.export[0].format = deckle_digest::config::ExportFormat::Pdf;
    c.remarkable.enabled = true;
    c.remarkable.screen = Screen::RemarkablePaperPro;
    let out = tempfile::tempdir().unwrap();
    let r = build(&c, date(), now(), out.path(), &fake).unwrap();
    assert_eq!(r.pdfs.len(), 2);
    let rm2 = r.pdf(Screen::Remarkable2, Photos::Gray).unwrap();
    assert!(std::fs::read(rm2).unwrap().starts_with(b"%PDF-"));
    // A Paper Pro's PDF is in color, so the photos were kept in color.
    assert!(r.pdf(Screen::RemarkablePaperPro, Photos::Color).is_some());
    assert!(c.wants_color());
}

#[test]
fn the_starter_remarkable_block_parses_once_uncommented() {
    let start = STARTER.find("# [remarkable]").unwrap();
    let block: String = STARTER[start..]
        .lines()
        .take_while(|l| l.starts_with("# "))
        .map(|l| format!("{}\n", &l[2..]))
        .collect();
    let c = Config::parse(&format!("{STARTER}\n{block}")).unwrap();
    assert!(c.remarkable.enabled);
    assert_eq!(c.remarkable.photos(), Photos::Gray);
    assert_eq!(c.pdfs_wanted().len(), 1);
}

// ---- The edition bundle (edition bundle design §13) ----

fn bundle_export(c: &mut Config, dir: &Path, books: bool) {
    c.export = vec![ExportConfig {
        name: "iPad app".into(),
        dir: dir.display().to_string(),
        format: deckle_digest::config::ExportFormat::Bundle,
        books,
        keep: 2,
        ..Default::default()
    }];
}

fn edition_json(bundle: &Path) -> serde_json::Value {
    serde_json::from_slice(&std::fs::read(bundle.join("edition.json")).unwrap()).unwrap()
}

#[test]
fn the_bundle_validates_against_its_schema_and_holds_the_whole_edition() {
    let out = tempfile::tempdir().unwrap();
    let mut c = config();
    bundle_export(&mut c, out.path(), false);
    let g = gather(&c, date(), now(), &fake);
    let r = build(&c, date(), now(), out.path(), &fake).unwrap();
    let bundle = r.bundle(false).unwrap();
    assert_eq!(
        bundle.file_name().unwrap(),
        "deckle-daily-2026-10-20.deckle"
    );
    let v = edition_json(bundle);

    let schema: serde_json::Value = serde_json::from_str(include_str!(
        "../../../docs/formats/edition-bundle.schema.json"
    ))
    .unwrap();
    let validator = jsonschema::validator_for(&schema).unwrap();
    let errors: Vec<String> = validator.iter_errors(&v).map(|e| e.to_string()).collect();
    assert!(errors.is_empty(), "{errors:#?}");

    assert_eq!(v["format"], "deckle-edition");
    assert_eq!(v["version"], "1.1");
    assert_eq!(v["edition"]["date"], "2026-10-20");
    let sections = v["sections"].as_array().unwrap();
    assert_eq!(sections.len(), g.edition.sections.len());
    for (s, js) in g.edition.sections.iter().zip(sections) {
        assert_eq!(js["title"], s.title.as_str());
        assert_eq!(js["stories"].as_array().unwrap().len(), s.articles.len());
    }
    // Every story id the front page names exists.
    let ids: Vec<&str> = sections
        .iter()
        .flat_map(|s| s["stories"].as_array().unwrap())
        .map(|s| s["id"].as_str().unwrap())
        .collect();
    for t in v["front"]["tiles"].as_array().unwrap() {
        assert!(ids.contains(&t.as_str().unwrap()), "{t}");
    }
    // Feeds carry their links.
    let hn = sections.iter().find(|s| s["kind"] == "feeds").unwrap();
    assert!(
        hn["stories"][0]["url"]
            .as_str()
            .unwrap()
            .starts_with("http")
    );
    // Every file is listed at its size and hash, and the bundle reads as whole.
    for f in v["files"].as_array().unwrap() {
        let bytes = std::fs::read(bundle.join(f["path"].as_str().unwrap())).unwrap();
        assert_eq!(bytes.len() as u64, f["size"].as_u64().unwrap());
        use sha2::Digest;
        let hash: String = sha2::Sha256::digest(&bytes)
            .iter()
            .map(|b| format!("{b:02x}"))
            .collect();
        assert_eq!(hash, f["sha256"].as_str().unwrap());
    }
    assert!(deckle_digest::bundle::is_complete(bundle));
    assert!(bundle.join("epub/deckle-daily-2026-10-20.epub").is_file());
    // No icon path is left pointing into the EPUB.
    let text = std::fs::read_to_string(bundle.join("edition.json")).unwrap();
    assert!(!text.contains("images/wx-"), "icons point at media/");
}

/// [`fake`], plus every story page as a long article with a captioned photo, and every photo
/// as a 1600 × 1000 JPEG.
fn fake_with_articles(url: &str) -> Result<Vec<u8>, String> {
    if url.ends_with(".jpg") {
        // A different picture per story: the paper drops a photo it sees in many stories.
        let seed = url
            .bytes()
            .fold(7u32, |h, b| h.wrapping_mul(31).wrapping_add(u32::from(b)));
        let img = image::RgbImage::from_fn(1600, 1000, |x, y| {
            let v = (x / (8 + seed % 40)) ^ (y / (6 + seed % 30));
            image::Rgb([
                (v * 37 % 256) as u8,
                (v * 91 % 256) as u8,
                ((v * 13 + seed) % 256) as u8,
            ])
        });
        let mut out = Vec::new();
        image::DynamicImage::ImageRgb8(img)
            .write_to(
                &mut std::io::Cursor::new(&mut out),
                image::ImageFormat::Jpeg,
            )
            .unwrap();
        return Ok(out);
    }
    if let Ok(b) = fake(url) {
        return Ok(b);
    }
    if !url.starts_with("http") || url.starts_with("https://news.google.com/") {
        return Err(format!("offline: {url}"));
    }
    let para = "The council met on Tuesday night to talk about the old rail yard and what it could become. ".repeat(12);
    let html = format!(
        "<html lang=\"en\"><head><title>A Story</title></head><body><article><h1>A Story</h1>\
         <p>{para}</p><figure><img src=\"https://cdn.example/{}.jpg\" alt=\"The rail yard at dusk\"/>\
         <figcaption>The yard on Saturday. (Photo: Jane Doe / The Pilot)</figcaption></figure>\
         <p>“We have waited forty years for somebody to do something with that yard,” said one stall holder.</p>\
         <p>Advertisement</p>{}</article></body></html>",
        url.len() * 7919 % 100_000,
        format!("<p>{para}</p>").repeat(5)
    );
    Ok(html.into_bytes())
}

#[test]
fn a_bundle_carries_full_size_photos_with_captions_and_story_details() {
    let out = tempfile::tempdir().unwrap();
    let mut c = config();
    bundle_export(&mut c, out.path(), false);
    let r = build(&c, date(), now(), out.path(), &fake_with_articles).unwrap();
    let bundle = r.bundle(false).unwrap();
    let v = edition_json(bundle);
    let schema: serde_json::Value = serde_json::from_str(include_str!(
        "../../../docs/formats/edition-bundle.schema.json"
    ))
    .unwrap();
    let validator = jsonschema::validator_for(&schema).unwrap();
    let errors: Vec<String> = validator.iter_errors(&v).map(|e| e.to_string()).collect();
    assert!(errors.is_empty(), "{errors:#?}");
    let stories: Vec<&serde_json::Value> = v["sections"]
        .as_array()
        .unwrap()
        .iter()
        .flat_map(|s| s["stories"].as_array().unwrap())
        .collect();
    let with_photo = stories
        .iter()
        .find(|s| !s["photos"].as_array().unwrap().is_empty())
        .unwrap_or_else(|| {
            panic!(
                "a story with its photo: {:#?}",
                stories
                    .iter()
                    .map(|s| s["body"]["text"]
                        .as_str()
                        .unwrap()
                        .chars()
                        .take(300)
                        .collect::<String>())
                    .collect::<Vec<_>>()
            )
        });
    let photo = &with_photo["photos"][0];
    // The master, not the 1000-px e-ink copy.
    assert_eq!(
        (photo["width"].as_u64(), photo["height"].as_u64()),
        (Some(1600), Some(1000))
    );
    assert_eq!(photo["caption"], "The yard on Saturday.");
    assert_eq!(photo["credit"], "Jane Doe / The Pilot");
    assert_eq!(photo["alt"], "The rail yard at dusk");
    assert!(photo["focus"]["x"].as_f64().is_some());
    assert!(photo["color"].as_str().unwrap().starts_with('#'));
    let media = bundle.join(photo["file"].as_str().unwrap());
    let img = image::load_from_memory(&std::fs::read(media).unwrap()).unwrap();
    assert_eq!(img.color(), image::ColorType::Rgb8);
    assert_eq!(
        with_photo["pull_quote"].as_str().unwrap(),
        "\u{201c}We have waited forty years for somebody to do something with that yard,\u{201d} said one stall holder."
    );
    assert!(with_photo["minutes"].as_u64().unwrap() >= 3);
    assert_eq!(with_photo["partial"], false);
    assert!(
        !with_photo["body"]["text"]
            .as_str()
            .unwrap()
            .contains("Advertisement")
    );
    // Every story says where the front page put it, and the lead is the front's lead.
    let lead = v["front"]["lead"].as_str();
    for s in &stories {
        let p = s["prominence"].as_str().unwrap();
        assert!(["lead", "front", "inside"].contains(&p));
        assert_eq!(
            p == "lead",
            Some(s["id"].as_str().unwrap()) == lead,
            "{}",
            s["id"]
        );
    }
}

#[test]
fn weather_and_puzzle_data_match_what_the_page_prints() {
    let out = tempfile::tempdir().unwrap();
    let mut c = config();
    bundle_export(&mut c, out.path(), false);
    let r = build(&c, date(), now(), out.path(), &fake).unwrap();
    let v = edition_json(r.bundle(false).unwrap());
    let sections = v["sections"].as_array().unwrap();
    let w = sections.iter().find(|s| s["kind"] == "weather").unwrap();
    let days = w["weather"]["days"].as_array().unwrap();
    assert_eq!(days.len(), 5);
    let page = w["stories"][0]["body"]["text"].as_str().unwrap();
    for d in days {
        let high = format!("<b>{}°</b>", d["high"]);
        assert!(page.contains(&high), "{high} in {page}");
        let icon = d["icon"].as_str().unwrap();
        assert!(page.contains(icon), "the strip shows {icon}");
    }
    let p = sections.iter().find(|s| s["kind"] == "puzzle").unwrap();
    let puzzle = &p["puzzle"];
    let grid: Vec<&str> = puzzle["grid"]
        .as_array()
        .unwrap()
        .iter()
        .map(|r| r.as_str().unwrap())
        .collect();
    assert_eq!(grid.len(), 5);
    let mini = p["stories"][0]["body"]["text"].as_str().unwrap();
    for clue in puzzle["clues"].as_array().unwrap() {
        let text = clue["clue"].as_str().unwrap();
        assert!(mini.contains(&deckle_digest::esc(text)), "{text}");
        // Each answer is in the grid, across or down.
        let answer = clue["answer"].as_str().unwrap();
        let down: Vec<String> = (0..5)
            .map(|c| grid.iter().map(|r| r.as_bytes()[c] as char).collect())
            .collect();
        assert!(
            grid.iter().any(|r| r.contains(answer)) || down.iter().any(|d| d.contains(answer)),
            "{answer}"
        );
    }
}

#[test]
fn a_bundle_export_copies_it_in_whole_and_keeps_only_the_newest() {
    let out = tempfile::tempdir().unwrap();
    let ipad = out.path().join("ipad");
    std::fs::create_dir(&ipad).unwrap();
    for d in ["2026-10-17", "2026-10-18", "2026-10-19"] {
        std::fs::create_dir(ipad.join(format!("deckle-daily-{d}.deckle"))).unwrap();
    }
    // One being written, and a file of the owner's: neither is an edition.
    std::fs::create_dir(ipad.join(".deckle-daily-2026-10-16.deckle.tmp")).unwrap();
    std::fs::write(ipad.join("notes.txt"), b"mine").unwrap();
    let mut c = config();
    bundle_export(&mut c, &ipad, false);
    let built = out.path().join("build");
    let r = build(&c, date(), now(), &built, &fake).unwrap();
    let reports = deckle_digest::export::export_all(&c.export, date(), &r);
    assert_eq!(reports[0].error, None);
    let copied = ipad.join("deckle-daily-2026-10-20.deckle");
    assert_eq!(reports[0].written, std::slice::from_ref(&copied));
    assert!(deckle_digest::bundle::is_complete(&copied));
    let mut left: Vec<String> = std::fs::read_dir(&ipad)
        .unwrap()
        .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
        .collect();
    left.sort();
    assert_eq!(
        left,
        [
            ".deckle-daily-2026-10-16.deckle.tmp",
            "deckle-daily-2026-10-19.deckle",
            "deckle-daily-2026-10-20.deckle",
            "notes.txt"
        ]
    );
}

#[test]
fn a_partly_synced_bundle_never_reads_as_whole() {
    let out = tempfile::tempdir().unwrap();
    let mut c = config();
    bundle_export(&mut c, out.path(), false);
    let r = build(&c, date(), now(), out.path(), &fake).unwrap();
    let bundle = r.bundle(false).unwrap();
    assert!(deckle_digest::bundle::is_complete(bundle));
    std::fs::remove_file(bundle.join("cover.jpg")).unwrap();
    assert!(!deckle_digest::bundle::is_complete(bundle));
    std::fs::remove_file(bundle.join("edition.json")).unwrap();
    assert!(!deckle_digest::bundle::is_complete(bundle));
}

#[test]
fn long_reads_ride_inside_a_bundle_only_with_books() {
    let (_d, mut c, _q, _) = reading_setup(&["https://ex.com/long"]);
    let out = tempfile::tempdir().unwrap();
    bundle_export(&mut c, out.path(), true);
    let r = build(&c, date(), now(), out.path(), &net_with_pages).unwrap();
    let v = edition_json(r.bundle(true).unwrap());
    let read = v["sections"]
        .as_array()
        .unwrap()
        .iter()
        .flat_map(|s| s["stories"].as_array().unwrap())
        .find_map(|s| s["long_read"].as_str())
        .expect("the long read is named");
    assert!(read.starts_with("epub/deckle-reads-"));
    assert!(r.bundle(true).unwrap().join(read).is_file());
    assert!(
        r.bundle(false).is_none(),
        "only the bundle asked for is built"
    );
}

// ---- the masthead font (masthead design) -------------------------------------------------------

#[test]
fn the_masthead_setting_is_left_out_of_a_save_until_it_is_changed() {
    use deckle_digest::masthead::MastheadFont;
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("digest.toml");
    let mut c = config();
    deckle_digest::config::save(&path, &c).unwrap();
    let set = |t: &str| t.lines().any(|l| l.starts_with("masthead"));
    assert!(!set(&std::fs::read_to_string(&path).unwrap()));
    c.masthead = MastheadFont::Sans;
    deckle_digest::config::save(&path, &c).unwrap();
    let text = std::fs::read_to_string(&path).unwrap();
    assert!(text.contains("masthead = \"sans\""), "{text}");
    assert_eq!(Config::parse(&text).unwrap().masthead, MastheadFont::Sans);
}

#[test]
fn the_cover_is_drawn_in_the_chosen_masthead_face() {
    use deckle_digest::masthead::MastheadFont;
    let jpeg = |face: MastheadFont| {
        let mut c = config();
        c.masthead = face;
        deckle_digest::cover::render(&gather(&c, date(), now(), &fake)).unwrap()
    };
    let black = jpeg(MastheadFont::Blackletter);
    assert_ne!(black, jpeg(MastheadFont::Serif));
    assert_ne!(black, jpeg(MastheadFont::Sans));
    assert_eq!(black, jpeg(MastheadFont::Blackletter), "and it is stable");
}

#[test]
fn a_bad_masthead_file_falls_back_and_says_so() {
    use deckle_digest::masthead::MastheadFont;
    let mut c = config();
    c.masthead = MastheadFont::File("/no/such/font.ttf".into());
    let g = gather(&c, date(), now(), &fake);
    assert!(g.edition.masthead.is_blackletter());
    assert!(
        g.notes.iter().any(|n| n.starts_with("Masthead font:")),
        "{:?}",
        g.notes
    );
    deckle_digest::cover::render(&g).expect("the paper still comes out");
}

#[test]
fn the_pdf_embeds_the_chosen_masthead_face() {
    use deckle_digest::config::Photos;
    use deckle_digest::masthead::MastheadFont;
    use deckle_digest::pdf::{Screen, render};
    let has = |bytes: &[u8], needle: &[u8]| bytes.windows(needle.len()).any(|w| w == needle);
    let mut c = config();
    let default = render(
        &gather(&c, date(), now(), &fake),
        Screen::Remarkable2,
        Photos::Gray,
    );
    assert!(has(&default, b"UnifrakturMaguntia-Book"));
    c.masthead = MastheadFont::Serif;
    let serif = render(
        &gather(&c, date(), now(), &fake),
        Screen::Remarkable2,
        Photos::Gray,
    );
    assert!(!has(&serif, b"UnifrakturMaguntia-Book"));
    assert!(has(&serif, b"/Masthead"));
}

/// An extra paper (many papers §3), with its state in a temp folder.
fn as_extra(mut c: Config, slug: &str, dir: &std::path::Path) -> Config {
    c.paper = Some(deckle_digest::config::PaperRef {
        slug: slug.into(),
        state_dir: dir.join(slug),
    });
    c
}

#[test]
fn an_extra_paper_never_prints_saved_articles() {
    let (d, c, _q, _ids) = reading_setup(&[WXII]);
    let extra = as_extra(c.clone(), "kid-courier", d.path());
    let g = gather(&extra, date(), now(), &net_with_pages);
    assert!(g.edition.sections.iter().all(|s| s.title != "Your Reading"));
    assert!(g.reading_outcomes.is_empty());
    assert_eq!(g.reading, None);
    assert!(
        g.notes
            .iter()
            .any(|n| n.contains("Saved articles go to the main paper"))
    );
    // The main paper still prints them.
    let g = gather(&c, date(), now(), &net_with_pages);
    assert_eq!(g.edition.sections[0].title, "Your Reading");
}

#[test]
fn an_extra_papers_build_is_named_by_its_stem() {
    let out = tempfile::tempdir().unwrap();
    let extra = as_extra(config(), "kid-courier", out.path());
    let r = build(&extra, date(), now(), out.path(), &fake).unwrap();
    assert_eq!(
        r.epub,
        out.path().join("deckle-daily-kid-courier-2026-10-20.epub")
    );
    assert_eq!(r.prefix, "deckle-daily-kid-courier-");
}

// ---- A subscribed paper's section group (publisher subscriptions PR 2) ----

use deckle_digest::SectionKind;
use deckle_digest::config::{SubscribedConfig, SubscriptionConfig};

const PILOT_FEEDS: &str = "https://www.thepilot.com/search/?f=rss&t=article&c=";
/// A Local feed carrying one story the group's News feed also carries.
const SHARED_LOCAL: &str = "https://local.example/rss";

/// The two BLOX section feeds, their story pages, and a local feed sharing one story.
fn pilot_net(url: &str) -> Result<Vec<u8>, String> {
    if let Some(rest) = url.strip_prefix(PILOT_FEEDS) {
        let slug = rest.split('&').next().unwrap();
        return match slug {
            "news" => Ok(fixture("blox-news.xml")),
            "sports" => Ok(fixture("blox-sports.xml")),
            _ => Err(format!("offline: {url}")),
        };
    }
    if url.starts_with("https://www.thepilot.com/") && url.ends_with(".html") {
        return Ok(long_page(600).into_bytes());
    }
    if url == SHARED_LOCAL {
        return Ok(
            br#"<?xml version="1.0"?><rss version="2.0"><channel><title>Local Wire</title>
<item><title>Town board weighs a new water tower</title><link>https://local.example/tower</link>
<description><![CDATA[<p>The wire's synthetic copy.</p>]]></description>
<pubDate>Tue, 20 Oct 2026 21:00:00 +0000</pubDate></item></channel></rss>"#
                .to_vec(),
        );
    }
    fake(url)
}

fn subscribed_config(sections: Vec<SectionKind>, subscribed_account: bool) -> Config {
    let mut c = config();
    c.sections = sections;
    c.subscribed = vec![SubscribedConfig {
        site: "thepilot.com".into(),
        name: Some("The Pilot".into()),
        sections: vec!["news".into(), "sports".into()],
        max_items: None,
        max_age_hours: None,
    }];
    if subscribed_account {
        c.subscriptions = vec![SubscriptionConfig {
            site: "thepilot.com".into(),
            username: "reader".into(),
            kind: Some("blox".into()),
        }];
    }
    c
}

const STRANGER_NOTE: &str =
    "No subscription for thepilot.com in digest.toml; stories print as a stranger sees them.";

#[test]
fn a_subscribed_group_prints_one_section_per_slug_after_local() {
    let c = subscribed_config(vec![SectionKind::Local, SectionKind::Subscribed], true);
    let asked = std::sync::Mutex::new(Vec::<String>::new());
    let net = |url: &str| {
        asked.lock().unwrap().push(url.to_owned());
        pilot_net(url)
    };
    let g = gather(&c, date(), now(), &net);
    let keys: Vec<&str> = g.edition.sections.iter().map(|s| s.key.as_str()).collect();
    assert_eq!(
        keys,
        [
            "local",
            "subscribed:thepilot.com:news",
            "subscribed:thepilot.com:sports"
        ]
    );
    let (news, sports) = (&g.edition.sections[1], &g.edition.sections[2]);
    assert_eq!(news.title, "News");
    assert_eq!(sports.title, "Sports");
    assert_eq!(news.kicker.as_deref(), Some("The Pilot"));
    assert_eq!(sports.kicker.as_deref(), Some("The Pilot"));
    let titles: Vec<&str> = news.articles.iter().map(|a| a.title.as_str()).collect();
    assert_eq!(
        titles,
        [
            "Town board weighs a new water tower",
            "Library reopens after roof repairs"
        ]
    );
    // whole stories from the story pages, not the feed's summaries
    assert!(
        news.articles[0].body.contains("library"),
        "{}",
        news.articles[0].body
    );
    assert_eq!(sports.articles.len(), 2);
    // both are news (lead and puzzle theme)
    assert_eq!(g.news, [0, 1, 2]);
    // the feed asked for [local] max_items stories
    let asked = asked.lock().unwrap();
    assert!(
        asked
            .iter()
            .any(|u| u.starts_with(&format!("{PILOT_FEEDS}news&l=12&"))),
        "{asked:?}"
    );
    assert!(!g.notes.iter().any(|n| n == STRANGER_NOTE), "{:?}", g.notes);
}

#[test]
fn a_story_in_local_and_the_group_prints_once_in_local() {
    let mut c = subscribed_config(vec![SectionKind::Local, SectionKind::Subscribed], true);
    c.local.google_news = String::new();
    c.local.feeds = vec![SHARED_LOCAL.into()];
    let g = gather(&c, date(), now(), &pilot_net);
    let shared = "Town board weighs a new water tower";
    let holders: Vec<&str> = g
        .edition
        .sections
        .iter()
        .filter(|s| s.articles.iter().any(|a| a.title == shared))
        .map(|s| s.key.as_str())
        .collect();
    assert_eq!(holders, ["local"]);
    // the group's News refills with its next story
    let news = &g.edition.sections[1];
    assert_eq!(news.key, "subscribed:thepilot.com:news");
    assert_eq!(news.articles.len(), 1);
    assert_eq!(news.articles[0].title, "Library reopens after roof repairs");
}

#[test]
fn a_group_before_local_prints_the_shared_story() {
    let mut c = subscribed_config(vec![SectionKind::Subscribed, SectionKind::Local], true);
    c.local.google_news = String::new();
    c.local.feeds = vec![SHARED_LOCAL.into()];
    let g = gather(&c, date(), now(), &pilot_net);
    let news = &g.edition.sections[0];
    assert_eq!(news.key, "subscribed:thepilot.com:news");
    assert_eq!(
        news.articles[0].title,
        "Town board weighs a new water tower"
    );
    assert_eq!(news.articles.len(), 2);
}

#[test]
fn a_group_without_an_account_prints_as_a_stranger_with_a_note() {
    let c = subscribed_config(vec![SectionKind::Local, SectionKind::Subscribed], false);
    let g = gather(&c, date(), now(), &pilot_net);
    let keys: Vec<&str> = g.edition.sections.iter().map(|s| s.key.as_str()).collect();
    assert_eq!(
        keys,
        [
            "local",
            "subscribed:thepilot.com:news",
            "subscribed:thepilot.com:sports"
        ]
    );
    assert_eq!(g.edition.sections[1].articles.len(), 2);
    assert_eq!(
        g.notes.iter().filter(|n| *n == STRANGER_NOTE).count(),
        1,
        "{:?}",
        g.notes
    );
}
