// SPDX-License-Identifier: GPL-3.0-or-later
//! `deckle-web`: the container's one binary. `deckle-web serve` runs the web UI and the
//! `digest daemon` loop together; any other argv is the `deckle` command line, which is also
//! how the daemon's children (`digest run`, `digest watch`) run, as they re-exec this binary.

use std::path::{Path, PathBuf};
use std::process::ExitCode;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use clap::Parser;
use deckle_app::dispatch::Ctx;
use deckle_app::state::AppState;
use deckle_server::{ServerConfig, SseEvents};

/// How long requests still running at a stop get before the process exits anyway.
const STOP_GRACE: Duration = Duration::from_secs(10);

#[derive(Parser)]
#[command(name = "deckle-web serve", bin_name = "deckle-web serve", version)]
/// Serve the web UI and run the daily papers' schedule, until SIGTERM or SIGINT.
struct Serve {
    /// The port, over `DECKLE_WEB_PORT` (default 8080).
    #[arg(long)]
    port: Option<u16>,
}

fn main() -> ExitCode {
    let args: Vec<std::ffi::OsString> = std::env::args_os().collect();
    if args.get(1).and_then(|a| a.to_str()) == Some("serve") {
        let serve = match Serve::try_parse_from(&args[1..]) {
            Ok(s) => s,
            Err(e) => {
                let _ = e.print();
                return ExitCode::from(if e.use_stderr() { 2 } else { 0 });
            }
        };
        return serve.run();
    }
    deckle_cli::main_with(args)
}

impl Serve {
    fn run(self) -> ExitCode {
        // `serve` is a daemon install by definition: the env-var passwords, the config as the
        // schedule and the daemon's own watcher apply to the web UI's commands and to every
        // child. Set before any thread starts, so nothing reads the environment meanwhile.
        if std::env::var_os("DECKLE_DAEMON").is_none_or(|v| v.is_empty()) {
            // SAFETY: still single-threaded; no other code reads or writes the environment.
            unsafe { std::env::set_var("DECKLE_DAEMON", "1") };
        }
        let web = self.web_config();

        let (done, daemon_done) = tokio::sync::oneshot::channel::<()>();
        let daemon = std::thread::spawn(move || {
            let r = deckle_cli::run_daemon(deckle_cli::DAEMON_TICK);
            // The daemon returns on SIGTERM/SIGINT (or once, in tests): the server stops too.
            let _ = done.send(());
            r
        });
        if let Some(cfg) = web {
            serve_web(cfg, daemon_done);
        }
        match daemon.join() {
            Ok(Ok(())) => ExitCode::SUCCESS,
            Ok(Err(msg)) => {
                eprintln!("error: {msg}");
                ExitCode::FAILURE
            }
            Err(_) => ExitCode::FAILURE,
        }
    }

    /// The web UI's settings, or `None` (said) when it stays off and only the daemon runs.
    fn web_config(&self) -> Option<ServerConfig> {
        let web_dir = std::env::var_os("DECKLE_WEB_DIR")
            .filter(|d| !d.is_empty())
            .map(PathBuf::from)
            .unwrap_or_else(|| PathBuf::from("/opt/deckle/web"));
        let mut cfg = match ServerConfig::from_env(web_dir) {
            Ok(c) => c,
            Err(why) => {
                eprintln!("web UI off: {why}");
                return None;
            }
        };
        if let Some(port) = self.port {
            cfg.port = port;
        }
        if !cfg.web_dir.is_dir() {
            // The API still answers; only the pages 404.
            eprintln!("web UI files not found at {}", cfg.web_dir.display());
        }
        cfg.roots.retain(|root| {
            let there = root.is_dir();
            if !there {
                eprintln!("web UI: skipping missing root {}", root.display());
            }
            there
        });
        if cfg.roots.is_empty() {
            // Uploads (always under /data) fail too.
            eprintln!("web UI: no file roots; the file browser shows nothing");
        }
        Some(cfg)
    }
}

/// Serves until `stop` fires (or the daemon thread ends and drops it). A failure to start is
/// said and leaves the daemon running.
fn serve_web(cfg: ServerConfig, stop: tokio::sync::oneshot::Receiver<()>) {
    let rt = match tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
    {
        Ok(rt) => rt,
        Err(e) => {
            eprintln!("web UI off: can't start its runtime: {e}");
            return;
        }
    };
    let result = rt.block_on(deckle_server::serve(cfg, Arc::new(ctx()), async {
        let _ = stop.await;
    }));
    if let Err(e) = result {
        eprintln!("web UI off: {e}");
    }
    // A command still running on a blocking thread mustn't hold up the exit.
    rt.shutdown_timeout(STOP_GRACE);
}

/// The app's state as the desktop app keeps it: its `settings.json` and `devices.json` (which
/// `digest push`/`watch` read too) in the same config folder.
fn ctx() -> Ctx {
    let devices_file = deckle_core::devices::default_registry_file();
    let settings_file = devices_file
        .parent()
        .unwrap_or_else(|| Path::new("."))
        .join("settings.json");
    Ctx {
        state: AppState {
            settings_file,
            devices_file,
            preload: Mutex::new(Vec::new()),
            preload_target: Mutex::new(None),
            start_tab: "books",
        },
        events: Arc::new(SseEvents),
        exe: std::env::current_exe().unwrap_or_else(|_| PathBuf::from("deckle-web")),
        web: true,
    }
}
