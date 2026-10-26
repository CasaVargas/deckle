// SPDX-License-Identifier: GPL-3.0-or-later
//! Nudges (interests design §7 as amended by §15): what Deckle noticed, one tappable page each,
//! after everything else in the paper.

use deckle_digest::config::{Config, SectionKind};
use deckle_digest::{GatherOptions, Gathered, Net, Nudge};

struct Offline;
impl Net for Offline {
    fn get(&self, url: &str) -> Result<Vec<u8>, String> {
        Err(format!("offline: {url}"))
    }
}

fn config(nudges: bool, learning: bool) -> Config {
    let mut c = Config::parse(deckle_digest::config::STARTER).unwrap();
    c.sections = vec![SectionKind::Horoscope];
    c.photos = false;
    c.learning.enabled = learning;
    c.learning.nudges = nudges;
    c
}

fn nudge(key: &str, name: &str) -> Nudge {
    Nudge {
        key: key.into(),
        name: name.into(),
        why: format!("You read 4 stories about {name} in the last 14 days."),
    }
}

fn gather(cfg: &Config, nudges: Vec<Nudge>) -> Gathered {
    let opts = GatherOptions {
        nudges,
        ..GatherOptions::default()
    };
    deckle_digest::gather_with(
        cfg,
        "2026-10-26".parse().unwrap(),
        "2026-10-26T10:00:00Z".parse().unwrap(),
        &Offline,
        &opts,
    )
}

fn file(g: &Gathered, name: &str) -> String {
    deckle_digest::render::render(g, vec![])
        .into_iter()
        .find(|(p, _)| p.ends_with(name))
        .map(|(_, b)| String::from_utf8(b).unwrap())
        .unwrap_or_else(|| panic!("no {name}"))
}

#[test]
fn nudges_print_as_their_own_sections_after_everything() {
    let g = gather(
        &config(true, true),
        vec![
            nudge("bryce young", "Bryce Young"),
            nudge("brandon sanderson", "Brandon Sanderson"),
        ],
    );
    let keys: Vec<&str> = g.edition.sections.iter().map(|s| s.key.as_str()).collect();
    assert_eq!(
        keys,
        [
            "horoscope",
            "nudge:bryce young",
            "nudge-ok:bryce young",
            "nudge:brandon sanderson",
            "nudge-ok:brandon sanderson"
        ]
    );
    assert_eq!(g.edition.sections[1].title, "Deckle noticed: Bryce Young");
    assert_eq!(
        g.edition.sections[1].kicker.as_deref(),
        Some("Deckle noticed")
    );
    assert_eq!(g.edition.sections[1].articles.len(), 1);
    assert!(g.news.iter().all(|i| *i == 0), "{:?}", g.news);
    // the nudge page: one link, to its confirmation page, and nothing else to tap
    let page = file(&g, "s2.xhtml");
    assert!(page.contains("href=\"s3.xhtml\""), "{page}");
    assert!(
        page.contains("Tap here to make Bryce Young a section."),
        "{page}"
    );
    assert!(!page.contains("class=\"back\""), "{page}");
    assert!(!page.contains("In this section"), "{page}");
    assert_eq!(page.matches("<a ").count(), 1, "{page}");
    // the confirmation page: what happens next, and one link back to the front
    let ok = file(&g, "s3.xhtml");
    assert!(ok.contains("Bryce Young becomes a section"), "{ok}");
    assert!(ok.contains("href=\"front.xhtml\""), "{ok}");
    assert!(ok.contains("Change your mind on the Daily tab."), "{ok}");
    assert_eq!(ok.matches("<a ").count(), 1, "{ok}");
    // the front's index lists them as any section, with their one item
    let front = file(&g, "front.xhtml");
    assert!(
        front.contains("s2.xhtml\">Deckle noticed: Bryce Young</a> <span class=\"meta\">1 item"),
        "{front}"
    );
    assert!(
        !front.contains("s2.xhtml#a"),
        "no tile for a nudge: {front}"
    );
}

#[test]
fn other_outputs_get_the_paper_without_nudges() {
    let g = gather(
        &config(true, true),
        vec![nudge("bryce young", "Bryce Young")],
    );
    let plain = g.without_nudges();
    assert_eq!(plain.edition.sections.len(), 1);
    assert_eq!(plain.news, g.news);
    let pdf = deckle_digest::pdf::render(
        &plain,
        deckle_digest::pdf::Screen::Remarkable2,
        deckle_digest::config::Photos::Gray,
    );
    assert!(!pdf.is_empty());
}

#[test]
fn a_name_with_markup_is_escaped() {
    let g = gather(&config(true, true), vec![nudge("tom", "Tom & <Jerry>")]);
    let page = file(&g, "s2.xhtml");
    assert!(page.contains("Tom &amp; &lt;Jerry&gt;"), "{page}");
    assert!(!page.contains("<Jerry>"), "{page}");
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("e.epub");
    deckle_digest::write_edition(&g, &path).unwrap();
    assert!(deckle_core::inspect::inspect(&path).is_ok());
}

#[test]
fn nudges_need_learning_on_and_the_flag() {
    for (flag, learning) in [(false, true), (true, false), (false, false)] {
        let g = gather(&config(flag, learning), vec![nudge("x", "X")]);
        assert_eq!(
            g.edition.sections.len(),
            1,
            "flag {flag} learning {learning}"
        );
    }
}

#[test]
fn at_most_three() {
    let nudges: Vec<Nudge> = (0..5)
        .map(|n| nudge(&format!("k{n}"), &format!("N{n}")))
        .collect();
    let g = gather(&config(true, true), nudges);
    assert_eq!(g.edition.sections.len(), 1 + 3 * 2);
}
