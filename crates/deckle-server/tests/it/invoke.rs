// SPDX-License-Identifier: GPL-3.0-or-later
//! `POST /api/invoke/<command>`: the dispatch table over HTTP.

use axum::body::Body;
use axum::http::{Request, StatusCode};
use serde_json::{Value, json};

use crate::{app, login, post, send};

#[tokio::test]
async fn login_then_invoke_app_version() {
    let tmp = tempfile::tempdir().unwrap();
    let app = app(tmp.path(), false);
    let cookie = login(&app).await;
    for body in ["{}", "", "null"] {
        let r = send(&app, post("/api/invoke/app_version", body, Some(&cookie))).await;
        assert_eq!(r.status, StatusCode::OK, "{body:?}: {}", r.body);
        let v: Value = serde_json::from_str(&r.body).unwrap();
        assert!(v.is_string(), "{v}");
        assert!(
            r.headers["content-type"]
                .to_str()
                .unwrap()
                .starts_with("application/json")
        );
    }
}

#[tokio::test]
async fn invoke_error_is_400_with_message() {
    let tmp = tempfile::tempdir().unwrap();
    let app = app(tmp.path(), false);
    let cookie = login(&app).await;
    let r = send(
        &app,
        post("/api/invoke/no_such_command", "{}", Some(&cookie)),
    )
    .await;
    assert_eq!(r.status, StatusCode::BAD_REQUEST);
    assert_eq!(
        serde_json::from_str::<Value>(&r.body).unwrap(),
        json!({"error": "unknown command: no_such_command"})
    );
    // Wrong argument types come back as dispatch words them.
    let r = send(
        &app,
        post(
            "/api/invoke/book_preflight",
            r#"{"path": 3}"#,
            Some(&cookie),
        ),
    )
    .await;
    assert_eq!(r.status, StatusCode::BAD_REQUEST);
    let v: Value = serde_json::from_str(&r.body).unwrap();
    assert!(
        v["error"].as_str().unwrap().starts_with("book_preflight: "),
        "{v}"
    );
    // A body that isn't JSON.
    let r = send(
        &app,
        post("/api/invoke/app_version", "{nope", Some(&cookie)),
    )
    .await;
    assert_eq!(r.status, StatusCode::BAD_REQUEST);
    assert!(serde_json::from_str::<Value>(&r.body).unwrap()["error"].is_string());
}

#[tokio::test]
async fn invoke_is_post_only() {
    let tmp = tempfile::tempdir().unwrap();
    let app = app(tmp.path(), false);
    let cookie = login(&app).await;
    let req = Request::get("/api/invoke/app_version")
        .header("cookie", &cookie)
        .body(Body::empty())
        .unwrap();
    assert_eq!(send(&app, req).await.status, StatusCode::METHOD_NOT_ALLOWED);
}

#[tokio::test]
async fn unknown_api_paths_are_404_not_the_spa() {
    let tmp = tempfile::tempdir().unwrap();
    let app = app(tmp.path(), false);
    let r = send(&app, Request::get("/api/nope").body(Body::empty()).unwrap()).await;
    assert_eq!(r.status, StatusCode::NOT_FOUND);
    assert!(!r.body.contains("<html>"));
}
