// SPDX-License-Identifier: GPL-3.0-or-later
//! The built frontend, and the headers every response carries.

use std::path::Path;

use axum::Router;
use axum::http::HeaderValue;
use axum::http::header::{CONTENT_SECURITY_POLICY, REFERRER_POLICY, X_CONTENT_TYPE_OPTIONS};
use tower_http::services::{ServeDir, ServeFile};
use tower_http::set_header::SetResponseHeaderLayer;

/// The desktop app's CSP (tauri.conf.json), so the page runs under the same rules.
const CSP: &str = "default-src 'self'; style-src 'self' 'unsafe-inline'; img-src 'self' data:";

/// Files under `web_dir`; any other path gets `index.html`, for the frontend's own routes.
pub(crate) fn service(web_dir: &Path) -> ServeDir<ServeFile> {
    ServeDir::new(web_dir).fallback(ServeFile::new(web_dir.join("index.html")))
}

pub(crate) fn security_headers<S: Clone + Send + Sync + 'static>(router: Router<S>) -> Router<S> {
    router
        .layer(SetResponseHeaderLayer::overriding(
            CONTENT_SECURITY_POLICY,
            HeaderValue::from_static(CSP),
        ))
        .layer(SetResponseHeaderLayer::overriding(
            X_CONTENT_TYPE_OPTIONS,
            HeaderValue::from_static("nosniff"),
        ))
        .layer(SetResponseHeaderLayer::overriding(
            REFERRER_POLICY,
            HeaderValue::from_static("no-referrer"),
        ))
}
