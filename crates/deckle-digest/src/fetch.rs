// SPDX-License-Identifier: GPL-3.0-or-later
//! HTTP for the sources. Tests pass their own [`Get`] that serves fixtures by URL.

use std::time::Duration;

/// GET a URL, returning the body.
pub type Get<'a> = dyn FnMut(&str) -> Result<Vec<u8>, String> + 'a;

const USER_AGENT: &str = concat!(
    "Deckle/",
    env!("CARGO_PKG_VERSION"),
    " (+https://github.com/prjoni99/deckle)"
);

/// Real HTTP GET with a 15 s budget per request.
pub fn http_get(url: &str) -> Result<Vec<u8>, String> {
    let agent = ureq::Agent::config_builder()
        .timeout_global(Some(Duration::from_secs(15)))
        .user_agent(USER_AGENT)
        .build()
        .new_agent();
    let mut resp = agent.get(url).call().map_err(|e| e.to_string())?;
    resp.body_mut()
        .with_config()
        .limit(20 * 1024 * 1024)
        .read_to_vec()
        .map_err(|e| e.to_string())
}

/// Percent-encodes a query value.
pub fn encode(s: &str) -> String {
    let mut out = String::new();
    for b in s.bytes() {
        match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                out.push(b as char)
            }
            b' ' => out.push('+'),
            _ => out.push_str(&format!("%{b:02X}")),
        }
    }
    out
}
