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

// --- the section on the page (Task 7) ---

use deckle_digest::puzzle::section;
use deckle_digest::puzzle::store::Solve;
use deckle_digest::{Edition, Gathered, Section, render};

fn fixture_words() -> Words {
    Words::from_tsv(include_str!("fixtures/puzzle/words.tsv"))
}

fn page_cfg() -> Config {
    Config::default()
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

/// Yesterday made from the fixture list, solved 4 of its clues, with "leaf" in its headlines.
fn solved_yesterday(words: &Words) -> Store {
    let mut store = Store::default();
    let cfg = PuzzleConfig::default();
    let mut day = Day {
        puzzle: make(yesterday(), &cfg, words, &store),
        headlines: vec![leaf()],
        solve: None,
    };
    day.solve = Some(Solve {
        solved: 4,
        of: day.puzzle.clues.len(),
        ladder: false,
    });
    store.days.insert(yesterday(), day);
    store
}

#[test]
fn golden_page() {
    let words = fixture_words();
    let mut store = solved_yesterday(&words);
    let s = section(date(), &page_cfg(), &mut store, &words);
    assert!(s.unavailable.is_none());
    let got = xhtml(s);
    let path = concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/tests/fixtures/puzzle/golden-page.xhtml"
    );
    if std::env::var_os("UPDATE_GOLDEN").is_some() {
        std::fs::write(path, &got).unwrap();
    }
    assert_eq!(got, std::fs::read_to_string(path).unwrap());
    roxmltree::Document::parse(&got).expect("well-formed XHTML");
    // yesterday's small grid is letters only: numbers would crowd its cells out of square
    let small = got.split("<table class=\"grid small\">").nth(1).unwrap();
    let small = small.split("</table>").next().unwrap();
    assert!(!small.contains("class=\"n\""), "{small}");
    let of = store.days[&yesterday()].puzzle.clues.len();
    for want in [
        "Answers in tomorrow&#39;s paper",
        "The Mini · No. 2",
        "Word ladder",
        "Yesterday&#39;s answers",
        "Highlight a clue and add your answer as a note; tomorrow&#39;s paper checks it.",
        &format!("You solved 4 of {of}"),
        "Words and definitions from Wiktionary and wordfreq (CC BY-SA 4.0)",
        "(Local, yesterday)",
        "<table class=\"grid\">",
        "<table class=\"grid small\">",
        "<td class=\"b\"></td>",
        "<span class=\"n\">1</span>",
        "<h3 class=\"subhead\">Across</h3>",
        "<h3 class=\"subhead\">Down</h3>",
    ] {
        assert!(got.contains(want), "missing {want:?}");
    }
}

#[test]
fn no_yesterday_no_answers() {
    // two days ago is stored, yesterday isn't: no stale answers
    let words = fixture_words();
    let mut store = Store::default();
    let cfg = PuzzleConfig::default();
    today(date() - Days::new(2), &cfg, &words, &mut store);
    let s = section(date(), &page_cfg(), &mut store, &words);
    let titles: Vec<&str> = s.articles.iter().map(|a| a.title.as_str()).collect();
    assert_eq!(titles, ["The Mini · No. 2", "Word ladder"]);
    assert!(xhtml(s).contains("Wiktionary and wordfreq"));
}

#[test]
fn hint_hidden_when_notes_unreadable() {
    let words = fixture_words();
    let mut store = Store {
        notes_readable: Some(false),
        ..Store::default()
    };
    let s = section(date(), &page_cfg(), &mut store, &words);
    assert!(!xhtml(s).contains("Highlight a clue"));
    store.notes_readable = Some(true);
    let s = section(date(), &page_cfg(), &mut store, &words);
    assert!(xhtml(s).contains("Highlight a clue"));
}

#[test]
fn fill_failure_keeps_the_ladder() {
    let words = Words::from_tsv("cold\tx\ncord\tx\nword\tx\nward\tx\nwarm\tx\n");
    let s = section(date(), &page_cfg(), &mut Store::default(), &words);
    let titles: Vec<&str> = s.articles.iter().map(|a| a.title.as_str()).collect();
    assert_eq!(titles, ["Word ladder"]);
    assert!(
        s.articles[0]
            .body
            .contains("COLD → ____ → ____ → ____ → WARM")
    );
}

#[test]
fn everything_fails_unavailable() {
    let s = section(
        date(),
        &page_cfg(),
        &mut Store::default(),
        &Words::default(),
    );
    assert!(s.unavailable.is_some());
    assert!(s.articles.is_empty());
    assert_eq!((s.key.as_str(), s.title.as_str()), ("puzzle", "Puzzle"));
}

#[test]
fn yesterdays_ladder_solved_says_so() {
    let words = fixture_words();
    let mut store = solved_yesterday(&words);
    let y = store.days.get_mut(&yesterday()).unwrap();
    y.solve.as_mut().unwrap().ladder = true;
    let s = section(date(), &page_cfg(), &mut store, &words);
    let answers = s
        .articles
        .iter()
        .find(|a| a.title == "Yesterday's answers")
        .unwrap();
    assert!(answers.body.contains("Ladder: solved"), "{}", answers.body);
}

// --- checking answers from Kindle notes (Task 8) ---

use deckle_digest::puzzle::check::{record, score};
use deckle_digest::puzzle::grid::Dir;
use deckle_digest::puzzle::ladder::Ladder;
use deckle_digest::puzzle::store::{Clue, Puzzle};

const CLIPPINGS: &[u8] = include_bytes!("fixtures/puzzle/clippings.txt");

fn clue(number: u8, dir: Dir, answer: &str, text: &str) -> Clue {
    Clue {
        number,
        dir,
        answer: answer.into(),
        clue: text.into(),
        theme: false,
    }
}

/// The 2026-10-22 puzzle the fixture's notes answer.
fn checked_puzzle() -> Puzzle {
    Puzzle {
        number: 41,
        rows: Some([
            "CARD#".into(),
            "O...#".into(),
            "R....".into(),
            "D....".into(),
            "#....".into(),
        ]),
        clues: vec![
            clue(1, Dir::Across, "card", "A flat piece of stiff paper"),
            clue(
                6,
                Dir::Across,
                "area",
                "Any particular extent of space or surface",
            ),
            clue(7, Dir::Across, "leaf", "A green organ of a plant"),
            clue(1, Dir::Down, "cord", "A long thin rope"),
            clue(2, Dir::Down, "tree", "A tall woody plant"),
        ],
        ladder: Some(Ladder {
            from: "cold".into(),
            to: "warm".into(),
            path: ["cold", "cord", "word", "ward", "warm"]
                .map(String::from)
                .to_vec(),
        }),
    }
}

fn entries(bytes: &[u8]) -> Vec<deckle_highlights::parse::Entry> {
    deckle_highlights::parse::parse(bytes).entries
}

#[test]
fn notes_score_the_edition() {
    // 1A right; 6A wrong ("AREX"); 7A wrong then right (the latest wins, " Leaf. " counts);
    // 1D right from the renamed paper ("c o r d"); 2D answered only on yesterday's edition;
    // a note on a headline is no answer
    let s = score(&checked_puzzle(), date(), &entries(CLIPPINGS)).unwrap();
    assert_eq!((s.solved, s.of, s.ladder), (3, 5, true));
}

#[test]
fn another_editions_notes_dont_count() {
    assert_eq!(
        score(
            &checked_puzzle(),
            yesterday() - Days::new(1),
            &entries(CLIPPINGS)
        ),
        None
    );
    // yesterday's own edition has one note, on a clue today's puzzle has
    let s = score(&checked_puzzle(), yesterday(), &entries(CLIPPINGS)).unwrap();
    assert_eq!((s.solved, s.ladder), (1, false));
}

#[test]
fn a_short_highlight_inside_a_clue_counts() {
    let text = "\u{feff}The Deckle Daily — Thursday, October 22, 2026 (Deckle)\r\n- Your Highlight on Location 5-6 | Added on Thursday, October 22, 2026 8:00:00 AM\r\n\r\nextent of space\r\n==========\r\nThe Deckle Daily — Thursday, October 22, 2026 (Deckle)\r\n- Your Note on Location 6 | Added on Thursday, October 22, 2026 8:00:00 AM\r\n\r\narea\r\n==========\r\nThe Deckle Daily — Thursday, October 22, 2026 (Deckle)\r\n- Your Highlight on Location 9-9 | Added on Thursday, October 22, 2026 8:00:00 AM\r\n\r\ntall\r\n==========\r\nThe Deckle Daily — Thursday, October 22, 2026 (Deckle)\r\n- Your Note on Location 9 | Added on Thursday, October 22, 2026 8:00:00 AM\r\n\r\ntree\r\n==========\r\n";
    // 12+ letters of a clue name it; "tall" alone could be anything
    let s = score(&checked_puzzle(), date(), &entries(text.as_bytes())).unwrap();
    assert_eq!(s.solved, 1);
}

#[test]
fn record_scores_recent_days_and_marks_readable() {
    let mut store = Store::default();
    store.days.insert(
        date(),
        Day {
            puzzle: checked_puzzle(),
            headlines: Vec::new(),
            solve: None,
        },
    );
    record(&mut store, Some(CLIPPINGS), date() + Days::new(1));
    assert_eq!(store.notes_readable, Some(true));
    let s = store.days[&date()].solve.clone().unwrap();
    assert_eq!((s.solved, s.of), (3, 5));
    // a Kindle without readable notes leaves the solves alone
    record(&mut store, None, date() + Days::new(1));
    assert_eq!(store.notes_readable, Some(false));
    assert!(store.days[&date()].solve.is_some());
    // nor do notes that say nothing about it (another Kindle, a cleared file)
    record(&mut store, Some(b""), date() + Days::new(1));
    assert!(store.days[&date()].solve.is_some());
    // more than a week back isn't looked at
    let mut old = Store::default();
    old.days.insert(
        date(),
        Day {
            puzzle: checked_puzzle(),
            headlines: Vec::new(),
            solve: None,
        },
    );
    record(&mut old, Some(CLIPPINGS), date() + Days::new(8));
    assert_eq!(old.days[&date()].solve, None);
}
