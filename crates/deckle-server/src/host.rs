// SPDX-License-Identifier: GPL-3.0-or-later
//! Which host names the API answers to. A web page can point a name it owns at this server's
//! address after the browser has loaded it ("DNS rebinding") and then call the API under that
//! name. A request under a name that could be public, and that the owner did not list in
//! `DECKLE_WEB_HOSTS`, is refused.

use axum::extract::{Request, State};
use axum::http::StatusCode;
use axum::http::header::HOST;
use axum::middleware::Next;
use axum::response::Response;

use crate::Shared;
use crate::invoke::error;

/// The name in a `Host` value: lowercased, without its port, brackets or one trailing dot.
fn name(host: &str) -> String {
    let host = host.trim();
    let host = if let Some(rest) = host.strip_prefix('[') {
        // `[::1]:8080`
        rest.split_once(']').map_or(rest, |(ip, _)| ip)
    } else if host.parse::<std::net::IpAddr>().is_ok() {
        // A bare IPv6 address has colons of its own.
        host
    } else {
        match host.rsplit_once(':') {
            Some((name, port)) if port.bytes().all(|b| b.is_ascii_digit()) => name,
            _ => host,
        }
    };
    let host = host.strip_suffix('.').unwrap_or(host);
    host.to_ascii_lowercase()
}

/// Whether `host` (a `Host` header's value) may reach the API: an IP address, a name that
/// cannot be public, or one of `allowed` (`DECKLE_WEB_HOSTS`).
pub fn host_allowed(host: &str, allowed: &[String]) -> bool {
    let host = name(host);
    if host.is_empty() {
        return false;
    }
    // An address can't be rebound.
    if host.parse::<std::net::IpAddr>().is_ok() {
        return true;
    }
    // Nobody can buy these, so no web page can point them at us.
    let private = !host.contains('.')
        || [".localhost", ".local", ".home.arpa", ".internal"]
            .iter()
            .any(|suffix| host.ends_with(suffix));
    private || allowed.iter().any(|a| name(a) == host)
}

/// Turns away a request whose `Host` isn't [`host_allowed`] with `403`. A request with no
/// host at all didn't come from a browser and passes.
pub(crate) async fn require_allowed_host(
    State(server): State<Shared>,
    req: Request,
    next: Next,
) -> Response {
    let allowed = &server.cfg.hosts;
    let from_header = req
        .headers()
        .get_all(HOST)
        .iter()
        .all(|v| v.to_str().is_ok_and(|h| host_allowed(h, allowed)));
    let from_uri = req
        .uri()
        .authority()
        .is_none_or(|a| host_allowed(a.host(), allowed));
    if from_header && from_uri {
        next.run(req).await
    } else {
        error(
            StatusCode::FORBIDDEN,
            "this server doesn't answer to that host name; \
             add it to DECKLE_WEB_HOSTS if it is yours"
                .into(),
        )
    }
}
