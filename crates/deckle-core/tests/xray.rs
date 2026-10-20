// SPDX-License-Identifier: GPL-3.0-or-later
use deckle_core::xray::{LlmConfig, XRayOptions, build, build_with};
use deckle_kfx::text::Run;
use deckle_xray::llm::Provider;

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
        "Jane Austen",
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
        build(
            "B0ABCDDECK",
            "T",
            "",
            "en",
            &runs,
            21,
            &XRayOptions::default()
        )
        .unwrap()
        .is_none()
    );
}

fn llm_opts() -> XRayOptions {
    XRayOptions {
        llm: Some(LlmConfig {
            provider: Provider::OpenAi,
            model: "m".into(),
            endpoint: "http://local/v1".into(),
            api_key: "sk-secret".into(),
        }),
        wikipedia: false,
    }
}

fn openai(items: &str) -> String {
    serde_json::json!({"choices":[{"finish_reason":"stop","message":{"content": items}}]})
        .to_string()
}

#[test]
fn symmetric_llm_aliases_keep_the_main_character() {
    let (runs, erl) = runs();
    let answer = openai(
        r#"[{"name":"Darcy","kind":"person","description":"Darcy.","aliases":["Mr. Darcy"]},{"name":"Mr. Darcy","kind":"person","description":"A proud gentleman.","aliases":["Darcy"]},{"name":"Elizabeth","description":"The heroine."},{"name":"Netherfield","kind":"term","description":""},{"name":"Longbourn","kind":"term","description":"The Bennet home.","aliases":["Netherfield"]}]"#,
    );
    let out = build_with(
        "B0ABCDDECK",
        "T",
        "",
        "en",
        &runs,
        erl,
        &llm_opts(),
        &mut |_, _, _| Ok(answer.clone()),
        &mut |_| Err("no".into()),
    )
    .unwrap()
    .expect("entities");
    assert_eq!(out.summary.source, "llm");
    let dir = tempfile::tempdir().unwrap();
    let p = dir.path().join(&out.file_name);
    std::fs::write(&p, &out.bytes).unwrap();
    let db = rusqlite::Connection::open(&p).unwrap();
    let labels: Vec<String> = db
        .prepare("SELECT label FROM entity WHERE id > 0 ORDER BY label")
        .unwrap()
        .query_map([], |r| r.get(0))
        .unwrap()
        .map(Result::unwrap)
        .collect();
    assert!(labels.iter().any(|l| l == "Mr. Darcy"), "{labels:?}");
    assert!(
        !labels.iter().any(|l| l == "Netherfield"),
        "dropped by the LLM: {labels:?}"
    );
    assert!(labels.iter().any(|l| l == "Longbourn"));
    let (darcy_count, darcy_kind): (i64, i64) = db
        .query_row(
            "SELECT count, type FROM entity WHERE label = 'Mr. Darcy'",
            [],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )
        .unwrap();
    assert_eq!(darcy_kind, 1);
    assert!(darcy_count >= 3, "{darcy_count}");
    let eliz_kind: i64 = db
        .query_row(
            "SELECT type FROM entity WHERE label = 'Elizabeth'",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(
        eliz_kind, 1,
        "no kind from the LLM keeps the heuristic's person"
    );
}

#[test]
fn llm_failure_falls_back_to_basic_with_a_warning_and_no_key_in_it() {
    let (runs, erl) = runs();
    let out = build_with(
        "B0ABCDDECK",
        "T",
        "",
        "en",
        &runs,
        erl,
        &llm_opts(),
        &mut |_, _, _| Err("HTTP 401 unauthorized".into()),
        &mut |_| Err("no".into()),
    )
    .unwrap()
    .unwrap();
    assert_eq!(out.summary.source, "basic");
    assert!(
        out.warnings.iter().any(|w| w.contains("401")),
        "{:?}",
        out.warnings
    );
    assert!(
        !format!("{:?}", llm_opts()).contains("sk-secret"),
        "Debug must redact the key"
    );
}

#[test]
fn wikipedia_uses_the_primary_language_subtag() {
    let (runs, erl) = runs();
    let opts = XRayOptions {
        llm: None,
        wikipedia: true,
    };
    let mut urls = Vec::new();
    let out = build_with(
        "B0ABCDDECK",
        "T",
        "",
        "en-US",
        &runs,
        erl,
        &opts,
        &mut |_, _, _| Err("no".into()),
        &mut |u| {
            urls.push(u.to_owned());
            Ok(r#"{"type":"standard","extract":"A place."}"#.into())
        },
    )
    .unwrap()
    .unwrap();
    assert!(!urls.is_empty());
    assert!(
        urls.iter()
            .all(|u| u.starts_with("https://en.wikipedia.org/")),
        "{urls:?}"
    );
    assert_eq!(out.summary.source, "wikipedia");
}
