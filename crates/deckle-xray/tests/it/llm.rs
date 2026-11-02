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

// ---- Ollama, on its native API (local-model design §4) ----

#[test]
fn local_servers_are_told_apart_from_hosted_ones() {
    use deckle_xray::llm::is_local;
    assert!(is_local(Provider::Ollama, ""));
    assert!(is_local(Provider::Ollama, "http://nas.local:11434"));
    assert!(is_local(Provider::OpenAi, "http://localhost:1234/v1"));
    assert!(!is_local(Provider::OpenAi, ""));
    assert!(!is_local(Provider::OpenAi, "https://api.openai.com/v1/"));
    assert!(!is_local(Provider::Anthropic, "http://localhost:11434"));
}

#[test]
fn the_recommended_model_follows_the_machines_memory() {
    use deckle_xray::llm::{RECOMMENDED_MODELS, recommended_model};
    let gib = 1u64 << 30;
    assert_eq!(recommended_model(None).name, "qwen3:4b");
    assert_eq!(recommended_model(Some(8 * gib)).name, "qwen3:4b");
    assert_eq!(recommended_model(Some(16 * gib)).name, "qwen3:8b");
    assert_eq!(recommended_model(Some(64 * gib)).name, "qwen3:8b");
    assert_eq!(Provider::Ollama.default_model(), RECOMMENDED_MODELS[0].name);
    assert!(
        RECOMMENDED_MODELS
            .windows(2)
            .all(|w| w[0].min_memory < w[1].min_memory)
    );
}

#[test]
fn ollama_requests_use_the_native_api_with_context_schema_and_no_thinking() {
    let body = request_body(Provider::Ollama, "qwen3:4b", "T", "", &cands(), "en");
    assert_eq!(body["stream"], false);
    assert_eq!(
        body["think"], false,
        "thinking doubles a local model's output"
    );
    assert_eq!(
        body["options"]["num_ctx"], 8192,
        "Ollama's own default is 4k"
    );
    assert!(body["options"]["num_predict"].as_u64().unwrap() >= 1024);
    assert_eq!(body["format"]["type"], "array", "held to the entity schema");
    assert_eq!(
        body["format"]["items"]["properties"]["kind"]["enum"][0],
        "person"
    );
    assert_eq!(body["messages"][0]["role"], "system");
    assert!(body.get("max_tokens").is_none());
    let canned = r#"{"model":"qwen3:4b","message":{"role":"assistant","content":"[{\"name\":\"Elizabeth\",\"kind\":\"person\",\"description\":\"The second Bennet sister.\",\"aliases\":[]}]"},"done":true,"done_reason":"stop"}"#;
    let out = describe(
        Provider::Ollama,
        "qwen3:4b",
        "",
        "",
        "T",
        "",
        &cands(),
        "en",
        &mut |url, headers, body| {
            assert_eq!(url, "http://localhost:11434/api/chat", "default endpoint");
            assert!(!headers.iter().any(|(k, _)| k == "authorization"));
            let b: serde_json::Value = serde_json::from_str(body).unwrap();
            assert_eq!(b["keep_alive"], "10m");
            Ok(canned.to_owned())
        },
    )
    .unwrap();
    assert_eq!(out[0].kind, Some(EntityKind::Person));
}

#[test]
fn ollama_answers_report_truncation_and_errors() {
    use deckle_xray::llm::{LlmError, Options, complete_with};
    let cut = r#"{"message":{"content":"[{\"name\":\"A\""},"done_reason":"length"}"#;
    let err = describe(
        Provider::Ollama,
        "qwen3:4b",
        "",
        "http://nas.local:11434/",
        "T",
        "",
        &cands(),
        "en",
        &mut |url, _, _| {
            assert_eq!(url, "http://nas.local:11434/api/chat");
            Ok(cut.to_owned())
        },
    )
    .unwrap_err();
    assert!(matches!(err, LlmError::Truncated), "{err}");
    // a 200 with an error body (no such model) is an HTTP-class failure, not "no content"
    let err = complete_with(
        Provider::Ollama,
        "nope:1b",
        "",
        "",
        "s",
        "u",
        64,
        &Options {
            num_ctx: Some(32_768),
            schema: None,
        },
        &mut |_, _, body| {
            let b: serde_json::Value = serde_json::from_str(body).unwrap();
            assert_eq!(b["options"]["num_ctx"], 32_768);
            assert!(b.get("format").is_none());
            Ok(r#"{"error":"model 'nope:1b' not found"}"#.to_owned())
        },
    )
    .unwrap_err();
    assert!(
        matches!(&err, LlmError::Http(e) if e.contains("not found")),
        "{err}"
    );
}

#[test]
fn probe_reads_the_version_and_installed_models() {
    use deckle_xray::llm::probe;
    let server = probe("", &mut |url| match url {
        "http://localhost:11434/api/version" => Ok(r#"{"version":"0.12.3"}"#.into()),
        "http://localhost:11434/api/tags" => Ok(
            r#"{"models":[{"name":"qwen3:4b","size":2600000000,"digest":"x"},{"name":"llama3.2:latest","size":2000000000}]}"#.into(),
        ),
        other => panic!("{other}"),
    })
    .unwrap();
    assert_eq!(server.version, "0.12.3");
    assert_eq!(server.models.len(), 2);
    assert!(server.has("qwen3:4b") && server.has("llama3.2") && server.has("llama3.2:latest"));
    assert!(!server.has("qwen3:8b"));
    let down = probe("http://localhost:11434", &mut |_| {
        Err("connection refused".into())
    })
    .unwrap_err();
    assert!(
        down.starts_with("Ollama isn't answering at http://localhost:11434"),
        "{down}"
    );
    let other = probe("", &mut |_| Ok("<html>".into())).unwrap_err();
    assert!(other.contains("not like Ollama"), "{other}");
}

#[test]
fn pull_streams_progress_and_ends_on_success_or_error() {
    use deckle_xray::llm::{Progress, pull};
    let mut seen = Vec::new();
    pull(
        "",
        "qwen3:4b",
        &mut |url, body, line| {
            assert_eq!(url, "http://localhost:11434/api/pull");
            let b: serde_json::Value = serde_json::from_str(body).unwrap();
            assert_eq!(b["model"], "qwen3:4b");
            assert_eq!(b["stream"], true);
            line(r#"{"status":"pulling manifest"}"#);
            line(r#"{"status":"pulling abc","digest":"abc","total":1000,"completed":250}"#);
            line(r#"{"status":"pulling abc","digest":"abc","total":1000,"completed":1000}"#);
            line(r#"{"status":"success"}"#);
            Ok(())
        },
        &mut |p: &Progress| {
            seen.push(p.clone());
            true
        },
    )
    .unwrap();
    assert_eq!(seen.len(), 4);
    assert_eq!((seen[1].completed, seen[1].total), (250, 1000));
    assert_eq!(seen[3].status, "success");

    let err = pull(
        "",
        "nope:1b",
        &mut |_, _, line| {
            line(r#"{"status":"pulling manifest"}"#);
            line(r#"{"error":"pull model manifest: file does not exist"}"#);
            Ok(())
        },
        &mut |_| true,
    )
    .unwrap_err();
    assert!(err.contains("couldn't download nope:1b"), "{err}");
    let err = pull("", "m", &mut |_, _, _| Ok(()), &mut |_| true).unwrap_err();
    assert!(err.contains("ended early"), "{err}");
    let err = pull("", "m", &mut |_, _, _| Err("refused".into()), &mut |_| true).unwrap_err();
    assert!(err.contains("refused"), "{err}");
    // stopping: the stream is left as soon as progress says so
    let mut fed = 0;
    let err = pull(
        "",
        "m",
        &mut |_, _, line| {
            for l in [
                r#"{"status":"pulling a","total":10,"completed":1}"#,
                r#"{"status":"pulling a","total":10,"completed":2}"#,
                r#"{"status":"success"}"#,
            ] {
                fed += 1;
                if !line(l) {
                    break;
                }
            }
            Ok(())
        },
        &mut |p| p.completed < 2,
    )
    .unwrap_err();
    assert_eq!(err, deckle_xray::llm::PULL_CANCELLED);
    assert_eq!(fed, 2, "the stream is dropped at the stop");
}

#[test]
fn check_runs_the_three_steps_and_stops_at_the_first_that_fails() {
    use deckle_xray::llm::check;
    let tags = r#"{"models":[{"name":"qwen3:4b","size":1}]}"#;
    let version = r#"{"version":"0.12.3"}"#;
    let mut get = |url: &str| -> Result<String, String> {
        Ok(if url.ends_with("/api/version") {
            version
        } else {
            tags
        }
        .into())
    };
    // ready
    let mut posted = 0;
    let c = check("", "qwen3:4b", &mut get, &mut |_, _, body| {
        posted += 1;
        let b: serde_json::Value = serde_json::from_str(body).unwrap();
        assert_eq!(b["options"]["num_ctx"], 2048, "a tiny test prompt");
        Ok(r#"{"message":{"content":"OK"},"done_reason":"stop"}"#.into())
    });
    assert_eq!(posted, 1);
    assert!(c.ok());
    assert_eq!(c.installed, Some(true));
    assert_eq!(c.endpoint, "http://localhost:11434");
    // not installed: no prompt is sent
    let c = check("", "qwen3:8b", &mut get, &mut |_, _, _| {
        panic!("no prompt without the model")
    });
    assert!(!c.ok());
    assert_eq!(c.installed, Some(false));
    assert_eq!(c.answered, None);
    // not running: nothing else is tried
    let c = check(
        "http://nas.local:11434",
        "qwen3:4b",
        &mut |_| Err("refused".into()),
        &mut |_, _, _| panic!("no prompt without a server"),
    );
    assert!(c.server.is_err() && c.installed.is_none() && c.answered.is_none());
    // an empty model is the recommended one
    let c = check("", "", &mut get, &mut |_, _, _| {
        Ok(r#"{"message":{"content":"OK"}}"#.into())
    });
    assert!(c.model.starts_with("qwen3:"), "{}", c.model);
}

#[test]
fn a_blank_ollama_address_follows_the_environment() {
    use deckle_xray::llm::default_ollama_endpoint;
    // Other tests don't set these, so the process-wide variable is ours to try.
    // SAFETY: this test's process; nothing else reads the environment concurrently here.
    unsafe {
        std::env::set_var("DECKLE_OLLAMA_URL", "http://ollama:11434/");
    }
    assert_eq!(default_ollama_endpoint(), "http://ollama:11434");
    unsafe {
        std::env::remove_var("DECKLE_OLLAMA_URL");
        std::env::set_var("OLLAMA_HOST", "0.0.0.0:11434");
    }
    assert_eq!(
        default_ollama_endpoint(),
        "http://localhost:11434",
        "not a URL: ignored"
    );
    unsafe {
        std::env::set_var("OLLAMA_HOST", "http://nas.local:11434");
    }
    assert_eq!(default_ollama_endpoint(), "http://nas.local:11434");
    unsafe {
        std::env::remove_var("OLLAMA_HOST");
    }
}
