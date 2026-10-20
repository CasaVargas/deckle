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
        "",
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
        "",
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
    let body = request_body(Provider::OpenAi, "llama3", "T", "", &cands(), "en");
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
        "",
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
        "",
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
        "",
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
        "",
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
        "",
        &cands(),
        "en",
        &mut |_, _, _| Err("HTTP 401".into()),
    )
    .unwrap_err();
    assert!(err.to_string().contains("401"));
}

fn many(n: usize) -> Vec<Candidate> {
    (0..n)
        .map(|i| Candidate {
            label: format!("Name{i}"),
            kind: EntityKind::Term,
            quote: format!("Name{i} appears."),
            occurrences: vec![(i as i64 * 10, 5), (i as i64 * 10 + 5, 5)],
            aliases: vec![],
        })
        .collect()
}

/// An OpenAI-style answer describing every `NameN` that the request asked about.
fn echo(body: &str) -> String {
    let v: serde_json::Value = serde_json::from_str(body).unwrap();
    let prompt = v["messages"][1]["content"].as_str().unwrap();
    let items: Vec<serde_json::Value> = prompt
        .lines()
        .filter_map(|l| l.strip_prefix("- ")?.split(' ').next())
        .filter(|n| n.starts_with("Name"))
        .map(|n| serde_json::json!({"name": n, "kind": "person", "description": format!("{n} is described.")}))
        .collect();
    serde_json::json!({"choices": [{"message": {"content": serde_json::to_string(&items).unwrap()}}]})
        .to_string()
}

#[test]
fn candidates_go_out_in_batches_and_a_failed_batch_spares_the_rest() {
    use deckle_xray::llm::{BATCH_SIZE, describe_all};
    let mut sizes = Vec::new();
    let out = describe_all(
        Provider::OpenAi,
        "qwen3:8b",
        "",
        "http://localhost:11434/v1",
        "T",
        "A. Author",
        &many(60),
        "en",
        &mut |_, _, body| {
            let n = echo(body).matches("is described").count();
            sizes.push(n);
            if sizes.len() == 2 {
                Err("timeout".into())
            } else {
                Ok(echo(body))
            }
        },
    );
    assert_eq!(BATCH_SIZE, 25);
    assert_eq!(sizes, vec![25, 25, 10]);
    assert_eq!(out.batches, 3);
    assert_eq!(out.failures.len(), 1);
    assert_eq!(out.items.len(), 35, "batches 1 and 3 still count");
    assert_eq!(out.items[25].name, "Name50");
}

#[test]
fn self_hosted_endpoints_are_asked_not_to_think_and_a_rejection_is_retried_without_it() {
    let mut bodies: Vec<serde_json::Value> = Vec::new();
    let out = describe(
        Provider::OpenAi,
        "qwen3:8b",
        "",
        "http://localhost:11434/v1",
        "T",
        "",
        &many(2),
        "en",
        &mut |_, _, body| {
            bodies.push(serde_json::from_str(body).unwrap());
            if bodies.len() == 1 {
                Err("http status: 400".into())
            } else {
                Ok(echo(body))
            }
        },
    )
    .unwrap();
    assert_eq!(bodies[0]["reasoning_effort"], "none");
    assert!(
        bodies[1].get("reasoning_effort").is_none(),
        "retried without it"
    );
    assert_eq!(out.len(), 2);
    // a timeout is not retried
    let mut calls = 0;
    let err = describe(
        Provider::OpenAi,
        "m",
        "",
        "http://localhost:11434/v1",
        "T",
        "",
        &many(2),
        "en",
        &mut |_, _, _| {
            calls += 1;
            Err("timeout".into())
        },
    );
    assert!(err.is_err());
    assert_eq!(calls, 1);
    // OpenAI's own endpoint and Anthropic never get the field
    for (provider, endpoint) in [(Provider::OpenAi, ""), (Provider::Anthropic, "")] {
        let mut sent = serde_json::Value::Null;
        let _ = describe(
            provider,
            "m",
            "k",
            endpoint,
            "T",
            "",
            &many(1),
            "en",
            &mut |_, _, body| {
                sent = serde_json::from_str(body).unwrap();
                Err("stop".into())
            },
        );
        assert!(sent.get("reasoning_effort").is_none(), "{provider:?}");
    }
}

#[test]
fn the_prompt_names_the_author_and_leaves_the_kind_to_the_model() {
    let body = request_body(
        Provider::OpenAi,
        "m",
        "In the Shadow of Eight",
        "Collin Gosselin",
        &cands(),
        "en",
    );
    let prompt = body["messages"][1]["content"].as_str().unwrap();
    assert!(prompt.contains("Author: Collin Gosselin"), "{prompt}");
    assert!(
        !prompt.contains("guessed")
            && prompt.contains("Any individual")
            && prompt.contains("\"Yeah\""),
        "{prompt}"
    );
    let anonymous = request_body(Provider::OpenAi, "m", "T", "", &cands(), "en");
    assert!(
        !anonymous["messages"][1]["content"]
            .as_str()
            .unwrap()
            .contains("Author:")
    );
}
