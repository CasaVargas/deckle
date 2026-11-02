// SPDX-License-Identifier: GPL-3.0-or-later
//! Optional LLM pass: given the heuristic candidates, an LLM writes spoiler-free one-line
//! descriptions, fixes person/term kinds and names aliases. Anthropic Messages, Ollama's own
//! API, or any OpenAI-compatible chat endpoint (OpenAI, LM Studio, vLLM, …). The HTTP call is
//! injected so the logic is testable offline.
//!
//! Ollama gets its native API (`/api/chat`) rather than its OpenAI shim because only the
//! native one takes the context size, a JSON schema for the answer and a thinking switch per
//! request, and only it lists installed models and downloads one with progress
//! (local-model design §4).

use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::asc::EntityKind;
use crate::extract::Candidate;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Provider {
    Anthropic,
    OpenAi,
    /// Ollama on this computer (or another on the network), through its native API.
    Ollama,
}

impl Provider {
    pub fn default_model(self) -> &'static str {
        match self {
            Provider::Anthropic => "claude-sonnet-5-5",
            Provider::OpenAi => "gpt-5-mini",
            Provider::Ollama => RECOMMENDED_MODELS[0].name,
        }
    }
    pub fn default_endpoint(self) -> &'static str {
        match self {
            Provider::Anthropic => "https://api.anthropic.com",
            Provider::OpenAi => "https://api.openai.com/v1",
            Provider::Ollama => "http://localhost:11434",
        }
    }
    /// The environment variable holding the key. Ollama needs none; the variable only serves a
    /// proxy or Ollama's hosted service put in front of it.
    pub fn env_key(self) -> &'static str {
        match self {
            Provider::Anthropic => "ANTHROPIC_API_KEY",
            Provider::OpenAi => "OPENAI_API_KEY",
            Provider::Ollama => "OLLAMA_API_KEY",
        }
    }
}

/// Whether requests stay on this computer or the owner's own network: Ollama always, an
/// OpenAI-compatible server at any address but OpenAI's own. Local servers need no key, get
/// smaller requests and are asked not to "think" first.
pub fn is_local(provider: Provider, endpoint: &str) -> bool {
    match provider {
        Provider::Ollama => true,
        Provider::OpenAi => {
            let base = endpoint.trim().trim_end_matches('/');
            !base.is_empty() && base != Provider::OpenAi.default_endpoint()
        }
        Provider::Anthropic => false,
    }
}

/// A model Deckle recommends for Ollama, with the memory it wants and its download size.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Recommended {
    pub name: &'static str,
    /// Total memory the machine should have, in bytes.
    pub min_memory: u64,
    /// About, in bytes, as shown before a download.
    pub download: u64,
}

const GIB: u64 = 1 << 30;

/// The models Deckle recommends, smallest first (local-model design §5): the same family the
/// repo's X-Ray and recap measurements were made with. Pinned, so the setting never asks for a
/// tag; the fixtures must pass before either changes.
pub const RECOMMENDED_MODELS: [Recommended; 2] = [
    Recommended {
        name: "qwen3:4b",
        min_memory: 0,
        download: 2_600_000_000,
    },
    Recommended {
        name: "qwen3:8b",
        min_memory: 16 * GIB,
        download: 5_200_000_000,
    },
];

/// The recommended model for a machine with `total_memory` bytes (`None`: unknown, the
/// smallest).
pub fn recommended_model(total_memory: Option<u64>) -> Recommended {
    let mem = total_memory.unwrap_or(0);
    RECOMMENDED_MODELS
        .iter()
        .rev()
        .find(|m| mem >= m.min_memory)
        .copied()
        .unwrap_or(RECOMMENDED_MODELS[0])
}

/// This machine's memory, when the OS says. Linux reads `/proc/meminfo`; macOS asks `sysctl`;
/// Windows asks PowerShell. Anything else is unknown.
pub fn total_memory() -> Option<u64> {
    #[cfg(target_os = "linux")]
    {
        let info = std::fs::read_to_string("/proc/meminfo").ok()?;
        let kb: u64 = info
            .lines()
            .find_map(|l| l.strip_prefix("MemTotal:"))?
            .split_whitespace()
            .next()?
            .parse()
            .ok()?;
        return Some(kb * 1024);
    }
    #[cfg(target_os = "macos")]
    {
        let out = std::process::Command::new("sysctl")
            .args(["-n", "hw.memsize"])
            .output()
            .ok()?;
        return String::from_utf8_lossy(&out.stdout).trim().parse().ok();
    }
    #[cfg(target_os = "windows")]
    {
        let out = std::process::Command::new("powershell")
            .args([
                "-NoProfile",
                "-Command",
                "(Get-CimInstance Win32_ComputerSystem).TotalPhysicalMemory",
            ])
            .output()
            .ok()?;
        return String::from_utf8_lossy(&out.stdout).trim().parse().ok();
    }
    #[allow(unreachable_code)]
    None
}

/// Where to get Ollama.
pub const OLLAMA_DOWNLOAD: &str = "https://ollama.com/download";

/// Per-request knobs Ollama honours (the other providers ignore them).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Options {
    /// The context window to load the model with; `None` is [`OLLAMA_CONTEXT`].
    pub num_ctx: Option<u64>,
    /// A JSON schema the answer must follow.
    pub schema: Option<serde_json::Value>,
}

/// Ollama's context for a request that doesn't say: an X-Ray batch or a recap part with their
/// answer, with room to spare, on a 4B model's memory. (Ollama's own default is 4,096, which
/// its OpenAI shim can't raise.)
pub const OLLAMA_CONTEXT: u64 = 8192;
/// Keep the model loaded between a book's batches.
const OLLAMA_KEEP_ALIVE: &str = "10m";

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
/// `url -> response body`. Production uses [`http_get`].
pub type Get<'a> = dyn FnMut(&str) -> Result<String, String> + 'a;
/// `(url, json body, line)`: POSTs and hands each line of a streamed answer to `line` as it
/// arrives, until `line` answers `false`. Production uses [`http_post_lines`].
pub type Lines<'a> = dyn FnMut(&str, &str, &mut dyn FnMut(&str) -> bool) -> Result<(), String> + 'a;

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

/// What an X-Ray answer must look like, for a server that can be held to a schema.
pub fn entity_schema() -> serde_json::Value {
    serde_json::json!({
        "type": "array",
        "items": {
            "type": "object",
            "properties": {
                "name": {"type": "string"},
                "kind": {"type": "string", "enum": ["person", "term"]},
                "description": {"type": "string"},
                "aliases": {"type": "array", "items": {"type": "string"}}
            },
            "required": ["name", "kind", "description", "aliases"]
        }
    })
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
        Provider::Ollama => ollama_body(
            model,
            SYSTEM,
            &user,
            max_tokens,
            &Options {
                num_ctx: None,
                schema: Some(entity_schema()),
            },
        ),
    }
}

/// An `/api/chat` request: one answer, no thinking, the context and schema asked for.
fn ollama_body(
    model: &str,
    system: &str,
    user: &str,
    max_tokens: u64,
    opts: &Options,
) -> serde_json::Value {
    let mut body = serde_json::json!({
        "model": model,
        "messages": [
            {"role": "system", "content": system},
            {"role": "user", "content": user},
        ],
        "stream": false,
        "think": false,
        "keep_alive": OLLAMA_KEEP_ALIVE,
        "options": {
            "num_ctx": opts.num_ctx.unwrap_or(OLLAMA_CONTEXT),
            "num_predict": max_tokens,
        },
    });
    if let Some(schema) = &opts.schema {
        body["format"] = schema.clone();
    }
    body
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
    complete_with(
        provider,
        model,
        api_key,
        endpoint,
        system,
        user,
        max_tokens,
        &Options::default(),
        post,
    )
}

/// [`complete`] with the context size and answer schema a provider can be asked for.
#[allow(clippy::too_many_arguments)]
pub fn complete_with(
    provider: Provider,
    model: &str,
    api_key: &str,
    endpoint: &str,
    system: &str,
    user: &str,
    max_tokens: u64,
    opts: &Options,
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
        Provider::Ollama => ollama_body(model, system, user, max_tokens, opts),
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
    // A self-hosted OpenAI-compatible server (LM Studio, Ollama's shim): ask reasoning models
    // such as qwen3 not to "think" first, which roughly doubles a local model's output. (The
    // native Ollama request says `think: false` instead.)
    let local = provider == Provider::OpenAi && is_local(provider, base);
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
        Provider::Ollama => {
            if !api_key.is_empty() {
                headers.push(("authorization".to_owned(), format!("Bearer {api_key}")));
            }
            format!("{base}/api/chat")
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
    if let Some(e) = v["error"].as_str() {
        // Ollama answers 200 with {"error"} for a model that isn't installed
        return Err(LlmError::Http(e.to_owned()));
    }
    let truncated = match provider {
        Provider::Anthropic => v["stop_reason"].as_str() == Some("max_tokens"),
        Provider::OpenAi => v["choices"][0]["finish_reason"].as_str() == Some("length"),
        Provider::Ollama => v["done_reason"].as_str() == Some("length"),
    };
    let text = match provider {
        Provider::Anthropic => v["content"]
            .as_array()
            .and_then(|a| a.iter().find_map(|c| c["text"].as_str()))
            .map(str::to_owned),
        Provider::OpenAi => v["choices"][0]["message"]["content"]
            .as_str()
            .map(str::to_owned),
        Provider::Ollama => v["message"]["content"].as_str().map(str::to_owned),
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
        .tls_config(tls())
        .build()
        .send(body)
        .map_err(|e| e.to_string())?;
    resp.body_mut().read_to_string().map_err(|e| e.to_string())
}

/// The platform's certificate store, so TLS-inspecting proxies the machine trusts work.
fn tls() -> ureq::tls::TlsConfig {
    ureq::tls::TlsConfig::builder()
        .root_certs(ureq::tls::RootCerts::PlatformVerifier)
        .build()
}

/// Real HTTP GET (ureq), for use as the `get` argument.
pub fn http_get(url: &str) -> Result<String, String> {
    let mut resp = ureq::get(url)
        .config()
        .timeout_global(Some(std::time::Duration::from_secs(10)))
        .tls_config(tls())
        .build()
        .call()
        .map_err(|e| e.to_string())?;
    resp.body_mut().read_to_string().map_err(|e| e.to_string())
}

/// Real streaming HTTP POST (ureq), for use as the `lines` argument: no overall timeout, since
/// a model download takes as long as it takes; a stall of a minute ends it. Dropping the
/// connection when `line` says stop is how Ollama learns to stop downloading.
pub fn http_post_lines(
    url: &str,
    body: &str,
    line: &mut dyn FnMut(&str) -> bool,
) -> Result<(), String> {
    use std::io::BufRead;
    let mut resp = ureq::post(url)
        .config()
        .timeout_connect(Some(std::time::Duration::from_secs(10)))
        .timeout_recv_body(Some(std::time::Duration::from_secs(60)))
        .tls_config(tls())
        .build()
        .header("content-type", "application/json")
        .send(body)
        .map_err(|e| e.to_string())?;
    let reader = std::io::BufReader::new(resp.body_mut().as_reader());
    for l in reader.lines() {
        let l = l.map_err(|e| e.to_string())?;
        if !l.trim().is_empty() && !line(&l) {
            break;
        }
    }
    Ok(())
}

/// A model Ollama has on disk.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct InstalledModel {
    pub name: String,
    /// Bytes on disk.
    pub size: u64,
}

/// What an Ollama server says about itself.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Server {
    pub version: String,
    pub models: Vec<InstalledModel>,
}

impl Server {
    /// Whether `model` is installed; a bare name matches its `:latest` tag and vice versa.
    pub fn has(&self, model: &str) -> bool {
        let want = model.trim().trim_end_matches(":latest");
        self.models
            .iter()
            .any(|m| m.name.trim_end_matches(":latest") == want)
    }
}

/// Asks the Ollama at `endpoint` (empty: the default) its version and installed models. The
/// error is for a person: the server isn't running, or isn't Ollama.
pub fn probe(endpoint: &str, get: &mut Get<'_>) -> Result<Server, String> {
    let base = ollama_base(endpoint);
    let version = get(&format!("{base}/api/version"))
        .map_err(|e| format!("Ollama isn't answering at {base} ({e})"))
        .and_then(|raw| {
            serde_json::from_str::<serde_json::Value>(&raw)
                .ok()
                .and_then(|v| v["version"].as_str().map(str::to_owned))
                .ok_or_else(|| format!("{base} answered, but not like Ollama"))
        })?;
    let raw =
        get(&format!("{base}/api/tags")).map_err(|e| format!("Ollama's model list failed: {e}"))?;
    let v: serde_json::Value =
        serde_json::from_str(&raw).map_err(|_| "Ollama's model list wasn't JSON".to_owned())?;
    let models = v["models"]
        .as_array()
        .map(|a| {
            a.iter()
                .filter_map(|m| {
                    Some(InstalledModel {
                        name: m["name"].as_str()?.to_owned(),
                        size: m["size"].as_u64().unwrap_or(0),
                    })
                })
                .collect()
        })
        .unwrap_or_default();
    Ok(Server { version, models })
}

/// Where a download stands: Ollama's status line and, while a layer transfers, its bytes.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Progress {
    pub status: String,
    pub completed: u64,
    pub total: u64,
}

/// What stopped a download.
pub const PULL_CANCELLED: &str = "the download was stopped";

/// Downloads `model` into the Ollama at `endpoint`, reporting each status line as it comes;
/// `progress` answers `false` to stop (the error is then [`PULL_CANCELLED`]). Returns when
/// Ollama says success; an error line (no such model, no disk) is the error.
pub fn pull(
    endpoint: &str,
    model: &str,
    lines: &mut Lines<'_>,
    progress: &mut dyn FnMut(&Progress) -> bool,
) -> Result<(), String> {
    let base = ollama_base(endpoint);
    let body = serde_json::json!({"model": model, "stream": true}).to_string();
    let mut failed = None;
    let mut done = false;
    let mut stopped = false;
    lines(&format!("{base}/api/pull"), &body, &mut |line| {
        let Ok(v) = serde_json::from_str::<serde_json::Value>(line) else {
            return true;
        };
        if let Some(e) = v["error"].as_str() {
            failed.get_or_insert_with(|| e.to_owned());
            return true;
        }
        let p = Progress {
            status: v["status"].as_str().unwrap_or_default().to_owned(),
            completed: v["completed"].as_u64().unwrap_or(0),
            total: v["total"].as_u64().unwrap_or(0),
        };
        if p.status == "success" {
            done = true;
        }
        if !progress(&p) {
            stopped = true;
            return false;
        }
        true
    })
    .map_err(|e| format!("the download failed: {e}"))?;
    match (failed, done, stopped) {
        (Some(e), _, _) => Err(format!("Ollama couldn't download {model}: {e}")),
        (None, true, _) => Ok(()),
        (None, false, true) => Err(PULL_CANCELLED.to_owned()),
        (None, false, false) => Err(format!("the download of {model} ended early")),
    }
}

/// The three checks the setting runs (local-model design §3): Ollama answers, the model is
/// installed, a one-line prompt comes back.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Check {
    pub endpoint: String,
    pub model: String,
    /// The server, or why it couldn't be reached.
    pub server: Result<Server, String>,
    /// Whether the model is installed; `None` when the server wasn't reached.
    pub installed: Option<bool>,
    /// How long the test prompt took, in milliseconds, or why it failed; `None` when not tried.
    pub answered: Option<Result<u64, String>>,
}

impl Check {
    pub fn ok(&self) -> bool {
        matches!(self.answered, Some(Ok(_)))
    }
}

/// Runs the three checks against the Ollama at `endpoint` with `model` (empty: the recommended
/// one for this machine).
pub fn check(endpoint: &str, model: &str, get: &mut Get<'_>, post: &mut Post<'_>) -> Check {
    let model = if model.trim().is_empty() {
        recommended_model(total_memory()).name.to_owned()
    } else {
        model.trim().to_owned()
    };
    let endpoint = ollama_base(endpoint);
    let server = probe(&endpoint, get);
    let installed = server.as_ref().ok().map(|s| s.has(&model));
    let answered = (installed == Some(true)).then(|| {
        let start = std::time::Instant::now();
        complete_with(
            Provider::Ollama,
            &model,
            "",
            &endpoint,
            "Answer with one word.",
            "Say OK.",
            16,
            &Options {
                num_ctx: Some(2048),
                schema: None,
            },
            post,
        )
        .map(|_| start.elapsed().as_millis() as u64)
        .map_err(|e| e.to_string())
    });
    Check {
        endpoint,
        model,
        server,
        installed,
        answered,
    }
}

fn ollama_base(endpoint: &str) -> String {
    let e = endpoint.trim().trim_end_matches('/');
    if e.is_empty() {
        Provider::Ollama.default_endpoint().to_owned()
    } else {
        e.to_owned()
    }
}
