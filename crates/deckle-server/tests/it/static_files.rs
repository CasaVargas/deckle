// SPDX-License-Identifier: GPL-3.0-or-later
//! The built frontend, with every unknown path answered by `index.html`.

use axum::body::Body;
use axum::http::{Request, StatusCode};

use crate::{app, send};

fn get(uri: &str) -> Request<Body> {
    Request::get(uri).body(Body::empty()).unwrap()
}

#[tokio::test]
async fn static_fallback_serves_index() {
    let tmp = tempfile::tempdir().unwrap();
    let app = app(tmp.path(), false);
    for uri in ["/", "/index.html", "/daily", "/books/settings"] {
        let r = send(&app, get(uri)).await;
        assert_eq!(r.status, StatusCode::OK, "{uri}");
        assert_eq!(r.body, "<html>deckle</html>", "{uri}");
    }
    let r = send(&app, get("/assets/app.js")).await;
    assert_eq!(r.body, "console.log(1)");
    // Nothing outside the web dir.
    std::fs::write(tmp.path().join("secret.txt"), "secret").unwrap();
    let r = send(&app, get("/../secret.txt")).await;
    assert!(!r.body.contains("secret"), "{}", r.body);
}

#[tokio::test]
async fn security_headers_present() {
    let tmp = tempfile::tempdir().unwrap();
    let app = app(tmp.path(), false);
    for uri in ["/", "/assets/app.js", "/api/session", "/api/nope"] {
        let r = send(&app, get(uri)).await;
        assert_eq!(
            r.headers["content-security-policy"],
            "default-src 'self'; style-src 'self' 'unsafe-inline'; img-src 'self' data:",
            "{uri}"
        );
        assert_eq!(r.headers["x-content-type-options"], "nosniff", "{uri}");
        assert_eq!(r.headers["referrer-policy"], "no-referrer", "{uri}");
    }
}
