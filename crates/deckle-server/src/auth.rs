// SPDX-License-Identifier: GPL-3.0-or-later
//! One password, in-memory sessions in an `HttpOnly; SameSite=Strict` cookie, and the
//! `X-Deckle: 1` header every state-changing request carries (a cross-site form can't set it).

use std::collections::HashMap;
use std::net::SocketAddr;
use std::sync::Mutex;
use std::time::{Duration, Instant};

use axum::body::Bytes;
use axum::extract::{ConnectInfo, Request, State};
use axum::http::header::{COOKIE, SET_COOKIE};
use axum::http::{HeaderMap, HeaderValue, StatusCode};
use axum::middleware::Next;
use axum::response::{IntoResponse, Response};
use axum::{Extension, Json};
use serde::Deserialize;
use serde_json::json;
use sha2::{Digest, Sha256};
use subtle::ConstantTimeEq;

use crate::Shared;
use crate::invoke::error;

const COOKIE_NAME: &str = "deckle_session";
const SESSION_LIFE: Duration = Duration::from_secs(30 * 24 * 60 * 60);
/// Every wrong password costs this long, so guessing online is slow.
const FAILURE_DELAY: Duration = Duration::from_secs(1);
/// Password checks in flight at once; more get `429`.
pub(crate) const LOGIN_SLOTS: usize = 4;

/// Session tokens and when each expires. Restarting the server logs everyone out.
#[derive(Default)]
pub(crate) struct Sessions(Mutex<HashMap<String, Instant>>);

impl Sessions {
    fn create(&self) -> Result<String, getrandom::Error> {
        let mut bytes = [0u8; 32];
        getrandom::fill(&mut bytes)?;
        let token: String = bytes.iter().map(|b| format!("{b:02x}")).collect();
        let now = Instant::now();
        let mut map = self.0.lock().unwrap_or_else(|e| e.into_inner());
        map.retain(|_, expiry| *expiry > now);
        map.insert(token.clone(), now + SESSION_LIFE);
        Ok(token)
    }

    fn valid(&self, token: &str) -> bool {
        let mut map = self.0.lock().unwrap_or_else(|e| e.into_inner());
        match map.get(token) {
            Some(expiry) if *expiry > Instant::now() => true,
            Some(_) => {
                map.remove(token);
                false
            }
            None => false,
        }
    }

    fn remove(&self, token: &str) {
        self.0
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .remove(token);
    }
}

/// Our cookie's value, from any of the request's `Cookie` headers.
fn session_token(headers: &HeaderMap) -> Option<&str> {
    headers
        .get_all(COOKIE)
        .iter()
        .filter_map(|v| v.to_str().ok())
        .flat_map(|v| v.split(';'))
        .find_map(|pair| pair.trim().strip_prefix(COOKIE_NAME)?.strip_prefix('='))
        .filter(|t| !t.is_empty())
}

fn logged_in(server: &Shared, headers: &HeaderMap) -> bool {
    server.cfg.auth_none || session_token(headers).is_some_and(|t| server.sessions.valid(t))
}

/// `Set-Cookie` for `value`; `Secure` only when a TLS proxy says the browser came over https,
/// since a LAN install over plain http would otherwise never get its cookie back.
fn cookie(headers: &HeaderMap, value: &str, max_age: u64) -> HeaderValue {
    let https = headers
        .get("x-forwarded-proto")
        .and_then(|v| v.to_str().ok())
        .is_some_and(|v| v.eq_ignore_ascii_case("https"));
    let secure = if https { "; Secure" } else { "" };
    let s = format!(
        "{COOKIE_NAME}={value}; HttpOnly; SameSite=Strict; Path=/; Max-Age={max_age}{secure}"
    );
    HeaderValue::from_str(&s).expect("a token is hex")
}

/// Hashing both sides first gives equal-length inputs, so the comparison's time says nothing
/// about the password's length either.
fn password_matches(expected: &str, given: &str) -> bool {
    let (a, b) = (Sha256::digest(expected), Sha256::digest(given));
    a.as_slice().ct_eq(b.as_slice()).into()
}

/// Turns away a non-GET request without `X-Deckle: 1` with `403`.
pub(crate) async fn require_csrf_header(req: Request, next: Next) -> Response {
    let safe = req.method().is_safe();
    if safe || req.headers().get("x-deckle").is_some_and(|v| v == "1") {
        next.run(req).await
    } else {
        error(StatusCode::FORBIDDEN, "missing X-Deckle header".into())
    }
}

/// Turns away a request without a live session with `401`.
pub(crate) async fn require_session(
    State(server): State<Shared>,
    req: Request,
    next: Next,
) -> Response {
    if logged_in(&server, req.headers()) {
        next.run(req).await
    } else {
        error(StatusCode::UNAUTHORIZED, "not logged in".into())
    }
}

#[derive(Deserialize)]
struct Login {
    password: String,
}

/// `POST /api/login {"password"}`.
pub(crate) async fn login(
    State(server): State<Shared>,
    // Absent when served without connect info (tests).
    peer: Option<Extension<ConnectInfo<SocketAddr>>>,
    headers: HeaderMap,
    body: Bytes,
) -> Response {
    let Ok(Login { password }) = serde_json::from_slice::<Login>(&body) else {
        return error(
            StatusCode::BAD_REQUEST,
            "expected {\"password\": \"…\"}".into(),
        );
    };
    if server.cfg.auth_none {
        return Json(json!({ "ok": true })).into_response();
    }
    // A few checks at a time, each failure holding its slot for the delay: guessing in
    // parallel gets LOGIN_SLOTS tries a second, not one per connection.
    let Ok(permit) = server.login_slots.clone().try_acquire_owned() else {
        return error(
            StatusCode::TOO_MANY_REQUESTS,
            "too many login attempts; try again in a moment".into(),
        );
    };
    let expected = server.cfg.password.as_deref().unwrap_or_default();
    if expected.is_empty() || !password_matches(expected, &password) {
        // The socket's address, never X-Forwarded-For, which the client writes.
        match peer {
            Some(Extension(ConnectInfo(addr))) => eprintln!("web UI: failed login from {addr}"),
            None => eprintln!("web UI: failed login"),
        }
        // A success answers at once, so a client could hang up when none comes and free the
        // slot early; a task of its own keeps it taken for the whole delay.
        tokio::spawn(async move {
            tokio::time::sleep(FAILURE_DELAY).await;
            drop(permit);
        });
        tokio::time::sleep(FAILURE_DELAY).await;
        return error(StatusCode::UNAUTHORIZED, "wrong password".into());
    }
    drop(permit);
    match server.sessions.create() {
        Ok(token) => {
            let set = cookie(&headers, &token, SESSION_LIFE.as_secs());
            ([(SET_COOKIE, set)], Json(json!({ "ok": true }))).into_response()
        }
        Err(e) => error(
            StatusCode::INTERNAL_SERVER_ERROR,
            format!("no random source: {e}"),
        ),
    }
}

/// `POST /api/logout`: ends the session, if any, and clears the cookie.
pub(crate) async fn logout(State(server): State<Shared>, headers: HeaderMap) -> Response {
    if let Some(token) = session_token(&headers) {
        server.sessions.remove(token);
    }
    let set = cookie(&headers, "", 0);
    ([(SET_COOKIE, set)], Json(json!({ "ok": true }))).into_response()
}

/// `GET /api/session`: `{"auth": "password"|"none", "ok": logged in}`.
pub(crate) async fn session(State(server): State<Shared>, headers: HeaderMap) -> Response {
    let auth = if server.cfg.auth_none {
        "none"
    } else {
        "password"
    };
    Json(json!({ "auth": auth, "ok": logged_in(&server, &headers) })).into_response()
}
