// SPDX-License-Identifier: GPL-3.0-or-later
//! The puzzle book's PDF (puzzle books §4), read back through PDFium.

use deckle_digest::chrono::NaiveDate;
use deckle_digest::pdf::Screen;
use deckle_digest::puzzle::book::pdf::{PageSize, pages, render};
use deckle_digest::puzzle::book::{Book, BookError, BookOptions, Difficulty, Kind, make};
use deckle_digest::puzzle::words::Words;

fn book(count: usize, size: usize, difficulty: Difficulty) -> Book {
    make(
        &BookOptions {
            kind: Kind::Search,
            count,
            size,
            difficulty,
            words: None,
            title: None,
            seed: Some(7),
            date: NaiveDate::from_ymd_opt(2026, 11, 1).unwrap(),
        },
        Words::embedded(),
    )
    .unwrap()
}

fn flat(text: String) -> String {
    text.split_whitespace().collect::<Vec<_>>().join(" ")
}

#[test]
fn book_reads_back() {
    let b = book(4, 12, Difficulty::Medium);
    let bytes = render(&b, PageSize::KindleScribe).unwrap();
    let pdfium = deckle_pdf::engine::pdfium().expect("PDFium (tools/fetch-pdfium.sh)");
    let doc = pdfium.load_pdf_from_byte_slice(&bytes, None).unwrap();
    let pages = doc.pages();
    assert_eq!(
        pages.len(),
        1 + 4 + 1,
        "title, four puzzles, one page of answers"
    );
    let p = pages.get(0).unwrap();
    assert!((p.width().value - 446.4).abs() < 1.0, "{}", p.width().value);
    assert!(
        (p.height().value - 595.2).abs() < 1.0,
        "{}",
        p.height().value
    );
    let title = flat(p.text().unwrap().all());
    assert!(title.contains("Word Search"), "{title}");
    assert!(title.contains("12×12 · medium · 4 puzzles"), "{title}");
    assert!(title.contains("1 November 2026"), "{title}");
    let first = flat(pages.get(1).unwrap().text().unwrap().all());
    assert!(first.contains("No. 1"), "{first}");
    for h in &b.searches()[0].words {
        assert!(
            first.contains(&h.word.to_uppercase()),
            "{} in {first}",
            h.word
        );
    }
    assert!(first.contains("Answers on page 6"), "{first}");
    let answers = flat(pages.get(5).unwrap().text().unwrap().all());
    assert!(
        answers.contains("Answers") && answers.contains("No. 4"),
        "{answers}"
    );
    assert_eq!(doc.bookmarks().iter().count(), 1 + 4 + 1);
}

#[test]
fn same_book_same_bytes() {
    let a = render(&book(2, 10, Difficulty::Hard), PageSize::KindleScribe).unwrap();
    let b = render(&book(2, 10, Difficulty::Hard), PageSize::KindleScribe).unwrap();
    assert_eq!(a, b);
}

/// A pen stroke on a link would jump away mid-solve (puzzle books §2).
#[test]
fn no_links_on_any_page() {
    let bytes = render(&book(5, 12, Difficulty::Medium), PageSize::KindleScribe).unwrap();
    let doc = deckle_pdf::engine::pdfium()
        .unwrap()
        .load_pdf_from_byte_slice(&bytes, None)
        .unwrap();
    for page in doc.pages().iter() {
        assert_eq!(page.annotations().len(), 0);
    }
    assert!(!bytes.windows(7).any(|w| w == b"/Annots"));
}

#[test]
fn size_20_hard_fits_the_scribe() {
    let b = book(2, 20, Difficulty::Hard);
    assert!(b.searches().iter().all(|p| p.words.len() >= 32));
    let bytes = render(&b, PageSize::KindleScribe).unwrap();
    let doc = deckle_pdf::engine::pdfium()
        .unwrap()
        .load_pdf_from_byte_slice(&bytes, None)
        .unwrap();
    let page = flat(doc.pages().get(1).unwrap().text().unwrap().all());
    for h in &b.searches()[0].words {
        assert!(page.contains(&h.word.to_uppercase()), "{}", h.word);
    }
}

/// Never a grid too fine to write in: a small screen refuses it and names the largest that
/// fits, which then renders (puzzle books §4.2).
#[test]
fn a_small_screen_refuses_a_big_grid() {
    let small = PageSize::Screen(Screen::RemarkablePaperProMove);
    let Err(BookError::DoesNotFit { size, largest, .. }) =
        render(&book(1, 20, Difficulty::Medium), small)
    else {
        panic!("a 20×20 grid fits a Paper Pro Move");
    };
    assert_eq!(size, 20);
    assert!((8..20).contains(&largest), "{largest}");
    assert!(render(&book(1, largest, Difficulty::Medium), small).is_ok());
    assert!(matches!(
        render(&book(1, largest + 1, Difficulty::Medium), small),
        Err(BookError::DoesNotFit { .. })
    ));
}

fn crosswords(count: usize, size: usize) -> Book {
    make(
        &BookOptions {
            kind: Kind::Crossword,
            count,
            size,
            difficulty: Difficulty::Medium,
            words: None,
            title: None,
            seed: Some(7),
            date: NaiveDate::from_ymd_opt(2026, 11, 1).unwrap(),
        },
        Words::embedded(),
    )
    .unwrap()
}

#[test]
fn crossword_book_reads_back() {
    let b = crosswords(3, 11);
    let bytes = render(&b, PageSize::KindleScribe).unwrap();
    let doc = deckle_pdf::engine::pdfium()
        .unwrap()
        .load_pdf_from_byte_slice(&bytes, None)
        .unwrap();
    assert_eq!(
        doc.pages().len() as usize,
        pages(&b, PageSize::KindleScribe)
    );
    let first = flat(doc.pages().get(1).unwrap().text().unwrap().all());
    assert!(first.starts_with("No. 1"), "{first}");
    assert!(first.contains("Across"), "{first}");
    assert_eq!(doc.bookmarks().iter().count(), 1 + 3 + 1);
    // every clue prints, on the puzzle's page or the one after
    let all: String = doc
        .pages()
        .iter()
        .map(|p| flat(p.text().unwrap().all()))
        .collect::<Vec<_>>()
        .join(" ");
    assert!(all.contains("Down"), "{all}");
    for c in &b.crosswords()[0].clues {
        let start: String = c
            .clue
            .split_whitespace()
            .take(3)
            .collect::<Vec<_>>()
            .join(" ");
        assert!(all.contains(&start), "{c:?}");
    }
    assert!(!bytes.windows(7).any(|w| w == b"/Annots"));
}

/// A 15×15's seventy-odd clues don't fit under its grid: they carry on over the page, which
/// says whose clues they are, and the answers' page number counts it.
#[test]
fn fifteen_carries_its_clues_on_to_the_next_page() {
    let b = crosswords(1, 15);
    let bytes = render(&b, PageSize::KindleScribe).unwrap();
    let doc = deckle_pdf::engine::pdfium()
        .unwrap()
        .load_pdf_from_byte_slice(&bytes, None)
        .unwrap();
    let n = doc.pages().len() as usize;
    assert!(n >= 4, "title, two pages of puzzle, answers: {n}");
    let second = flat(doc.pages().get(2).unwrap().text().unwrap().all());
    assert!(second.starts_with("No. 1 (continued)"), "{second}");
    assert!(second.contains(&format!("Answers on page {n}")), "{second}");
}

#[test]
fn same_crossword_book_same_bytes() {
    let a = render(&crosswords(2, 9), PageSize::KindleScribe).unwrap();
    let b = render(&crosswords(2, 9), PageSize::KindleScribe).unwrap();
    assert_eq!(a, b);
}
