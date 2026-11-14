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

/// A login from `addr`, as a real socket would give it.
fn login_from(
    addr: &str,
    password: &str,
    xff: Option<&str>,
) -> axum::http::Request<axum::body::Body> {
    let mut req = post(
        "/api/login",
        &json!({ "password": password }).to_string(),
        None,
    );
    if let Some(x) = xff {
        req.headers_mut()
            .insert("x-forwarded-for", x.parse().unwrap());
    }
    let addr: std::net::SocketAddr = addr.parse().unwrap();
    req.extensions_mut()
        .insert(axum::extract::ConnectInfo(addr));
    req
}

#[tokio::test]
async fn five_wrong_passwords_are_free_then_that_address_waits() {
    let tmp = tempfile::tempdir().unwrap();
    let app = app(tmp.path(), false);
    let start = Instant::now();
    for _ in 0..6 {
        let r = send(&app, login_from("192.168.1.50:1000", "nope", None)).await;
        assert_eq!(r.status, StatusCode::UNAUTHORIZED, "{}", r.body);
        assert!(r.headers.get("set-cookie").is_none());
    }
    // Wrong guesses are no longer each held back a second.
    assert!(
        start.elapsed() < Duration::from_secs(1),
        "{:?}",
        start.elapsed()
    );
    // The 6th failure starts a wait, even for the right password from there.
    let r = send(&app, login_from("192.168.1.50:1001", PASSWORD, None)).await;
    assert_eq!(r.status, StatusCode::TOO_MANY_REQUESTS, "{}", r.body);
    assert_eq!(r.headers["retry-after"], "1");
    assert_eq!(r.headers["x-content-type-options"], "nosniff");
    // Another address is untouched.
    let r = send(&app, login_from("192.168.1.2:1000", PASSWORD, None)).await;
    assert_eq!(r.status, StatusCode::OK, "{}", r.body);
    // After the wait, the right password works and clears the count.
    tokio::time::sleep(Duration::from_millis(1100)).await;
    let r = send(&app, login_from("192.168.1.50:1002", PASSWORD, None)).await;
    assert_eq!(r.status, StatusCode::OK, "{}", r.body);
}

#[tokio::test]
async fn a_thousand_parallel_guesses_from_one_address_get_a_handful_checked() {
    let tmp = tempfile::tempdir().unwrap();
    let app = app(tmp.path(), false);
    let mut tries = tokio::task::JoinSet::new();
    for i in 0..1000 {
        let app = app.clone();
        tries.spawn(async move {
            send(
                &app,
                login_from("203.0.113.9:1", &format!("guess {i}"), None),
            )
            .await
            .status
        });
    }
    let (mut checked, mut refused) = (0, 0);
    while let Some(status) = tries.join_next().await {
        match status.unwrap() {
            StatusCode::UNAUTHORIZED => checked += 1,
            StatusCode::TOO_MANY_REQUESTS => refused += 1,
            other => panic!("{other}"),
        }
    }
    // Five free, the one that starts the wait, and one more at most in flight beside it.
    assert!(checked <= 7, "{checked} checked");
    assert_eq!(checked + refused, 1000);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn the_owner_signs_in_while_someone_else_guesses() {
    let tmp = tempfile::tempdir().unwrap();
    let app = app(tmp.path(), false);
    let deadline = Instant::now() + Duration::from_secs(2);
    let mut guessers = tokio::task::JoinSet::new();
    for c in 0..8 {
        let app = app.clone();
        guessers.spawn(async move {
            while Instant::now() < deadline {
                send(
                    &app,
                    login_from("203.0.113.9:1", &format!("guess {c}"), None),
                )
                .await;
                tokio::task::yield_now().await;
            }
        });
    }
    let mut ok = 0;
    while Instant::now() < deadline {
        let r = send(&app, login_from("192.168.1.2:5", PASSWORD, None)).await;
        assert_eq!(r.status, StatusCode::OK, "{}", r.body);
        ok += 1;
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
    assert!(ok >= 5, "{ok}");
    while guessers.join_next().await.is_some() {}
}

#[tokio::test]
async fn behind_a_listed_proxy_each_client_counts_on_its_own() {
    let tmp = tempfile::tempdir().unwrap();
    let mut cfg = crate::config(tmp.path(), false);
    cfg.trusted_proxies = vec![deckle_server::Range::parse("172.18.0.0/16").unwrap()];
    let app = deckle_server::router(cfg, crate::ctx(tmp.path()));
    for _ in 0..8 {
        send(
            &app,
            login_from("172.18.0.2:1", "nope", Some("203.0.113.9")),
        )
        .await;
    }
    let r = send(
        &app,
        login_from("172.18.0.2:1", PASSWORD, Some("203.0.113.9")),
    )
    .await;
    assert_eq!(r.status, StatusCode::TOO_MANY_REQUESTS);
    let r = send(
        &app,
        login_from("172.18.0.2:1", PASSWORD, Some("198.51.100.4")),
    )
    .await;
    assert_eq!(r.status, StatusCode::OK, "{}", r.body);

    // Without the setting the header changes nothing: everyone is the proxy.
    let app = crate::app(tmp.path(), false);
    for _ in 0..8 {
        send(
            &app,
            login_from("172.18.0.2:1", "nope", Some("203.0.113.9")),
        )
        .await;
    }
    let r = send(
        &app,
        login_from("172.18.0.2:1", PASSWORD, Some("198.51.100.4")),
    )
    .await;
    assert_eq!(r.status, StatusCode::TOO_MANY_REQUESTS);
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
            ("DECKLE_WEB_HOSTS", "books.example.com, ,deckle.example.org"),
            ("DECKLE_WEB_TRUSTED_PROXIES", "10.0.0.5, 172.18.0.0/16"),
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
    assert_eq!(cfg.hosts, ["books.example.com", "deckle.example.org"]);
    assert_eq!(cfg.trusted_proxies.len(), 2);

    for (k, v) in [
        ("DECKLE_WEB_PORT", "http"),
        ("DECKLE_WEB_BIND", "localhost"),
        ("DECKLE_WEB_TRUSTED_PROXIES", "10.0.0.0/40"),
    ] {
        let err =
            ServerConfig::from_lookup("/web".into(), env(&[("DECKLE_WEB_PASSWORD", "pw"), (k, v)]))
                .unwrap_err();
        assert!(err.contains(k), "{err}");
    }
}

#[test]
fn start_warnings_name_an_ignored_or_short_password() {
    let cfg =
        |pairs: &[(&str, &str)]| ServerConfig::from_lookup("/web".into(), env(pairs)).unwrap();
    assert_eq!(
        cfg(&[
            ("DECKLE_WEB_PASSWORD", "a-long-password"),
            ("DECKLE_WEB_AUTH", "none")
        ])
        .warnings(),
        ["DECKLE_WEB_AUTH=none: the password is ignored"]
    );
    assert_eq!(
        cfg(&[("DECKLE_WEB_PASSWORD", "short")]).warnings(),
        ["web UI password is short; use 12+ characters"]
    );
    assert!(
        cfg(&[("DECKLE_WEB_PASSWORD", "a-long-password")])
            .warnings()
            .is_empty()
    );
    assert!(cfg(&[("DECKLE_WEB_AUTH", "none")]).warnings().is_empty());
}
