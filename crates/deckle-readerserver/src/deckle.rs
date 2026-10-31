// SPDX-License-Identifier: GPL-3.0-or-later
//! What the Deckle plugin for KOReader asks (KOReader plugins design §5.3, §6): today's paper,
//! described before it's fetched, so the plugin can skip a paper it has and check the one it
//! gets.

use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::SystemTime;

use axum::Json;
use axum::extract::{Query, State};
use axum::http::{HeaderMap, StatusCode};
use axum::response::{IntoResponse, Response};
use serde::Deserialize;
use sha2::{Digest, Sha256};

use crate::{Ctx, auth, opds};

/// Files kept in the cache of served bytes.
const KEEP: usize = 6;

/// A file as served: the bytes (a KEPUB is made on request) and their SHA-256.
#[derive(Debug)]
pub struct File {
    pub bytes: Arc<Vec<u8>>,
    pub sha256: String,
}

/// What a cached file was made from: so a paper rebuilt at the same path is made again.
#[derive(Debug, Clone)]
pub struct Key {
    path: PathBuf,
    modified: Option<SystemTime>,
    kepub: bool,
    file: Arc<File>,
}

/// `path` as served, as a KEPUB when `kepub`: made once per version of the file, so the size and
/// hash `today` gives are the bytes the download sends.
pub async fn served(ctx: &Ctx, path: &Path, kepub: bool) -> Option<Arc<File>> {
    let modified = tokio::fs::metadata(path).await.ok()?.modified().ok();
    {
        let cache = ctx.served.lock().ok()?;
        if let Some(k) = cache
            .iter()
            .find(|k| k.path == path && k.modified == modified && k.kepub == kepub)
        {
            return Some(k.file.clone());
        }
    }
    let raw = tokio::fs::read(path).await.ok()?;
    let bytes = match kepub {
        true => tokio::task::spawn_blocking(move || deckle_core::kepub::convert(&raw))
            .await
            .ok()?
            .ok()?,
        false => raw,
    };
    let sha256 = Sha256::digest(&bytes)
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect();
    let file = Arc::new(File {
        bytes: Arc::new(bytes),
        sha256,
    });
    if let Ok(mut cache) = ctx.served.lock() {
        cache.retain(|k| !(k.path == path && k.kepub == kepub));
        cache.push(Key {
            path: path.to_path_buf(),
            modified,
            kepub,
            file: file.clone(),
        });
        let excess = cache.len().saturating_sub(KEEP);
        cache.drain(..excess);
    }
    Some(file)
}

#[derive(Deserialize)]
pub struct TodayQuery {
    format: Option<String>,
}

/// `GET /deckle/v1/today[?format=kepub]`: the newest edition of the household's main paper (any
/// paper's when the main one has none), as `{date, file, size, sha256, url}`; 204 when there's
/// none yet.
pub async fn today(
    State(ctx): State<Arc<Ctx>>,
    headers: HeaderMap,
    Query(q): Query<TodayQuery>,
) -> Response {
    if !auth::either_ok(&headers, &ctx.settings) {
        return (
            StatusCode::UNAUTHORIZED,
            Json(serde_json::json!({ "message": "Unauthorized" })),
        )
            .into_response();
    }
    let catalog = (ctx.hooks.source)();
    let newest = |main: bool| {
        catalog
            .papers
            .iter()
            .filter(|p| !main || p.main)
            .max_by_key(|p| p.date)
    };
    let Some(paper) = newest(true).or_else(|| newest(false)) else {
        return StatusCode::NO_CONTENT.into_response();
    };
    let kepub = q.format.as_deref() == Some("kepub");
    let Some(file) = served(&ctx, &paper.epub, kepub).await else {
        return StatusCode::NO_CONTENT.into_response();
    };
    let id = opds::file_id(&paper.epub);
    let (name, url) = match kepub {
        true => (
            format!("{}.kepub.epub", paper.stem),
            format!("/opds/file/{id}?format=kepub"),
        ),
        false => (format!("{}.epub", paper.stem), format!("/opds/file/{id}")),
    };
    Json(serde_json::json!({
        "date": paper.date.to_string(),
        "title": paper.title,
        "file": name,
        "size": file.bytes.len(),
        "sha256": file.sha256,
        "url": url,
    }))
    .into_response()
}
