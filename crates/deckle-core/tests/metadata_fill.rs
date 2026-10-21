// SPDX-License-Identifier: GPL-3.0-or-later
//! The Open Library metadata fill (issue #28), offline: saved Open Library records stand in for
//! the network.
use deckle_core::inspect::{BookInfo, Creator, NO_TITLE_WARNING, Series, inspect};
use deckle_core::metadata_fill::{
    FillOutcome, Metadata, Missing, edition_url, fill, lookup, parse_series, search_url,
};
use deckle_core::normalize::{NormalizeOptions, normalize};

const ISBN: &str = "9780547928210";
const EDITION: &str = include_str!("data/openlibrary/isbn-9780547928210.json");
const WORK: &str = include_str!("data/openlibrary/works-OL27448W.json");
const AUTHOR: &str = include_str!("data/openlibrary/authors-OL26320A.json");
const SEARCH: &str = include_str!("data/openlibrary/search-fellowship.json");

/// Answers the saved records by URL and logs every request.
fn saved(log: &mut Vec<String>) -> impl FnMut(&str) -> Result<String, String> + '_ {
    move |url: &str| {
        log.push(url.to_owned());
        match url {
            u if u == edition_url(ISBN) => Ok(EDITION.into()),
            "https://openlibrary.org/works/OL27448W.json" => Ok(WORK.into()),
            "https://openlibrary.org/authors/OL26320A.json" => Ok(AUTHOR.into()),
            u if u.starts_with("https://openlibrary.org/search.json?") => Ok(SEARCH.into()),
            u => Err(format!("http status: 404 for {u}")),
        }
    }
}

fn panicking(url: &str) -> Result<String, String> {
    panic!("no request expected, got {url}")
}

/// A book with an ISBN and nothing else Open Library could fill; the title came from the file.
fn bare(isbn: Option<&str>) -> BookInfo {
    BookInfo {
        title: "fellowship".into(),
        language: "en".into(),
        isbn: isbn.map(str::to_owned),
        warnings: vec![NO_TITLE_WARNING.into()],
        ..Default::default()
    }
}

#[test]
fn the_saved_isbn_records_give_title_author_series_description_and_subjects() {
    let mut log = Vec::new();
    let info = bare(Some(ISBN));
    let (md, source) = lookup(&info, Missing::of(&info), &mut saved(&mut log))
        .unwrap()
        .expect("found");
    assert_eq!(source, "isbn");
    assert_eq!(md.title.as_deref(), Some("The Fellowship of the Ring"));
    assert_eq!(md.authors, vec!["J.R.R. Tolkien".to_owned()]);
    assert_eq!(
        md.series,
        Some(Series {
            name: "The Lord of the Rings".into(),
            index: Some(1.0),
        })
    );
    assert!(
        md.description
            .as_deref()
            .unwrap()
            .starts_with("Originally published from 1954"),
        "{md:?}"
    );
    assert!(md.subjects.contains(&"Hobbits".to_owned()), "{md:?}");
    assert_eq!(
        md.subjects.iter().filter(|s| *s == "Fiction").count(),
        1,
        "deduplicated: {:?}",
        md.subjects
    );
    assert!(md.subjects.len() <= 10, "{:?}", md.subjects);
    assert_eq!(
        log,
        vec![
            edition_url(ISBN),
            "https://openlibrary.org/works/OL27448W.json".to_owned(),
            "https://openlibrary.org/authors/OL26320A.json".to_owned(),
        ]
    );
}

#[test]
fn fill_writes_every_empty_field_and_drops_the_file_name_title_warning() {
    let mut log = Vec::new();
    let mut info = bare(Some(ISBN));
    let (outcome, wrote) = fill(&mut info, &mut saved(&mut log));
    assert_eq!(
        outcome,
        FillOutcome::Filled {
            fields: vec!["title", "author", "series", "description", "subjects"],
            source: "isbn",
        }
    );
    assert_eq!(info.title, "The Fellowship of the Ring");
    assert_eq!(info.creators[0].name, "J.R.R. Tolkien");
    assert_eq!(info.series.as_ref().unwrap().index, Some(1.0));
    assert!(info.description.is_some());
    assert!(!info.subjects.is_empty());
    assert!(!info.warnings.iter().any(|w| w == NO_TITLE_WARNING));
    assert_eq!(wrote.title.as_deref(), Some("The Fellowship of the Ring"));
}

#[test]
fn an_existing_description_and_the_books_own_metadata_are_kept() {
    let mut log = Vec::new();
    let mut info = BookInfo {
        title: "My Fellowship".into(),
        creators: vec![Creator {
            name: "Me".into(),
            file_as: None,
        }],
        language: "en".into(),
        isbn: Some(ISBN.into()),
        description: Some("The owner's own blurb.".into()),
        ..Default::default()
    };
    let (outcome, wrote) = fill(&mut info, &mut saved(&mut log));
    assert_eq!(
        outcome,
        FillOutcome::Filled {
            fields: vec!["series", "subjects"],
            source: "isbn",
        }
    );
    assert_eq!(info.description.as_deref(), Some("The owner's own blurb."));
    assert_eq!(info.title, "My Fellowship");
    assert_eq!(info.creators[0].name, "Me");
    assert_eq!(wrote.description, None);
    assert_eq!(wrote.title, None);
    assert!(wrote.authors.is_empty());
    assert!(
        !log.iter().any(|u| u.contains("/authors/")),
        "authors are not looked up when the book has them: {log:?}"
    );
}

#[test]
fn a_book_with_every_field_makes_no_request() {
    let mut info = BookInfo {
        title: "Done".into(),
        creators: vec![Creator {
            name: "A".into(),
            file_as: None,
        }],
        isbn: Some(ISBN.into()),
        series: Some(Series {
            name: "S".into(),
            index: None,
        }),
        description: Some("D".into()),
        subjects: vec!["X".into()],
        ..Default::default()
    };
    let before = info.clone();
    let (outcome, wrote) = fill(&mut info, &mut panicking);
    assert_eq!(outcome, FillOutcome::Complete);
    assert!(wrote.is_empty());
    assert_eq!(info, before);
}

#[test]
fn a_network_failure_or_unreadable_json_is_an_error_outcome_and_changes_nothing() {
    for body in [
        Err("Connection refused".to_owned()),
        Ok("<html>Service Unavailable</html>".to_owned()),
    ] {
        let mut info = bare(Some(ISBN));
        let before = info.clone();
        let (outcome, wrote) = fill(&mut info, &mut |_: &str| body.clone());
        assert!(
            matches!(&outcome, FillOutcome::Error { message } if !message.is_empty()),
            "{outcome:?}"
        );
        assert!(wrote.is_empty());
        assert_eq!(info, before);
    }
    // A failure on the second request (the work) is an error too, not a half fill.
    let mut info = bare(Some(ISBN));
    let (outcome, _) = fill(&mut info, &mut |u: &str| {
        if u == edition_url(ISBN) {
            Ok(EDITION.to_owned())
        } else {
            Err("timed out".to_owned())
        }
    });
    assert_eq!(
        outcome,
        FillOutcome::Error {
            message: "timed out".into()
        }
    );
}

#[test]
fn without_an_isbn_a_title_search_finds_the_work() {
    let mut log = Vec::new();
    let mut info = BookInfo {
        title: "The Fellowship of the Ring".into(),
        creators: vec![Creator {
            name: "J.R.R. Tolkien".into(),
            file_as: None,
        }],
        ..Default::default()
    };
    let (outcome, _) = fill(&mut info, &mut saved(&mut log));
    assert_eq!(
        outcome,
        FillOutcome::Filled {
            fields: vec!["description", "subjects"],
            source: "search",
        }
    );
    assert_eq!(
        log[0],
        search_url("The Fellowship of the Ring", "J.R.R. Tolkien")
    );
    assert!(
        log[0].contains("title=The+Fellowship+of+the+Ring"),
        "{}",
        log[0]
    );
    assert_eq!(log[1], "https://openlibrary.org/works/OL27448W.json");
    assert!(info.subjects.contains(&"Hobbits".to_owned()));
}

#[test]
fn an_isbn_open_library_does_not_know_falls_back_to_the_title() {
    let mut log = Vec::new();
    let mut info = BookInfo {
        title: "The Fellowship of the Ring".into(),
        isbn: Some("9780000000002".into()),
        ..Default::default()
    };
    let (outcome, _) = fill(&mut info, &mut saved(&mut log));
    assert!(
        matches!(
            outcome,
            FillOutcome::Filled {
                source: "search",
                ..
            }
        ),
        "{outcome:?}"
    );
    assert_eq!(log[0], edition_url("9780000000002"));
}

#[test]
fn a_search_without_a_matching_title_is_not_found() {
    let mut info = BookInfo {
        title: "Something Else Entirely".into(),
        ..Default::default()
    };
    let (outcome, wrote) = fill(&mut info, &mut |_: &str| Ok(SEARCH.to_owned()));
    assert_eq!(outcome, FillOutcome::NotFound);
    assert!(wrote.is_empty());
}

#[test]
fn series_strings_split_into_name_and_index() {
    let s = |n: &str, i: Option<f32>| {
        Some(Series {
            name: n.into(),
            index: i,
        })
    };
    assert_eq!(
        parse_series("The Lord of the Rings ; 1"),
        s("The Lord of the Rings", Some(1.0))
    );
    assert_eq!(parse_series("Discworld, #3"), s("Discworld", Some(3.0)));
    assert_eq!(parse_series("(Dune -- bk. 2)"), s("Dune", Some(2.0)));
    assert_eq!(
        parse_series("Foundation, vol. 1.5"),
        s("Foundation", Some(1.5))
    );
    assert_eq!(
        parse_series("Penguin Classics"),
        s("Penguin Classics", None)
    );
    assert_eq!(parse_series("Catch-22"), s("Catch-22", None));
    assert_eq!(parse_series("  "), None);
}

#[test]
fn the_fill_lands_in_the_normalized_opf_where_inspect_reads_it_back() {
    let src = deckle_kfx::fixtures::path("deckle-mini.epub");
    let out = tempfile::Builder::new().suffix(".epub").tempfile().unwrap();
    let opts = NormalizeOptions {
        fill: Metadata {
            description: Some("A <small> book & more.".into()),
            subjects: vec!["Testing".into(), "Fixtures".into()],
            ..Default::default()
        },
        ..Default::default()
    };
    let report = normalize(&src, out.path(), &opts).unwrap();
    assert!(report.warnings.is_empty(), "{:?}", report.warnings);
    let info = inspect(out.path()).unwrap();
    assert_eq!(info.description.as_deref(), Some("A <small> book & more."));
    assert_eq!(info.subjects, vec!["Testing".to_owned(), "Fixtures".into()]);
    // The book's own metadata is untouched.
    assert_eq!(info.title, "Deckle Mini Fixture");
    assert_eq!(info.series.unwrap().name, "Fixture Series");
    // Without a fill, normalize adds nothing.
    let plain = tempfile::Builder::new().suffix(".epub").tempfile().unwrap();
    normalize(&src, plain.path(), &NormalizeOptions::default()).unwrap();
    let info = inspect(plain.path()).unwrap();
    assert_eq!(info.description, None);
    assert!(info.subjects.is_empty());
}

#[test]
fn a_filled_author_round_trips_through_the_opf() {
    let src = deckle_kfx::fixtures::path("deckle-mini.epub");
    let out = tempfile::Builder::new().suffix(".epub").tempfile().unwrap();
    let opts = NormalizeOptions {
        fill: Metadata {
            authors: vec!["Second Author".into()],
            ..Default::default()
        },
        ..Default::default()
    };
    normalize(&src, out.path(), &opts).unwrap();
    let info = inspect(out.path()).unwrap();
    assert!(
        info.creators.iter().any(|c| c.name == "Second Author"),
        "{:?}",
        info.creators
    );
}
