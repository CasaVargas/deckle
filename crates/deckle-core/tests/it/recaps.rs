// SPDX-License-Identifier: GPL-3.0-or-later
//! Chapter recaps (docs/superpowers/specs/2026-11-01-ai-extras-design.md).

use deckle_core::recaps::{parts, word_count};

/// `n` distinct words, `w0 w1 …`.
fn words(n: usize, from: usize) -> String {
    (from..from + n)
        .map(|i| format!("w{i}"))
        .collect::<Vec<_>>()
        .join(" ")
}

#[test]
fn word_count_counts_spaced_words() {
    assert_eq!(word_count("The cat sat.\n\nOn a mat"), 6);
}

#[test]
fn word_count_counts_each_cjk_char() {
    assert_eq!(word_count(&"猫".repeat(1000)), 1000);
    assert_eq!(word_count("彼は ねこ"), 4);
}

#[test]
fn parts_packs_paragraphs_in_order() {
    let text = (0..5)
        .map(|p| words(600, p * 600))
        .collect::<Vec<_>>()
        .join("\n\n");
    let got = parts(&text, 2_000);
    let counts: Vec<usize> = got.iter().map(|p| word_count(p)).collect();
    assert_eq!(counts, [1800, 1200]);
    let back: Vec<&str> = got.iter().flat_map(|p| p.split_whitespace()).collect();
    let want: Vec<&str> = text.split_whitespace().collect();
    assert_eq!(back, want);
}

#[test]
fn parts_cuts_a_long_paragraph() {
    let got = parts(&words(5_000, 0), 2_000);
    let counts: Vec<usize> = got.iter().map(|p| word_count(p)).collect();
    assert_eq!(counts, [2000, 2000, 1000]);
}

#[test]
fn one_part_when_it_fits() {
    assert_eq!(parts(&words(900, 0), 40_000).len(), 1);
}
