// SPDX-License-Identifier: GPL-3.0-or-later
use deckle_digest::editor::{EditorConfig, edit};
use deckle_digest::{
    Article, Config, GatherOptions, Gathered, Section, SectionKind, gather_with_editor,
};
use deckle_xray::llm::Provider;
use serde_json::{Value, json};

fn config() -> Config {
    let mut c = Config {
        sections: vec![SectionKind::Feeds, SectionKind::Horoscope],
        full_text: false,
        ..Default::default()
    };
    c.feeds = ["First", "Second"]
        .into_iter()
        .map(|name| deckle_digest::config::FeedConfig {
            name: name.into(),
            url: format!("https://example.test/{name}"),
            max_items: 4,
            ..Default::default()
        })
        .collect();
    c.horoscope.signs = vec!["leo".into()];
    c
}
fn feed(url: &str) -> Result<Vec<u8>, String> {
    let title = if url.ends_with("First") {
        "Council approves a park"
    } else {
        "New park approved by council"
    };
    Ok(format!(r#"<rss version="2.0"><channel><title>News</title><item><title>{title}</title><link>{url}/story</link><pubDate>Sat, 24 Oct 2026 10:00:00 GMT</pubDate><description>The council voted to open a park.</description></item></channel></rss>"#).into_bytes())
}
fn original() -> Gathered {
    gather_with_editor(
        &config(),
        date(),
        now(),
        &feed,
        &GatherOptions::default(),
        "",
        &mut |_, _, _| panic!("disabled editor called provider"),
    )
}
fn date() -> chrono::NaiveDate {
    chrono::NaiveDate::from_ymd_opt(2026, 10, 24).unwrap()
}
fn now() -> chrono::DateTime<chrono::Utc> {
    "2026-10-24T12:00:00Z".parse().unwrap()
}
fn on() -> EditorConfig {
    EditorConfig {
        enabled: true,
        ..Default::default()
    }
}
fn answer() -> Value {
    json!({
        "duplicate_groups": [["s1a0", "s0a0"]],
        "briefs": [{"section":"s0", "lines":["A park was approved."]}, {"section":"s1", "lines":["The same park was approved."]}],
        "horoscopes": [{"id":"s2a0", "text":"Enjoy <today> & a quiet walk."}]
    })
}
fn envelope(answer: Value) -> String {
    json!({"choices":[{"finish_reason":"stop","message":{"content":answer.to_string()}}]})
        .to_string()
}
fn snapshot(g: &Gathered) -> Vec<(String, Vec<u8>)> {
    deckle_digest::render::render(g, vec![])
}

#[test]
fn disabled_is_byte_identical_and_never_calls_a_provider() {
    let mut g = original();
    let before = snapshot(&g);
    edit(
        &mut g,
        &EditorConfig::default(),
        "secret",
        &mut |_, _, _| panic!("called"),
        &[],
        &[],
    );
    assert_eq!(before, snapshot(&g));
    assert!(g.notes.is_empty());
    assert!(
        !toml::to_string(&Config::default())
            .unwrap()
            .contains("[llm]")
    );
}

#[test]
fn edits_reach_epub_and_bundle_and_preserve_news_text() {
    let mut c = config();
    c.llm = on();
    let mut calls = 0;
    let g = gather_with_editor(
        &c,
        date(),
        now(),
        &feed,
        &GatherOptions::default(),
        "secret",
        &mut |url, headers, body| {
            calls += 1;
            assert_eq!(url, "https://api.openai.com/v1/chat/completions");
            assert!(headers.contains(&("authorization".into(), "Bearer secret".into())));
            let request: Value = serde_json::from_str(body).unwrap();
            assert_eq!(request["max_completion_tokens"], 8192);
            assert!(request.get("max_tokens").is_none());
            assert!(!body.contains("secret"));
            Ok(envelope(answer()))
        },
    );
    assert_eq!(calls, 1);
    assert_eq!(
        g.edition.sections[0].articles[0].body,
        original().edition.sections[0].articles[0].body
    );
    assert!(g.edition.sections[1].articles.is_empty());
    assert!(g.edition.sections[1].brief.is_empty());
    let files = snapshot(&g);
    let html = files
        .iter()
        .filter(|(p, _)| p.ends_with(".xhtml"))
        .map(|(_, b)| String::from_utf8_lossy(b))
        .collect::<Vec<_>>()
        .join("\n");
    assert!(html.contains("AI-written section brief") && html.contains("A park was approved."));
    assert!(html.contains("Enjoy &lt;today&gt; &amp; a quiet walk."));
    assert!(html.contains("AI-written by Deckle, for entertainment."));
    assert!(!html.contains("secret"));
    for (_, bytes) in files.iter().filter(|(p, _)| p.ends_with(".xhtml")) {
        roxmltree::Document::parse(std::str::from_utf8(bytes).unwrap()).unwrap();
    }
    let tmp = tempfile::tempdir().unwrap();
    let r = deckle_digest::build(&config(), date(), now(), tmp.path(), &feed).unwrap();
    let path = deckle_digest::bundle::write(&g, &r, tmp.path(), false, now()).unwrap();
    let v: Value =
        serde_json::from_slice(&std::fs::read(path.join("edition.json")).unwrap()).unwrap();
    assert_eq!(
        v["sections"][0]["ai_brief"],
        json!(["A park was approved."])
    );
    let schema: Value = serde_json::from_str(include_str!(
        "../../../docs/formats/edition-bundle.schema.json"
    ))
    .unwrap();
    assert!(jsonschema::validator_for(&schema).unwrap().is_valid(&v));
}

#[test]
fn invalid_plans_and_transport_failures_leave_the_whole_edition_intact() {
    let mut bad_id = answer();
    bad_id["duplicate_groups"] = json!([["s0a0", "nonexistent"]]);
    let mut overlap = answer();
    overlap["duplicate_groups"] = json!([["s0a0", "s1a0"], ["s0a0", "s1a0"]]);
    let mut long_brief = answer();
    long_brief["briefs"][0]["lines"] = json!(["one", "two", "three", "four"]);
    let mut missing = answer();
    missing["horoscopes"] = json!([]);
    let mut control = answer();
    control["horoscopes"][0]["text"] = json!("bad\u{0001}");
    let truncated =
        json!({"choices":[{"finish_reason":"length","message":{"content":answer().to_string()}}]})
            .to_string();
    for response in [
        Err("secret leaked in provider error".into()),
        Ok("not JSON secret".into()),
        Ok(truncated),
        Ok(envelope(bad_id)),
        Ok(envelope(overlap)),
        Ok(envelope(long_brief)),
        Ok(envelope(missing)),
        Ok(envelope(control)),
    ] {
        let mut g = original();
        let before = snapshot(&g);
        edit(
            &mut g,
            &on(),
            "secret",
            &mut |_, _, _| response.clone(),
            &[],
            &[],
        );
        assert_eq!(snapshot(&g), before);
        assert_eq!(
            g.notes,
            ["AI editor unavailable; using the original edition."]
        );
    }
}

#[test]
fn private_sections_urls_and_images_do_not_enter_the_prompt() {
    let mut g = original();
    g.edition.sections[0].articles[0]
        .photos
        .push(b"PRIVATE PHOTO".to_vec());
    g.edition.sections[0].articles[0]
        .body
        .push_str("<img src=\"PRIVATE_IMAGE_URL\" alt=\"PRIVATE_ALT\"/>");
    for key in ["reading", "shelf", "weather"] {
        let i = g.edition.sections.len();
        g.edition.sections.push(Section {
            key: key.into(),
            title: "PRIVATE TITLE".into(),
            articles: vec![Article {
                title: "PRIVATE HEADLINE".into(),
                body: "PRIVATE BODY".into(),
                ..Default::default()
            }],
            ..Default::default()
        });
        g.news.push(i); // Even a mistaken news index cannot disclose a personal section.
    }
    edit(
        &mut g,
        &on(),
        "secret",
        &mut |_, _, body| {
            assert!(!body.contains("PRIVATE"));
            assert!(!body.contains("https://example.test"));
            assert!(body.contains("council"));
            Ok(envelope(answer()))
        },
        &[],
        &[],
    );
    assert!(g.notes.is_empty());
    assert_eq!(g.edition.sections[3].articles[0].body, "PRIVATE BODY");
}

#[test]
fn credentials_and_input_limits_fail_without_a_request() {
    let mut g = original();
    let before = snapshot(&g);
    edit(
        &mut g,
        &on(),
        "",
        &mut |_, _, _| panic!("missing credential called provider"),
        &[],
        &[],
    );
    assert_eq!(snapshot(&g), before);
    assert_eq!(g.notes.len(), 1);
    let mut g = original();
    g.edition.sections[0].articles = vec![g.edition.sections[0].articles[0].clone(); 81];
    edit(
        &mut g,
        &on(),
        "secret",
        &mut |_, _, _| panic!("oversized input called provider"),
        &[],
        &[],
    );
    assert_eq!(g.edition.sections[0].articles.len(), 81);
    assert_eq!(g.notes.len(), 1);
}

#[test]
fn anthropic_and_local_compatible_servers_share_the_transport() {
    for provider in [Provider::Anthropic, Provider::OpenAi] {
        let mut g = original();
        let cfg = EditorConfig {
            provider,
            endpoint: if provider == Provider::OpenAi {
                "http://localhost:11434/v1".into()
            } else {
                String::new()
            },
            ..on()
        };
        edit(
            &mut g,
            &cfg,
            if provider == Provider::Anthropic {
                "secret"
            } else {
                ""
            },
            &mut |url, headers, body| {
                let b: Value = serde_json::from_str(body).unwrap();
                assert_eq!(b["max_tokens"], 8192);
                if provider == Provider::Anthropic {
                    assert_eq!(url, "https://api.anthropic.com/v1/messages");
                    assert!(headers.contains(&("x-api-key".into(), "secret".into())));
                    assert!(b["system"].as_str().unwrap().contains("untrusted"));
                    Ok(json!({"stop_reason":"end_turn","content":[{"type":"text","text":answer().to_string()}]}).to_string())
                } else {
                    assert_eq!(url, "http://localhost:11434/v1/chat/completions");
                    assert!(!headers.iter().any(|(k, _)| k == "authorization"));
                    Ok(envelope(answer()))
                }
            },
            &[],
            &[],
        );
        assert!(g.notes.is_empty());
        assert!(g.edition.sections[1].articles.is_empty());
    }
}

#[test]
fn settings_roundtrip_without_credentials_and_reject_secret_fields() {
    let tmp = tempfile::tempdir().unwrap();
    let file = tmp.path().join("digest.toml");
    let mut c = config();
    c.llm = on();
    c.llm.model = "chosen-model".into();
    deckle_digest::config::save(&file, &c).unwrap();
    assert_eq!(Config::load(&file).unwrap().llm, c.llm);
    let text = std::fs::read_to_string(&file).unwrap();
    assert!(!text.contains("api_key"));
    assert!(Config::parse("[llm]\napi_key = 'secret'").is_err());
    assert!(Config::parse("[llm]\nendpoint = 'https://secret@host/v1'").is_err());
    c.llm.enabled = false;
    deckle_digest::config::save(&file, &c).unwrap();
    assert!(!Config::load(&file).unwrap().llm.enabled);
}

// ---- interests (interests design §6): relevance scores ----

/// `config()` plus an interest section "Rust" between the feeds and the horoscope: sections
/// s0 First, s1 Second, s2 Rust, s3 Horoscope.
fn config_with_interest() -> Config {
    let mut c = config();
    c.sections = vec![
        SectionKind::Feeds,
        SectionKind::Interests,
        SectionKind::Horoscope,
    ];
    c.interests = vec![deckle_digest::config::InterestConfig {
        name: "Rust".into(),
        also: vec!["Cargo".into()],
        not: vec!["corrosion".into()],
        max_items: 5,
        max_age_hours: 72,
        full_text: Some(deckle_digest::config::FullText::No),
        ..Default::default()
    }];
    c
}

/// `feed()` plus the two Google News searches of the interest: three stories, newest first
/// "Rust 2.0 lands", "Rust Belt voters", "Cargo workspaces".
fn feed_with_interest(url: &str) -> Result<Vec<u8>, String> {
    if !url.contains("news.google.com") {
        return feed(url);
    }
    let items: &[(&str, &str, &str)] = if url.contains("Cargo") {
        &[("Cargo workspaces - Outlet", "https://x.test/c", "08:00:00")]
    } else {
        &[
            ("Rust 2.0 lands - Outlet", "https://x.test/a", "10:00:00"),
            ("Rust Belt voters - Outlet", "https://x.test/b", "09:00:00"),
        ]
    };
    let body: String = items
        .iter()
        .map(|(t, l, h)| {
            format!(
                "<item><title>{t}</title><link>{l}</link><pubDate>Sat, 24 Oct 2026 {h} GMT</pubDate></item>"
            )
        })
        .collect();
    Ok(
        format!("<rss version=\"2.0\"><channel><title>G</title>{body}</channel></rss>")
            .into_bytes(),
    )
}

fn original_with_interest() -> Gathered {
    gather_with_editor(
        &config_with_interest(),
        date(),
        now(),
        &feed_with_interest,
        &GatherOptions::default(),
        "",
        &mut |_, _, _| panic!("disabled editor called provider"),
    )
}

/// A valid answer for `config_with_interest()`, with the given relevance scores for s2.
fn answer_with_interest(relevance: Value) -> Value {
    json!({
        "duplicate_groups": [],
        "briefs": [
            {"section":"s0", "lines":["A park was approved."]},
            {"section":"s1", "lines":["The same park was approved."]},
            {"section":"s2", "lines":["Rust news."]}
        ],
        "horoscopes": [{"id":"s3a0", "text":"A quiet walk."}],
        "relevance": relevance
    })
}

fn titles(g: &Gathered, s: usize) -> Vec<String> {
    g.edition.sections[s]
        .articles
        .iter()
        .map(|a| a.title.clone())
        .collect()
}

fn edit_interest(g: &mut Gathered, body_check: impl Fn(&str), answer: Value) {
    edit_naming(g, body_check, answer, &[]);
}

fn edit_naming(
    g: &mut Gathered,
    body_check: impl Fn(&str),
    answer: Value,
    naming: &[deckle_digest::NamingCandidate],
) {
    let cfg = config_with_interest();
    deckle_digest::editor::edit(
        g,
        &on(),
        "secret",
        &mut |_, _, body| {
            body_check(body);
            Ok(envelope(answer.clone()))
        },
        &cfg.interests,
        naming,
    );
}

#[test]
fn interest_sections_carry_their_phrases_in_the_prompt() {
    let mut g = original_with_interest();
    assert_eq!(
        titles(&g, 2),
        ["Rust 2.0 lands", "Rust Belt voters", "Cargo workspaces"]
    );
    edit_interest(
        &mut g,
        |body| {
            // the user message is a JSON string inside the request JSON
            let b: Value = serde_json::from_str(body).unwrap();
            let user = b["messages"][1]["content"].as_str().unwrap().to_owned();
            assert!(
                user.contains("\"interest\":{\"also\":[\"Cargo\"],\"name\":\"Rust\"}"),
                "{user}"
            );
            assert!(!body.contains("corrosion"), "the not list stays home");
        },
        answer_with_interest(json!([])),
    );
    assert!(g.notes.is_empty(), "{:?}", g.notes);
}

#[test]
fn relevance_drops_zeros_and_orders_by_score_then_recency() {
    let mut g = original_with_interest();
    edit_interest(
        &mut g,
        |_| {},
        answer_with_interest(json!([{"section":"s2","scores":[
            {"id":"s2a0","score":1},{"id":"s2a1","score":0},{"id":"s2a2","score":2}]}])),
    );
    assert!(g.notes.is_empty(), "{:?}", g.notes);
    assert_eq!(titles(&g, 2), ["Cargo workspaces", "Rust 2.0 lands"]);
    assert_eq!(g.edition.sections[2].brief, ["Rust news."]);
    // an unscored story keeps 1: a lone 2 goes first, the two unscored keep their order
    let mut g = original_with_interest();
    edit_interest(
        &mut g,
        |_| {},
        answer_with_interest(json!([{"section":"s2","scores":[{"id":"s2a2","score":2}]}])),
    );
    assert_eq!(
        titles(&g, 2),
        ["Cargo workspaces", "Rust 2.0 lands", "Rust Belt voters"]
    );
}

#[test]
fn all_zero_leaves_the_note_and_no_lead() {
    let mut g = original_with_interest();
    edit_interest(
        &mut g,
        |_| {},
        answer_with_interest(json!([{"section":"s2","scores":[
            {"id":"s2a0","score":0},{"id":"s2a1","score":0},{"id":"s2a2","score":0}]}])),
    );
    assert!(g.notes.is_empty(), "{:?}", g.notes);
    assert!(g.edition.sections[2].articles.is_empty());
    assert_eq!(
        g.edition.sections[2].unavailable.as_deref(),
        Some("Nothing new about Rust today.")
    );
    assert!(g.edition.sections[2].brief.is_empty());
    let front = snapshot(&g)
        .into_iter()
        .find(|(p, _)| p.ends_with("front.xhtml"))
        .map(|(_, b)| String::from_utf8(b).unwrap())
        .unwrap();
    // section index 2 is s3.xhtml; no tile or lead may point into it
    assert!(!front.contains("s3.xhtml#a"), "{front}");
}

#[test]
fn scores_for_a_feed_section_or_a_foreign_story_invalidate_the_answer() {
    for relevance in [
        json!([{"section":"s0","scores":[{"id":"s0a0","score":2}]}]),
        json!([{"section":"s2","scores":[{"id":"s0a0","score":2}]}]),
        json!([{"section":"s2","scores":[{"id":"s2a0","score":3}]}]),
        json!([{"section":"s2","scores":[{"id":"s2a0","score":1},{"id":"s2a0","score":2}]}]),
    ] {
        let before = original_with_interest();
        let mut g = original_with_interest();
        edit_interest(&mut g, |_| {}, answer_with_interest(relevance));
        assert_eq!(g.notes.len(), 1, "{:?}", g.notes);
        assert_eq!(snapshot(&g), snapshot(&before));
    }
}

#[test]
fn an_old_shaped_answer_still_applies() {
    let mut g = original_with_interest();
    let mut answer = answer_with_interest(json!([]));
    answer.as_object_mut().unwrap().remove("relevance");
    edit_interest(&mut g, |_| {}, answer);
    assert!(g.notes.is_empty(), "{:?}", g.notes);
    assert_eq!(g.edition.sections[0].brief, ["A park was approved."]);
    assert_eq!(
        titles(&g, 2),
        ["Rust 2.0 lands", "Rust Belt voters", "Cargo workspaces"]
    );
}

#[test]
fn a_duplicate_removal_and_a_score_on_the_same_story_do_not_panic() {
    let mut g = original_with_interest();
    let mut answer = answer_with_interest(json!([{"section":"s2","scores":[
        {"id":"s2a0","score":2},{"id":"s2a1","score":2},{"id":"s2a2","score":0}]}]));
    // the park story in s0 and the Rust 2.0 story in s2 "cover the same event": s2a0 is removed
    answer["duplicate_groups"] = json!([["s0a0", "s2a0"]]);
    edit_interest(&mut g, |_| {}, answer);
    assert!(g.notes.is_empty(), "{:?}", g.notes);
    assert_eq!(titles(&g, 2), ["Rust Belt voters"]);
}

// ---- naming what Deckle noticed (interests design §6) ----

fn candidate(key: &str, phrase: &str, headlines: &[&str]) -> deckle_digest::NamingCandidate {
    deckle_digest::NamingCandidate {
        key: key.into(),
        phrase: phrase.into(),
        headlines: headlines.iter().map(|h| (*h).to_owned()).collect(),
    }
}

#[test]
fn candidates_go_in_and_names_come_out() {
    let naming = [
        candidate("young", "Young", &["Young throws four", "Young benched"]),
        candidate("panthers", "Panthers", &["Panthers sign a kicker"]),
    ];
    let mut g = original_with_interest();
    let mut answer = answer_with_interest(json!([]));
    answer["names"] = json!([
        {"key":"young","name":"Bryce Young","query":"\"Bryce Young\" Panthers"},
        {"key":"panthers","name":"Carolina Panthers","query":"\"Carolina Panthers\""}
    ]);
    edit_naming(
        &mut g,
        |body| {
            let b: Value = serde_json::from_str(body).unwrap();
            let user = b["messages"][1]["content"].as_str().unwrap().to_owned();
            assert!(user.contains("\"candidates\":["), "{user}");
            assert!(
                user.contains("Young throws four") && user.contains("\"phrase\":\"Panthers\""),
                "{user}"
            );
        },
        answer,
        &naming,
    );
    assert!(g.notes.is_empty(), "{:?}", g.notes);
    assert_eq!(
        g.names,
        [
            deckle_digest::Naming {
                key: "young".into(),
                name: "Bryce Young".into(),
                query: "\"Bryce Young\" Panthers".into()
            },
            deckle_digest::Naming {
                key: "panthers".into(),
                name: "Carolina Panthers".into(),
                query: "\"Carolina Panthers\"".into()
            },
        ]
    );
    // an unknown key, a duplicate, or an empty name invalidates everything: no names, no edits
    for names in [
        json!([{"key":"nobody","name":"X","query":"x"}]),
        json!([{"key":"young","name":"A","query":"a"},{"key":"young","name":"B","query":"b"}]),
        json!([{"key":"young","name":"","query":"a"}]),
    ] {
        let before = original_with_interest();
        let mut g = original_with_interest();
        let mut answer = answer_with_interest(json!([]));
        answer["names"] = names;
        edit_naming(&mut g, |_| {}, answer, &naming);
        assert_eq!(g.notes.len(), 1);
        assert!(g.names.is_empty());
        assert_eq!(snapshot(&g), snapshot(&before));
    }
}

#[test]
fn more_than_ten_candidates_send_the_first_ten_and_no_candidates_send_none() {
    let naming: Vec<_> = (0..12)
        .map(|n| candidate(&format!("k{n}"), &format!("Phrase{n}"), &["h"]))
        .collect();
    let mut g = original_with_interest();
    edit_naming(
        &mut g,
        |body| {
            let b: Value = serde_json::from_str(body).unwrap();
            let user = b["messages"][1]["content"].as_str().unwrap().to_owned();
            assert!(
                user.contains("\"key\":\"k9\"") && !user.contains("\"key\":\"k10\""),
                "{user}"
            );
        },
        answer_with_interest(json!([])),
        &naming,
    );
    assert!(g.notes.is_empty());
    let mut g = original_with_interest();
    edit_naming(
        &mut g,
        |body| assert!(!body.contains("candidates"), "{body}"),
        answer_with_interest(json!([])),
        &[],
    );
    assert!(g.notes.is_empty() && g.names.is_empty());
}

#[test]
fn an_answer_without_names_leaves_names_empty() {
    let naming = [candidate("young", "Young", &["Young throws four"])];
    let mut g = original_with_interest();
    edit_naming(&mut g, |_| {}, answer_with_interest(json!([])), &naming);
    assert!(g.notes.is_empty());
    assert!(g.names.is_empty());
}
