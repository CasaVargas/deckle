// SPDX-License-Identifier: GPL-3.0-or-later
mod common;

use common::*;
use deckle_readership::{parse_yjf, position_id};

#[test]
fn reads_furthest_and_last_from_a_real_scribe_yjf() {
    let p = parse_yjf(&fixture("daily-2026-10-21.yjf")).unwrap();
    assert_eq!(p.furthest, Some(103303));
    assert_eq!(p.last, Some(103303));
    // 22:06:30 on the Kindle (US Eastern, EDT)
    assert_eq!(p.last_read.as_deref(), Some("2026-10-22T02:06:30Z"));
}

#[test]
fn furthest_can_be_ahead_of_last() {
    let p = parse_yjf(&fixture("daily-2026-10-20.yjf")).unwrap();
    assert_eq!((p.furthest, p.last), (Some(195894), Some(153130)));
}

#[test]
fn a_book_yjf_decodes_too() {
    let p = parse_yjf(&fixture("book.yjf")).unwrap();
    assert_eq!((p.furthest, p.last), (Some(56657), Some(54797)));
}

#[test]
fn unknown_objects_and_trailing_fields_are_skipped() {
    let bytes = synthetic(&[
        obj("x.new", &[int(7), utf("hi")]),
        obj("lpr", &[int(2), utf("AQ:42"), long(-1), int(9)]),
    ]);
    let p = parse_yjf(&bytes).unwrap();
    assert_eq!(p.last, Some(42));
    assert_eq!(p.last_read, None, "time -1 is no time");
    assert_eq!(p.furthest, None);
}

#[test]
fn truncated_input_is_an_error_not_a_panic() {
    let b = fixture("book.yjf");
    for n in [0, 4, 8, 20, b.len() - 1] {
        assert!(parse_yjf(&b[..n]).is_err(), "len {n}");
    }
}

#[test]
fn a_bad_signature_is_an_error() {
    let mut b = yjf(5);
    b[5] = 0;
    assert!(parse_yjf(&b).is_err());
}

#[test]
fn the_synthetic_builder_matches_the_real_format() {
    let p = parse_yjf(&yjf(1300)).unwrap();
    assert_eq!((p.furthest, p.last), (Some(1300), Some(1300)));
}

#[test]
fn position_ids() {
    assert_eq!(position_id("AQULAAB1AgAA:54797"), Some(54797));
    assert_eq!(position_id("nonsense"), None);
}

#[test]
fn deep_nesting_is_an_error_not_a_crash() {
    let mut b = SIGNATURE.to_vec();
    b.extend(long(1));
    b.extend(int(1));
    for _ in 0..200_000 {
        b.extend([0xfe, 1]); // object begin, name flagged empty
    }
    assert!(parse_yjf(&b).is_err());
}
