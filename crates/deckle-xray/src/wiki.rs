// SPDX-License-Identifier: GPL-3.0-or-later
//! Opt-in Wikipedia descriptions for terms (places, works, real people) via the REST summary
//! endpoint. Fetch is injected for offline tests.

pub type Fetch<'a> = dyn FnMut(&str) -> Result<String, String> + 'a;

fn encode(title: &str) -> String {
    let mut s = String::new();
    for b in title.replace(' ', "_").bytes() {
        match b {
            b'A'..=b'Z'
            | b'a'..=b'z'
            | b'0'..=b'9'
            | b'-'
            | b'_'
            | b'.'
            | b'~'
            | b'('
            | b')'
            | b','
            | b'\'' => s.push(b as char),
            _ => s.push_str(&format!("%{b:02X}")),
        }
    }
    s
}

pub fn summary_url(lang: &str, title: &str) -> String {
    format!(
        "https://{lang}.wikipedia.org/api/rest_v1/page/summary/{}",
        encode(title)
    )
}

/// The article's lead extract, or None when there is no article, it is a disambiguation
/// page, or the request failed.
pub fn summary(lang: &str, title: &str, fetch: &mut Fetch<'_>) -> Option<String> {
    let raw = fetch(&summary_url(lang, title)).ok()?;
    let v: serde_json::Value = serde_json::from_str(&raw).ok()?;
    if v["type"].as_str() != Some("standard") {
        return None;
    }
    let extract = v["extract"].as_str()?.trim();
    if extract.is_empty() {
        return None;
    }
    Some(extract.to_owned())
}

/// Real HTTP GET for use as the `fetch` argument.
pub fn http_get(url: &str) -> Result<String, String> {
    let mut resp = ureq::get(url)
        .header(
            "user-agent",
            "Deckle/0.1 (https://github.com/prjoni99/deckle)",
        )
        .config()
        .timeout_global(Some(std::time::Duration::from_secs(20)))
        .build()
        .call()
        .map_err(|e| e.to_string())?;
    resp.body_mut().read_to_string().map_err(|e| e.to_string())
}
