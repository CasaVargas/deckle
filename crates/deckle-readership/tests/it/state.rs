// SPDX-License-Identifier: GPL-3.0-or-later

use crate::common::*;
use deckle_readership::state::{OnDevice, State};

fn stem(date: &str) -> String {
    format!("deckle-daily-{date}")
}

#[test]
fn missing_or_corrupt_file_starts_empty() {
    let tmp = tempfile::tempdir().unwrap();
    assert!(
        State::load(&tmp.path().join("none.json"))
            .editions
            .is_empty()
    );
    let bad = tmp.path().join("bad.json");
    std::fs::write(&bad, "{").unwrap();
    assert!(State::load(&bad).editions.is_empty());
}

#[test]
fn save_then_load_round_trips() {
    let tmp = tempfile::tempdir().unwrap();
    let path = tmp.path().join("deckle").join("readership.json");
    let mut s = State::default();
    s.record_layout(layout("2026-10-21", &[4, 1204]));
    s.record_reads(
        "K",
        &[(stem("2026-10-21"), OnDevice::Opened(yjf(1300)))],
        NOW,
    );
    s.save(&path).unwrap();
    let back = State::load(&path);
    assert_eq!(back.editions, s.editions);
    let r = &back.reads[&d("2026-10-21")]["K"];
    assert!(r.opened);
    assert_eq!((r.furthest, r.last), (Some(1300), Some(1300)));
    assert_eq!(r.seen, NOW);
}

#[test]
fn rebuilding_a_date_with_different_sections_drops_its_reads() {
    let mut s = State::default();
    s.record_layout(layout("2026-10-21", &[4, 1204]));
    s.record_reads(
        "K",
        &[(stem("2026-10-21"), OnDevice::Opened(yjf(1300)))],
        NOW,
    );
    s.record_layout(layout("2026-10-21", &[4, 1204]));
    assert!(
        s.reads.contains_key(&d("2026-10-21")),
        "same starts: reads stay"
    );
    s.record_layout(layout("2026-10-21", &[4, 900]));
    assert!(
        !s.reads.contains_key(&d("2026-10-21")),
        "different starts: reads go"
    );
    assert_eq!(s.editions[&d("2026-10-21")].sections[1].start, 900);
}

#[test]
fn keeps_the_last_sixty_editions() {
    let mut s = State::default();
    let start = d("2026-01-01");
    for i in 0..61 {
        let date = (start + chrono::Days::new(i)).to_string();
        s.record_layout(layout(&date, &[4]));
        s.record_reads("K", &[(stem(&date), OnDevice::Unopened)], NOW);
    }
    assert_eq!(s.editions.len(), 60);
    assert!(!s.editions.contains_key(&start));
    assert!(!s.reads.contains_key(&start));
}

#[test]
fn stems_are_newest_first() {
    let mut s = State::default();
    s.record_layout(layout("2026-10-20", &[4]));
    s.record_layout(layout("2026-10-21", &[4]));
    assert_eq!(s.stems(), [stem("2026-10-21"), stem("2026-10-20")]);
}

#[test]
fn absent_editions_record_nothing_and_unopened_record_unopened() {
    let mut s = State::default();
    s.record_layout(layout("2026-10-20", &[4]));
    s.record_layout(layout("2026-10-21", &[4]));
    let r = s.record_reads(
        "K",
        &[
            (stem("2026-10-20"), OnDevice::Absent),
            (stem("2026-10-21"), OnDevice::Unopened),
        ],
        NOW,
    );
    assert_eq!((r.opened, r.unopened), (0, 1));
    assert!(
        s.reads
            .get(&d("2026-10-20"))
            .is_none_or(|m| m.get("K").is_none())
    );
    assert!(!s.reads[&d("2026-10-21")]["K"].opened);
}

#[test]
fn an_unknown_stem_is_ignored() {
    let mut s = State::default();
    let r = s.record_reads(
        "K",
        &[("someone-elses-book".into(), OnDevice::Opened(yjf(5)))],
        NOW,
    );
    assert_eq!((r.opened, r.unopened, r.errors.len()), (0, 0, 0));
    assert!(s.reads.is_empty());
}

#[test]
fn unopened_never_replaces_an_opened_read() {
    let mut s = State::default();
    s.record_layout(layout("2026-10-21", &[4]));
    s.record_reads(
        "K",
        &[(stem("2026-10-21"), OnDevice::Opened(yjf(700)))],
        NOW,
    );
    s.record_reads("K", &[(stem("2026-10-21"), OnDevice::Unopened)], NOW);
    assert_eq!(s.reads[&d("2026-10-21")]["K"].furthest, Some(700));
}

#[test]
fn one_bad_yjf_does_not_stop_the_others() {
    let mut s = State::default();
    s.record_layout(layout("2026-10-20", &[4]));
    s.record_layout(layout("2026-10-21", &[4]));
    let r = s.record_reads(
        "K",
        &[
            (stem("2026-10-20"), OnDevice::Opened(b"junk".to_vec())),
            (stem("2026-10-21"), OnDevice::Opened(yjf(50))),
        ],
        NOW,
    );
    assert_eq!(r.opened, 1);
    assert_eq!(r.errors.len(), 1);
    assert_eq!(r.errors[0].0, stem("2026-10-20"));
    assert_eq!(s.reads[&d("2026-10-21")]["K"].furthest, Some(50));
}

#[test]
fn a_read_keeps_the_page_history_and_old_files_load_without_it() {
    let mut s = State::default();
    s.record_layout(layout("2026-10-21", &[4, 1204]));
    let bytes = synthetic(&[
        obj(
            "fpr",
            &[utf("AQ:1300"), long(-1), int(-1), utf(""), utf("")],
        ),
        obj(
            "page.history.store",
            &[
                int(1),
                obj("page.history.record", &[utf("AQ:821"), long(1)]),
            ],
        ),
    ]);
    s.record_reads("K", &[(stem("2026-10-21"), OnDevice::Opened(bytes))], NOW);
    assert_eq!(s.reads[&d("2026-10-21")]["K"].history, [821]);
    let old: deckle_readership::state::Read =
        serde_json::from_str("{\"opened\":true,\"seen\":\"x\"}").unwrap();
    assert!(old.history.is_empty());
}

#[test]
fn renaming_a_section_key_moves_layouts_and_dismissals() {
    let mut s = State::default();
    let mut l = layout("2026-10-21", &[4, 1204, 3000]);
    l.sections[1].key = "interests:Rust".into();
    l.sections[2].key = "feeds:Rust".into();
    s.record_layout(l);
    s.dismissed
        .insert("drop:interests:Rust".into(), d("2026-10-21"));
    s.dismissed
        .insert("up:interests:Rust".into(), d("2026-10-21"));
    s.dismissed
        .insert("drop:feeds:Rust".into(), d("2026-10-21"));
    let n = s.rename_section_key("interests:Rust", "interests:Rust lang");
    assert_eq!(n, 3);
    let keys: Vec<&str> = s.editions[&d("2026-10-21")]
        .sections
        .iter()
        .map(|x| x.key.as_str())
        .collect();
    assert_eq!(keys, ["front", "interests:Rust lang", "feeds:Rust"]);
    assert!(s.dismissed.contains_key("drop:interests:Rust lang"));
    assert!(s.dismissed.contains_key("up:interests:Rust lang"));
    assert!(
        s.dismissed.contains_key("drop:feeds:Rust"),
        "another kind's key stays"
    );
    assert_eq!(s.rename_section_key("interests:Nobody", "x"), 0);
}
