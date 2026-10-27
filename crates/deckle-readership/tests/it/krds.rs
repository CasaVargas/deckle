// SPDX-License-Identifier: GPL-3.0-or-later

use crate::common::*;
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
fn book_info_store_from_a_book() {
    let p = parse_yjf(&fixture("book.yjf")).unwrap();
    assert_eq!(p.words_read, Some(9519));
    assert_eq!((p.kindle_fraction.unwrap() * 10_000.0).round(), 1505.0);
    assert_eq!(p.wpm.len(), 4);
    assert_eq!(p.wpm[0].round(), 322.0);
}

#[test]
fn book_info_store_from_an_edition() {
    let p = parse_yjf(&fixture("daily-2026-10-21.yjf")).unwrap();
    assert_eq!(p.words_read, Some(4611));
    assert_eq!((p.kindle_fraction.unwrap() * 10_000.0).round(), 2984.0);
    assert_eq!(p.furthest, Some(103303));
}

#[test]
fn wpm_samples_flatten_all_groups() {
    let p = parse_yjf(&fixture("daily-2026-10-21.yjf")).unwrap();
    assert_eq!(p.wpm.len(), 13);
}

#[test]
fn doubles_are_decoded_in_a_synthetic_file() {
    let bytes = synthetic(&[
        obj("book.info.store", &[int(12), dbl(0.5)]),
        obj(
            "timer.average.calculator",
            &[int(2), dbl(300.0), dbl(250.0)],
        ),
    ]);
    let p = parse_yjf(&bytes).unwrap();
    assert_eq!((p.words_read, p.kindle_fraction), (Some(12), Some(0.5)));
    assert_eq!(p.wpm, vec![300.0, 250.0]);
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

#[test]
fn non_finite_doubles_are_dropped_not_kept() {
    let bytes = synthetic(&[
        obj("book.info.store", &[int(12), dbl(f64::NAN)]),
        obj(
            "timer.average.calculator",
            &[int(3), dbl(300.0), dbl(f64::INFINITY), dbl(f64::NAN)],
        ),
    ]);
    let p = parse_yjf(&bytes).unwrap();
    assert_eq!(p.kindle_fraction, None);
    assert_eq!(p.wpm, vec![300.0], "only finite samples survive");
    assert_eq!(p.words_read, Some(12));
}

#[test]
fn page_history_lists_the_jumps_origins_in_order() {
    assert_eq!(
        parse_yjf(&fixture("daily-2026-10-21.yjf")).unwrap().history,
        [821, 4, 1015]
    );
    assert_eq!(
        parse_yjf(&fixture("daily-2026-10-20.yjf")).unwrap().history,
        [4]
    );
    assert!(parse_yjf(&yjf(5)).unwrap().history.is_empty());
}
