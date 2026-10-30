// SPDX-License-Identifier: GPL-3.0-or-later
//! KOReader's progress sync (KOReader hub design §B), as its kosync plugin calls it.
use std::sync::{Arc, Mutex};

use axum::body::Body;
use axum::http::{Request, StatusCode};
use deckle_readerserver::sync::Progress;
use deckle_readerserver::{Catalog, Ctx, Hooks, Settings, auth, router};
use http_body_util::BodyExt;
use tower::ServiceExt;

struct Fixture {
    tmp: tempfile::TempDir,
    ctx: Arc<Ctx>,
    told: Arc<Mutex<Vec<Progress>>>,
}

fn fixture() -> Fixture {
    let tmp = tempfile::tempdir().unwrap();
    let told = Arc::new(Mutex::new(Vec::new()));
    let sink = told.clone();
    let ctx = Arc::new(Ctx::new(
        Settings {
            bind: "127.0.0.1".into(),
            port: 0,
            username: "deckle".into(),
            key: auth::md5_hex(b"hunter2"),
        },
        Hooks {
            source: Arc::new(Catalog::default),
            served: Arc::new(|_, _| {}),
            progress: Arc::new(move |p| sink.lock().unwrap().push(p.clone())),
            sync_store: tmp.path().join("koreader-sync.json"),
        },
    ));
    Fixture { tmp, ctx, told }
}

async fn call(
    ctx: &Arc<Ctx>,
    method: &str,
    uri: &str,
    key: Option<&str>,
    body: Option<serde_json::Value>,
) -> (StatusCode, serde_json::Value) {
    let mut req = Request::builder()
        .method(method)
        .uri(uri)
        .header("accept", "application/vnd.koreader.v1+json");
    if let Some(k) = key {
        req = req.header("x-auth-user", "deckle").header("x-auth-key", k);
    }
    let req = match body {
        Some(b) => req
            .header("content-type", "application/json")
            .body(Body::from(b.to_string())),
        None => req.body(Body::empty()),
    }
    .unwrap();
    let res = router(ctx.clone()).oneshot(req).await.unwrap();
    let status = res.status();
    let bytes = res.into_body().collect().await.unwrap().to_bytes();
    (
        status,
        serde_json::from_slice(&bytes).unwrap_or(serde_json::Value::Null),
    )
}

fn key() -> String {
    auth::md5_hex(b"hunter2")
}

#[tokio::test]
async fn sign_in_takes_the_md5_and_registering_is_closed() {
    let f = fixture();
    let (s, body) = call(&f.ctx, "GET", "/users/auth", Some(&key()), None).await;
    assert_eq!(
        (s, body["authorized"].as_str()),
        (StatusCode::OK, Some("OK"))
    );
    let (s, _) = call(
        &f.ctx,
        "GET",
        "/users/auth",
        Some(&auth::md5_hex(b"x")),
        None,
    )
    .await;
    assert_eq!(s, StatusCode::UNAUTHORIZED);
    let (s, _) = call(&f.ctx, "GET", "/users/auth", None, None).await;
    assert_eq!(s, StatusCode::UNAUTHORIZED);
    let reg = serde_json::json!({ "username": "x", "password": "y" });
    let (s, _) = call(&f.ctx, "POST", "/users/create", None, Some(reg)).await;
    assert_eq!(s, StatusCode::PAYMENT_REQUIRED);
}

#[tokio::test]
async fn a_position_is_kept_returned_and_passed_on() {
    let f = fixture();
    let doc = "0F1E2D3C4B5A69788796A5B4C3D2E1F0";
    let (s, body) = call(
        &f.ctx,
        "GET",
        &format!("/syncs/progress/{doc}"),
        Some(&key()),
        None,
    )
    .await;
    assert_eq!((s, body), (StatusCode::OK, serde_json::json!({})));

    let put = serde_json::json!({
        "document": doc,
        "progress": "/body/DocFragment[4]/body/div/p[2]/text().17",
        "percentage": 0.31,
        "device": "Kobo Libra 2",
        "device_id": "ABCDEF",
        "metadata": { "filename": "deckle-daily-2026-10-30.kepub.epub", "title": "The Deckle Daily" },
    });
    let (s, body) = call(&f.ctx, "PUT", "/syncs/progress", Some(&key()), Some(put)).await;
    assert_eq!(s, StatusCode::OK);
    assert_eq!(body["document"], doc.to_ascii_lowercase());
    assert!(body["timestamp"].as_i64().unwrap() > 0);

    let (s, body) = call(
        &f.ctx,
        "GET",
        &format!("/syncs/progress/{doc}"),
        Some(&key()),
        None,
    )
    .await;
    assert_eq!(s, StatusCode::OK);
    assert_eq!(
        body["progress"],
        "/body/DocFragment[4]/body/div/p[2]/text().17"
    );
    assert_eq!(body["percentage"], 0.31);
    assert_eq!(body["device_id"], "ABCDEF");
    assert!(f.tmp.path().join("koreader-sync.json").is_file());

    // The hook runs on a blocking thread; give it a moment.
    for _ in 0..50 {
        if !f.told.lock().unwrap().is_empty() {
            break;
        }
        tokio::time::sleep(std::time::Duration::from_millis(10)).await;
    }
    let told = f.told.lock().unwrap();
    assert_eq!(told.len(), 1);
    assert_eq!(
        told[0].metadata.as_ref().unwrap().filename.as_deref(),
        Some("deckle-daily-2026-10-30.kepub.epub")
    );
}

#[tokio::test]
async fn bad_updates_are_refused_and_a_pdf_page_is_kept_as_text() {
    let f = fixture();
    let (s, _) = call(
        &f.ctx,
        "PUT",
        "/syncs/progress",
        None,
        Some(serde_json::json!({})),
    )
    .await;
    assert_eq!(s, StatusCode::UNAUTHORIZED);
    let bad = serde_json::json!({ "document": "", "progress": "x", "percentage": 0.1, "device": "d", "device_id": "i" });
    let (s, _) = call(&f.ctx, "PUT", "/syncs/progress", Some(&key()), Some(bad)).await;
    assert_eq!(s, StatusCode::BAD_REQUEST);
    let pdf = serde_json::json!({ "document": "abc", "progress": 42, "percentage": 0.5, "device": "d", "device_id": "i" });
    let (s, _) = call(&f.ctx, "PUT", "/syncs/progress", Some(&key()), Some(pdf)).await;
    assert_eq!(s, StatusCode::OK);
    let (_, body) = call(&f.ctx, "GET", "/syncs/progress/abc", Some(&key()), None).await;
    assert_eq!(body["progress"], "42");
}
