// SPDX-License-Identifier: GPL-3.0-or-later
//! Login, sessions, the CSRF header and the server's settings from the environment.

use std::collections::HashMap;
use std::time::{Duration, Instant};

use axum::body::Body;
use axum::http::{Request, StatusCode};
use deckle_server::ServerConfig;
use serde_json::{Value, json};

use crate::{PASSWORD, app, login, post, send};

#[tokio::test]
async fn invoke_requires_session() {
    let tmp = tempfile::tempdir().unwrap();
    let app = app(tmp.path(), false);
    let r = send(&app, post("/api/invoke/app_version", "{}", None)).await;
    assert_eq!(r.status, StatusCode::UNAUTHORIZED);
    let r = send(
        &app,
        post("/api/invoke/app_version", "{}", Some("deckle_session=00ff")),
    )
    .await;
    assert_eq!(r.status, StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn invoke_requires_csrf_header() {
    let tmp = tempfile::tempdir().unwrap();
    let app = app(tmp.path(), false);
    let cookie = login(&app).await;
    let req = Request::post("/api/invoke/app_version")
        .header("cookie", &cookie)
        .body(Body::from("{}"))
        .unwrap();
    assert_eq!(send(&app, req).await.status, StatusCode::FORBIDDEN);
    let req = Request::post("/api/invoke/app_version")
        .header("cookie", &cookie)
        .header("x-deckle", "0")
        .body(Body::from("{}"))
        .unwrap();
    assert_eq!(send(&app, req).await.status, StatusCode::FORBIDDEN);
    // Login and logout change state too.
    let req = Request::post("/api/login")
        .body(Body::from(json!({ "password": PASSWORD }).to_string()))
        .unwrap();
    assert_eq!(send(&app, req).await.status, StatusCode::FORBIDDEN);
}

#[tokio::test]
async fn login_sets_a_strict_http_only_cookie() {
    let tmp = tempfile::tempdir().unwrap();
    let app = app(tmp.path(), false);
    let body = json!({ "password": PASSWORD }).to_string();
    let r = send(&app, post("/api/login", &body, None)).await;
    let set = r.headers["set-cookie"].to_str().unwrap().to_owned();
    let value = set
        .split(';')
        .next()
        .unwrap()
        .strip_prefix("deckle_session=")
        .unwrap();
    assert_eq!(value.len(), 64, "32 random bytes in hex: {set}");
    assert!(value.bytes().all(|b| b.is_ascii_hexdigit()));
    for part in ["HttpOnly", "SameSite=Strict", "Path=/", "Max-Age=2592000"] {
        assert!(set.contains(part), "{part} in {set}");
    }
    assert!(!set.contains("Secure"), "plain http on a LAN: {set}");

    // Behind a TLS proxy the cookie is Secure.
    let mut req = post("/api/login", &body, None);
    req.headers_mut()
        .insert("x-forwarded-proto", "https".parse().unwrap());
    let r = send(&app, req).await;
    assert!(
        r.headers["set-cookie"]
            .to_str()
            .unwrap()
            .contains("; Secure")
    );
}

#[tokio::test]
async fn login_wrong_password_is_slow() {
    let tmp = tempfile::tempdir().unwrap();
    let app = app(tmp.path(), false);
    for pw in ["correct hors", "", "correct horse!"] {
        let start = Instant::now();
        let r = send(
            &app,
            post("/api/login", &json!({ "password": pw }).to_string(), None),
        )
        .await;
        assert_eq!(r.status, StatusCode::UNAUTHORIZED);
        assert!(
            start.elapsed() >= Duration::from_secs(1),
            "{:?}",
            start.elapsed()
        );
        assert!(r.headers.get("set-cookie").is_none());
    }
    let cookie = login(&app).await;
    let r = send(&app, post("/api/invoke/app_version", "{}", Some(&cookie))).await;
    assert_eq!(r.status, StatusCode::OK);
}

/// Over a real socket, where the handler gets the peer's address to log (`--no-capture`
/// shows `web UI: failed login from 127.0.0.1:…`).
#[tokio::test]
async fn failed_login_is_logged_with_peer() {
    use tokio::io::{AsyncReadExt, AsyncWriteExt};

    let tmp = tempfile::tempdir().unwrap();
    let (mut conn, stop, server) = crate::serving(tmp.path(), false).await;
    let body = json!({ "password": "nope" }).to_string();
    let req = format!(
        "POST /api/login HTTP/1.1\r\nHost: x\r\nX-Deckle: 1\r\nConnection: close\r\n\
         Content-Length: {}\r\n\r\n{body}",
        body.len()
    );
    conn.write_all(req.as_bytes()).await.unwrap();
    let mut reply = String::new();
    conn.read_to_string(&mut reply).await.unwrap();
    assert!(reply.starts_with("HTTP/1.1 401"), "{reply}");
    stop.send(()).unwrap();
    server.await.unwrap().unwrap();
}

#[tokio::test]
async fn parallel_logins_are_capped_with_429() {
    let tmp = tempfile::tempdir().unwrap();
    let app = app(tmp.path(), false);
    let cookie = login(&app).await;
    let start = Instant::now();
    let mut tries = tokio::task::JoinSet::new();
    for i in 0..20 {
        let app = app.clone();
        tries.spawn(async move {
            let body = json!({ "password": format!("guess {i}") }).to_string();
            let r = send(&app, post("/api/login", &body, None)).await;
            (r.status, r.body, r.headers, start.elapsed())
        });
    }
    let (mut wrong, mut busy) = (0, 0);
    while let Some(got) = tries.join_next().await {
        let (status, body, headers, _) = got.unwrap();
        match status {
            StatusCode::UNAUTHORIZED => wrong += 1,
            StatusCode::TOO_MANY_REQUESTS => {
                busy += 1;
                assert_eq!(
                    serde_json::from_str::<Value>(&body).unwrap(),
                    json!({"error": "too many login attempts; try again in a moment"})
                );
                assert_eq!(headers["x-content-type-options"], "nosniff");
            }
            other => panic!("{other}: {body}"),
        }
    }
    assert!(wrong <= 4, "{wrong} checked at once");
    assert_eq!(wrong + busy, 20);
    // The slots free up, and a session from before was never affected.
    login(&app).await;
    let r = send(&app, post("/api/invoke/app_version", "{}", Some(&cookie))).await;
    assert_eq!(r.status, StatusCode::OK);
}

/// A wrong guess answers only after the delay, so a client could hang up as soon as no quick
/// success comes back; its slot must stay taken for the full delay anyway.
#[tokio::test]
async fn abandoned_failed_logins_still_hold_their_slot() {
    let tmp = tempfile::tempdir().unwrap();
    let app = app(tmp.path(), false);
    let mut tries = Vec::new();
    for i in 0..4 {
        let app = app.clone();
        tries.push(tokio::spawn(async move {
            let body = json!({ "password": format!("guess {i}") }).to_string();
            send(&app, post("/api/login", &body, None)).await
        }));
    }
    tokio::time::sleep(Duration::from_millis(200)).await;
    for t in tries {
        t.abort();
        assert!(t.await.is_err_and(|e| e.is_cancelled()));
    }
    let body = json!({ "password": PASSWORD }).to_string();
    let r = send(&app, post("/api/login", &body, None)).await;
    assert_eq!(r.status, StatusCode::TOO_MANY_REQUESTS);
    tokio::time::sleep(Duration::from_millis(1300)).await;
    login(&app).await;
}

#[tokio::test]
async fn login_with_a_bad_body_is_400() {
    let tmp = tempfile::tempdir().unwrap();
    let app = app(tmp.path(), false);
    let r = send(&app, post("/api/login", "password=x", None)).await;
    assert_eq!(r.status, StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn session_reports_login_state_and_logout_ends_it() {
    let tmp = tempfile::tempdir().unwrap();
    let app = app(tmp.path(), false);
    let session = |cookie: Option<String>| {
        let mut req = Request::get("/api/session");
        if let Some(c) = cookie {
            req = req.header("cookie", c);
        }
        req.body(Body::empty()).unwrap()
    };
    let r = send(&app, session(None)).await;
    assert_eq!(
        serde_json::from_str::<Value>(&r.body).unwrap(),
        json!({"auth": "password", "ok": false})
    );

    let cookie = login(&app).await;
    // Other cookies alongside ours are fine.
    let both = format!("theme=dark; {cookie}");
    let r = send(&app, session(Some(both))).await;
    assert_eq!(
        serde_json::from_str::<Value>(&r.body).unwrap(),
        json!({"auth": "password", "ok": true})
    );

    let r = send(&app, post("/api/logout", "", Some(&cookie))).await;
    assert_eq!(r.status, StatusCode::OK);
    let cleared = r.headers["set-cookie"].to_str().unwrap();
    assert!(
        cleared.starts_with("deckle_session=;") && cleared.contains("Max-Age=0"),
        "{cleared}"
    );
    let r = send(&app, session(Some(cookie.clone()))).await;
    assert_eq!(
        serde_json::from_str::<Value>(&r.body).unwrap(),
        json!({"auth": "password", "ok": false})
    );
    let r = send(&app, post("/api/invoke/app_version", "{}", Some(&cookie))).await;
    assert_eq!(r.status, StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn auth_none_needs_no_login() {
    let tmp = tempfile::tempdir().unwrap();
    let app = app(tmp.path(), true);
    let r = send(
        &app,
        Request::get("/api/session").body(Body::empty()).unwrap(),
    )
    .await;
    assert_eq!(
        serde_json::from_str::<Value>(&r.body).unwrap(),
        json!({"auth": "none", "ok": true})
    );
    let r = send(&app, post("/api/invoke/app_version", "{}", None)).await;
    assert_eq!(r.status, StatusCode::OK);
    // The CSRF header is still required.
    let req = Request::post("/api/invoke/app_version")
        .body(Body::from("{}"))
        .unwrap();
    assert_eq!(send(&app, req).await.status, StatusCode::FORBIDDEN);
}

fn env(pairs: &[(&str, &str)]) -> impl Fn(&str) -> Option<String> {
    let map: HashMap<String, String> = pairs
        .iter()
        .map(|(k, v)| (k.to_string(), v.to_string()))
        .collect();
    move |k| map.get(k).cloned()
}

#[test]
fn from_env_without_password_errors_naming_var() {
    let err = ServerConfig::from_lookup("/web".into(), env(&[])).unwrap_err();
    assert!(err.contains("DECKLE_WEB_PASSWORD"), "{err}");
    // Unraid passes unset variables as blanks.
    let err =
        ServerConfig::from_lookup("/web".into(), env(&[("DECKLE_WEB_PASSWORD", "")])).unwrap_err();
    assert!(err.contains("DECKLE_WEB_PASSWORD"), "{err}");
    let err =
        ServerConfig::from_lookup("/web".into(), env(&[("DECKLE_WEB_AUTH", "off")])).unwrap_err();
    assert!(err.contains("DECKLE_WEB_AUTH"), "{err}");
}

#[test]
fn from_env_reads_every_setting() {
    let cfg =
        ServerConfig::from_lookup("/web".into(), env(&[("DECKLE_WEB_PASSWORD", "pw")])).unwrap();
    assert_eq!(cfg.bind, std::net::IpAddr::from([127, 0, 0, 1]));
    assert_eq!(cfg.port, 8080);
    assert_eq!(cfg.password.as_deref(), Some("pw"));
    assert!(!cfg.auth_none);
    assert_eq!(cfg.web_dir, std::path::PathBuf::from("/web"));
    assert_eq!(
        cfg.roots,
        ["/config", "/data"].map(std::path::PathBuf::from)
    );

    let cfg = ServerConfig::from_lookup(
        "/web".into(),
        env(&[
            ("DECKLE_WEB_BIND", "0.0.0.0"),
            ("DECKLE_WEB_PORT", "0"),
            ("DECKLE_WEB_PASSWORD", ""),
            ("DECKLE_WEB_AUTH", "none"),
            ("DECKLE_WEB_ROOTS", "/mnt/books::/media"),
        ]),
    )
    .unwrap();
    assert_eq!(cfg.bind, std::net::IpAddr::from([0, 0, 0, 0]));
    assert_eq!(cfg.port, 0);
    assert!(cfg.auth_none);
    assert_eq!(cfg.password, None);
    assert_eq!(
        cfg.roots,
        ["/config", "/data", "/mnt/books", "/media"].map(std::path::PathBuf::from)
    );

    for (k, v) in [
        ("DECKLE_WEB_PORT", "http"),
        ("DECKLE_WEB_BIND", "localhost"),
    ] {
        let err =
            ServerConfig::from_lookup("/web".into(), env(&[("DECKLE_WEB_PASSWORD", "pw"), (k, v)]))
                .unwrap_err();
        assert!(err.contains(k), "{err}");
    }
}
