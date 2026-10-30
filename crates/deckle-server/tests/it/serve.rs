// SPDX-License-Identifier: GPL-3.0-or-later
//! `deckle-web serve`: the web UI and the `digest daemon` loop in one process. Offline, as the
//! daemon tests are: a horoscope-only paper exporting to a folder, so no Previewer is needed.

use std::path::{Path, PathBuf};
use std::process::Command;

fn config_dir(home: &Path) -> PathBuf {
    if cfg!(target_os = "macos") {
        home.join("Library/Application Support/deckle")
    } else {
        home.join("deckle")
    }
}

fn today() -> String {
    deckle_digest::chrono::Utc::now().date_naive().to_string()
}

/// A temp home with a horoscope-only main paper due at 05:30, exporting to `<home>/iPad`.
fn setup() -> (tempfile::TempDir, PathBuf) {
    let tmp = tempfile::tempdir().unwrap();
    let home = tmp.path().join("home");
    let dir = config_dir(&home);
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::create_dir_all(home.join("mounts")).unwrap();
    std::fs::write(
        dir.join("digest.toml"),
        format!(
            "sections = [\"horoscope\"]\n[schedule]\nat = \"05:30\"\n\
             [[export]]\nname = \"iPad\"\ndir = {:?}\n",
            home.join("iPad").display().to_string()
        ),
    )
    .unwrap();
    (tmp, home)
}

/// `deckle-web serve` in `home` with the daemon's clock at `hh:mm` today, UTC, and no web
/// settings but those the test adds.
fn serve(home: &Path, hhmm: &str) -> Command {
    let mut c = Command::new(assert_cmd::cargo::cargo_bin("deckle-web"));
    c.env("HOME", home)
        .env("USERPROFILE", home)
        .env("XDG_CONFIG_HOME", home)
        .env("APPDATA", home)
        .env("LOCALAPPDATA", home)
        .env("XDG_CACHE_HOME", home.join("cache"))
        .env("DECKLE_MOUNTS", home.join("mounts"))
        .env("DECKLE_PREVIEWER", home.join("no-previewer"))
        .env("DECKLE_WEB_DIR", home.join("no-web"))
        .env("PATH", "/nonexistent")
        .env("TZ", "UTC")
        .env(
            "DECKLE_DAEMON_FAKE_NOW",
            format!("{}T{hhmm}:00+00:00", today()),
        )
        .env_remove("DECKLE_WEB_PASSWORD")
        .env_remove("DECKLE_WEB_AUTH")
        .env_remove("DECKLE_WEB_PORT")
        .env_remove("DECKLE_WEB_BIND")
        .env_remove("DECKLE_WEB_ROOTS")
        .arg("serve");
    c
}

fn text(b: &[u8]) -> String {
    String::from_utf8_lossy(b).into_owned()
}

#[test]
fn serve_without_password_runs_daemon_only() {
    let (_tmp, home) = setup();
    let o = serve(&home, "06:00")
        .env("DECKLE_DAEMON_ONCE", "1")
        .output()
        .unwrap();
    let err = text(&o.stderr);
    assert!(o.status.success(), "{err}");
    assert!(
        home.join("iPad")
            .join(format!("deckle-daily-{}.epub", today()))
            .is_file(),
        "{err}"
    );
    assert!(text(&o.stdout).contains("[main] "), "{}", text(&o.stdout));
    assert!(
        err.contains("web UI off") && err.contains("DECKLE_WEB_PASSWORD"),
        "{err}"
    );
}

/// A first run with no digest.toml gets `digest init`'s starter and carries on.
#[test]
fn serve_writes_starter_config_when_missing() {
    let (_tmp, home) = setup();
    let path = config_dir(&home).join("digest.toml");
    std::fs::remove_file(&path).unwrap();
    let o = serve(&home, "06:00")
        .env("DECKLE_DAEMON_ONCE", "1")
        .output()
        .unwrap();
    let err = text(&o.stderr);
    assert!(o.status.success(), "{err}");
    assert_eq!(
        std::fs::read_to_string(&path).unwrap(),
        deckle_digest::config::STARTER
    );
    assert!(err.contains("wrote a starter"), "{err}");
    assert!(!err.contains("daemon off"), "{err}");
}

/// The other argv is the `deckle` command line.
#[test]
fn other_arguments_are_the_command_line() {
    let o = Command::new(assert_cmd::cargo::cargo_bin("deckle-web"))
        .arg("--version")
        .output()
        .unwrap();
    assert!(o.status.success(), "{}", text(&o.stderr));
    assert!(
        text(&o.stdout).contains(env!("CARGO_PKG_VERSION")),
        "{}",
        text(&o.stdout)
    );
}

/// Unix only: the clean stop is a SIGTERM, which Windows hasn't got, and a killed serve would
/// leave its `digest watch` child running. The server itself is tested everywhere in-process.
#[cfg(unix)]
mod over_http {
    use std::io::{BufRead, BufReader, Read, Write};
    use std::net::TcpStream;
    use std::path::Path;
    use std::process::{Child, Command, ExitStatus, Stdio};
    use std::sync::mpsc;
    use std::time::{Duration, Instant};

    use super::{serve, setup};

    /// One HTTP/1.1 request on its own connection: the status line, headers and body as text.
    fn http(port: u16, method: &str, path: &str, cookie: Option<&str>, body: &str) -> String {
        let mut s = TcpStream::connect(("127.0.0.1", port)).unwrap();
        s.set_read_timeout(Some(Duration::from_secs(30))).unwrap();
        let cookie = cookie
            .map(|c| format!("Cookie: {c}\r\n"))
            .unwrap_or_default();
        write!(
            s,
            "{method} {path} HTTP/1.1\r\nHost: localhost\r\nConnection: close\r\nX-Deckle: 1\r\n\
             Content-Type: application/json\r\n{cookie}Content-Length: {}\r\n\r\n{body}",
            body.len()
        )
        .unwrap();
        let mut out = String::new();
        s.read_to_string(&mut out).unwrap();
        out
    }

    /// A running `serve` with a password on a free port, and its log so far.
    struct Running {
        child: Child,
        lines: mpsc::Receiver<String>,
        reader: std::thread::JoinHandle<()>,
        port: u16,
        log: String,
    }

    /// Starts `serve` and waits for its port and for a log line containing `ready`.
    fn start(home: &Path, hhmm: &str, ready: &str) -> Running {
        let mut child = serve(home, hhmm)
            .env("DECKLE_WEB_PASSWORD", PASSWORD)
            .env("DECKLE_WEB_PORT", "0")
            .stdout(Stdio::null())
            .stderr(Stdio::piped())
            .spawn()
            .unwrap();
        let (tx, lines) = mpsc::channel();
        let stderr = child.stderr.take().unwrap();
        let reader = std::thread::spawn(move || {
            for line in BufReader::new(stderr).lines().map_while(Result::ok) {
                let _ = tx.send(line);
            }
        });
        let (mut port, mut seen, mut log) = (None, false, String::new());
        let deadline = Instant::now() + Duration::from_secs(60);
        while port.is_none() || !seen {
            let line = lines
                .recv_timeout(deadline.saturating_duration_since(Instant::now()))
                .unwrap_or_else(|_| panic!("serve starts within 60 s:\n{log}"));
            if let Some(addr) = line.strip_prefix("web UI on http://") {
                port = addr.rsplit(':').next().and_then(|p| p.parse::<u16>().ok());
            }
            seen |= line.contains(ready);
            log.push_str(&line);
            log.push('\n');
        }
        Running {
            child,
            lines,
            reader,
            port: port.unwrap(),
            log,
        }
    }

    impl Running {
        /// Logs in and asks for the app's version.
        fn answers(&self) {
            let body = format!("{{\"password\":\"{PASSWORD}\"}}");
            let r = http(self.port, "POST", "/api/login", None, &body);
            assert!(r.starts_with("HTTP/1.1 200"), "{r}");
            let cookie = r
                .lines()
                .find_map(|l| {
                    l.strip_prefix("set-cookie: ")
                        .or_else(|| l.strip_prefix("Set-Cookie: "))
                })
                .and_then(|v| v.split(';').next())
                .unwrap_or_else(|| panic!("a session cookie: {r}"))
                .to_owned();
            let r = http(
                self.port,
                "POST",
                "/api/invoke/app_version",
                Some(&cookie),
                "{}",
            );
            assert!(r.starts_with("HTTP/1.1 200"), "{r}");
            assert!(r.contains(env!("CARGO_PKG_VERSION")), "{r}");
        }

        /// SIGTERM, then the exit status and the whole log.
        fn terminate(mut self) -> (ExitStatus, String) {
            let term = Command::new("kill")
                .args(["-TERM", &self.child.id().to_string()])
                .status()
                .unwrap();
            assert!(term.success());
            let status = self.child.wait().unwrap();
            self.reader.join().unwrap();
            self.log.extend(self.lines.try_iter().map(|l| l + "\n"));
            (status, self.log)
        }
    }

    const PASSWORD: &str = "a test password";

    #[test]
    fn serve_with_password_answers_http() {
        let (_tmp, home) = setup();
        // 04:00: nothing due, so the daemon idles and the server stays up. Its banner comes
        // after its signal handlers, which stop us cleanly.
        let running = start(&home, "04:00", "daemon: config");
        running.answers();
        let (status, log) = running.terminate();
        assert!(status.success(), "{status}\n{log}");
        assert!(log.contains("stopped"), "{log}");
    }

    /// A config that doesn't parse stops the daemon, not the web UI that can fix it.
    #[test]
    fn serve_keeps_ui_when_daemon_fails() {
        let (_tmp, home) = setup();
        std::fs::write(
            super::config_dir(&home).join("digest.toml"),
            "sections = [\"unterminated\n",
        )
        .unwrap();
        let running = start(&home, "04:00", "daemon off:");
        running.answers();
        let (status, log) = running.terminate();
        assert_eq!(status.code(), Some(1), "{log}");
    }
}
