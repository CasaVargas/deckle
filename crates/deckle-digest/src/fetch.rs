// SPDX-License-Identifier: GPL-3.0-or-later
//! HTTP for the sources. [`Net`] is shared by parallel article fetches, so it is `Sync`;
//! tests pass a closure that serves fixtures by URL (GET only).

use std::time::Duration;

pub trait Net: Sync {
    /// An API or feed, fetched as Deckle.
    fn get(&self, url: &str) -> Result<Vec<u8>, String>;

    /// A publisher's web page or image, fetched as a desktop browser: many sites refuse
    /// unknown agents (design §5a).
    fn get_page(&self, url: &str) -> Result<Vec<u8>, String> {
        self.get(url)
    }

    /// A page fetched with any signed-in session, and how it was got. Plain nets are open.
    fn get_page_as(&self, url: &str) -> Result<(Vec<u8>, crate::publisher::Access), String> {
        self.get_page(url)
            .map(|b| (b, crate::publisher::Access::Open))
    }

    /// A form POST (Google News link decoding).
    fn post_form(&self, url: &str, _body: &str) -> Result<Vec<u8>, String> {
        Err(format!("POST not supported: {url}"))
    }
}

/// Any `Fn(url) -> body` is a GET-only [`Net`] (tests, fixtures).
impl<F: Fn(&str) -> Result<Vec<u8>, String> + Sync> Net for F {
    fn get(&self, url: &str) -> Result<Vec<u8>, String> {
        self(url)
    }
}

const USER_AGENT: &str = concat!(
    "Deckle/",
    env!("CARGO_PKG_VERSION"),
    " (+https://github.com/CasaVargas/deckle)"
);
const BROWSER_AGENT: &str = "Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) AppleWebKit/605.1.15 \
     (KHTML, like Gecko) Version/18.0 Safari/605.1.15";
const LIMIT: u64 = 20 * 1024 * 1024;

/// Real HTTP: 15 s per API request, 25 s per publisher page (local sites can be slow).
pub struct Http;

fn agent(user_agent: &str, timeout_secs: u64) -> ureq::Agent {
    ureq::Agent::config_builder()
        .timeout_global(Some(Duration::from_secs(timeout_secs)))
        .user_agent(user_agent)
        // The OS certificate store, so TLS-inspecting proxies (Zscaler, …) the machine trusts work.
        .tls_config(
            ureq::tls::TlsConfig::builder()
                .root_certs(ureq::tls::RootCerts::PlatformVerifier)
                .build(),
        )
        .build()
        .new_agent()
}

fn read(resp: Result<ureq::http::Response<ureq::Body>, ureq::Error>) -> Result<Vec<u8>, String> {
    let mut resp = resp.map_err(|e| e.to_string())?;
    resp.body_mut()
        .with_config()
        .limit(LIMIT)
        .read_to_vec()
        .map_err(|e| e.to_string())
}

impl Net for Http {
    fn get(&self, url: &str) -> Result<Vec<u8>, String> {
        read(agent(USER_AGENT, 15).get(url).call())
    }

    fn get_page(&self, url: &str) -> Result<Vec<u8>, String> {
        read(agent(BROWSER_AGENT, 25).get(url).call())
    }

    fn post_form(&self, url: &str, body: &str) -> Result<Vec<u8>, String> {
        read(
            agent(BROWSER_AGENT, 15)
                .post(url)
                .header(
                    "Content-Type",
                    "application/x-www-form-urlencoded;charset=UTF-8",
                )
                .send(body),
        )
    }
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
