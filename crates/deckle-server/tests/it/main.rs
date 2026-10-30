// SPDX-License-Identifier: GPL-3.0-or-later
//! All integration tests in one binary: one link instead of one per file.

mod auth;
mod events;
mod files;
mod invoke;
mod static_files;

use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use axum::Router;
use axum::body::Body;
use axum::http::{HeaderMap, Request, StatusCode};
use deckle_app::commands::AppState;
use deckle_app::dispatch::Ctx;
use deckle_server::{ServerConfig, SseEvents};
use http_body_util::BodyExt;
use tower::ServiceExt;

pub const PASSWORD: &str = "correct horse";

pub fn ctx(dir: &Path) -> Arc<Ctx> {
    Arc::new(Ctx {
        state: AppState {
            settings_file: dir.join("settings.json"),
            devices_file: dir.join("devices.json"),
            preload: Mutex::new(Vec::new()),
            preload_target: Mutex::new(None),
            start_tab: "books",
        },
        events: Arc::new(SseEvents),
        exe: PathBuf::from("/opt/deckle/deckle"),
        web: true,
    })
}

/// A server with a password and a `web` dir holding an index page and one asset.
pub fn config(dir: &Path, auth_none: bool) -> ServerConfig {
    let web = dir.join("web");
    std::fs::create_dir_all(web.join("assets")).unwrap();
    std::fs::write(web.join("index.html"), "<html>deckle</html>").unwrap();
    std::fs::write(web.join("assets/app.js"), "console.log(1)").unwrap();
    ServerConfig {
        bind: [127, 0, 0, 1].into(),
        port: 0,
        password: (!auth_none).then(|| PASSWORD.to_owned()),
        auth_none,
        web_dir: web,
        roots: vec!["/config".into(), "/data".into()],
    }
}

pub fn app(dir: &Path, auth_none: bool) -> Router {
    deckle_server::router(config(dir, auth_none), ctx(dir))
}

pub struct Reply {
    pub status: StatusCode,
    pub headers: HeaderMap,
    pub body: String,
}

pub async fn send(app: &Router, req: Request<Body>) -> Reply {
    let res = app.clone().oneshot(req).await.unwrap();
    let status = res.status();
    let headers = res.headers().clone();
    let bytes = res.into_body().collect().await.unwrap().to_bytes();
    Reply {
        status,
        headers,
        body: String::from_utf8_lossy(&bytes).into_owned(),
    }
}

/// A `POST` with the CSRF header, a JSON body and, when given, the session cookie.
pub fn post(uri: &str, body: &str, cookie: Option<&str>) -> Request<Body> {
    let mut req = Request::post(uri)
        .header("x-deckle", "1")
        .header("content-type", "application/json");
    if let Some(c) = cookie {
        req = req.header("cookie", c);
    }
    req.body(Body::from(body.to_owned())).unwrap()
}

/// Logs in and returns the `name=value` cookie to send back.
pub async fn login(app: &Router) -> String {
    let body = serde_json::json!({ "password": PASSWORD }).to_string();
    let r = send(app, post("/api/login", &body, None)).await;
    assert_eq!(r.status, StatusCode::OK, "{}", r.body);
    let set = r.headers["set-cookie"].to_str().unwrap();
    set.split(';').next().unwrap().to_owned()
}

/// `serve` on a free port, connected to: the connection, its stop switch and the server task.
pub async fn serving(
    dir: &Path,
    auth_none: bool,
) -> (
    tokio::net::TcpStream,
    tokio::sync::oneshot::Sender<()>,
    tokio::task::JoinHandle<std::io::Result<()>>,
) {
    let mut cfg = config(dir, auth_none);
    // A free port: serve logs the one it gets, but a test can't read the log.
    cfg.port = std::net::TcpListener::bind("127.0.0.1:0")
        .unwrap()
        .local_addr()
        .unwrap()
        .port();
    let port = cfg.port;
    let (stop, stopped) = tokio::sync::oneshot::channel::<()>();
    let server = tokio::spawn(deckle_server::serve(cfg, ctx(dir), async {
        stopped.await.ok();
    }));
    for _ in 0..250 {
        if let Ok(c) = tokio::net::TcpStream::connect(("127.0.0.1", port)).await {
            return (c, stop, server);
        }
        tokio::time::sleep(std::time::Duration::from_millis(20)).await;
    }
    panic!("serve listens within 5 s");
}
