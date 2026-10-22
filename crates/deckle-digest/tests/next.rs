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
            },
            FeedConfig {
                name: "Other".into(),
                url: OTHER.into(),
                max_items: 10,
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
