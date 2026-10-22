// SPDX-License-Identifier: GPL-3.0-or-later
//! From Your Shelf: its settings and `shelf.json` (shelf design §4).

use chrono::{Days, NaiveDate};
use deckle_digest::config::{Config, STARTER, SectionKind, section_key};
use deckle_digest::shelf::store::{BookSnap, KindleSnapshot, Shelf};

fn today() -> NaiveDate {
    NaiveDate::from_ymd_opt(2026, 10, 22).unwrap()
}

fn snap(name: &str, seen: NaiveDate) -> KindleSnapshot {
    KindleSnapshot {
        name: name.into(),
        seen: seen.format("%Y-%m-%d").to_string(),
        books: vec![BookSnap {
            file: "Book.kfx".into(),
            title: "Book".into(),
            wpm: vec![300.0],
            ..Default::default()
        }],
        ..Default::default()
    }
}

#[test]
fn starter_has_shelf_between_feeds_and_lottery() {
    let cfg = Config::parse(STARTER).unwrap();
    let at = |k| cfg.sections.iter().position(|s| *s == k).unwrap();
    assert_eq!(at(SectionKind::Shelf), at(SectionKind::Feeds) + 1);
    assert_eq!(at(SectionKind::Shelf) + 1, at(SectionKind::Lottery));
}

#[test]
fn default_sections_too() {
    let cfg = Config::default();
    let at = |k| cfg.sections.iter().position(|s| *s == k).unwrap();
    assert_eq!(at(SectionKind::Shelf), at(SectionKind::Feeds) + 1);
    assert_eq!(at(SectionKind::Shelf) + 1, at(SectionKind::Lottery));
}

#[test]
fn untouched_config_saves_no_shelf_table() {
    let text = toml::to_string(&Config::default()).unwrap();
    assert!(!text.contains("[shelf]"), "{text}");
    let cfg = Config::parse("[shelf]\nhide = [\"Draft\"]\n").unwrap();
    assert!(cfg.shelf.nightstand && cfg.shelf.highlights && cfg.shelf.words);
    assert_eq!(cfg.shelf.hide, ["Draft"]);
    assert!(toml::to_string(&cfg).unwrap().contains("[shelf]"));
}

#[test]
fn sync_shelf_defaults_on_and_follows_devices() {
    let c = Config::parse("[sync.devices.\"G0\"]\nshelf = false\n").unwrap();
    assert!(c.sync.shelf);
    assert!(!c.sync.for_device("G0").shelf);
    assert!(c.sync.for_device("other").shelf);
    assert!(
        STARTER.contains("shelf = true"),
        "the starter documents the switch"
    );
}

#[test]
fn the_shelf_section_key() {
    assert_eq!(section_key(SectionKind::Shelf, None), "shelf");
}

#[test]
fn the_shelf_store_sits_beside_digest_toml() {
    let mut cfg = Config::default();
    assert_eq!(cfg.shelf_store().file_name().unwrap(), "shelf.json");
    cfg.shelf_path = Some("/tmp/x/s.json".into());
    assert_eq!(cfg.shelf_store(), std::path::Path::new("/tmp/x/s.json"));
}

#[test]
fn round_trip_and_corrupt_file() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("shelf.json");
    let mut shelf = Shelf::default();
    shelf.put("G0", snap("Scribe", today()));
    shelf.shown.insert("h1".into(), today());
    shelf.save(&path).unwrap();
    assert_eq!(Shelf::load(&path), shelf);
    std::fs::write(&path, b"\x00not json").unwrap();
    assert_eq!(Shelf::load(&path), Shelf::default());
    assert_eq!(
        Shelf::load(&dir.path().join("missing.json")),
        Shelf::default()
    );
}

#[test]
fn forget_removes_the_file_and_a_missing_one_is_fine() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("shelf.json");
    Shelf::default().save(&path).unwrap();
    Shelf::forget(&path).unwrap();
    assert!(!path.exists());
    Shelf::forget(&path).unwrap();
}

#[test]
fn put_replaces_one_kindle_only() {
    let mut shelf = Shelf::default();
    shelf.put("A", snap("Paperwhite", today()));
    shelf.put("B", snap("Scribe", today()));
    let b = shelf.kindles["B"].clone();
    let mut again = snap("Paperwhite", today());
    again.books.clear();
    shelf.put("A", again);
    assert!(shelf.kindles["A"].books.is_empty());
    assert_eq!(shelf.kindles["B"], b);
}

#[test]
fn fresh_ignores_old_snapshots() {
    let mut shelf = Shelf::default();
    shelf.put("old", snap("Old", today() - Days::new(31)));
    shelf.put("edge", snap("Edge", today() - Days::new(30)));
    shelf.put("new", snap("New", today() - Days::new(1)));
    let names: Vec<&str> = shelf
        .fresh(today())
        .into_iter()
        .map(|(_, k)| k.name.as_str())
        .collect();
    assert_eq!(names, ["New", "Edge"], "newest first, 31 days is out");
}

#[test]
fn prune_shown_keeps_ninety_days() {
    let mut shelf = Shelf::default();
    shelf.shown.insert("old".into(), today() - Days::new(90));
    shelf.shown.insert("kept".into(), today() - Days::new(89));
    shelf.prune_shown(today());
    assert!(shelf.shown.contains_key("kept") && !shelf.shown.contains_key("old"));
}

// ---- selection rules (shelf design §4) ------------------------------------------------

mod select {
    use super::*;
    use chrono_tz::{America::New_York, Tz};
    use deckle_digest::config::ShelfConfig;
    use deckle_digest::shelf::select;
    use deckle_digest::shelf::store::{HighlightSnap, WordSnap};

    const TZ: Tz = New_York;

    fn date() -> NaiveDate {
        super::today()
    }

    /// A book last read `days_ago` days before the edition, at noon Eastern.
    fn book(title: &str, days_ago: u64, last: i64, end: i64) -> BookSnap {
        let day = date() - Days::new(days_ago);
        BookSnap {
            file: format!("{title}.kfx"),
            title: title.into(),
            last: Some(last),
            end: Some(end),
            last_read: Some(format!("{}T16:00:00Z", day.format("%Y-%m-%d"))),
            ..Default::default()
        }
    }

    fn at_percent(title: &str, days_ago: u64, pct: i64) -> BookSnap {
        book(title, days_ago, pct, 100)
    }

    fn shelf_of(name: &str, books: Vec<BookSnap>) -> Shelf {
        let mut shelf = Shelf::default();
        let mut k = snap(name, date());
        k.books = books;
        shelf.put("G0", k);
        shelf
    }

    fn lines(books: Vec<BookSnap>) -> Vec<(String, String)> {
        let shelf = shelf_of("Scribe", books);
        select::nightstand(&shelf, &ShelfConfig::default(), date(), TZ, false)
            .map(|n| n.books.into_iter().map(|l| (l.title, l.state)).collect())
            .unwrap_or_default()
    }

    #[test]
    fn percent_by_source() {
        let mut b = BookSnap {
            last: Some(410004),
            end: Some(664520),
            ..Default::default()
        };
        assert_eq!(select::percent(&b, false), Some((61, false)));
        b.last = Some(664520);
        assert_eq!(select::percent(&b, false), Some((100, false)));
        b.last = Some(664519);
        assert_eq!(select::percent(&b, false), Some((99, false)));
        b.end = None;
        b.kindle_fraction = Some(0.613);
        assert_eq!(select::percent(&b, true), Some((61, true)));
        assert_eq!(select::percent(&b, false), None);
    }

    #[test]
    fn in_progress_window() {
        let titles = |bs| lines(bs).into_iter().map(|l| l.0).collect::<Vec<_>>();
        assert_eq!(titles(vec![at_percent("in60", 60, 50)]), ["in60"]);
        assert!(titles(vec![at_percent("out61", 61, 50)]).is_empty());
        for pct in [0, 98] {
            assert!(titles(vec![at_percent("edge", 1, pct)]).is_empty(), "{pct}");
        }
        for pct in [1, 97] {
            assert_eq!(titles(vec![at_percent("edge", 1, pct)]), ["edge"], "{pct}");
        }
        let mut nopct = at_percent("nopct", 1, 50);
        nopct.end = None;
        assert_eq!(titles(vec![nopct]), ["nopct"]);
        let mut notime = at_percent("notime", 1, 50);
        notime.last_read = None;
        assert!(
            titles(vec![notime]).is_empty(),
            "no time is not in progress"
        );
        let edition = at_percent("deckle-daily-2026-10-21", 1, 50);
        assert!(titles(vec![edition]).is_empty(), "never the paper itself");
        let shelf = shelf_of("Scribe", vec![at_percent("Mush Pie", 1, 50)]);
        let cfg = ShelfConfig {
            hide: vec!["mush".into()],
            ..Default::default()
        };
        assert!(select::nightstand(&shelf, &cfg, date(), TZ, false).is_none());
    }

    #[test]
    fn states() {
        let state = |days, pct| lines(vec![at_percent("B", days, pct)])[0].1.clone();
        assert_eq!(state(1, 91), "nearly done");
        assert_eq!(state(23, 50), "not opened in 23 days");
        assert_eq!(state(0, 50), "last read today");
        assert_eq!(state(1, 50), "last read yesterday");
        assert_eq!(state(2, 50), "last read Tuesday");
        assert_eq!(state(9, 50), "last read Oct 13");
    }

    #[test]
    fn newest_three_and_two_kindles() {
        let titles = |bs| lines(bs).into_iter().map(|l| l.0).collect::<Vec<_>>();
        let four = vec![
            at_percent("A", 5, 10),
            at_percent("B", 1, 10),
            at_percent("C", 3, 10),
            at_percent("D", 2, 10),
        ];
        assert_eq!(titles(four), ["B", "D", "C"]);

        let mut a = at_percent("Same", 4, 20);
        a.asin = Some("B0SAME".into());
        let mut b = at_percent("Same", 1, 40);
        b.asin = Some("B0SAME".into());
        let mut shelf = Shelf::default();
        let mut k1 = snap("Paperwhite", date());
        k1.books = vec![a];
        let mut k2 = snap("Scribe", date());
        k2.books = vec![b];
        shelf.put("P", k1);
        shelf.put("S", k2);
        let n = select::nightstand(&shelf, &ShelfConfig::default(), date(), TZ, false).unwrap();
        assert_eq!(n.books.len(), 1);
        assert_eq!(n.books[0].percent, Some(40), "the later read wins");
        assert_eq!(n.from, "Scribe");
    }

    #[test]
    fn nudge_rules() {
        let nudge = |b| {
            let shelf = shelf_of("Scribe", vec![b]);
            select::nightstand(&shelf, &ShelfConfig::default(), date(), TZ, false)
                .and_then(|n| n.nudge)
        };
        assert_eq!(
            nudge(at_percent("The Count of Monte Cristo", 2, 62)).as_deref(),
            Some("Pick up *The Count of Monte Cristo* tonight? You left off 62% in.")
        );
        assert_eq!(
            nudge(at_percent("The Count of Monte Cristo", 2, 91)).as_deref(),
            Some("*The Count of Monte Cristo*: one more evening should finish it.")
        );
        assert_eq!(nudge(at_percent("Old", 7, 50)), None);
        let mut nopct = at_percent("Walden", 2, 50);
        nopct.end = None;
        assert_eq!(nudge(nopct).as_deref(), Some("Pick up *Walden* tonight?"));
    }

    fn highlight(id: &str, text_len: usize, days_ago: u64, notes: bool) -> HighlightSnap {
        HighlightSnap {
            id: id.into(),
            title: "Walden".into(),
            author: "Thoreau".into(),
            text: "x".repeat(text_len),
            notes: if notes { vec!["mine".into()] } else { vec![] },
            page: Some("12".into()),
            added: Some(date() - Days::new(days_ago)),
        }
    }

    fn shelf_with(hs: Vec<HighlightSnap>) -> Shelf {
        let mut shelf = Shelf::default();
        let mut k = snap("Paperwhite", date());
        k.highlights_shared = true;
        k.highlights = hs;
        shelf.put("P", k);
        shelf
    }

    #[test]
    fn highlight_age_and_preference() {
        let pick = |hs| select::highlight(&shelf_with(hs), date()).map(|p| p.highlight.id);
        assert_eq!(
            pick(vec![
                highlight("plain200", 100, 200, false),
                highlight("noted200", 100, 200, true),
                highlight("is40", 100, 40, true),
            ])
            .as_deref(),
            Some("noted200")
        );
        assert_eq!(
            pick(vec![highlight("is40", 100, 40, false)]).as_deref(),
            Some("is40")
        );
        assert_eq!(pick(vec![highlight("is10", 100, 10, false)]), None);
        assert_eq!(pick(vec![highlight("short", 39, 200, false)]), None);
        assert_eq!(pick(vec![highlight("long", 601, 200, false)]), None);
        assert!(pick(vec![highlight("ok", 40, 200, false)]).is_some());
        assert!(pick(vec![highlight("ok", 600, 200, false)]).is_some());
    }

    #[test]
    fn highlight_no_repeat_and_stable() {
        let hs = vec![
            highlight("a", 100, 200, false),
            highlight("b", 100, 200, false),
            highlight("c", 100, 200, false),
        ];
        let mut shelf = shelf_with(hs);
        let first = select::highlight(&shelf, date()).unwrap();
        assert_eq!(
            select::highlight(&shelf, date()).unwrap().highlight,
            first.highlight
        );
        // Shown on the 1st: out. Shown today (a rebuild): still picked.
        shelf
            .shown
            .insert("a".into(), NaiveDate::from_ymd_opt(2026, 9, 1).unwrap());
        shelf
            .shown
            .insert("b".into(), NaiveDate::from_ymd_opt(2026, 9, 1).unwrap());
        assert_eq!(select::highlight(&shelf, date()).unwrap().highlight.id, "c");
        shelf.shown.insert("c".into(), date());
        assert_eq!(select::highlight(&shelf, date()).unwrap().highlight.id, "c");
        // Everything shown recently: nothing to print.
        shelf
            .shown
            .insert("c".into(), NaiveDate::from_ymd_opt(2026, 9, 1).unwrap());
        assert!(select::highlight(&shelf, date()).is_none());
        assert_eq!(first.from, "Paperwhite");
    }

    #[test]
    fn age_wording() {
        let ago = |days| select::age(date() - Days::new(days), date());
        assert_eq!(ago(30), "4 weeks");
        let months = NaiveDate::from_ymd_opt(2025, 3, 22).unwrap();
        assert_eq!(select::age(months, date()), "19 months");
        let years = NaiveDate::from_ymd_opt(2023, 10, 22).unwrap();
        assert_eq!(select::age(years, date()), "3 years");
    }

    fn word(w: &str, stem: &str, days_ago: u64) -> WordSnap {
        let day = date() - Days::new(days_ago);
        WordSnap {
            word: w.into(),
            stem: stem.into(),
            usage: format!("a sentence with {w}"),
            book: "Walden".into(),
            at: format!("{}T16:00:00Z", day.format("%Y-%m-%d")),
            definition: None,
        }
    }

    #[test]
    fn words_rules() {
        let mut shelf = Shelf::default();
        let mut k = snap("Paperwhite", date());
        k.words_shared = true;
        k.words = vec![
            word("ephemeral", "ephemeral", 8),
            word("ineffable", "ineffable", 3),
            word("run", "run", 1),
            word("ran", "run", 2),
            word("quixotic", "quixotic", 7),
        ];
        shelf.put("P", k);
        let got = select::words(&shelf, date(), TZ);
        let words: Vec<&str> = got.iter().map(|(w, _)| w.word.as_str()).collect();
        assert_eq!(words, ["run", "ineffable", "quixotic"]);
        assert_eq!(got[0].1, "Walden, Wednesday");
        assert_eq!(select::words(&Shelf::default(), date(), TZ).len(), 0);
    }
}

// ---- the section on the page (shelf design §5) ---------------------------------------

mod page {
    use super::*;
    use chrono_tz::{America::New_York, Tz};
    use deckle_digest::shelf::section;
    use deckle_digest::shelf::store::{HighlightSnap, WordSnap};
    use deckle_digest::{Edition, GatherOptions, Gathered, Section, gather_with, render};

    const TZ: Tz = New_York;

    fn date() -> NaiveDate {
        super::today()
    }

    fn at_percent(title: &str, days_ago: u64, pct: i64) -> BookSnap {
        let day = date() - Days::new(days_ago);
        BookSnap {
            file: format!("{title}.kfx"),
            title: title.into(),
            last: Some(pct),
            end: Some(100),
            last_read: Some(format!("{}T16:00:00Z", day.format("%Y-%m-%d"))),
            ..Default::default()
        }
    }

    /// The Scribe (books only, plugged in yesterday) and the Paperwhite (a highlight with a
    /// note, two words; plugged in two days ago).
    fn fixed_shelf() -> Shelf {
        let mut shelf = Shelf::default();
        let mut scribe = snap("Kindle Scribe", date() - Days::new(1));
        scribe.seen = "2026-10-21T20:00:00Z".into();
        scribe.books = vec![
            at_percent("The Count of Monte Cristo", 2, 62),
            at_percent("Walden", 9, 35),
        ];
        shelf.put("S", scribe);
        let mut pw = snap("Kindle Paperwhite", date() - Days::new(2));
        pw.seen = "2026-10-20T20:00:00Z".into();
        pw.books.clear();
        pw.highlights_shared = true;
        pw.highlights = vec![HighlightSnap {
            id: "h1".into(),
            title: "Walden".into(),
            author: "Henry David Thoreau".into(),
            text: "I went to the woods because I wished to live deliberately, to front only the essential facts of life.".into(),
            notes: vec!["read this again before the cabin trip".into()],
            page: Some("88".into()),
            added: NaiveDate::from_ymd_opt(2025, 3, 22),
        }];
        pw.words_shared = true;
        pw.words = vec![
            WordSnap {
                word: "ephemeral".into(),
                stem: "ephemeral".into(),
                usage: "Fame is ephemeral & fickle.".into(),
                book: "Walden".into(),
                at: "2026-10-20T15:00:00Z".into(),
                definition: Some("lasting a very short time".into()),
            },
            WordSnap {
                word: "quixotic".into(),
                stem: "quixotic".into(),
                usage: "A quixotic plan.".into(),
                book: "Don Quixote".into(),
                at: "2026-10-18T15:00:00Z".into(),
                definition: None,
            },
        ];
        shelf.put("P", pw);
        shelf
    }

    fn xhtml(s: Section) -> String {
        let g = Gathered {
            edition: Edition {
                paper: "The Deckle Daily".into(),
                date: date(),
                place: "New York, NY".into(),
                weather_line: None,
                sections: vec![s],
                assets: Vec::new(),
            },
            news: Vec::new(),
            reading: None,
            long_reads: Vec::new(),
            reading_outcomes: Vec::new(),
            notes: Vec::new(),
        };
        let (_, bytes) = render::render(&g, Vec::new())
            .into_iter()
            .find(|(p, _)| p == "OEBPS/s1.xhtml")
            .unwrap();
        String::from_utf8(bytes).unwrap()
    }

    fn titles(s: &Section) -> Vec<String> {
        s.articles.iter().map(|a| a.title.clone()).collect()
    }

    #[test]
    fn golden_page() {
        let mut shelf = fixed_shelf();
        let s = section(&mut shelf, &Config::default(), date(), TZ, false).unwrap();
        assert_eq!(s.key, "shelf");
        let got = xhtml(s);
        let path = concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/tests/fixtures/shelf/golden-page.xhtml"
        );
        if std::env::var_os("UPDATE_GOLDEN").is_some() {
            std::fs::write(path, &got).unwrap();
        }
        assert_eq!(got, std::fs::read_to_string(path).unwrap());
        roxmltree::Document::parse(&got).expect("well-formed XHTML");
        for want in [
            "On your nightstand",
            "You highlighted this 19 months ago",
            " (from your Kindle Paperwhite)",
            "Your note: read this again before the cabin trip",
            "Words you looked up this week",
            "Taken from your Kindle (Kindle Scribe) when it was last plugged in, Wednesday",
            "<i>The Count of Monte Cristo</i>",
            "width:62%",
            "62% · last read Tuesday",
            "Pick up <i>The Count of Monte Cristo</i> tonight? You left off 62% in.",
            "Walden, Henry David Thoreau · page 88 · March 22, 2025",
            "Fame is ephemeral &amp; fickle.",
        ] {
            assert!(got.contains(want), "missing {want:?} in\n{got}");
        }
        assert_eq!(
            shelf.shown.get("h1"),
            Some(&date()),
            "the pick is remembered"
        );
    }

    #[test]
    fn scribe_only_prints_nightstand() {
        let mut shelf = fixed_shelf();
        shelf.kindles.remove("P");
        let s = section(&mut shelf, &Config::default(), date(), TZ, false).unwrap();
        assert_eq!(titles(&s), ["On your nightstand"]);
        assert!(shelf.shown.is_empty());
    }

    #[test]
    fn parts_switch_off() {
        let mut cfg = Config::default();
        cfg.shelf.words = false;
        let s = section(&mut fixed_shelf(), &cfg, date(), TZ, false).unwrap();
        assert!(
            titles(&s).iter().all(|t| !t.starts_with("Words")),
            "{:?}",
            titles(&s)
        );
        assert_eq!(s.articles.len(), 2);
        cfg.shelf.highlights = false;
        cfg.shelf.nightstand = false;
        assert!(section(&mut fixed_shelf(), &cfg, date(), TZ, false).is_none());
    }

    /// A config with only the shelf, and stores in a temp dir.
    fn cfg_in(dir: &std::path::Path) -> Config {
        let mut cfg = Config {
            sections: vec![SectionKind::Shelf],
            shelf_path: Some(dir.join("shelf.json")),
            puzzle_path: Some(dir.join("puzzles.json")),
            ..Default::default()
        };
        cfg.reading.queue_path = Some(dir.join("reading.jsonl"));
        cfg
    }

    fn now() -> chrono::DateTime<chrono::Utc> {
        "2026-10-22T14:00:00Z".parse().unwrap()
    }

    fn offline(_: &str) -> Result<Vec<u8>, String> {
        Err("offline".into())
    }

    #[test]
    fn nothing_leaves_section_out() {
        let mut empty = Shelf::default();
        assert!(section(&mut empty, &Config::default(), date(), TZ, false).is_none());

        let dir = tempfile::tempdir().unwrap();
        let cfg = cfg_in(dir.path());
        let g = gather_with(&cfg, date(), now(), &offline, &GatherOptions::default());
        assert!(g.edition.sections.iter().all(|s| s.key != "shelf"));
        assert!(
            g.notes
                .iter()
                .any(|n| n == "From Your Shelf left out: plug your Kindle in once to fill it"),
            "{:?}",
            g.notes
        );

        // A 40-day-old snapshot of finished books: out, and no plug-in note (it isn't fresh,
        // but the Kindle has been seen).
        let mut shelf = Shelf::default();
        let mut old = snap("Scribe", date() - Days::new(40));
        old.books = vec![at_percent("Done", 40, 99)];
        shelf.put("S", old);
        shelf.save(&cfg.shelf_store()).unwrap();
        let g = gather_with(&cfg, date(), now(), &offline, &GatherOptions::default());
        assert!(g.edition.sections.iter().all(|s| s.key != "shelf"));
        assert!(
            g.notes.iter().all(|n| !n.contains("plug your Kindle in")),
            "a Kindle has been seen: {:?}",
            g.notes
        );
    }

    #[test]
    fn rebuild_same_day_same_highlight() {
        let dir = tempfile::tempdir().unwrap();
        let cfg = cfg_in(dir.path());
        let mut shelf = fixed_shelf();
        // Three equally good old highlights: the pick must not move between builds.
        let k = shelf.kindles.get_mut("P").unwrap();
        for id in ["h2", "h3"] {
            let mut h = k.highlights[0].clone();
            h.id = id.into();
            h.notes.clear();
            k.highlights.push(h);
        }
        k.highlights[0].notes.clear();
        shelf.save(&cfg.shelf_store()).unwrap();
        let body = |g: &Gathered| {
            let s = g
                .edition
                .sections
                .iter()
                .find(|s| s.key == "shelf")
                .unwrap();
            s.articles
                .iter()
                .map(|a| a.body.clone())
                .collect::<Vec<_>>()
        };
        let opts = GatherOptions::default();
        let first = gather_with(&cfg, date(), now(), &offline, &opts);
        let second = gather_with(&cfg, date(), now(), &offline, &opts);
        assert_eq!(body(&first), body(&second));
        assert!(Shelf::load(&cfg.shelf_store()).shown.len() == 1);
    }

    #[test]
    fn gather_without_options_keeps_the_fraction_off() {
        assert!(!GatherOptions::default().shelf_fraction_ok);
    }
}
