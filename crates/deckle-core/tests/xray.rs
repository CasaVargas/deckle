// SPDX-License-Identifier: GPL-3.0-or-later
use deckle_core::xray::{XRayOptions, build};
use deckle_kfx::text::Run;

fn runs() -> (Vec<Run>, i64) {
    let paras = [
        "Elizabeth Bennet looked at Jane. Jane smiled at Elizabeth, and Mr. Darcy watched them both from Netherfield.",
        "Darcy said nothing. Elizabeth laughed. Netherfield was quiet that evening, and Longbourn quieter still.",
        "The next day Elizabeth walked to Longbourn with Mr. Darcy and Jane.",
    ];
    let mut pid = 0;
    let mut v = Vec::new();
    for p in paras {
        v.push(Run {
            pid,
            text: p.to_owned(),
        });
        pid += p.chars().count() as i64 + 1;
    }
    (v, pid)
}

#[test]
fn basic_xray_from_the_text_alone_is_a_valid_sidecar() {
    let (runs, erl) = runs();
    let out = build(
        "B0ABCDDECK",
        "Pride and Prejudice",
        "en",
        &runs,
        erl,
        &XRayOptions::default(),
    )
    .unwrap()
    .expect("entities");
    assert_eq!(out.summary.source, "basic");
    assert!(
        out.summary.people >= 3 && out.summary.terms >= 2,
        "{:?}",
        out.summary
    );
    assert_eq!(out.file_name, "XRAY.entities.B0ABCDDECK.asc");
    let dir = tempfile::tempdir().unwrap();
    let p = dir.path().join(&out.file_name);
    std::fs::write(&p, &out.bytes).unwrap();
    let db = rusqlite::Connection::open(&p).unwrap();
    let n: i64 = db
        .query_row("SELECT count(*) FROM entity WHERE id > 0", [], |r| r.get(0))
        .unwrap();
    assert_eq!(n as usize, out.summary.people + out.summary.terms);
    let erl_db: i64 = db
        .query_row("SELECT erl FROM book_metadata", [], |r| r.get(0))
        .unwrap();
    assert_eq!(erl_db, erl);
    let desc: String = db
        .query_row("SELECT text FROM entity_description WHERE entity = (SELECT id FROM entity WHERE label = 'Elizabeth')", [], |r| r.get(0))
        .unwrap();
    assert!(
        desc.contains("Elizabeth Bennet looked at Jane"),
        "quote fallback: {desc}"
    );
}

#[test]
fn nothing_recurring_means_no_sidecar() {
    let runs = vec![Run {
        pid: 0,
        text: "It was a dark night.".into(),
    }];
    assert!(
        build("B0ABCDDECK", "T", "en", &runs, 21, &XRayOptions::default())
            .unwrap()
            .is_none()
    );
}
