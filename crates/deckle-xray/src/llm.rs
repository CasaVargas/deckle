// SPDX-License-Identifier: GPL-3.0-or-later
//! Optional LLM pass: given the heuristic candidates, an LLM writes spoiler-free one-line
//! descriptions, fixes person/term kinds and names aliases. Anthropic Messages or any
//! OpenAI-compatible chat endpoint (OpenAI, Ollama, LM Studio, …). The HTTP call is
//! injected so the logic is testable offline.

use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::asc::EntityKind;
use crate::extract::Candidate;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Provider {
    Anthropic,
    OpenAi,
}

impl Provider {
    pub fn default_model(self) -> &'static str {
        match self {
            Provider::Anthropic => "claude-sonnet-5-5",
            Provider::OpenAi => "gpt-5-mini",
        }
    }
    pub fn default_endpoint(self) -> &'static str {
        match self {
            Provider::Anthropic => "https://api.anthropic.com",
            Provider::OpenAi => "https://api.openai.com/v1",
        }
    }
    pub fn env_key(self) -> &'static str {
        match self {
            Provider::Anthropic => "ANTHROPIC_API_KEY",
            Provider::OpenAi => "OPENAI_API_KEY",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Described {
    pub name: String,
    /// `None` when the model did not say: keep the heuristic's guess.
    pub kind: Option<EntityKind>,
    pub description: String,
    pub aliases: Vec<String>,
}

#[derive(Debug, Error)]
pub enum LlmError {
    #[error("LLM request failed: {0}")]
    Http(String),
    #[error("LLM answer had no content")]
    NoContent,
    #[error("LLM answer was not a JSON array of entities")]
    NotJson,
    #[error(
        "LLM answer was truncated by the model's output limit; raise max tokens or use a bigger model"
    )]
    Truncated,
    #[error("bad LLM response: {0}")]
    Decode(#[from] serde_json::Error),
}

/// `(url, headers, json body) -> response body`. Production uses [`http_post`].
pub type Post<'a> = dyn FnMut(&str, &[(String, String)], &str) -> Result<String, String> + 'a;

/// How many candidates we send: enough for a novel's cast, cheap enough for every run.
pub const MAX_CANDIDATES: usize = 120;
/// Candidates per request: fits a local model's default 4k context (Ollama) and answers well
/// inside the per-request timeout at ~16 tok/s.
pub const BATCH_SIZE: usize = 25;

const SYSTEM: &str = "You annotate novels for an e-reader's X-Ray feature. Answer with a JSON array only, no prose, no markdown.";

fn prompt(title: &str, author: &str, cands: &[Candidate], lang: &str) -> String {
    let by = if author.is_empty() {
        String::new()
    } else {
        format!("Author: {author}\n")
    };
    let mut p = format!(
        "Book: {title}\n{by}Language: {lang}\n\nBelow are recurring proper nouns found in the text, \
each with its first-mention sentence. For each one, reply with an object \
{{\"name\", \"kind\", \"description\", \"aliases\"}}:\n\
- name: copy the input name exactly\n\
- kind: \"person\" for characters (people, named animals), \"term\" for places, \
organisations, objects, concepts. Any individual (a first name, a family member, a \
nickname) is a person. In a memoir or a first-person book the narrator is usually the \
author\n\
- description: one or two sentences, in the book's language, that help a reader who is \
partway through the book. No spoilers: describe who or what it is as introduced, never \
what happens later or how it ends. If the name is noise (a page header, a chapter word, \
a publisher, or an ordinary word capitalized at the start of a sentence such as \"Yeah\", \
\"Hey\" or \"Thank\"), set description to \"\" and it will be dropped.\n\
- aliases: other names in the list that refer to the same entity (e.g. a nickname), \
else []\n\nEntities:\n"
    );
    // No guessed kind: qwen3:8b copied the heuristic's "term" onto brothers and bodyguards.
    for c in cands.iter().take(MAX_CANDIDATES) {
        p.push_str(&format!(
            "- {} ({} mentions): \"{}\"\n",
            c.label,
            c.occurrences.len(),
            c.quote.replace('"', "'")
        ));
    }
    p.push_str("\nJSON array:");
    p
}

/// Output tokens to allow: ~120 per entity (two sentences plus JSON) with headroom.
pub fn output_budget(candidates: usize) -> u64 {
    (1024 + candidates as u64 * 120).min(16384)
}

/// The request body for a provider (public so tests can check its shape).
pub fn request_body(
    provider: Provider,
    model: &str,
    title: &str,
    author: &str,
    cands: &[Candidate],
    lang: &str,
) -> serde_json::Value {
    let user = prompt(title, author, cands, lang);
    let max_tokens = output_budget(cands.len().min(MAX_CANDIDATES));
    match provider {
        Provider::Anthropic => serde_json::json!({
            "model": model,
            "max_tokens": max_tokens,
            "system": SYSTEM,
            "messages": [{"role": "user", "content": user}],
        }),
        Provider::OpenAi => serde_json::json!({
            "model": model,
            "max_tokens": max_tokens,
            "messages": [
                {"role": "system", "content": SYSTEM},
                {"role": "user", "content": user},
            ],
        }),
    }
}

/// Finds the first JSON array in a possibly chatty / fenced answer.
pub fn extract_json_array(raw: &str) -> Option<Vec<serde_json::Value>> {
    let start = raw.find('[')?;
    let end = raw.rfind(']')?;
    if end <= start {
        return None;
    }
    serde_json::from_str::<Vec<serde_json::Value>>(&raw[start..=end]).ok()
}

#[derive(Deserialize)]
struct Item {
    name: String,
    #[serde(default)]
    kind: Option<String>,
    #[serde(default)]
    description: String,
    #[serde(default)]
    aliases: Vec<String>,
}

fn parse_items(raw: &str) -> Result<Vec<Described>, LlmError> {
    let arr = extract_json_array(raw).ok_or(LlmError::NotJson)?;
    let mut out = Vec::new();
    for v in arr {
        let Ok(it) = serde_json::from_value::<Item>(v) else {
            continue;
        };
        let kind = match it.kind.as_deref().map(str::to_ascii_lowercase).as_deref() {
            Some("term") => Some(EntityKind::Term),
            Some("person") => Some(EntityKind::Person),
            _ => None,
        };
        out.push(Described {
            name: it.name,
            kind,
            description: it.description.trim().to_owned(),
            aliases: it.aliases,
        });
    }
    Ok(out)
}

/// Asks the model to describe the candidates. `endpoint` empty = provider default;
/// `api_key` empty = no auth header (local servers).
#[allow(clippy::too_many_arguments)]
/// What [`describe_all`] got back: the answers of the batches that worked, and why the others
/// failed (their entities keep the basic descriptions).
#[derive(Debug, Default)]
pub struct Batched {
    pub items: Vec<Described>,
    pub batches: usize,
    pub failures: Vec<LlmError>,
}

/// [`describe`] for up to [`MAX_CANDIDATES`] candidates, [`BATCH_SIZE`] per request.
#[allow(clippy::too_many_arguments)]
pub fn describe_all(
    provider: Provider,
    model: &str,
    api_key: &str,
    endpoint: &str,
    title: &str,
    author: &str,
    cands: &[Candidate],
    lang: &str,
    post: &mut Post<'_>,
) -> Batched {
    let mut out = Batched::default();
    for batch in cands[..cands.len().min(MAX_CANDIDATES)].chunks(BATCH_SIZE) {
        out.batches += 1;
        match describe(
            provider, model, api_key, endpoint, title, author, batch, lang, post,
        ) {
            Ok(items) => out.items.extend(items),
            Err(e) => out.failures.push(e),
        }
    }
    out
}

/// One request for `cands`.
#[allow(clippy::too_many_arguments)]
pub fn describe(
    provider: Provider,
    model: &str,
    api_key: &str,
    endpoint: &str,
    title: &str,
    author: &str,
    cands: &[Candidate],
    lang: &str,
    post: &mut Post<'_>,
) -> Result<Vec<Described>, LlmError> {
    let body = request_body(provider, model, title, author, cands, lang);
    let (text, truncated) = send_completion(provider, api_key, endpoint, body, post)?;
    match parse_items(&text) {
        Err(LlmError::NotJson) if truncated => Err(LlmError::Truncated),
        other => other,
    }
}

/// Generic provider completion, sharing authentication, transport and response decoding
/// with X-Ray. Consumers validate their own response schema.
#[allow(clippy::too_many_arguments)]
pub fn complete(
    provider: Provider,
    model: &str,
    api_key: &str,
    endpoint: &str,
    system: &str,
    user: &str,
    max_tokens: u64,
    post: &mut Post<'_>,
) -> Result<String, LlmError> {
    let mut body = match provider {
        Provider::Anthropic => serde_json::json!({
            "model": model, "max_tokens": max_tokens, "system": system,
            "messages": [{"role": "user", "content": user}]
        }),
        Provider::OpenAi => serde_json::json!({
            "model": model, "max_tokens": max_tokens,
            "messages": [{"role": "system", "content": system}, {"role": "user", "content": user}]
        }),
    };
    // Hosted OpenAI uses the current completion budget field. Local compatible servers
    // retain max_tokens, as in the existing X-Ray path.
    if provider == Provider::OpenAi
        && (endpoint.is_empty() || endpoint.trim_end_matches('/') == provider.default_endpoint())
    {
        body.as_object_mut().unwrap().remove("max_tokens");
        body["max_completion_tokens"] = serde_json::json!(max_tokens);
    }
    let (text, truncated) = send_completion(provider, api_key, endpoint, body, post)?;
    if truncated {
        Err(LlmError::Truncated)
    } else {
        Ok(text)
    }
}

fn send_completion(
    provider: Provider,
    api_key: &str,
    endpoint: &str,
    mut body: serde_json::Value,
    post: &mut Post<'_>,
) -> Result<(String, bool), LlmError> {
    let base = if endpoint.is_empty() {
        provider.default_endpoint()
    } else {
        endpoint
    }
    .trim_end_matches('/');
    // A self-hosted OpenAI-compatible server (Ollama, LM Studio): ask reasoning models such as
    // qwen3 not to "think" first, which roughly doubles a local model's output.
    let local = provider == Provider::OpenAi && base != provider.default_endpoint();
    if local {
        body["reasoning_effort"] = serde_json::json!("none");
    }
    let mut headers = vec![("content-type".to_owned(), "application/json".to_owned())];
    let url = match provider {
        Provider::Anthropic => {
            headers.push(("x-api-key".to_owned(), api_key.to_owned()));
            headers.push(("anthropic-version".to_owned(), "2023-06-01".to_owned()));
            format!("{base}/v1/messages")
        }
        Provider::OpenAi => {
            if !api_key.is_empty() {
                headers.push(("authorization".to_owned(), format!("Bearer {api_key}")));
            }
            format!("{base}/chat/completions")
        }
    };
    let raw = match post(&url, &headers, &body.to_string()) {
        // a server that rejects the field (4xx) gets the request again without it
        Err(e) if local && e.contains("status: 4") => {
            if let Some(o) = body.as_object_mut() {
                o.remove("reasoning_effort");
            }
            post(&url, &headers, &body.to_string())
        }
        other => other,
    }
    .map_err(LlmError::Http)?;
    let v: serde_json::Value = serde_json::from_str(&raw)?;
    let truncated = match provider {
        Provider::Anthropic => v["stop_reason"].as_str() == Some("max_tokens"),
        Provider::OpenAi => v["choices"][0]["finish_reason"].as_str() == Some("length"),
    };
    let text = match provider {
        Provider::Anthropic => v["content"]
            .as_array()
            .and_then(|a| a.iter().find_map(|c| c["text"].as_str()))
            .map(str::to_owned),
        Provider::OpenAi => v["choices"][0]["message"]["content"]
            .as_str()
            .map(str::to_owned),
    }
    .ok_or(LlmError::NoContent)?;
    Ok((text, truncated))
}

/// Real HTTP POST (ureq), for use as the `post` argument.
pub fn http_post(url: &str, headers: &[(String, String)], body: &str) -> Result<String, String> {
    let mut req = ureq::post(url);
    for (k, v) in headers {
        req = req.header(k, v);
    }
    let mut resp = req
        .config()
        .timeout_global(Some(std::time::Duration::from_secs(180)))
        // The OS certificate store, so TLS-inspecting proxies (Zscaler, …) the machine trusts work.
        .tls_config(
            ureq::tls::TlsConfig::builder()
                .root_certs(ureq::tls::RootCerts::PlatformVerifier)
                .build(),
        )
        .build()
        .send(body)
        .map_err(|e| e.to_string())?;
    resp.body_mut().read_to_string().map_err(|e| e.to_string())
}
