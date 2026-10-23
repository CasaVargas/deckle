// SPDX-License-Identifier: GPL-3.0-or-later
//! A reMarkable (other readers design §9): the paper as a PDF over its USB web interface, here a
//! stand-in on 127.0.0.1 through `DECKLE_REMARKABLE_URL`, with no Kindle Previewer.

use std::io::{BufRead, BufReader, Read, Write};
use std::net::TcpListener;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use assert_cmd::Command;

/// The tablet: lists `listing`, records every upload's body.
fn tablet(listing: &'static str) -> (String, Arc<Mutex<Vec<Vec<u8>>>>) {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let base = format!("http://{}", listener.local_addr().unwrap());
    let uploads = Arc::new(Mutex::new(Vec::new()));
    let seen = uploads.clone();
    std::thread::spawn(move || {
        for stream in listener.incoming().flatten() {
            let mut r = BufReader::new(stream.try_clone().unwrap());
            let mut line = String::new();
            if r.read_line(&mut line).is_err() {
                continue;
            }
            let mut len = 0usize;
            loop {
                let mut h = String::new();
                if r.read_line(&mut h).is_err() || h == "\r\n" || h.is_empty() {
                    break;
                }
                if let Some(v) = h.to_ascii_lowercase().strip_prefix("content-length:") {
                    len = v.trim().parse().unwrap_or(0);
                }
            }
            let mut body = vec![0; len];
            let _ = r.read_exact(&mut body);
            let (status, reply) = if line.starts_with("GET /documents/") {
                ("200 OK", listing)
            } else if line.starts_with("POST /upload") {
                seen.lock().unwrap().push(body);
                ("201 Created", "{}")
            } else {
                ("404 Not Found", "")
            };
            let mut s = stream;
            let _ = write!(
                s,
                "HTTP/1.1 {status}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{reply}",
                reply.len()
            );
        }
    });
    (base, uploads)
}

fn deckle(home: &Path, tablet: &str) -> Command {
    let mut c = Command::cargo_bin("deckle").unwrap();
    c.env("HOME", home)
        .env("XDG_CONFIG_HOME", home)
        .env("APPDATA", home)
        .env("LOCALAPPDATA", home)
        .env("XDG_CACHE_HOME", home.join("cache"))
        .env("DECKLE_MOUNTS", home.join("no-drives"))
        .env("DECKLE_PREVIEWER", home.join("no-previewer"))
        .env("DECKLE_REMARKABLE_URL", tablet)
        .env("PATH", "/nonexistent");
    c
}

fn setup(extra: &str) -> (tempfile::TempDir, PathBuf) {
    let tmp = tempfile::tempdir().unwrap();
    let home = tmp.path().join("home");
    let dir = if cfg!(target_os = "macos") {
        home.join("Library/Application Support/deckle")
    } else {
        home.join("deckle")
    };
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(
        dir.join("digest.toml"),
        format!("sections = [\"horoscope\", \"puzzle\"]\n\n[remarkable]\nenabled = true\n{extra}"),
    )
    .unwrap();
    (tmp, home)
}

#[test]
fn the_morning_paper_goes_to_a_remarkable_as_a_pdf() {
    let (base, uploads) = tablet("[]");
    let (_tmp, home) = setup("");
    let o = deckle(&home, &base)
        .args(["--json", "digest", "run", "--date", "2026-10-23"])
        .output()
        .unwrap();
    assert!(o.status.success(), "{}", String::from_utf8_lossy(&o.stderr));
    let v: serde_json::Value = serde_json::from_slice(&o.stdout).unwrap();
    assert_eq!(v["remarkable_queued"], true);
    assert_eq!(
        v["remarkable"]["uploaded"][0], "deckle-daily-2026-10-23.pdf",
        "{v}"
    );
    let got = uploads.lock().unwrap();
    let body = &got[0];
    let at = body.windows(5).position(|w| w == b"%PDF-").expect("a PDF");
    assert!(at < 400, "the PDF is the form's file");
}

#[test]
fn an_absent_tablet_keeps_the_paper_for_the_next_push_and_it_goes_once() {
    let (_tmp, home) = setup("");
    // No tablet answers.
    let free = TcpListener::bind("127.0.0.1:0").unwrap();
    let gone = format!("http://{}", free.local_addr().unwrap());
    drop(free);
    let o = deckle(&home, &gone)
        .args(["digest", "run", "--date", "2026-10-23"])
        .output()
        .unwrap();
    assert!(o.status.success(), "{}", String::from_utf8_lossy(&o.stderr));
    assert!(String::from_utf8_lossy(&o.stdout).contains("queued for the reMarkable"));
    // Plugged in: the push sends it.
    let (base, uploads) = tablet("[]");
    let o = deckle(&home, &base)
        .args(["digest", "push"])
        .output()
        .unwrap();
    assert!(o.status.success(), "{}", String::from_utf8_lossy(&o.stderr));
    assert!(
        String::from_utf8_lossy(&o.stdout).contains("reMarkable -> deckle-daily-2026-10-23.pdf"),
        "{}",
        String::from_utf8_lossy(&o.stdout)
    );
    assert_eq!(uploads.lock().unwrap().len(), 1);
    // A second push has nothing left to send.
    deckle(&home, &base)
        .args(["digest", "push"])
        .assert()
        .success();
    assert_eq!(uploads.lock().unwrap().len(), 1);
}
