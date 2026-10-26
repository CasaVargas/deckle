// SPDX-License-Identifier: GPL-3.0-or-later
//! Tomorrow's paper, so far: live headlines (Task 5) and the composed view (Task 6).

use std::sync::Mutex;

use chrono::{DateTime, Duration, Utc};
use deckle_digest::config::{Config, FeedConfig};
use deckle_digest::next::live::{LiveCache, REFRESH_EVERY_MIN, checked, live_urls, refresh};

const SCIENCE: &str = "https://science.example.org/rss";
const OTHER: &str = "https://other.example.org/feed";

fn t0() -> DateTime<Utc> {
    "2026-10-22T19:08:00Z".parse().unwrap()
}

fn cfg() -> Config {
    Config {
        title: "The Morning Deckle".into(),
        place: "Linden Falls, NC".into(),
        feeds: vec![
            FeedConfig {
                name: "Science".into(),
                url: SCIENCE.into(),
                max_items: 10,
                ..Default::default()
            },
            FeedConfig {
                name: "Other".into(),
                url: OTHER.into(),
                max_items: 10,
                ..Default::default()
            },
        ],
        ..Config::default()
    }
}

/// Serves the fixtures and counts requests; any other URL fails.
struct Counting {
    log: Mutex<Vec<String>>,
}

impl Counting {
    fn new() -> Self {
        Counting {
            log: Mutex::new(Vec::new()),
        }
    }
    fn count(&self) -> usize {
        self.log.lock().unwrap().len()
    }
}

impl deckle_digest::Net for Counting {
    fn get(&self, url: &str) -> Result<Vec<u8>, String> {
        self.log.lock().unwrap().push(url.to_owned());
        if url.starts_with("https://news.google.com/") {
            Ok(include_bytes!("fixtures/next/gnews.xml").to_vec())
        } else if url == SCIENCE || url == OTHER {
            Ok(include_bytes!("fixtures/next/science.xml").to_vec())
        } else {
            Err(format!("offline: {url}"))
        }
    }
    fn get_page(&self, url: &str) -> Result<Vec<u8>, String> {
        panic!("an article page was fetched: {url}")
    }
}

#[test]
fn live_throttles_per_url() {
    let net = Counting::new();
    let mut cache = LiveCache::default();
    let c = cfg();
    assert_eq!(refresh(&c, &mut cache, t0(), &net), 2);
    assert_eq!(
        refresh(&c, &mut cache, t0() + Duration::minutes(10), &net),
        0
    );
    assert_eq!(
        refresh(
            &c,
            &mut cache,
            t0() + Duration::minutes(REFRESH_EVERY_MIN + 1),
            &net
        ),
        2
    );
    assert_eq!(net.count(), 4);
    assert_eq!(checked(&cache), Some(t0() + Duration::minutes(31)));
}

#[test]
fn live_failure_waits_too() {
    let net = Counting::new();
    let mut c = cfg();
    c.feeds[1].url = "https://down.example.org/feed".into();
    let mut cache = LiveCache::default();
    refresh(&c, &mut cache, t0(), &net);
    let f = &cache.feeds["https://down.example.org/feed"];
    assert!(f.error.as_deref().unwrap().contains("offline"));
    assert!(f.items.is_empty());
    assert_eq!(
        refresh(&c, &mut cache, t0() + Duration::minutes(10), &net),
        0
    );
    assert_eq!(
        refresh(&c, &mut cache, t0() + Duration::minutes(31), &net),
        2
    );
}

#[test]
fn live_only_feed_urls() {
    // weather, lottery and On This Day are on in the default sections; none is fetched
    let net = Counting::new();
    let mut c = cfg();
    c.local.google_news = "Linden Falls".into();
    c.local.feeds = vec![OTHER.into()];
    let mut cache = LiveCache::default();
    refresh(&c, &mut cache, t0(), &net);
    let urls = live_urls(&c);
    assert_eq!(urls.len(), 3, "{urls:?}: the shared URL is fetched once");
    assert!(urls[0].starts_with("https://news.google.com/rss/search?q=Linden+Falls"));
    for u in net.log.lock().unwrap().iter() {
        assert!(urls.contains(u), "{u}");
    }
}

#[test]
fn live_briefs_and_age_window() {
    let net = Counting::new();
    let mut c = cfg();
    c.local.google_news = "Linden Falls".into();
    c.local.max_age_hours = 48;
    c.max_age_hours = 36;
    let mut cache = LiveCache::default();
    refresh(&c, &mut cache, t0(), &net);
    let g = &cache.feeds[&live_urls(&c)[0]];
    let titles: Vec<(&str, Option<&str>)> = g
        .items
        .iter()
        .map(|i| (i.title.as_str(), i.source.as_deref()))
        .collect();
    // split into headline and source; the 10-day-old one is past Local's 48 hours
    assert_eq!(
        titles,
        [
            (
                "Linden Falls council votes on the bridge",
                Some("The Linden Ledger")
            ),
            ("Leaf season peaks on the ridge", Some("WXII")),
        ]
    );
    let s = &cache.feeds[SCIENCE];
    assert_eq!(s.title.as_deref(), Some("Science Daily Brief"));
    assert_eq!(s.items.len(), 3, "last month's item is past 36 hours");
}

#[test]
fn renamed_feed_keeps_headlines_removed_is_pruned() {
    let net = Counting::new();
    let mut c = cfg();
    let mut cache = LiveCache::default();
    refresh(&c, &mut cache, t0(), &net);
    c.feeds[0].name = "Discoveries".into();
    assert_eq!(
        refresh(&c, &mut cache, t0() + Duration::minutes(5), &net),
        0
    );
    assert_eq!(cache.feeds[SCIENCE].items.len(), 3);
    c.feeds.remove(0);
    refresh(&c, &mut cache, t0() + Duration::minutes(6), &net);
    assert!(!cache.feeds.contains_key(SCIENCE));
}

#[test]
fn live_cache_round_trip() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("digest/next.json");
    assert_eq!(LiveCache::load(&path), LiveCache::default());
    let mut cache = LiveCache::default();
    refresh(&cfg(), &mut cache, t0(), &Counting::new());
    cache.save(&path).unwrap();
    assert_eq!(LiveCache::load(&path), cache);
    std::fs::write(&path, "garbage").unwrap();
    assert_eq!(LiveCache::load(&path), LiveCache::default());
}

// --- composing the next paper (Task 6) ---

use std::collections::BTreeMap;

use chrono::{NaiveDate, TimeZone};
use chrono_tz::Tz;
use deckle_digest::config::{SectionKind, Units};
use deckle_digest::next::History;
use deckle_digest::next::live::{Fetched, LiveItem};
use deckle_digest::next::plan::{Inputs, NextPaper, POSITIONS_PER_PAGE, plan, saved_line};
use deckle_digest::reading::{Entry, State};

const LOCAL: &str = "https://ledger.example.com/rss";

fn ny() -> Tz {
    "America/New_York".parse().unwrap()
}

fn local(y: i32, mo: u32, d: u32, h: u32, mi: u32) -> DateTime<Tz> {
    ny().with_ymd_and_hms(y, mo, d, h, mi, 0).single().unwrap()
}

/// 2026-10-22 15:08 in New York.
fn now() -> DateTime<Tz> {
    local(2026, 10, 22, 15, 8)
}

fn paper_cfg() -> Config {
    Config {
        title: "The Morning Deckle".into(),
        place: "Linden Falls, NC".into(),
        sections: vec![
            SectionKind::Reading,
            SectionKind::Weather,
            SectionKind::Local,
            SectionKind::Feeds,
            SectionKind::Horoscope,
            SectionKind::Onthisday,
        ],
        feeds: vec![FeedConfig {
            name: "Science".into(),
            url: SCIENCE.into(),
            max_items: 10,
            ..Default::default()
        }],
        ..Config::default()
    }
}

fn saved(id: &str, url: Option<&str>, title: Option<&str>, at: DateTime<Tz>) -> Entry {
    Entry {
        id: id.into(),
        url: url.map(String::from),
        file: None,
        title: title.map(String::from),
        saved_at: at.with_timezone(&Utc),
        source: "cli".into(),
        state: State::Pending,
        tries: 0,
        printed_in: None,
        note: None,
        tried_on: None,
        waits: 0,
    }
}

fn two_saved() -> Vec<Entry> {
    vec![
        saved(
            "a",
            Some("https://www.theatlantic.com/a"),
            Some("The long way home"),
            local(2026, 10, 22, 9, 14),
        ),
        saved(
            "b",
            Some("https://example.org/b"),
            Some("Second thoughts"),
            local(2026, 10, 22, 11, 2),
        ),
    ]
}

fn fetched(items: &[(&str, DateTime<Tz>)]) -> Fetched {
    Fetched {
        at: now().with_timezone(&Utc),
        title: Some("Science Daily Brief".into()),
        items: items
            .iter()
            .map(|(t, at)| LiveItem {
                title: (*t).into(),
                source: None,
                published: Some(at.with_timezone(&Utc)),
            })
            .collect(),
        error: None,
    }
}

struct Setup {
    cfg: Config,
    queue: Vec<Entry>,
    live: LiveCache,
    history: History,
    shares: BTreeMap<String, u8>,
    stopped: Option<NaiveDate>,
}

impl Setup {
    fn new() -> Self {
        Setup {
            cfg: paper_cfg(),
            queue: Vec::new(),
            live: LiveCache::default(),
            history: History::default(),
            shares: BTreeMap::new(),
            stopped: None,
        }
    }
    fn plan(&self) -> NextPaper {
        plan(&Inputs {
            cfg: &self.cfg,
            now: now(),
            at: Some((5, 30)),
            stopped: self.stopped,
            queue: &self.queue,
            live: &self.live,
            history: &self.history,
            shares: &self.shares,
            week: Vec::new(),
        })
    }
}

fn labels(p: &NextPaper) -> Vec<String> {
    p.tiles
        .iter()
        .map(|t| match &t.title {
            Some(title) => format!("{} | {title}", t.kicker),
            None => t.kicker.clone(),
        })
        .collect()
}

#[test]
fn masthead_is_the_next_edition() {
    let p = Setup::new().plan();
    assert_eq!(p.date, NaiveDate::from_ymd_opt(2026, 10, 23).unwrap());
    assert_eq!(p.number, 296);
    assert_eq!(p.paper, "The Morning Deckle");
    assert_eq!(p.place, "Linden Falls, NC");
    assert!(p.weather);
}

#[test]
fn saved_article_leads_without_local() {
    let mut s = Setup::new();
    s.queue = two_saved();
    let p = s.plan();
    let lead = p.lead.as_ref().unwrap();
    assert_eq!(lead.kicker, "YOUR READING · THEATLANTIC.COM");
    assert_eq!(lead.title.as_deref(), Some("The long way home"));
    assert_eq!(
        lead.reason.as_deref(),
        Some("Saved at 9:14 this morning. Leads the paper because nothing local outranks it yet.")
    );
    assert_eq!(
        labels(&p),
        [
            "YOUR READING · EXAMPLE.ORG | Second thoughts",
            "Local · 12 slots",
            "Science · 10 slots",
            "Horoscope · fills at press time",
            "On This Day · fills at press time",
        ]
    );
    assert_eq!(
        p.tiles[0].reason.as_deref(),
        Some("Saved at 11:02 this morning.")
    );
    assert!(p.tiles[1].dashed && p.tiles[3].dashed && !p.tiles[0].dashed);
}

#[test]
fn local_leads_when_fetched() {
    let mut s = Setup::new();
    s.queue = two_saved();
    s.cfg.local.feeds = vec![LOCAL.into()];
    s.live.feeds.insert(
        LOCAL.into(),
        fetched(&[("Bridge vote tonight", local(2026, 10, 22, 12, 0))]),
    );
    let p = s.plan();
    let lead = p.lead.as_ref().unwrap();
    assert_eq!(lead.title.as_deref(), Some("Bridge vote tonight"));
    assert_eq!(
        lead.reason.as_deref(),
        Some("Leads the paper: local news always leads.")
    );
    let l = labels(&p);
    assert_eq!(
        &l[..2],
        [
            "YOUR READING · THEATLANTIC.COM | The long way home",
            "YOUR READING · EXAMPLE.ORG | Second thoughts",
        ]
    );
}

#[test]
fn feed_leads_line() {
    let mut s = Setup::new();
    s.live.feeds.insert(
        SCIENCE.into(),
        fetched(&[("Comet brightens", local(2026, 10, 22, 14, 0))]),
    );
    let p = s.plan();
    assert_eq!(
        p.lead.unwrap().reason.as_deref(),
        Some("Leads the paper: nothing local yet, and Science comes first in your page plan.")
    );
}

#[test]
fn a_feed_with_no_source_is_a_row_not_a_crash() {
    let mut s = Setup::new();
    s.cfg.feeds.push(FeedConfig {
        name: "Blank".into(),
        max_items: 10,
        ..Default::default()
    });
    let p = s.plan();
    let t = p.tiles.iter().find(|t| t.key == "feeds:Blank").unwrap();
    assert!(t.dashed);
}

#[test]
fn new_since_last_press() {
    let mut s = Setup::new();
    s.queue = two_saved(); // so Science is a tile, not the lead
    s.live.feeds.insert(
        SCIENCE.into(),
        fetched(&[
            ("Comet brightens", local(2026, 10, 22, 14, 0)),
            ("Ocean floor mapped", local(2026, 10, 22, 6, 10)),
            ("Bees count", local(2026, 10, 22, 5, 0)),
        ]),
    );
    let p = s.plan();
    let t = p.tiles.iter().find(|t| t.key == "feeds:Science").unwrap();
    assert_eq!(t.kicker, "SCIENCE · 2 NEW SINCE 05:30");
    assert_eq!(t.title.as_deref(), Some("Comet brightens"));
    assert!(!t.dashed);
}

#[test]
fn saved_line_table() {
    let at = |d: u32, h: u32, m: u32| local(2026, 10, d, h, m);
    let now = at(22, 18, 0);
    let line = |s| saved_line(s, now, Units::Imperial);
    assert_eq!(line(at(22, 11, 59)), "Saved at 11:59 this morning.");
    assert_eq!(line(at(22, 12, 0)), "Saved at 12:00 this afternoon.");
    assert_eq!(line(at(22, 17, 0)), "Saved at 5:00 this evening.");
    assert_eq!(line(at(21, 9, 14)), "Saved yesterday at 9:14.");
    assert_eq!(line(at(16, 9, 14)), "Saved Friday.");
    assert_eq!(line(at(15, 9, 14)), "Saved Oct 15.");
    assert_eq!(
        saved_line(at(22, 9, 14), now, Units::Metric),
        "Saved at 09:14 this morning."
    );
}

#[test]
fn untitled_saved_page() {
    let mut s = Setup::new();
    let mut e = saved("p", None, None, local(2026, 10, 22, 9, 0));
    e.file = Some("reading/x/page.html".into());
    s.queue = vec![e];
    let lead = s.plan().lead.unwrap();
    assert_eq!(lead.kicker, "YOUR READING");
    assert_eq!(lead.title.as_deref(), Some("page.html"));
}

#[test]
fn plan_rows_and_budgets() {
    let mut s = Setup::new();
    s.queue = (0..3)
        .map(|i| {
            saved(
                &format!("{i}"),
                Some("https://e.com/x"),
                Some("X"),
                local(2026, 10, 22, 9, i),
            )
        })
        .collect();
    s.cfg.local.max_items = 3;
    let p = s.plan();
    let row = |key: &str| p.plan.iter().find(|r| r.key == key).unwrap().clone();
    let r = row("reading");
    assert_eq!(
        (r.detail.as_str(), r.budget.as_str(), r.items),
        ("3 saved today", "all", Some(10))
    );
    let l = row("local");
    assert_eq!((l.budget.as_str(), l.items), ("3 stories", Some(3)));
    let sci = row("feeds:Science");
    assert_eq!(
        (sci.feed, sci.detail.as_str()),
        (Some(0), "science.example.org")
    );
    assert_eq!(
        row("weather").detail,
        "Linden Falls, NC · 5-day box and almanac"
    );
    assert_eq!(row("horoscope").budget, "12 signs");
    assert_eq!(row("horoscope").detail, "all 12 signs");
    assert_eq!(p.plan.last().unwrap().budget, "back page");
    assert_eq!(p.plan.last().unwrap().key, "onthisday");
    let keys: Vec<&str> = p.plan.iter().map(|r| r.key.as_str()).collect();
    assert_eq!(
        keys,
        [
            "reading",
            "weather",
            "local",
            "feeds:Science",
            "horoscope",
            "onthisday"
        ]
    );
    let off: Vec<&str> = p.off.iter().map(|r| r.key.as_str()).collect();
    assert_eq!(off, ["shelf", "lottery", "puzzle"]);

    // twelve waiting, ten fit; a cached channel title names the feed; Your Day on the weather
    s.queue = (0..12)
        .map(|i| {
            saved(
                &format!("{i}"),
                Some("https://e.com/x"),
                Some("X"),
                local(2026, 10, 21, 9, i),
            )
        })
        .collect();
    s.live.feeds.insert(SCIENCE.into(), fetched(&[]));
    s.cfg.day.calendars = vec!["~/home.ics".into()];
    let p = s.plan();
    let row = |key: &str| p.plan.iter().find(|r| r.key == key).unwrap().clone();
    let r = row("reading");
    assert_eq!(
        (r.detail.as_str(), r.budget.as_str()),
        ("10 of 12 fit; the rest wait", "10")
    );
    assert_eq!(row("feeds:Science").detail, "Science Daily Brief");
    assert!(row("weather").detail.ends_with(" · Your Day"));
}

#[test]
fn readership_note_at_threshold() {
    let mut s = Setup::new();
    s.shares.insert("horoscope".into(), 20);
    s.shares.insert("local".into(), 21);
    let p = s.plan();
    let row = |key: &str| p.plan.iter().find(|r| r.key == key).unwrap().clone();
    assert!(
        row("horoscope")
            .detail
            .ends_with(" · you get to it about 20% of days")
    );
    assert!(!row("local").detail.contains("you get to it"));
}

#[test]
fn estimate_defaults_and_history() {
    let mut s = Setup::new();
    s.queue = two_saved();
    s.queue
        .push(saved("c", None, Some("C"), local(2026, 10, 22, 12, 0)));
    // front 2, reading 3×3, weather 2, local 1.5×12, Science 1.5×10, horoscope 0.25×12, OTD 2
    let want: f64 = 2.0 + 3.0 * 3.0 + 2.0 + 1.5 * 12.0 + 1.5 * 10.0 + 0.25 * 12.0 + 2.0;
    assert_eq!(s.plan().pages, want.round() as u32);

    s.history = History {
        per_item: [("local".to_owned(), 900.0)].into(),
        whole: [("front".to_owned(), 3600.0)].into(),
    };
    let local_pages = 900.0 * 12.0 / POSITIONS_PER_PAGE;
    let want: f64 = 2.0 + 3.0 * 3.0 + 2.0 + local_pages + 1.5 * 10.0 + 0.25 * 12.0 + 2.0;
    assert_eq!(s.plan().pages, want.round() as u32);

    // fetched Science with two fresh items plans two, not its budget of ten
    s.live.feeds.insert(
        SCIENCE.into(),
        fetched(&[
            ("A", local(2026, 10, 22, 14, 0)),
            ("B", local(2026, 10, 22, 13, 0)),
        ]),
    );
    let want: f64 = 2.0 + 3.0 * 3.0 + 2.0 + local_pages + 1.5 * 2.0 + 0.25 * 12.0 + 2.0;
    assert_eq!(s.plan().pages, want.round() as u32);
}

#[test]
fn stopped_shows_the_paper_after() {
    let mut s = Setup::new();
    s.stopped = NaiveDate::from_ymd_opt(2026, 10, 23);
    let p = s.plan();
    assert_eq!(p.date, NaiveDate::from_ymd_opt(2026, 10, 24).unwrap());
    assert_eq!(p.number, 297);
}

#[test]
fn next_paper_serializes_camel_case() {
    let mut s = Setup::new();
    s.queue = two_saved();
    let v = serde_json::to_value(s.plan()).unwrap();
    assert!(
        v["lead"]["kicker"]
            .as_str()
            .unwrap()
            .starts_with("YOUR READING")
    );
    assert_eq!(v["plan"][0]["key"], "reading");
    assert!(v["pages"].as_u64().unwrap() >= 1);
    assert!(v["press"]["next"].is_string());
}

#[test]
fn saved_articles_print_even_with_reading_off() {
    // the press puts Your Reading first when something is saved and it's off (gather): the
    // preview must lead with it too
    let mut s = Setup::new();
    s.cfg.sections.retain(|k| *k != SectionKind::Reading);
    s.live.feeds.insert(
        SCIENCE.into(),
        fetched(&[("Comet brightens", local(2026, 10, 22, 14, 0))]),
    );
    assert_eq!(s.plan().lead.unwrap().kicker, "SCIENCE · 1 NEW SINCE 05:30");
    s.queue = two_saved();
    let p = s.plan();
    assert_eq!(p.plan[0].key, "reading");
    assert!(p.off.iter().all(|r| r.key != "reading"));
    assert_eq!(p.lead.unwrap().kicker, "YOUR READING · THEATLANTIC.COM");
}

#[test]
fn the_puzzle_has_a_row_and_a_tile() {
    let mut s = Setup::new();
    let before = s.plan().pages;
    // between the horoscope and On This Day, as the starter has it
    s.cfg.sections.insert(5, SectionKind::Puzzle);
    let p = s.plan();
    let row = p.plan.iter().find(|r| r.key == "puzzle").unwrap();
    assert_eq!(row.name, "Puzzle");
    assert_eq!(row.detail, "Mini crossword from yesterday's headlines");
    assert_eq!((row.budget.as_str(), row.items), ("1 page", None));
    let t = p.tiles.iter().find(|t| t.key == "puzzle").unwrap();
    assert_eq!(t.kicker, "Puzzle · yesterday's answers");
    assert!(t.dashed);
    assert_eq!(p.pages, before + 1);
    // off, it's listed to switch on
    s.cfg.sections.retain(|k| *k != SectionKind::Puzzle);
    assert!(s.plan().off.iter().any(|r| r.key == "puzzle"));
}

#[test]
fn other_places_have_rows_tiles_and_headlines() {
    use deckle_digest::config::PlaceNews;
    let mut s = Setup::new();
    s.cfg.local.places = vec![
        PlaceNews {
            name: "Asheville, NC".into(),
            google_news: "Asheville NC".into(),
            feeds: Vec::new(),
            max_items: Some(4),
        },
        PlaceNews {
            name: "Boone".into(),
            google_news: String::new(),
            feeds: vec!["https://boone.example/rss".into()],
            max_items: None,
        },
    ];
    // each place's sources are fetched, under Local's limits
    let urls = live_urls(&s.cfg);
    assert!(
        urls.iter().any(|u| u.contains("q=Asheville+NC")),
        "{urls:?}"
    );
    assert!(urls.contains(&"https://boone.example/rss".to_owned()));
    s.live.feeds.insert(
        "https://boone.example/rss".into(),
        fetched(&[("Boone council meets", local(2026, 10, 22, 13, 0))]),
    );
    let p = s.plan();
    let keys: Vec<&str> = p.plan.iter().map(|r| r.key.as_str()).collect();
    assert_eq!(
        keys,
        [
            "reading",
            "weather",
            "local",
            "local:Asheville, NC",
            "local:Boone",
            "feeds:Science",
            "horoscope",
            "onthisday"
        ]
    );
    let row = |k: &str| p.plan.iter().find(|r| r.key == k).unwrap().clone();
    let a = row("local:Asheville, NC");
    assert_eq!(
        (a.name.as_str(), a.kind, a.place, a.items),
        ("Asheville, NC", SectionKind::Local, Some(0), Some(4))
    );
    assert_eq!(a.detail, "Google News");
    assert_eq!(
        (row("local:Boone").place, row("local:Boone").items),
        (Some(1), Some(12))
    );
    assert_eq!(row("local").place, None);
    // the main Local is still the only local tier: Boone's headline leads like a feed's
    let lead = p.lead.clone().unwrap();
    assert_eq!(lead.title.as_deref(), Some("Boone council meets"));
    assert_eq!(
        lead.reason.as_deref(),
        Some("Leads the paper: nothing local yet, and Boone comes first in your page plan.")
    );
    let t = labels(&p);
    assert!(t.contains(&"Asheville, NC · 4 slots".to_owned()), "{t:?}");
}

// --- topic sections (Task 8) ---

use deckle_digest::config::Topic;

#[test]
fn topic_feed_is_fetched_by_its_topic_url() {
    let net = Counting::new();
    let mut c = cfg();
    c.feeds = vec![FeedConfig {
        name: "Science".into(),
        topic: Some(Topic::Science),
        max_items: 5,
        ..Default::default()
    }];
    let topic_url = Topic::Science.url();
    assert_eq!(live_urls(&c), std::slice::from_ref(&topic_url));
    let mut cache = LiveCache::default();
    assert_eq!(refresh(&c, &mut cache, t0(), &net), 1);
    // "Headline - Source" is split, as for any Google News item
    let item = &cache.feeds[&topic_url].items[0];
    assert_eq!(item.title, "Linden Falls council votes on the bridge");
    assert_eq!(item.source.as_deref(), Some("The Linden Ledger"));
    // and the plan row reads it, titled by the feed's own title
    let mut s = Setup::new();
    s.cfg.feeds = c.feeds.clone();
    s.live = cache;
    let p = s.plan();
    let row = p.plan.iter().find(|r| r.name == "Science").unwrap();
    assert_eq!(row.detail, "\"Linden Falls\" - Google News");
}

#[test]
fn topic_detail_falls_back_to_its_title() {
    let mut s = Setup::new();
    s.cfg.feeds = vec![FeedConfig {
        name: "Science".into(),
        topic: Some(Topic::Science),
        max_items: 5,
        ..Default::default()
    }];
    let p = s.plan();
    assert_eq!(
        p.plan.iter().find(|r| r.name == "Science").unwrap().detail,
        "Science"
    );
}

#[test]
fn multi_source_row_merges_and_dedupes() {
    let mut s = Setup::new();
    s.cfg.feeds = vec![FeedConfig {
        name: "Science".into(),
        url: SCIENCE.into(),
        urls: vec![OTHER.into()],
        max_items: 3,
        ..Default::default()
    }];
    s.queue = two_saved(); // so the feed is a tile list, not the lead
    s.live.feeds.insert(
        SCIENCE.into(),
        fetched(&[
            ("Comet brightens", local(2026, 10, 22, 14, 0)),
            ("Bees count", local(2026, 10, 22, 9, 0)),
        ]),
    );
    s.live.feeds.insert(
        OTHER.into(),
        fetched(&[
            ("COMET  brightens!", local(2026, 10, 22, 13, 0)),
            ("Ocean floor mapped", local(2026, 10, 22, 11, 0)),
            ("Old news", local(2026, 10, 20, 11, 0)),
        ]),
    );
    let p = s.plan();
    let row = p.plan.iter().find(|r| r.name == "Science").unwrap();
    assert_eq!(row.detail, "Science Daily Brief");
    assert_eq!(row.budget, "3 stories");
    // newest first, the repeated story once, cut to 3: the tile shows the newest
    assert!(
        labels(&p)
            .iter()
            .any(|l| l == "SCIENCE · 3 NEW SINCE 05:30 | Comet brightens"),
        "{:?}",
        labels(&p)
    );
}

#[test]
fn row_drops_a_title_an_earlier_row_has() {
    let mut s = Setup::new();
    s.cfg.local.feeds = vec![LOCAL.into()];
    s.cfg.feeds = vec![
        FeedConfig {
            name: "Science".into(),
            url: SCIENCE.into(),
            max_items: 1,
            ..Default::default()
        },
        FeedConfig {
            name: "Other".into(),
            url: OTHER.into(),
            max_items: 1,
            ..Default::default()
        },
    ];
    s.queue = two_saved();
    s.live.feeds.insert(
        LOCAL.into(),
        fetched(&[("Comet brightens", local(2026, 10, 22, 12, 0))]),
    );
    s.live.feeds.insert(
        SCIENCE.into(),
        fetched(&[
            ("Comet brightens", local(2026, 10, 22, 14, 0)),
            ("Bees count", local(2026, 10, 22, 9, 0)),
        ]),
    );
    s.live.feeds.insert(
        OTHER.into(),
        fetched(&[
            ("Bees count", local(2026, 10, 22, 13, 0)),
            ("Ocean floor mapped", local(2026, 10, 22, 11, 0)),
        ]),
    );
    let p = s.plan();
    // Local keeps the comet; Science drops it and refills with the bees; Other drops the bees
    let l = labels(&p);
    assert!(
        l.iter()
            .any(|x| x == "SCIENCE · 1 NEW SINCE 05:30 | Bees count"),
        "{l:?}"
    );
    assert!(
        l.iter()
            .any(|x| x == "OTHER · 1 NEW SINCE 05:30 | Ocean floor mapped"),
        "{l:?}"
    );
    // Local still leads, by the press's rule
    assert_eq!(p.lead.unwrap().title.as_deref(), Some("Comet brightens"));
}

#[test]
fn url_only_feed_row_unchanged() {
    let net = Counting::new();
    let c = cfg();
    let mut cache = LiveCache::default();
    assert_eq!(refresh(&c, &mut cache, t0(), &net), 2);
    assert_eq!(live_urls(&c), [SCIENCE, OTHER]);
    let mut s = Setup::new();
    s.live = cache;
    let p = s.plan();
    let row = p.plan.iter().find(|r| r.name == "Science").unwrap();
    assert_eq!(row.detail, "Science Daily Brief");
    assert_eq!(row.budget, "10 stories");
}

// --- subscribed papers ---

#[test]
fn a_subscribed_paper_has_a_row_per_slug_fetched_like_a_feed() {
    let mut s = Setup::new();
    s.cfg = Config::parse(
        "sections = [\"weather\", \"subscribed\"]\n\n[[subscribed]]\nsite = \"thepilot.com\"\nname = \"The Pilot\"\nsections = [\"news\", \"sports\"]\nmax_items = 5\n",
    )
    .unwrap();
    // max_items plus five spare, so a group after Local can refill
    let news = "https://www.thepilot.com/search/?f=rss&t=article&c=news&l=10&s=start_time&sd=desc";
    assert!(live_urls(&s.cfg).contains(&news.to_owned()));
    // the same twice-an-hour limit as a feed's
    let net = Counting::new();
    let mut cache = LiveCache::default();
    assert_eq!(refresh(&s.cfg, &mut cache, t0(), &net), 2);
    assert_eq!(
        refresh(&s.cfg, &mut cache, t0() + Duration::minutes(10), &net),
        0
    );
    s.live.feeds.insert(
        news.into(),
        fetched(&[("Council votes", local(2026, 10, 22, 13, 0))]),
    );
    let p = s.plan();
    let rows: Vec<_> = p
        .plan
        .iter()
        .filter(|r| r.kind == SectionKind::Subscribed)
        .collect();
    assert_eq!(rows.len(), 2);
    assert_eq!(rows[0].key, "subscribed:thepilot.com:news");
    assert_eq!(rows[0].name, "The Pilot · News");
    assert_eq!(rows[0].detail, "thepilot.com · news");
    assert_eq!(rows[0].items, Some(5));
    let r = rows[0].subscribed.as_ref().unwrap();
    assert_eq!((r.site.as_str(), r.slug.as_str()), ("thepilot.com", "news"));
    // the fetched headline leads (nothing local), and the other slug is a dashed slot
    assert_eq!(
        p.lead.clone().unwrap().title.as_deref(),
        Some("Council votes")
    );
    assert!(labels(&p).contains(&"The Pilot · Sports · 5 slots".to_owned()));
    let json = serde_json::to_string(&p.plan[0]).unwrap();
    assert!(!json.contains("subscribed"), "{json}");
}

#[test]
fn local_after_a_group_drops_the_groups_story_and_refills_as_the_press_does() {
    let mut s = Setup::new();
    s.cfg = Config::parse(
        "sections = [\"subscribed\", \"local\"]\n\n[local]\nmax_items = 2\n\n[[subscribed]]\nsite = \"thepilot.com\"\nsections = [\"news\"]\nmax_items = 1\n",
    )
    .unwrap();
    s.cfg.local.google_news = String::new();
    s.cfg.local.feeds = vec![LOCAL.into()];
    let news = "https://www.thepilot.com/search/?f=rss&t=article&c=news&l=6&s=start_time&sd=desc";
    s.live.feeds.insert(
        news.into(),
        fetched(&[("Water tower vote", local(2026, 10, 22, 14, 0))]),
    );
    s.live.feeds.insert(
        LOCAL.into(),
        fetched(&[
            ("Water tower vote", local(2026, 10, 22, 13, 0)),
            ("Library reopens", local(2026, 10, 22, 12, 0)),
            ("Fair opens", local(2026, 10, 22, 11, 0)),
        ]),
    );
    let p = s.plan();
    // the group prints the shared story first; Local drops it and refills to its two stories,
    // the count the press prints
    let lead = p.lead.clone().unwrap();
    assert_eq!(lead.key, "local");
    assert_eq!(lead.kicker, "LOCAL · 2 NEW SINCE 05:30");
    assert_eq!(lead.title.as_deref(), Some("Library reopens"));
    assert_eq!(
        labels(&p),
        [
            "THEPILOT.COM · NEWS · 1 NEW SINCE 05:30 | Water tower vote",
            "LOCAL · 2 NEW SINCE 05:30 | Fair opens"
        ]
    );
}

#[test]
fn subscribed_is_off_when_no_group_exists() {
    let p = Setup::new().plan();
    assert!(p.plan.iter().all(|r| r.kind != SectionKind::Subscribed));
    assert!(p.off.iter().all(|r| r.kind != SectionKind::Subscribed));
}
