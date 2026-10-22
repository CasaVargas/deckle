// SPDX-License-Identifier: GPL-3.0-or-later
//! Topic sections: many sources merged into one, the top story printed whole.

use std::collections::HashSet;
use std::path::Path;
use std::sync::Mutex;

use chrono::{DateTime, Utc};
use deckle_digest::Net;
use deckle_digest::config::{Config, FeedConfig, FullText, Topic};
use deckle_digest::sources::feeds::feed_section;

fn fixture(name: &str) -> Vec<u8> {
    std::fs::read(
        Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("tests/fixtures")
            .join(name),
    )
    .unwrap()
}

fn now() -> DateTime<Utc> {
    "2026-10-20T22:00:00Z".parse().unwrap()
}

fn config() -> Config {
    let mut c = Config::parse(deckle_digest::config::STARTER).unwrap();
    c.max_age_hours = 24 * 7;
    c.photos = false;
    c
}

/// `(title, link, hour on 2026-10-20)` as an RSS feed.
fn rss(items: &[(&str, &str, u32)]) -> Vec<u8> {
    let body: String = items
        .iter()
        .map(|(t, l, h)| {
            format!(
                "<item><title>{t}</title><link>{l}</link>\
                 <pubDate>Tue, 20 Oct 2026 {h:02}:00:00 +0000</pubDate>\
                 <description>&lt;p&gt;Feed text that stays out of briefs.&lt;/p&gt;</description></item>"
            )
        })
        .collect();
    format!("<rss version=\"2.0\"><channel><title>Feed</title>{body}</channel></rss>").into_bytes()
}

/// Serves `feeds` by URL and the WXII article for `ok.example` pages; records every request.
struct Fake {
    feeds: Vec<(String, Vec<u8>)>,
    seen: Mutex<Vec<String>>,
}

impl Fake {
    fn new(feeds: Vec<(String, Vec<u8>)>) -> Fake {
        Fake {
            feeds,
            seen: Mutex::new(Vec::new()),
        }
    }
    fn seen(&self) -> Vec<String> {
        self.seen.lock().unwrap().clone()
    }
    fn pages(&self) -> Vec<String> {
        self.seen()
            .into_iter()
            .filter(|u| u.contains("ok.example/") || u.contains("bad.example/"))
            .collect()
    }
}

impl Net for Fake {
    fn get(&self, url: &str) -> Result<Vec<u8>, String> {
        self.seen.lock().unwrap().push(url.to_owned());
        if let Some((_, b)) = self.feeds.iter().find(|(u, _)| u == url) {
            Ok(b.clone())
        } else if url.starts_with("https://ok.example/") {
            Ok(fixture("article-wxii.html"))
        } else {
            Err(format!("offline: {url}"))
        }
    }
}

fn run(feed: &FeedConfig, net: &Fake) -> deckle_digest::Section {
    feed_section(feed, &config(), now(), net, &mut HashSet::new())
}

fn url(path: &str) -> String {
    format!("https://feeds.example/{path}")
}

fn is_brief(a: &deckle_digest::Article) -> bool {
    a.teaser.is_none() && !a.body.contains("<p>Feed text") && !a.body.contains("<p>The ")
}

fn topic_feed(max: usize) -> FeedConfig {
    FeedConfig {
        name: "Sports".into(),
        topic: Some(Topic::Sports),
        max_items: max,
        ..Default::default()
    }
}

#[test]
fn topic_feed_requests_the_topic_url() {
    let net = Fake::new(vec![(
        Topic::Sports.url(),
        rss(&[("Big game - Outlet", "https://x.example/1", 10)]),
    )]);
    let feed = FeedConfig {
        full_text: Some(FullText::No),
        ..topic_feed(5)
    };
    let s = run(&feed, &net);
    assert_eq!(net.seen(), [Topic::Sports.url()]);
    assert_eq!(s.articles.len(), 1);
    assert_eq!(s.articles[0].title, "Big game");
    assert!(s.articles[0].meta.as_deref().unwrap().contains("Outlet"));
}

#[test]
fn sources_are_merged_and_deduped_by_title() {
    let net = Fake::new(vec![
        (
            Topic::Sports.url(),
            rss(&[
                ("Alpha - Outlet", "https://x.example/a", 10),
                ("Bravo - Outlet", "https://x.example/b", 8),
            ]),
        ),
        (
            url("more"),
            rss(&[
                ("Bravo!", "https://x.example/b2", 9),
                ("Charlie", "https://x.example/c", 11),
                ("Delta", "https://x.example/d", 1),
            ]),
        ),
    ]);
    let feed = FeedConfig {
        urls: vec![url("more")],
        full_text: Some(FullText::No),
        ..topic_feed(3)
    };
    let s = run(&feed, &net);
    let titles: Vec<_> = s.articles.iter().map(|a| a.title.as_str()).collect();
    // Newest first, one Bravo (the newer copy), cut to three.
    assert_eq!(titles, ["Charlie", "Alpha", "Bravo!"]);
}

#[test]
fn one_source_failing_keeps_the_others() {
    let net = Fake::new(vec![(
        url("good"),
        rss(&[("Alpha", "https://x.example/a", 10)]),
    )]);
    let feed = FeedConfig {
        name: "Mixed".into(),
        urls: vec![url("dead"), url("good")],
        full_text: Some(FullText::No),
        max_items: 5,
        ..Default::default()
    };
    let s = run(&feed, &net);
    assert!(s.unavailable.is_none());
    assert_eq!(s.articles.len(), 1);
}

#[test]
fn all_failing_is_unavailable() {
    let net = Fake::new(vec![]);
    let feed = FeedConfig {
        name: "Dead".into(),
        urls: vec![url("a"), url("b")],
        max_items: 5,
        ..Default::default()
    };
    let s = run(&feed, &net);
    assert!(s.articles.is_empty());
    assert!(s.unavailable.as_deref().unwrap().contains("Unavailable"));
}

#[test]
fn lead_mode_fetches_only_the_top_story_whole() {
    let net = Fake::new(vec![(
        Topic::Sports.url(),
        rss(&[
            ("One - Outlet", "https://ok.example/1", 12),
            ("Two - Outlet", "https://ok.example/2", 11),
            ("Three - Outlet", "https://ok.example/3", 10),
        ]),
    )]);
    let feed = FeedConfig {
        full_text: Some(FullText::Lead),
        ..topic_feed(5)
    };
    let s = run(&feed, &net);
    assert_eq!(net.pages(), ["https://ok.example/1"]);
    assert_eq!(s.articles.len(), 3);
    assert!(s.articles[0].teaser.is_some());
    assert!(s.articles[0].body.contains("<p>"));
    for a in &s.articles[1..] {
        assert!(is_brief(a), "{a:?}");
        assert!(a.body.contains("Source: ok.example"));
    }
}

#[test]
fn lead_mode_tries_the_next_story_when_the_first_cannot_be_read() {
    let net = Fake::new(vec![(
        Topic::Sports.url(),
        rss(&[
            ("One - Outlet", "https://bad.example/1", 12),
            ("Two - Outlet", "https://ok.example/2", 11),
            ("Three - Outlet", "https://ok.example/3", 10),
        ]),
    )]);
    let feed = FeedConfig {
        full_text: Some(FullText::Lead),
        ..topic_feed(5)
    };
    let s = run(&feed, &net);
    assert_eq!(
        net.pages(),
        ["https://bad.example/1", "https://ok.example/2"]
    );
    assert_eq!(s.articles[0].title, "Two");
    assert!(s.articles[0].teaser.is_some());
    let rest: Vec<_> = s.articles[1..].iter().map(|a| a.title.as_str()).collect();
    assert!(rest.contains(&"One") && rest.contains(&"Three"));
}

#[test]
fn lead_mode_gives_up_after_three_attempts() {
    let items: Vec<(String, String, u32)> = (1..=5)
        .map(|n| {
            (
                format!("Story {n} - Outlet"),
                format!("https://bad.example/{n}"),
                20 - n,
            )
        })
        .collect();
    let refs: Vec<(&str, &str, u32)> = items
        .iter()
        .map(|(t, l, h)| (t.as_str(), l.as_str(), *h))
        .collect();
    let net = Fake::new(vec![(Topic::Sports.url(), rss(&refs))]);
    let feed = FeedConfig {
        full_text: Some(FullText::Lead),
        ..topic_feed(5)
    };
    let s = run(&feed, &net);
    assert_eq!(net.pages().len(), 3);
    assert_eq!(s.articles.len(), 5);
    assert!(s.articles.iter().all(is_brief));
}

#[test]
fn old_feed_behaviour_unchanged() {
    let net = Fake::new(vec![(
        url("plain"),
        rss(&[
            ("Alpha", "https://ok.example/a", 10),
            ("Beta", "https://ok.example/b", 9),
        ]),
    )]);
    let feed = FeedConfig {
        name: "Plain".into(),
        url: url("plain"),
        max_items: 10,
        ..Default::default()
    };
    let s = run(&feed, &net);
    // The global full_text (true) fetches every story.
    assert_eq!(net.pages().len(), 2);
    assert_eq!(s.title, "Plain");
    assert_eq!(s.articles.len(), 2);
    assert!(s.articles.iter().all(|a| a.teaser.is_some()));
}
