// SPDX-License-Identifier: GPL-3.0-or-later
//! Wrong passwords on the catalog and sync: a few are free, then that address waits.

use std::net::SocketAddr;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use axum::Router;
use axum::body::Body;
use axum::extract::ConnectInfo;
use axum::http::{Request, StatusCode};
use base64::Engine;
use deckle_readerserver::{Catalog, Ctx, Hooks, Settings, auth, router};
use tower::ServiceExt;

struct App {
    app: Router,
    now: Arc<Mutex<Instant>>,
    _tmp: tempfile::TempDir,
}

impl App {
    fn new() -> App {
        let tmp = tempfile::tempdir().unwrap();
        let now = Arc::new(Mutex::new(Instant::now()));
        let read = now.clone();
        let ctx = Arc::new(Ctx::with_clock(
            Settings {
                bind: "127.0.0.1".into(),
                port: 0,
                username: "deckle".into(),
                key: auth::md5_hex(b"hunter2"),
            },
            Hooks {
                source: Arc::new(Catalog::default),
                served: Arc::new(|_, _| {}),
                progress: Arc::new(|_| {}),
                sync_store: tmp.path().join("koreader-sync.json"),
            },
            Arc::new(move || *read.lock().unwrap()),
        ));
        App {
            app: router(ctx),
            now,
            _tmp: tmp,
        }
    }

    fn advance(&self, d: Duration) {
        *self.now.lock().unwrap() += d;
    }

    async fn status(&self, req: Request<Body>) -> (StatusCode, Option<String>) {
        let r = self.app.clone().oneshot(req).await.unwrap();
        let retry = r
            .headers()
            .get("retry-after")
            .map(|v| v.to_str().unwrap().to_owned());
        (r.status(), retry)
    }
}

fn from(addr: &str, mut req: Request<Body>) -> Request<Body> {
    let addr: SocketAddr = addr.parse().unwrap();
    req.extensions_mut().insert(ConnectInfo(addr));
    req
}

fn opds(addr: &str, password: Option<&str>) -> Request<Body> {
    let mut b = Request::get("/opds");
    if let Some(p) = password {
        let token = base64::engine::general_purpose::STANDARD.encode(format!("deckle:{p}"));
        b = b.header("authorization", format!("Basic {token}"));
    }
    from(addr, b.body(Body::empty()).unwrap())
}

fn kosync(addr: &str, key: &str) -> Request<Body> {
    from(
        addr,
        Request::get("/users/auth")
            .header("x-auth-user", "deckle")
            .header("x-auth-key", key)
            .body(Body::empty())
            .unwrap(),
    )
}

#[tokio::test]
async fn asking_without_a_password_is_not_a_failure() {
    let a = App::new();
    for _ in 0..100 {
        assert_eq!(
            a.status(opds("192.168.1.9:1", None)).await.0,
            StatusCode::UNAUTHORIZED
        );
    }
    assert_eq!(
        a.status(opds("192.168.1.9:1", Some("hunter2"))).await.0,
        StatusCode::OK
    );
}

#[tokio::test]
async fn the_sixth_to_tenth_wrong_tries_wait_one_to_sixteen_seconds() {
    let a = App::new();
    let peer = "192.168.1.9:1";
    for _ in 0..5 {
        assert_eq!(
            a.status(opds(peer, Some("wrong"))).await.0,
            StatusCode::UNAUTHORIZED
        );
    }
    for wait in [1u64, 2, 4, 8, 16] {
        assert_eq!(
            a.status(opds(peer, Some("wrong"))).await.0,
            StatusCode::UNAUTHORIZED
        );
        // Refused now, even with the right password, until the wait is over.
        let (status, retry) = a.status(opds(peer, Some("hunter2"))).await;
        assert_eq!(status, StatusCode::TOO_MANY_REQUESTS);
        assert_eq!(retry.as_deref(), Some(wait.to_string().as_str()));
        a.advance(Duration::from_secs(wait));
    }
    // Another reader on the network was never affected.
    assert_eq!(
        a.status(opds("192.168.1.20:1", Some("hunter2"))).await.0,
        StatusCode::OK
    );
    // And the right password, once allowed, clears the count.
    assert_eq!(
        a.status(opds(peer, Some("hunter2"))).await.0,
        StatusCode::OK
    );
    assert_eq!(
        a.status(opds(peer, Some("wrong"))).await.0,
        StatusCode::UNAUTHORIZED
    );
}

#[tokio::test]
async fn sync_keys_count_too() {
    let a = App::new();
    let peer = "192.168.1.9:1";
    for _ in 0..6 {
        assert_eq!(
            a.status(kosync(peer, &auth::md5_hex(b"wrong"))).await.0,
            StatusCode::UNAUTHORIZED
        );
    }
    assert_eq!(
        a.status(kosync(peer, &auth::md5_hex(b"hunter2"))).await.0,
        StatusCode::TOO_MANY_REQUESTS
    );
    assert_eq!(
        a.status(kosync("192.168.1.20:1", &auth::md5_hex(b"hunter2")))
            .await
            .0,
        StatusCode::OK
    );
}

#[tokio::test]
async fn a_reader_fetching_many_things_at_once_is_not_turned_away() {
    let a = App::new();
    let mut tries = tokio::task::JoinSet::new();
    for _ in 0..50 {
        let app = a.app.clone();
        tries.spawn(async move {
            app.oneshot(opds("192.168.1.9:1", Some("hunter2")))
                .await
                .unwrap()
                .status()
        });
    }
    while let Some(s) = tries.join_next().await {
        assert_eq!(s.unwrap(), StatusCode::OK);
    }
}
