// SPDX-License-Identifier: GPL-3.0-or-later
//! KOReader's progress sync (KOReader hub design §B), as its kosync plugin speaks it
//! (`plugins/kosync.koplugin/api.json`): one account, set in Deckle, so registering is closed;
//! the newest position per document, kept in `koreader-sync.json`.

use std::collections::BTreeMap;
use std::path::Path;
use std::sync::Arc;

use axum::Json;
use axum::extract::{Path as UrlPath, State};
use axum::http::{HeaderMap, StatusCode};
use axum::response::{IntoResponse, Response};
use serde::{Deserialize, Serialize};
use serde_json::json;

use crate::{Ctx, auth};

/// What KOReader sends with each page turn it syncs.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Progress {
    /// The book's partial MD5, or the MD5 of its file name.
    pub document: String,
    /// An xpointer (`/body/DocFragment[3]/…`) in a reflowable book, a page number in a PDF.
    pub progress: String,
    /// 0–1.
    pub percentage: f64,
    pub device: String,
    pub device_id: String,
    /// When it arrived, Unix seconds.
    #[serde(default)]
    pub timestamp: i64,
    /// `filename`, `title`, `authors`, when "Send document metadata" is on in KOReader.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub metadata: Option<Metadata>,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Metadata {
    #[serde(default)]
    pub filename: Option<String>,
    #[serde(default)]
    pub title: Option<String>,
    #[serde(default)]
    pub authors: Option<String>,
}

/// Every document's newest position.
pub fn load(path: &Path) -> BTreeMap<String, Progress> {
    std::fs::read(path)
        .ok()
        .and_then(|b| serde_json::from_slice(&b).ok())
        .unwrap_or_default()
}

fn save(path: &Path, all: &BTreeMap<String, Progress>) -> std::io::Result<()> {
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir)?;
    }
    let tmp = path.with_extension("json.tmp");
    std::fs::write(&tmp, serde_json::to_vec_pretty(all)?)?;
    std::fs::rename(&tmp, path)
}

fn unauthorized() -> Response {
    (
        StatusCode::UNAUTHORIZED,
        Json(json!({ "message": "Unauthorized" })),
    )
        .into_response()
}

/// `POST /users/create`: the account is the one set in Deckle.
pub async fn register() -> Response {
    (
        StatusCode::PAYMENT_REQUIRED,
        Json(json!({
            "message": "Registration is closed: use the username and password set in Deckle."
        })),
    )
        .into_response()
}

/// `GET /users/auth`.
pub async fn authorize(State(ctx): State<Arc<Ctx>>, headers: HeaderMap) -> Response {
    if !auth::kosync_ok(&headers, &ctx.settings) {
        return unauthorized();
    }
    Json(json!({ "authorized": "OK" })).into_response()
}

#[derive(Deserialize)]
pub struct Update {
    document: String,
    progress: serde_json::Value,
    percentage: f64,
    device: String,
    device_id: String,
    #[serde(default)]
    metadata: Option<Metadata>,
}

/// `PUT /syncs/progress`: keeps the position, then tells the rest of Deckle.
pub async fn update(
    State(ctx): State<Arc<Ctx>>,
    headers: HeaderMap,
    body: axum::body::Bytes,
) -> Response {
    if !auth::kosync_ok(&headers, &ctx.settings) {
        return unauthorized();
    }
    let Ok(u) = serde_json::from_slice::<Update>(&body) else {
        return (
            StatusCode::BAD_REQUEST,
            Json(json!({ "message": "Invalid request" })),
        )
            .into_response();
    };
    let document = u.document.trim().to_ascii_lowercase();
    if document.is_empty() || !(0.0..=1.0).contains(&u.percentage) {
        return (
            StatusCode::BAD_REQUEST,
            Json(json!({ "message": "Invalid request" })),
        )
            .into_response();
    }
    // A PDF's progress is a page number; keep it as text either way.
    let progress = match u.progress {
        serde_json::Value::String(s) => s,
        other => other.to_string(),
    };
    let p = Progress {
        document: document.clone(),
        progress,
        percentage: u.percentage,
        device: u.device,
        device_id: u.device_id,
        timestamp: chrono::Utc::now().timestamp(),
        metadata: u.metadata,
    };
    let store = ctx.hooks.sync_store.clone();
    {
        let _one = ctx.sync_lock.lock().await;
        let record = p.clone();
        let saved = tokio::task::spawn_blocking(move || {
            let mut all = load(&store);
            all.insert(record.document.clone(), record);
            save(&store, &all)
        })
        .await;
        if !matches!(saved, Ok(Ok(()))) {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({ "message": "Not saved" })),
            )
                .into_response();
        }
    }
    let hook = ctx.hooks.progress.clone();
    let told = p.clone();
    tokio::task::spawn_blocking(move || hook(&told));
    Json(json!({ "document": document, "timestamp": p.timestamp })).into_response()
}

/// `GET /syncs/progress/<document>`: the newest position, or `{}`.
pub async fn get_progress(
    State(ctx): State<Arc<Ctx>>,
    headers: HeaderMap,
    UrlPath(document): UrlPath<String>,
) -> Response {
    if !auth::kosync_ok(&headers, &ctx.settings) {
        return unauthorized();
    }
    let store = ctx.hooks.sync_store.clone();
    let doc = document.to_ascii_lowercase();
    let found = tokio::task::spawn_blocking(move || load(&store).remove(&doc))
        .await
        .ok()
        .flatten();
    match found {
        Some(p) => Json(json!({
            "document": p.document,
            "progress": p.progress,
            "percentage": p.percentage,
            "device": p.device,
            "device_id": p.device_id,
            "timestamp": p.timestamp,
        }))
        .into_response(),
        None => Json(json!({})).into_response(),
    }
}
