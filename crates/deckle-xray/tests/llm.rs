// SPDX-License-Identifier: GPL-3.0-or-later
use deckle_xray::asc::EntityKind;
use deckle_xray::extract::Candidate;
use deckle_xray::llm::{Described, Provider, describe, extract_json_array, request_body};

fn cands() -> Vec<Candidate> {
    vec![
        Candidate {
            label: "Elizabeth".into(),
            kind: EntityKind::Person,
            quote: "Elizabeth laughed.".into(),
            occurrences: vec![(10, 9), (50, 9)],
            aliases: vec![],
        },
        Candidate {
            label: "Netherfield".into(),
            kind: EntityKind::Term,
            quote: "Netherfield was quiet.".into(),
            occurrences: vec![(80, 11), (90, 11)],
            aliases: vec![],
        },
    ]
}

#[test]
fn json_array_is_extracted_from_chatty_answers_and_fences() {
    let raw = "Sure! Here you go:\n```json\n[{\"name\":\"A\",\"kind\":\"person\",\"description\":\"d\",\"aliases\":[]}]\n```\nHope this helps.";
    let v = extract_json_array(raw).unwrap();
    assert_eq!(v[0]["name"], "A");
    assert!(extract_json_array("no json here").is_none());
}

#[test]
fn anthropic_request_has_the_right_shape_and_response_is_parsed() {
    let body = request_body(
        Provider::Anthropic,
        "claude-sonnet-5-5",
        "Pride and Prejudice",
        &cands(),
        "en",
    );
    assert_eq!(body["model"], "claude-sonnet-5-5");
    assert!(body["max_tokens"].as_u64().unwrap() >= 1024);
    let prompt = body["messages"][0]["content"].as_str().unwrap();
    assert!(
        prompt.contains("Elizabeth")
            && prompt.contains("Netherfield")
            && prompt.contains("spoiler"),
        "{prompt}"
    );
    assert!(body["system"].as_str().unwrap().contains("JSON"));
    let canned = r#"{"content":[{"type":"text","text":"[{\"name\":\"Elizabeth\",\"kind\":\"person\",\"description\":\"The witty second Bennet sister.\",\"aliases\":[\"Lizzy\"]},{\"name\":\"Netherfield\",\"kind\":\"term\",\"description\":\"An estate let to Mr. Bingley.\",\"aliases\":[]}]"}]}"#;
    let mut seen_url = String::new();
    let out = describe(
        Provider::Anthropic,
        "claude-sonnet-5-5",
        "key-123",
        "",
        "Pride and Prejudice",
        &cands(),
        "en",
        &mut |url, headers, _body| {
            seen_url = url.to_owned();
            assert!(
                headers
                    .iter()
                    .any(|(k, v)| k == "x-api-key" && v == "key-123")
            );
            assert!(headers.iter().any(|(k, _)| k == "anthropic-version"));
            Ok(canned.to_owned())
        },
    )
    .unwrap();
    assert_eq!(seen_url, "https://api.anthropic.com/v1/messages");
    assert_eq!(out.len(), 2);
    assert_eq!(
        out[0],
        Described {
            name: "Elizabeth".into(),
            kind: Some(EntityKind::Person),
            description: "The witty second Bennet sister.".into(),
            aliases: vec!["Lizzy".into()]
        }
    );
    assert_eq!(out[1].kind, Some(EntityKind::Term));
}

#[test]
fn openai_compatible_request_and_response() {
    let body = request_body(Provider::OpenAi, "llama3", "T", &cands(), "en");
    assert_eq!(body["messages"][0]["role"], "system");
    assert_eq!(body["messages"][1]["role"], "user");
    assert!(
        body["max_tokens"].as_u64().unwrap() >= 1024,
        "local servers default to tiny completions"
    );
    let canned = r#"{"choices":[{"message":{"role":"assistant","content":"[{\"name\":\"Elizabeth\",\"description\":\"x\"}]"}}]}"#;
    let out = describe(
        Provider::OpenAi,
        "llama3",
        "",
        "http://localhost:11434/v1",
        "T",
        &cands(),
        "en",
        &mut |url, headers, _| {
            assert_eq!(url, "http://localhost:11434/v1/chat/completions");
            assert!(
                !headers.iter().any(|(k, _)| k == "authorization"),
                "no key, no auth header"
            );
            Ok(canned.to_owned())
        },
    )
    .unwrap();
    assert_eq!(out[0].aliases, Vec::<String>::new());
    assert_eq!(
        out[0].kind, None,
        "an omitted kind keeps the heuristic's guess"
    );
}

#[test]
fn truncated_answers_are_reported_as_such() {
    let canned = r#"{"choices":[{"finish_reason":"length","message":{"content":"[{\"name\":\"A\",\"description\":\"x\"},{\"name\":\"B\",\"desc"}}]}"#;
    let err = describe(
        Provider::OpenAi,
        "m",
        "",
        "http://x/v1",
        "T",
        &cands(),
        "en",
        &mut |_, _, _| Ok(canned.to_owned()),
    )
    .unwrap_err();
    assert!(err.to_string().contains("truncated"), "{err}");
    let canned =
        r#"{"stop_reason":"max_tokens","content":[{"type":"text","text":"[{\"name\":\"A\""}]}"#;
    let err = describe(
        Provider::Anthropic,
        "m",
        "k",
        "",
        "T",
        &cands(),
        "en",
        &mut |_, _, _| Ok(canned.to_owned()),
    )
    .unwrap_err();
    assert!(err.to_string().contains("truncated"), "{err}");
}

#[test]
fn unusable_answers_are_errors_not_panics() {
    let err = describe(
        Provider::OpenAi,
        "m",
        "",
        "http://x/v1",
        "T",
        &cands(),
        "en",
        &mut |_, _, _| Ok("{\"choices\":[]}".into()),
    )
    .unwrap_err();
    assert!(err.to_string().contains("no content"), "{err}");
    let err = describe(
        Provider::Anthropic,
        "m",
        "k",
        "",
        "T",
        &cands(),
        "en",
        &mut |_, _, _| Err("HTTP 401".into()),
    )
    .unwrap_err();
    assert!(err.to_string().contains("401"));
}
