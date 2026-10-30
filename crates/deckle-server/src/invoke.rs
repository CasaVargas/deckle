// SPDX-License-Identifier: GPL-3.0-or-later
//! `POST /api/invoke/<command>`: the shared dispatch table, answering as Tauri would.

use axum::Json;
use axum::body::Bytes;
use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use serde_json::{Value, json};

use crate::Shared;

/// `{"error": message}` with `status`.
pub(crate) fn error(status: StatusCode, message: String) -> Response {
    (status, Json(json!({ "error": message }))).into_response()
}

pub(crate) async fn invoke(
    State(server): State<Shared>,
    Path(command): Path<String>,
    body: Bytes,
) -> Response {
    // Commands without arguments may be sent no body at all.
    let args = if body.trim_ascii().is_empty() {
        Value::Null
    } else {
        match serde_json::from_slice(&body) {
            Ok(v) => v,
            Err(e) => {
                return error(
                    StatusCode::BAD_REQUEST,
                    format!("{command}: body is not JSON: {e}"),
                );
            }
        }
    };
    let ctx = server.ctx.clone();
    let name = command.clone();
    // Commands block (files, the CLI, the Kindle), so they run off the async workers.
    match tokio::task::spawn_blocking(move || deckle_app::dispatch::dispatch(&ctx, &name, args))
        .await
    {
        Ok(Ok(v)) => Json(v).into_response(),
        Ok(Err(e)) => error(StatusCode::BAD_REQUEST, e),
        Err(e) => error(StatusCode::INTERNAL_SERVER_ERROR, format!("{command}: {e}")),
    }
}
