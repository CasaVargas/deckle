// SPDX-License-Identifier: GPL-3.0-or-later
//! The reader server (docs/superpowers/specs/2026-10-30-koreader-hub-design.md §A, §B): an
//! OPDS catalog of the papers, and KOReader's progress sync, on the home network. The watcher
//! starts it when `[reader_server]` is on and a password is set.

pub mod auth;
pub mod deckle;
pub mod opds;
pub mod sync;

use std::net::SocketAddr;
use std::path::PathBuf;
use std::sync::Arc;

use axum::Router;
use axum::routing::get;
use chrono::NaiveDate;

/// One edition the catalog lists.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Paper {
    pub date: NaiveDate,
    /// What the paper prints on its front page, e.g. "The Deckle Daily".
    pub title: String,
    pub epub: PathBuf,
    /// The edition's own name without extension, `deckle-daily-YYYY-MM-DD`: what it downloads
    /// as, and so the name KOReader knows it by.
    pub stem: String,
    /// The household's main paper (digest.toml's), which the Deckle plugin fetches.
    pub main: bool,
}

/// What the catalog lists, newest first. Built for each request, so a paper printed a minute
/// ago is in the next look.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Catalog {
    pub papers: Vec<Paper>,
    /// EPUBs waiting for the e-readers (the EPUB outbox).
    pub books: Vec<PathBuf>,
}

/// Where the catalog comes from: the CLI lists the Morgue and the outbox.
pub type Source = Arc<dyn Fn() -> Catalog + Send + Sync>;

/// A file handed to a reader: its file name and bytes.
pub type Served = Arc<dyn Fn(&str, &[u8]) + Send + Sync>;
/// A position KOReader synced.
pub type OnProgress = Arc<dyn Fn(&sync::Progress) + Send + Sync>;

/// What the server tells the rest of Deckle, and where it keeps sync records. The CLI fills
/// these in; tests give their own.
#[derive(Clone)]
pub struct Hooks {
    pub source: Source,
    /// A file was handed to a reader, as this file name with these bytes: so a later sync
    /// naming it by its MD5 can be matched.
    pub served: Served,
    /// KOReader reported where it is in a book.
    pub progress: OnProgress,
    /// `koreader-sync.json`.
    pub sync_store: PathBuf,
}

/// How the server runs.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Settings {
    /// `DECKLE_READER_SERVER_BIND`, else every interface: KOReader is on another device.
    pub bind: String,
    pub port: u16,
    pub username: String,
    /// `md5(password)` as lowercase hex, what KOReader's sync sends as its key.
    pub key: String,
}

pub struct Ctx {
    pub settings: Settings,
    pub hooks: Hooks,
    /// One writer at a time for `koreader-sync.json`.
    pub sync_lock: tokio::sync::Mutex<()>,
    /// Files lately served, with their SHA-256, so what `today` describes is what downloads.
    pub served: std::sync::Mutex<Vec<deckle::Key>>,
}

impl Ctx {
    pub fn new(settings: Settings, hooks: Hooks) -> Ctx {
        Ctx {
            settings,
            hooks,
            sync_lock: tokio::sync::Mutex::new(()),
            served: std::sync::Mutex::new(Vec::new()),
        }
    }
}

/// The address the server binds: `DECKLE_READER_SERVER_BIND`, else `0.0.0.0`.
pub fn bind_address() -> String {
    match std::env::var("DECKLE_READER_SERVER_BIND") {
        Ok(b) if !b.trim().is_empty() => b.trim().to_owned(),
        _ => "0.0.0.0".to_owned(),
    }
}

/// Every route, each behind its sign-in.
pub fn router(ctx: Arc<Ctx>) -> Router {
    Router::new()
        .route("/opds", get(opds::root))
        .route("/opds/today", get(opds::today))
        .route("/opds/papers", get(opds::papers))
        .route("/opds/books", get(opds::books))
        .route("/opds/file/{id}", get(opds::file))
        .route("/opds/cover/{id}", get(opds::cover))
        .route("/users/create", axum::routing::post(sync::register))
        .route("/users/auth", get(sync::authorize))
        .route("/syncs/progress", axum::routing::put(sync::update))
        .route("/syncs/progress/{document}", get(sync::get_progress))
        .route("/deckle/v1/today", get(deckle::today))
        .with_state(ctx)
}

/// Serves until `stop` is sent (or dropped), on a runtime of its own (the watcher is
/// synchronous).
pub fn serve_blocking(
    settings: Settings,
    hooks: Hooks,
    stop: std::sync::mpsc::Receiver<()>,
) -> std::io::Result<()> {
    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()?;
    rt.block_on(async move {
        let addr: SocketAddr = format!("{}:{}", settings.bind, settings.port)
            .parse()
            .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidInput, e))?;
        let listener = tokio::net::TcpListener::bind(addr).await?;
        let ctx = Arc::new(Ctx::new(settings, hooks));
        let stopped = async move {
            // The watcher's thread is synchronous: wait for its word off the runtime.
            let _ = tokio::task::spawn_blocking(move || stop.recv()).await;
        };
        axum::serve(listener, router(ctx))
            .with_graceful_shutdown(stopped)
            .await
    })
}

/// This computer's address on the home network, for the URLs to type into KOReader: the
/// interface a packet to the internet would leave by (nothing is sent).
pub fn lan_address() -> Option<std::net::IpAddr> {
    let s = std::net::UdpSocket::bind("0.0.0.0:0").ok()?;
    s.connect("192.0.2.1:9").ok()?;
    let ip = s.local_addr().ok()?.ip();
    (!ip.is_unspecified() && !ip.is_loopback()).then_some(ip)
}

/// The two URLs to type into KOReader: the catalog, and the sync server. In a container the
/// address it finds is the container's own, so `DECKLE_READER_SERVER_URL` (e.g.
/// `http://tower.local:47220`) says what the readers reach.
pub fn urls(port: u16) -> (String, String) {
    urls_from(
        port,
        std::env::var("DECKLE_READER_SERVER_URL").ok().as_deref(),
    )
}

fn urls_from(port: u16, url: Option<&str>) -> (String, String) {
    let base = match url.map(|u| u.trim().trim_end_matches('/')) {
        Some(u) if !u.is_empty() => u.to_owned(),
        _ => {
            let host = lan_address()
                .map(|ip| ip.to_string())
                .unwrap_or_else(|| "<this computer's address>".into());
            format!("http://{host}:{port}")
        }
    };
    (format!("{base}/opds"), base)
}

#[cfg(test)]
mod tests {
    #[test]
    fn the_url_override_wins_without_its_trailing_slash() {
        let (opds, sync) = super::urls_from(47220, Some("http://tower.local:47220/"));
        assert_eq!(opds, "http://tower.local:47220/opds");
        assert_eq!(sync, "http://tower.local:47220");
        assert!(super::urls_from(47220, Some(" ")).1.ends_with(":47220"));
    }
}
