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
    use std::process::{Command, Stdio};
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

    #[test]
    fn serve_with_password_answers_http() {
        let (_tmp, home) = setup();
        // 04:00: nothing due, so the daemon idles and the server stays up.
        let mut child = serve(&home, "04:00")
            .env("DECKLE_WEB_PASSWORD", "a test password")
            .env("DECKLE_WEB_PORT", "0")
            .stdout(Stdio::null())
            .stderr(Stdio::piped())
            .spawn()
            .unwrap();
        let (tx, rx) = mpsc::channel();
        let stderr = child.stderr.take().unwrap();
        let reader = std::thread::spawn(move || {
            for line in BufReader::new(stderr).lines().map_while(Result::ok) {
                let _ = tx.send(line);
            }
        });
        // The port, once the daemon (whose signal handlers stop us cleanly) has started too.
        let (mut port, mut daemon, mut log) = (None, false, String::new());
        let deadline = Instant::now() + Duration::from_secs(60);
        while port.is_none() || !daemon {
            let line = rx
                .recv_timeout(deadline.saturating_duration_since(Instant::now()))
                .unwrap_or_else(|_| panic!("serve starts within 60 s:\n{log}"));
            if let Some(addr) = line.strip_prefix("web UI on http://") {
                port = addr.rsplit(':').next().and_then(|p| p.parse::<u16>().ok());
            }
            daemon |= line.contains("daemon: config");
            log.push_str(&line);
            log.push('\n');
        }
        let port = port.unwrap();

        let r = http(
            port,
            "POST",
            "/api/login",
            None,
            r#"{"password":"a test password"}"#,
        );
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
        let r = http(port, "POST", "/api/invoke/app_version", Some(&cookie), "{}");
        assert!(r.starts_with("HTTP/1.1 200"), "{r}");
        assert!(r.contains(env!("CARGO_PKG_VERSION")), "{r}");

        let term = Command::new("kill")
            .args(["-TERM", &child.id().to_string()])
            .status()
            .unwrap();
        assert!(term.success());
        let status = child.wait().unwrap();
        reader.join().unwrap();
        log.extend(rx.try_iter().map(|l| l + "\n"));
        assert!(status.success(), "{status}\n{log}");
        assert!(log.contains("stopped"), "{log}");
    }
}
