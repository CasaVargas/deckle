// SPDX-License-Identifier: GPL-3.0-or-later
use deckle_core::inspect::{BookInfo, Creator};
use deckle_core::matcher::{MatchOutcome, isbn13_to_isbn10, match_book, open_library_url};

fn info(title: &str, author: &str, isbn: Option<&str>) -> BookInfo {
    BookInfo {
        title: title.into(),
        creators: vec![Creator {
            name: author.into(),
            file_as: None,
        }],
        language: "en".into(),
        isbn: isbn.map(str::to_owned),
        ..Default::default()
    }
}

#[test]
fn isbn13_converts_to_isbn10_with_check_digit() {
    assert_eq!(
        isbn13_to_isbn10("9780141439518").as_deref(),
        Some("0141439513")
    );
    assert_eq!(
        isbn13_to_isbn10("9780306406157").as_deref(),
        Some("0306406152")
    );
    assert_eq!(
        isbn13_to_isbn10("9781234567897").as_deref(),
        Some("123456789X")
    );
    assert_eq!(
        isbn13_to_isbn10("9790000000001"),
        None,
        "979 prefixes have no ISBN-10"
    );
    assert_eq!(
        isbn13_to_isbn10("0141439513").as_deref(),
        Some("0141439513"),
        "ISBN-10 passes through"
    );
    assert_eq!(isbn13_to_isbn10("nope"), None);
}

#[test]
fn a_book_with_an_isbn_needs_no_network() {
    let mut calls = 0;
    let r = match_book(
        &info("Pride and Prejudice", "Jane Austen", Some("9780141439518")),
        &mut |_url| {
            calls += 1;
            Err("offline".into())
        },
    );
    assert_eq!(calls, 0);
    match r {
        MatchOutcome::Candidate { asin, source, .. } => {
            assert_eq!(asin, "0141439513");
            assert_eq!(source, "isbn");
        }
        other => panic!("{other:?}"),
    }
}

#[test]
fn open_library_search_is_parsed_into_the_first_isbn10() {
    let url = open_library_url("Pride and Prejudice", "Jane Austen");
    assert!(
        url.starts_with("https://openlibrary.org/search.json?"),
        "{url}"
    );
    assert!(
        url.contains("title=Pride+and+Prejudice") && url.contains("author=Jane+Austen"),
        "{url}"
    );
    assert!(url.contains("fields=") && url.contains("isbn"), "{url}");
    let canned = r#"{"numFound":2,"docs":[{"key":"OL1W","title":"Pride and Prejudice","author_name":["Jane Austen"],"isbn":["9780141439518","0141439513","9781503290563"]}]}"#;
    let r = match_book(
        &info("Pride and Prejudice", "Jane Austen", None),
        &mut |u| {
            assert_eq!(u, url);
            Ok(canned.to_owned())
        },
    );
    match r {
        MatchOutcome::Candidate {
            asin,
            source,
            confidence,
        } => {
            assert_eq!(asin, "0141439513");
            assert_eq!(source, "openlibrary");
            assert!(confidence > 0.5);
        }
        other => panic!("{other:?}"),
    }
}

#[test]
fn no_results_or_network_failure_give_a_clean_outcome() {
    let r = match_book(&info("Unknown Book", "Nobody", None), &mut |_| {
        Ok(r#"{"numFound":0,"docs":[]}"#.into())
    });
    assert!(matches!(r, MatchOutcome::NotFound));
    let r = match_book(&info("Unknown Book", "Nobody", None), &mut |_| {
        Err("timeout".into())
    });
    match r {
        MatchOutcome::Error(e) => assert!(e.contains("timeout")),
        other => panic!("{other:?}"),
    }
}
