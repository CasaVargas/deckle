// SPDX-License-Identifier: GPL-3.0-or-later
use deckle_kfx::text::{content_runs, position_of};

#[test]
fn runs_from_the_mini_kfx_have_increasing_positions_and_the_right_text() {
    let kfx = std::fs::read(deckle_kfx::fixtures::path("deckle-mini-pdoc.golden.kfx")).unwrap();
    let (runs, erl) = content_runs(&kfx).unwrap();
    // 3 headings + 18 paragraphs of text (the cover image is not a text run)
    assert_eq!(
        runs.len(),
        21,
        "{:?}",
        runs.iter()
            .map(|r| (r.pid, r.text.chars().take(12).collect::<String>()))
            .collect::<Vec<_>>()
    );
    assert_eq!(runs[0].text, "Chapter One");
    assert_eq!(
        runs[0].pid, 3,
        "cover page template (0), cover image (1), section template (2), heading at 3"
    );
    assert!(runs.windows(2).all(|w| w[0].pid < w[1].pid));
    let end = runs
        .iter()
        .map(|r| r.pid + r.text.chars().count() as i64)
        .max()
        .unwrap();
    assert_eq!(
        erl,
        end.max(78982),
        "erl covers the end of every text run and the last $265 entry"
    );
    assert!(
        runs.iter()
            .all(|r| r.pid + r.text.chars().count() as i64 <= erl)
    );
    // position of a word inside a run is pid + char offset
    let p = position_of(&runs[1], "universally").unwrap();
    assert_eq!(p, runs[1].pid + "It is a truth ".chars().count() as i64);
}

#[test]
fn garbage_is_not_a_kfx() {
    assert!(content_runs(b"not a container").is_err());
}
