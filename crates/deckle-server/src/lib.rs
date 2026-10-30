// SPDX-License-Identifier: GPL-3.0-or-later
//! The web UI's server: the desktop app's commands over HTTP (`POST /api/invoke/<name>`), its
//! events as Server-Sent Events, a single-password login, and the built frontend.

mod auth;
mod events;
mod invoke;
mod static_files;

use std::fmt;
use std::future::Future;
use std::io;
use std::net::{IpAddr, SocketAddr};
use std::path::PathBuf;
use std::sync::Arc;

use axum::Router;
use axum::http::StatusCode;
use axum::middleware;
use axum::routing::{get, post};
use deckle_app::dispatch::Ctx;
use tokio::sync::watch;

pub use events::SseEvents;

/// How the web UI is served, from `DECKLE_WEB_*`.
pub struct ServerConfig {
    pub bind: IpAddr,
    pub port: u16,
    pub password: Option<String>,
    /// `DECKLE_WEB_AUTH=none`: no login, for installs behind a proxy that does its own.
    pub auth_none: bool,
    /// The built frontend.
    pub web_dir: PathBuf,
    /// What the file browser may show: `/config`, `/data` and `DECKLE_WEB_ROOTS`.
    pub roots: Vec<PathBuf>,
}

// By hand, so a logged config never shows the password.
impl fmt::Debug for ServerConfig {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("ServerConfig")
            .field("bind", &self.bind)
            .field("port", &self.port)
            .field("password", &self.password.as_ref().map(|_| "…"))
            .field("auth_none", &self.auth_none)
            .field("web_dir", &self.web_dir)
            .field("roots", &self.roots)
            .finish()
    }
}

impl ServerConfig {
    /// Reads the process environment. Errs, naming the variable, on a bad value or when there
    /// is neither a password nor `DECKLE_WEB_AUTH=none`.
    pub fn from_env(web_dir: PathBuf) -> Result<ServerConfig, String> {
        Self::from_lookup(web_dir, |k| std::env::var(k).ok())
    }

    /// [`from_env`](Self::from_env) over any lookup. A blank value counts as unset: Unraid
    /// passes the template's empty fields that way.
    pub fn from_lookup(
        web_dir: PathBuf,
        env: impl Fn(&str) -> Option<String>,
    ) -> Result<ServerConfig, String> {
        let var = |k: &str| env(k).filter(|v| !v.trim().is_empty());
        let bind = match var("DECKLE_WEB_BIND") {
            Some(v) => v
                .trim()
                .parse()
                .map_err(|_| format!("DECKLE_WEB_BIND: not an IP address: {v}"))?,
            None => IpAddr::from([127, 0, 0, 1]),
        };
        let port = match var("DECKLE_WEB_PORT") {
            Some(v) => v
                .trim()
                .parse()
                .map_err(|_| format!("DECKLE_WEB_PORT: not a port: {v}"))?,
            None => 8080,
        };
        let auth_none = match var("DECKLE_WEB_AUTH") {
            Some(v) if v.trim() == "none" => true,
            Some(v) => {
                return Err(format!(
                    "DECKLE_WEB_AUTH: only \"none\" is understood, not {v:?}"
                ));
            }
            None => false,
        };
        let password = var("DECKLE_WEB_PASSWORD");
        if password.is_none() && !auth_none {
            return Err("the web UI needs a password: set DECKLE_WEB_PASSWORD \
                        (or DECKLE_WEB_AUTH=none behind a proxy that logs in for you)"
                .into());
        }
        let mut roots = vec![PathBuf::from("/config"), PathBuf::from("/data")];
        if let Some(extra) = var("DECKLE_WEB_ROOTS") {
            roots.extend(
                extra
                    .split(':')
                    .filter(|r| !r.is_empty())
                    .map(PathBuf::from),
            );
        }
        Ok(ServerConfig {
            bind,
            port,
            password,
            auth_none,
            web_dir,
            roots,
        })
    }
}

/// What every handler shares.
pub(crate) struct Server {
    pub(crate) cfg: ServerConfig,
    pub(crate) ctx: Arc<Ctx>,
    pub(crate) sessions: auth::Sessions,
    pub(crate) login_slots: Arc<tokio::sync::Semaphore>,
    /// Set on shutdown: ends the event streams, which would otherwise never finish.
    pub(crate) stop: watch::Sender<bool>,
}

pub(crate) type Shared = Arc<Server>;

/// The whole site: `/api/…` and the frontend.
pub fn router(cfg: ServerConfig, ctx: Arc<Ctx>) -> Router {
    build(Arc::new(Server {
        cfg,
        ctx,
        sessions: Default::default(),
        login_slots: Arc::new(tokio::sync::Semaphore::new(auth::LOGIN_SLOTS)),
        stop: watch::channel(false).0,
    }))
}

fn build(server: Shared) -> Router {
    // Everything here needs a session; Task 6's files routes nest in here too.
    let private = Router::new()
        .route("/invoke/{command}", post(invoke::invoke))
        .route("/events", get(events::stream))
        .route_layer(middleware::from_fn_with_state(
            server.clone(),
            auth::require_session,
        ));
    let api = Router::new()
        .route("/login", post(auth::login))
        .route("/logout", post(auth::logout))
        .route("/session", get(auth::session))
        .merge(private)
        // An unknown API path is an error, not the frontend's index page.
        .fallback(|| async { invoke::error(StatusCode::NOT_FOUND, "no such API".into()) })
        .layer(middleware::from_fn(auth::require_csrf_header));
    let site = Router::new()
        .nest("/api", api)
        .fallback_service(static_files::service(&server.cfg.web_dir));
    static_files::security_headers(site).with_state(server)
}

/// Serves until `shutdown` resolves, then finishes the requests in flight and returns.
pub async fn serve(
    cfg: ServerConfig,
    ctx: Arc<Ctx>,
    shutdown: impl Future<Output = ()> + Send + 'static,
) -> io::Result<()> {
    if !cfg.auth_none
        && let Some(password) = &cfg.password
        && password.chars().count() < 12
    {
        eprintln!("web UI password is short; use 12+ characters");
    }
    let listener = tokio::net::TcpListener::bind((cfg.bind, cfg.port)).await?;
    // The address bound, not the one asked for: port 0 picks a free one.
    eprintln!("web UI on http://{}", listener.local_addr()?);
    let (stop, _) = watch::channel(false);
    let server = Arc::new(Server {
        cfg,
        ctx,
        sessions: Default::default(),
        login_slots: Arc::new(tokio::sync::Semaphore::new(auth::LOGIN_SLOTS)),
        stop: stop.clone(),
    });
    let app = build(server);
    // The peer's address, for logging failed logins.
    let app = app.into_make_service_with_connect_info::<SocketAddr>();
    axum::serve(listener, app)
        .with_graceful_shutdown(async move {
            shutdown.await;
            stop.send_replace(true);
        })
        .await
}
