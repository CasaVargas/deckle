// SPDX-License-Identifier: GPL-3.0-or-later
//! The puzzle page: each day's puzzle, made once and kept (puzzle design §3, §7).

use chrono::{Days, NaiveDate};
use deckle_digest::config::{Config, PuzzleConfig, STARTER, SectionKind};
use deckle_digest::puzzle::store::{Day, Store};
use deckle_digest::puzzle::theme::ThemeWord;
use deckle_digest::puzzle::words::Words;
use deckle_digest::puzzle::{make, today};

fn date() -> NaiveDate {
    NaiveDate::from_ymd_opt(2026, 10, 22).unwrap()
}

fn yesterday() -> NaiveDate {
    date() - Days::new(1)
}

fn leaf() -> ThemeWord {
    ThemeWord {
        word: "leaf".into(),
        headline: "Leaf season peaks on the ridge this weekend".into(),
        section: "Local".into(),
    }
}

/// A store holding yesterday, with its headlines.
fn store_with_yesterday(headlines: Vec<ThemeWord>) -> Store {
    let mut store = Store::default();
    let cfg = PuzzleConfig::default();
    let day = Day {
        puzzle: make(yesterday(), &cfg, Words::embedded(), &store),
        headlines,
        solve: None,
    };
    store.days.insert(yesterday(), day);
    store
}

#[test]
fn same_day_twice_is_identical() {
    let mut store = Store::default();
    let cfg = PuzzleConfig::default();
    let first = today(date(), &cfg, Words::embedded(), &mut store).clone();
    let second = today(date(), &cfg, Words::embedded(), &mut store).clone();
    assert_eq!(first, second);
    assert!(first.puzzle.rows.is_some());
    assert!(!first.puzzle.clues.is_empty());
    assert!(first.puzzle.ladder.is_some());
}

#[test]
fn stored_puzzle_survives_a_new_word_list() {
    // a puzzle made from another list (an app update mid-day) is the one printed
    let small = Words::from_tsv("cold\tx\ncord\tx\nword\tx\nward\tx\nwarm\tx\n");
    let mut store = Store::default();
    let cfg = PuzzleConfig::default();
    let made = today(date(), &cfg, &small, &mut store).clone();
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("puzzles.json");
    store.save(&path).unwrap();
    let mut store = Store::load(&path);
    let again = today(date(), &cfg, Words::embedded(), &mut store).clone();
    assert_eq!(made, again);
    assert_eq!(
        again.puzzle.rows, None,
        "the small list couldn't fill a mini"
    );
}

#[test]
fn lost_store_regenerates_the_same() {
    let store = store_with_yesterday(vec![leaf()]);
    let cfg = PuzzleConfig::default();
    assert_eq!(
        make(date(), &cfg, Words::embedded(), &store),
        make(date(), &cfg, Words::embedded(), &store)
    );
}

#[test]
fn theme_clues_come_from_yesterday() {
    let store = store_with_yesterday(vec![leaf()]);
    let p = make(date(), &PuzzleConfig::default(), Words::embedded(), &store);
    let themed: Vec<_> = p.clues.iter().filter(|c| c.theme).collect();
    assert_eq!(themed.len(), 1, "{:?}", p.clues);
    assert_eq!(themed[0].answer, "leaf");
    assert_eq!(
        themed[0].clue,
        "____ season peaks on the ridge this weekend (Local, yesterday)"
    );
    // the rest are clued from the dictionary
    for c in p.clues.iter().filter(|c| !c.theme) {
        assert_eq!(Some(c.clue.as_str()), Words::embedded().clue(&c.answer));
    }
}

#[test]
fn no_theme_when_off() {
    let store = store_with_yesterday(vec![leaf()]);
    let cfg = PuzzleConfig {
        theme: false,
        ..PuzzleConfig::default()
    };
    let p = make(date(), &cfg, Words::embedded(), &store);
    assert!(p.clues.iter().all(|c| !c.theme));
}

#[test]
fn no_ladder_when_off() {
    let cfg = PuzzleConfig {
        ladder: false,
        ..PuzzleConfig::default()
    };
    let p = make(date(), &cfg, Words::embedded(), &Store::default());
    assert_eq!(p.ladder, None);
    assert!(p.rows.is_some());
}

#[test]
fn numbers_count_up() {
    let mut store = Store::default();
    let cfg = PuzzleConfig::default();
    assert_eq!(
        today(yesterday(), &cfg, Words::embedded(), &mut store)
            .puzzle
            .number,
        1
    );
    assert_eq!(
        today(date(), &cfg, Words::embedded(), &mut store)
            .puzzle
            .number,
        2
    );
}

#[test]
fn ladders_dont_repeat_within_sixty_days() {
    let mut store = Store::default();
    let cfg = PuzzleConfig {
        theme: false,
        ..PuzzleConfig::default()
    };
    let mut seen = Vec::new();
    for d in 0..60 {
        let day = today(date() + Days::new(d), &cfg, Words::embedded(), &mut store);
        let l = day.puzzle.ladder.clone().unwrap();
        assert!(!seen.contains(&(l.from.clone(), l.to.clone())), "day {d}");
        seen.push((l.from, l.to));
    }
}

#[test]
fn prune_keeps_sixty_days() {
    let mut store = Store::default();
    let cfg = PuzzleConfig {
        ladder: false,
        ..PuzzleConfig::default()
    };
    let small = Words::from_tsv("cold\tx\n");
    for d in 0..70 {
        today(date() + Days::new(d), &cfg, &small, &mut store);
    }
    let last = date() + Days::new(69);
    store.prune(last);
    assert_eq!(store.days.len(), 60);
    assert_eq!(store.days.keys().next(), Some(&(last - Days::new(59))));
}

#[test]
fn store_round_trips_and_a_bad_file_loads_empty() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("puzzles.json");
    assert_eq!(Store::load(&path), Store::default());
    let mut store = store_with_yesterday(vec![leaf()]);
    store.notes_readable = Some(false);
    store.save(&path).unwrap();
    assert_eq!(Store::load(&path), store);
    std::fs::write(&path, "{ not json").unwrap();
    assert_eq!(Store::load(&path), Store::default());
}

#[test]
fn starter_has_puzzle_before_onthisday() {
    let c = Config::parse(STARTER).unwrap();
    let at = |k| c.sections.iter().position(|s| *s == k).unwrap();
    assert_eq!(at(SectionKind::Puzzle), at(SectionKind::Horoscope) + 1);
    assert_eq!(at(SectionKind::Onthisday), at(SectionKind::Puzzle) + 1);
    assert_eq!(c.puzzle, PuzzleConfig::default());
    assert!(c.puzzle.ladder && c.puzzle.theme);
    let d = Config::default();
    assert_eq!(d.sections, c.sections);
    assert_eq!(
        deckle_digest::config::section_key(SectionKind::Puzzle, None),
        "puzzle"
    );
}

#[test]
fn untouched_config_saves_no_puzzle_table() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("digest.toml");
    // the starter mentions it only in a comment; a save writes `[puzzle]` or `puzzle = {…}`
    let table = |text: &str| {
        text.lines()
            .any(|l| l.trim() == "[puzzle]" || l.starts_with("puzzle ="))
    };
    let mut c = Config::parse(STARTER).unwrap();
    deckle_digest::config::save(&path, &c).unwrap();
    assert!(!table(&std::fs::read_to_string(&path).unwrap()));
    c.puzzle.ladder = false;
    deckle_digest::config::save(&path, &c).unwrap();
    let text = std::fs::read_to_string(&path).unwrap();
    assert!(table(&text), "{text}");
    assert!(!Config::load(&path).unwrap().puzzle.ladder);
}

#[test]
fn the_store_sits_beside_digest_toml() {
    let c = Config::default();
    assert_eq!(c.puzzle_store().file_name().unwrap(), "puzzles.json");
    assert_eq!(
        c.puzzle_store().parent(),
        deckle_digest::config::default_path().parent()
    );
}
