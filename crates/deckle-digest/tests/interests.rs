// SPDX-License-Identifier: GPL-3.0-or-later
//! Your interests (interests design §4): one section per `[[interests]]` entry, hunted across
//! searches and feeds, kept only when a story names the interest, left out on a quiet day.

use std::collections::HashSet;
use std::path::Path;
use std::sync::Mutex;

use chrono::{DateTime, Utc};
use deckle_digest::Net;
use deckle_digest::config::{Config, FullText, InterestConfig, SectionKind};
use deckle_digest::sources::feeds::{google_news_url, title_key};
use deckle_digest::sources::interests::interest_section;

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
                 <pubDate>Tue, 20 Oct 2026 {h:02}:00:00 +0000</pubDate></item>"
            )
        })
        .collect();
    format!("<rss version=\"2.0\"><channel><title>Feed</title>{body}</channel></rss>").into_bytes()
}

/// The URL the gatherer requests for a search under the default 72-hour window.
fn search(q: &str) -> String {
    google_news_url(q, 72)
}

/// `[[interests]] name = "Rust", not = ["Rust Belt"]`, as `Config::parse` normalises it.
fn rust() -> InterestConfig {
    InterestConfig {
        name: "Rust".into(),
        not: vec!["Rust Belt".into()],
        max_items: 5,
        max_age_hours: 72,
        ..Default::default()
    }
}

/// Serves `feeds` and `pages` by URL and the WXII article for `ok.example` pages; records every
/// request.
struct Fake {
    feeds: Vec<(String, Vec<u8>)>,
    pages: Vec<(String, String)>,
    seen: Mutex<Vec<String>>,
}

impl Fake {
    fn new(feeds: Vec<(String, Vec<u8>)>) -> Fake {
        Fake {
            feeds,
            pages: Vec::new(),
            seen: Mutex::new(Vec::new()),
        }
    }
    fn with_page(url: &str, html: &str, feeds: Vec<(String, Vec<u8>)>) -> Fake {
        Fake {
            pages: vec![(url.to_owned(), html.to_owned())],
            ..Fake::new(feeds)
        }
    }
    fn seen(&self) -> Vec<String> {
        self.seen.lock().unwrap().clone()
    }
    fn pages(&self) -> Vec<String> {
        self.seen()
            .into_iter()
            .filter(|u| u.contains(".example/"))
            .filter(|u| !u.contains("feeds.example/"))
            .collect()
    }
}

impl Net for Fake {
    fn get(&self, url: &str) -> Result<Vec<u8>, String> {
        self.seen.lock().unwrap().push(url.to_owned());
        if let Some((_, b)) = self.feeds.iter().find(|(u, _)| u == url) {
            Ok(b.clone())
        } else if let Some((_, html)) = self.pages.iter().find(|(u, _)| u == url) {
            Ok(html.clone().into_bytes())
        } else if url.starts_with("https://ok.example/") {
            Ok(fixture("article-wxii.html"))
        } else {
            Err(format!("offline: {url}"))
        }
    }
}

fn is_brief(a: &deckle_digest::Article) -> bool {
    a.teaser.is_none() && !a.body.contains("<p>The ")
}

fn titles(s: &deckle_digest::Section) -> Vec<&str> {
    s.articles.iter().map(|a| a.title.as_str()).collect()
}

#[test]
fn an_interest_searches_its_name_and_also_and_keeps_only_relevant_stories() {
    let net = Fake::new(vec![
        (
            search("\"Rust\""),
            rss(&[
                ("Rust 2.0 lands - Outlet", "https://ok.example/1", 10),
                ("Trust funds - Outlet", "https://ok.example/2", 9),
                ("Rust Belt voters - Outlet", "https://ok.example/3", 8),
            ]),
        ),
        (
            search("Cargo"),
            rss(&[("Cargo gets workspaces - Outlet", "https://ok.example/4", 7)]),
        ),
    ]);
    let mut i = rust();
    i.also = vec!["Cargo".into()];
    let s = interest_section(&i, &config(), now(), &net, &mut HashSet::new()).unwrap();
    assert_eq!(s.key, "interests:Rust");
    assert_eq!(s.title, "Rust");
    assert_eq!(s.kicker.as_deref(), Some("Your interests"));
    assert_eq!(titles(&s), ["Rust 2.0 lands", "Cargo gets workspaces"]);
    let seen = net.seen();
    assert!(
        seen.contains(&search("\"Rust\"")) && seen.contains(&search("Cargo")),
        "{seen:?}"
    );
}

#[test]
fn lead_is_fetched_whole_and_the_rest_are_briefs() {
    let net = Fake::new(vec![(
        search("\"Rust\""),
        rss(&[
            ("Rust 2.0 lands - Outlet", "https://ok.example/1", 10),
            ("Rust jobs grow - Outlet", "https://ok.example/2", 9),
        ]),
    )]);
    let s = interest_section(&rust(), &config(), now(), &net, &mut HashSet::new()).unwrap();
    assert_eq!(net.pages(), ["https://ok.example/1"]);
    assert!(s.articles[0].teaser.is_some());
    assert!(is_brief(&s.articles[1]));
}

#[test]
fn a_lead_whose_text_hits_not_is_dropped_and_the_next_fetched() {
    let page = "<html><head><title>Rust 2.0</title></head><body><article><h1>Rust 2.0</h1>\
        <p>From the Rust Belt, a story about steel towns and corrosion that runs long enough \
        to count as a real paragraph for the extractor, with a second sentence to be safe.</p>\
        <p>The mills along the river have been closing for forty years, and the people who \
        stayed have watched the paint peel and the bridges rust through one winter at a time.</p>\
        <p>Nobody here is talking about a programming language; they are talking about the \
        price of scrap and whether the new plant will ever open its doors.</p>\
        <p>The county commissioners met again on Tuesday night to hear from the developer, who \
        promised the same four hundred jobs he promised two years ago and asked for another \
        extension on the tax abatement that was supposed to expire this spring.</p>\
        <p>Outside the hall, a man who worked the blast furnace for thirty-one years said he \
        had stopped coming to these meetings, and then admitted he had come to this one anyway, \
        because his grandson had asked him whether the plant would be there when he grew up.</p>\
        <p>The answer, he said, was the same as it had been for a decade: nobody knows, and the \
        river keeps running past the empty lots where the coke ovens used to glow all night.</p>\
        </article></body></html>";
    let net = Fake::with_page(
        "https://bad-lead.example/1",
        page,
        vec![(
            search("\"Rust\""),
            rss(&[
                ("Rust 2.0 - Outlet", "https://bad-lead.example/1", 10),
                ("Rust jobs grow - Outlet", "https://ok.example/2", 9),
            ]),
        )],
    );
    let s = interest_section(&rust(), &config(), now(), &net, &mut HashSet::new()).unwrap();
    assert_eq!(titles(&s), ["Rust jobs grow"]);
    assert_eq!(
        net.pages(),
        ["https://bad-lead.example/1", "https://ok.example/2"]
    );
}

#[test]
fn a_quiet_day_is_none_and_all_sources_failing_is_unavailable() {
    let net = Fake::new(vec![(
        search("\"Rust\""),
        rss(&[("Trust funds - Outlet", "https://ok.example/2", 9)]),
    )]);
    assert!(interest_section(&rust(), &config(), now(), &net, &mut HashSet::new()).is_none());
    let s = interest_section(
        &rust(),
        &config(),
        now(),
        &Fake::new(vec![]),
        &mut HashSet::new(),
    )
    .unwrap();
    assert!(
        s.unavailable.as_deref().unwrap().contains("Google News"),
        "{:?}",
        s.unavailable
    );
    assert_eq!(s.key, "interests:Rust");
}

#[test]
fn titles_taken_by_an_earlier_section_are_skipped_and_kept_ones_recorded() {
    let net = Fake::new(vec![(
        search("\"Rust\""),
        rss(&[
            ("Rust 2.0 lands - Outlet", "https://ok.example/1", 10),
            ("Rust jobs grow - Outlet", "https://ok.example/2", 9),
        ]),
    )]);
    let mut taken = HashSet::from([title_key("Rust 2.0 lands")]);
    let mut i = rust();
    i.full_text = Some(FullText::No);
    let s = interest_section(&i, &config(), now(), &net, &mut taken).unwrap();
    assert_eq!(titles(&s), ["Rust jobs grow"]);
    assert!(taken.contains(&title_key("Rust jobs grow")));
}

#[test]
fn gather_places_interest_sections_where_the_kind_sits_and_notes_quiet_ones() {
    let mut cfg = config();
    cfg.sections = vec![SectionKind::Interests, SectionKind::Horoscope];
    cfg.interests = vec![
        rust(),
        InterestConfig {
            name: "Quiet".into(),
            max_items: 5,
            max_age_hours: 72,
            ..Default::default()
        },
    ];
    let net = Fake::new(vec![
        (
            search("\"Rust\""),
            rss(&[("Rust 2.0 lands - Outlet", "https://ok.example/1", 10)]),
        ),
        (search("\"Quiet\""), rss(&[])),
    ]);
    let g = deckle_digest::gather(&cfg, "2026-10-20".parse().unwrap(), now(), &net);
    assert_eq!(
        g.edition
            .sections
            .iter()
            .map(|s| s.key.as_str())
            .collect::<Vec<_>>(),
        ["interests:Rust", "horoscope"]
    );
    assert_eq!(g.news, [0]);
    assert!(
        g.notes
            .iter()
            .any(|n| n == "Nothing new about Quiet today."),
        "{:?}",
        g.notes
    );
}

#[test]
fn interests_without_the_kind_in_sections_are_noted_not_printed() {
    let mut cfg = config();
    cfg.sections = vec![SectionKind::Horoscope];
    cfg.interests = vec![rust()];
    let g = deckle_digest::gather(
        &cfg,
        "2026-10-20".parse().unwrap(),
        now(),
        &Fake::new(vec![]),
    );
    assert_eq!(g.edition.sections.len(), 1);
    assert!(
        g.notes.iter().any(|n| n.contains("isn't in sections")),
        "{:?}",
        g.notes
    );
}

#[test]
fn a_blank_interest_prints_unavailable_with_a_hint() {
    let i = InterestConfig {
        name: "".into(),
        max_items: 5,
        max_age_hours: 72,
        ..Default::default()
    };
    let s = interest_section(
        &i,
        &config(),
        now(),
        &Fake::new(vec![]),
        &mut HashSet::new(),
    )
    .unwrap();
    assert!(s.unavailable.as_deref().unwrap().contains("give a name"));
}
