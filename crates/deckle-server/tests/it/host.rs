// SPDX-License-Identifier: GPL-3.0-or-later
//! The API answers only to host names a web page can't point at it (DNS rebinding).

use axum::body::Body;
use axum::http::{Request, StatusCode};
use deckle_server::host_allowed;

use crate::{PASSWORD, app, config, ctx, send};

fn get(uri: &str, host: &str) -> Request<Body> {
    Request::get(uri)
        .header("host", host)
        .body(Body::empty())
        .unwrap()
}

fn login(host: &str, password: &str) -> Request<Body> {
    Request::post("/api/login")
        .header("host", host)
        .header("x-deckle", "1")
        .header("content-type", "application/json")
        .body(Body::from(
            serde_json::json!({ "password": password }).to_string(),
        ))
        .unwrap()
}

#[test]
fn addresses_and_names_that_cannot_be_public_are_allowed() {
    let listed = ["Books.Example.com".to_owned()];
    for host in [
        "127.0.0.1",
        "127.0.0.1:8080",
        "192.168.1.20:8080",
        "[::1]:8080",
        "[::1]",
        "::1",
        "localhost",
        "LOCALHOST.",
        "localhost:8080",
        "tower",
        "tower.local",
        "tower.local.:8080",
        "nas.home.arpa",
        "deckle.internal",
        "app.localhost",
        "books.example.com",
        "BOOKS.example.com.:443",
    ] {
        assert!(host_allowed(host, &listed), "{host}");
    }
    for host in [
        "rebound.example",
        "rebound.example:8080",
        "localhost.example.com",
        "x.evil-local.com",
        "",
        ":8080",
    ] {
        assert!(!host_allowed(host, &listed), "{host}");
    }
    // Not listed, so not allowed.
    assert!(!host_allowed("books.example.com", &[]));
}

#[tokio::test]
async fn without_a_password_an_unlisted_host_is_refused_everywhere() {
    let dir = tempfile::tempdir().unwrap();
    let app = app(dir.path(), true);
    for uri in ["/api/session", "/api/fs/roots", "/api/nothing"] {
        let r = send(&app, get(uri, "rebound.example")).await;
        assert_eq!(r.status, StatusCode::FORBIDDEN, "{uri}: {}", r.body);
        assert!(r.body.contains("DECKLE_WEB_HOSTS"), "{}", r.body);
    }
    let r = send(&app, login("rebound.example", "")).await;
    assert_eq!(r.status, StatusCode::FORBIDDEN);
    for host in ["127.0.0.1:8080", "[::1]:8080", "LOCALHOST.", "tower.local"] {
        let r = send(&app, get("/api/session", host)).await;
        assert_eq!(r.status, StatusCode::OK, "{host}: {}", r.body);
    }
    // The page that says why is still served.
    let r = send(&app, get("/", "rebound.example")).await;
    assert_eq!(r.status, StatusCode::OK);
}

#[tokio::test]
async fn a_listed_name_passes() {
    let dir = tempfile::tempdir().unwrap();
    let mut cfg = config(dir.path(), true);
    cfg.hosts = vec!["books.example.com".into()];
    let app = deckle_server::router(cfg, ctx(dir.path()));
    let r = send(&app, get("/api/session", "books.example.com:443")).await;
    assert_eq!(r.status, StatusCode::OK, "{}", r.body);
    let r = send(&app, get("/api/session", "rebound.example")).await;
    assert_eq!(r.status, StatusCode::FORBIDDEN);
}

#[tokio::test]
async fn with_a_password_only_the_login_checks_the_host() {
    let dir = tempfile::tempdir().unwrap();
    let app = app(dir.path(), false);
    let r = send(&app, login("rebound.example", PASSWORD)).await;
    assert_eq!(r.status, StatusCode::FORBIDDEN, "{}", r.body);
    assert!(r.headers.get("set-cookie").is_none());
    // The rest needs a session cookie, which a rebinding page doesn't have.
    let r = send(&app, get("/api/session", "rebound.example")).await;
    assert_eq!(r.status, StatusCode::OK);
    let r = send(&app, login("tower.local", PASSWORD)).await;
    assert_eq!(r.status, StatusCode::OK, "{}", r.body);
}

#[tokio::test]
async fn a_request_without_a_host_passes() {
    let dir = tempfile::tempdir().unwrap();
    let app = app(dir.path(), true);
    let r = send(
        &app,
        Request::get("/api/session").body(Body::empty()).unwrap(),
    )
    .await;
    assert_eq!(r.status, StatusCode::OK);
}
