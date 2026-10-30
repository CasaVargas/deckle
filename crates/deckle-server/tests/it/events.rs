// SPDX-License-Identifier: GPL-3.0-or-later
//! `GET /api/events`: the app's events as Server-Sent Events.

use std::time::Duration;

use axum::body::Body;
use axum::http::{Request, StatusCode};
use deckle_app::events::Events;
use deckle_server::SseEvents;
use http_body_util::BodyExt;
use serde_json::json;
use tower::ServiceExt;

use crate::{app, login};

#[tokio::test]
async fn events_require_a_session() {
    let tmp = tempfile::tempdir().unwrap();
    let app = app(tmp.path(), false);
    let res = app
        .oneshot(Request::get("/api/events").body(Body::empty()).unwrap())
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn events_stream_delivers_emitted_event() {
    let tmp = tempfile::tempdir().unwrap();
    let app = app(tmp.path(), false);
    let cookie = login(&app).await;
    let res = app
        .oneshot(
            Request::get("/api/events")
                .header("cookie", &cookie)
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::OK);
    assert_eq!(res.headers()["content-type"], "text/event-stream");

    SseEvents.emit(
        "daily-finished",
        json!({ "ok": true, "path": "/data/a b.epub" }),
    );
    let mut body = res.into_body();
    let frame = tokio::time::timeout(Duration::from_secs(5), body.frame())
        .await
        .expect("an event within 5 s")
        .unwrap()
        .unwrap();
    let text = String::from_utf8(frame.into_data().unwrap().to_vec()).unwrap();
    assert_eq!(
        text,
        "event: daily-finished\ndata: {\"ok\":true,\"path\":\"/data/a b.epub\"}\n\n"
    );
}

#[tokio::test]
async fn shutdown_ends_open_event_streams() {
    use tokio::io::{AsyncReadExt, AsyncWriteExt};

    let tmp = tempfile::tempdir().unwrap();
    let (mut conn, stop, server) = crate::serving(tmp.path(), true).await;
    conn.write_all(b"GET /api/events HTTP/1.1\r\nHost: x\r\n\r\n")
        .await
        .unwrap();
    let mut buf = [0; 1024];
    let n = conn.read(&mut buf).await.unwrap();
    assert!(String::from_utf8_lossy(&buf[..n]).starts_with("HTTP/1.1 200"));

    stop.send(()).unwrap();
    tokio::time::timeout(Duration::from_secs(5), server)
        .await
        .expect("serve returns with an event stream open")
        .unwrap()
        .unwrap();
}
