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
                paper: "The Deckle Daily".into(),
                date: NaiveDate::from_ymd_opt(2026, 10, 23).unwrap(),
                place: String::new(),
                weather_line: None,
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
