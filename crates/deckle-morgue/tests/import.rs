// SPDX-License-Identifier: GPL-3.0-or-later
//! Importing the editions already in the cache folder (Morgue design §3, goal 3).

use std::path::Path;

use chrono::NaiveDate;
use deckle_digest::{Article, Edition, Gathered, Section};
use deckle_morgue::folder::Morgue;
use deckle_morgue::import::{import_cache, import_once, sidecar_from_epub};
use deckle_morgue::state::MorgueState;

fn d(s: &str) -> NaiveDate {
    s.parse().unwrap()
}

fn gathered(date: NaiveDate) -> Gathered {
    let article = |t: &str, b: &str| Article {
        title: t.into(),
        body: b.into(),
        ..Default::default()
    };
    Gathered {
        edition: Edition {
            paper: "The Deckle Daily".into(),
            date,
            place: "Linden Falls, NC".into(),
            weather_line: Some("Clear".into()),
            masthead: Default::default(),
            sections: vec![
                Section {
                    key: "local".into(),
                    title: "Local".into(),
                    articles: vec![
                        article(
                            "Mill sold",
                            "<p>The old mill &amp; brewery changed hands.</p>",
                        ),
                        article("Fair opens", "<p>Rides and funnel cake.</p>"),
                    ],
                    ..Default::default()
                },
                Section {
                    key: "lottery".into(),
                    title: "Lottery".into(),
                    articles: vec![article("Powerball", "<p>12 34 56</p>")],
                    ..Default::default()
                },
            ],
            assets: Vec::new(),
        },
        news: vec![0],
        reading: None,
        long_reads: Vec::new(),
        reading_outcomes: Vec::new(),
        notes: Vec::new(),
    }
}

/// A cache folder with editions for `dates` and a KFX for the first one only.
fn cache(dir: &Path, dates: &[&str]) {
    std::fs::create_dir_all(dir.join("kfx")).unwrap();
    for date in dates {
        let date = d(date);
        let name = format!("deckle-daily-{date}.epub");
        deckle_digest::write_edition(&gathered(date), &dir.join(name)).unwrap();
    }
    let first = d(dates[0]);
    let kfx = format!(
        "Deckle - The Deckle Daily -- {}.kfx",
        deckle_digest::long_date(first)
    );
    std::fs::write(dir.join("kfx").join(kfx), b"KFX").unwrap();
    // things that aren't editions
    std::fs::write(
        dir.join("deckle-reads-2026-10-20-north-carolina.epub"),
        b"x",
    )
    .unwrap();
    std::fs::write(dir.join("notes.txt"), b"x").unwrap();
}

#[test]
fn imports_two_with_one_kfx() {
    let tmp = tempfile::tempdir().unwrap();
    cache(&tmp.path().join("cache"), &["2026-10-21", "2026-10-20"]);
    let m = Morgue {
        root: tmp.path().join("Morgue"),
        keep_days: 0,
    };
    let got = import_cache(&m, &tmp.path().join("cache"));
    assert_eq!(got, [d("2026-10-21"), d("2026-10-20")]);
    assert_eq!(m.editions(), [d("2026-10-21"), d("2026-10-20")]);
    for date in [d("2026-10-21"), d("2026-10-20")] {
        let s = m.sidecar(date).unwrap();
        let titles: Vec<&str> = s
            .sections
            .iter()
            .flat_map(|x| x.stories.iter().map(|st| st.title.as_str()))
            .collect();
        assert_eq!(titles, ["Mill sold", "Fair opens", "Powerball"]);
        assert_eq!(s.sections[0].title, "Local");
        assert_eq!(
            s.sections[0].stories[0].text,
            "The old mill & brewery changed hands."
        );
        assert_eq!(
            (s.paper.as_str(), s.place.as_str()),
            ("The Deckle Daily", "Linden Falls, NC")
        );
        assert_eq!(
            s.number,
            date.format("%j").to_string().parse::<u32>().unwrap()
        );
        assert_eq!(s.weather.as_deref(), Some("Clear"));
        let lead = s
            .lead
            .as_ref()
            .expect("the lead is read from the front page");
        assert_eq!(lead.title, "Mill sold");
        assert!(lead.teaser.is_none(), "teasers are not kept in the EPUB");
        assert!(m.paths(date).0.exists());
    }
    assert_eq!(
        m.sidecar(d("2026-10-21")).unwrap().files.kfx.as_deref(),
        Some("2026-10-21.kfx")
    );
    assert!(m.paths(d("2026-10-21")).2.exists());
    assert!(
        !m.paths(d("2026-10-20")).2.exists(),
        "no KFX for the second"
    );
}

#[test]
fn skips_already_archived() {
    let tmp = tempfile::tempdir().unwrap();
    cache(&tmp.path().join("cache"), &["2026-10-21", "2026-10-20"]);
    let m = Morgue {
        root: tmp.path().join("Morgue"),
        keep_days: 0,
    };
    assert_eq!(import_cache(&m, &tmp.path().join("cache")).len(), 2);
    let before = std::fs::read(m.paths(d("2026-10-21")).1).unwrap();
    assert!(
        import_cache(&m, &tmp.path().join("cache")).is_empty(),
        "nothing new"
    );
    assert_eq!(std::fs::read(m.paths(d("2026-10-21")).1).unwrap(), before);
}

#[test]
fn import_once_runs_once() {
    let tmp = tempfile::tempdir().unwrap();
    cache(&tmp.path().join("cache"), &["2026-10-21"]);
    let m = Morgue {
        root: tmp.path().join("Morgue"),
        keep_days: 0,
    };
    let mut state = MorgueState::default();
    assert_eq!(
        import_once(&m, &tmp.path().join("cache"), &mut state),
        [d("2026-10-21")]
    );
    assert!(state.imported);
    // A later edition appearing in the cache is the archive's job now, not the import's.
    cache(&tmp.path().join("cache"), &["2026-10-22"]);
    assert!(import_once(&m, &tmp.path().join("cache"), &mut state).is_empty());
    assert_eq!(m.editions(), [d("2026-10-21")]);
}

#[test]
fn a_broken_epub_is_skipped_not_fatal() {
    let tmp = tempfile::tempdir().unwrap();
    let cache_dir = tmp.path().join("cache");
    cache(&cache_dir, &["2026-10-21"]);
    std::fs::write(cache_dir.join("deckle-daily-2026-10-19.epub"), b"not a zip").unwrap();
    let m = Morgue {
        root: tmp.path().join("Morgue"),
        keep_days: 0,
    };
    assert_eq!(import_cache(&m, &cache_dir), [d("2026-10-21")]);
    assert!(
        sidecar_from_epub(
            &cache_dir.join("deckle-daily-2026-10-19.epub"),
            d("2026-10-19")
        )
        .is_err()
    );
}

#[test]
fn a_failed_import_is_retried_not_marked_done() {
    let tmp = tempfile::tempdir().unwrap();
    cache(&tmp.path().join("cache"), &["2026-10-21", "2026-10-20"]);
    // The Morgue's root is a file: nothing can be archived.
    let blocked = tmp.path().join("Morgue");
    std::fs::write(&blocked, b"a file").unwrap();
    let mut state = MorgueState::default();
    let m = Morgue {
        root: blocked.clone(),
        keep_days: 0,
    };
    assert!(import_once(&m, &tmp.path().join("cache"), &mut state).is_empty());
    assert!(
        !state.imported,
        "nothing was imported, so try again next time"
    );
    // Once the folder works, the same cache is imported.
    std::fs::remove_file(&blocked).unwrap();
    assert_eq!(
        import_once(&m, &tmp.path().join("cache"), &mut state).len(),
        2
    );
    assert!(state.imported);
}

/// A minimal edition EPUB: a front page and the given `(file, xhtml)` section pages.
fn handmade(path: &Path, sections: &[(&str, &str)]) {
    use std::io::Write;
    let mut zip = zip::ZipWriter::new(std::fs::File::create(path).unwrap());
    let opts = zip::write::SimpleFileOptions::default();
    let front = "<html xmlns=\"http://www.w3.org/1999/xhtml\"><body><div class=\"masthead\"><p class=\"ears\">No. 294 · Linden Falls, NC</p><h1 class=\"paper\">The Deckle Daily</h1></div></body></html>";
    zip.start_file("OEBPS/front.xhtml", opts).unwrap();
    zip.write_all(front.as_bytes()).unwrap();
    for (name, body) in sections {
        zip.start_file(format!("OEBPS/{name}"), opts).unwrap();
        zip.write_all(body.as_bytes()).unwrap();
    }
    zip.finish().unwrap();
}

#[test]
fn one_bad_section_page_costs_that_page_not_the_edition() {
    let tmp = tempfile::tempdir().unwrap();
    let epub = tmp.path().join("deckle-daily-2026-10-21.epub");
    let good = "<html xmlns=\"http://www.w3.org/1999/xhtml\"><body><h1 class=\"section-head\">Local</h1><h2 class=\"headline\" id=\"a1\">Mill sold</h2><p>The mill.</p></body></html>";
    handmade(&epub, &[("s1.xhtml", "<<< not xml"), ("s2.xhtml", good)]);
    let s = sidecar_from_epub(&epub, d("2026-10-21")).unwrap();
    assert_eq!(s.sections.len(), 1);
    assert_eq!(s.sections[0].stories[0].title, "Mill sold");
    assert_eq!((s.paper.as_str(), s.number), ("The Deckle Daily", 294));
}

#[test]
fn an_edition_with_no_readable_pages_is_still_kept() {
    let tmp = tempfile::tempdir().unwrap();
    let cache_dir = tmp.path().join("cache");
    std::fs::create_dir_all(&cache_dir).unwrap();
    handmade(
        &cache_dir.join("deckle-daily-2026-10-21.epub"),
        &[("s1.xhtml", "garbage")],
    );
    let m = Morgue {
        root: tmp.path().join("Morgue"),
        keep_days: 0,
    };
    assert_eq!(import_cache(&m, &cache_dir), [d("2026-10-21")]);
    let s = m.sidecar(d("2026-10-21")).unwrap();
    assert!(
        s.sections.is_empty(),
        "a minimal sidecar: the EPUB itself is what matters"
    );
    assert!(m.paths(d("2026-10-21")).0.exists());
}

#[test]
fn an_imported_edition_keeps_its_weather_and_lead() {
    let tmp = tempfile::tempdir().unwrap();
    let epub = tmp.path().join("deckle-daily-2026-10-21.epub");
    let mut g = gathered(d("2026-10-21"));
    g.edition.sections[1].unavailable = Some("The draw wasn't posted.".into());
    deckle_digest::write_edition(&g, &epub).unwrap();
    let kept = deckle_morgue::from_gathered(&g);
    let s = sidecar_from_epub(&epub, d("2026-10-21")).unwrap();
    assert_eq!(s.weather.as_deref(), Some("Clear"));
    let (got, want) = (s.lead.unwrap(), kept.lead.unwrap());
    assert_eq!((got.section, got.title), (want.section, want.title));
    // "Unavailable" is a note on the page, not the last story's text.
    assert_eq!(s.sections[1].stories[0].text, "12 34 56");
}
