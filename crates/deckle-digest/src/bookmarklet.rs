// SPDX-License-Identifier: GPL-3.0-or-later
//! The bookmarklet (save-to-paper design §4): `deckle digest watch` listens on
//! `127.0.0.1:47219`; the bookmarklet opens `/save?token=…&url=…&title=…` in a small popup,
//! which avoids the browser's mixed-content and CORS rules. The token keeps other local pages
//! from adding to the queue blindly.

use std::io::{BufRead, BufReader, Read, Write};
use std::net::TcpListener;
use std::path::Path;
use std::time::Duration;

use chrono::{DateTime, Utc};

use crate::esc;
use crate::reading::{Added, ReadingQueue};

pub const PORT: u16 = 47219;

/// The token in `bookmarklet.token` in `dir`, created on first use: 32 hex characters.
pub fn token(dir: &Path) -> std::io::Result<String> {
    let path = dir.join("bookmarklet.token");
    if let Ok(t) = std::fs::read_to_string(&path) {
        let t = t.trim().to_owned();
        if t.len() == 32 && t.chars().all(|c| c.is_ascii_hexdigit()) {
            return Ok(t);
        }
    }
    let t = format!("{:032x}", rand::random::<u128>());
    std::fs::create_dir_all(dir)?;
    std::fs::write(&path, &t)?;
    Ok(t)
}

/// The bookmarklet to drag to the bookmarks bar.
pub fn javascript(token: &str) -> String {
    format!(
        "javascript:(()=>{{window.open('http://127.0.0.1:{PORT}/save?token={token}&url='+\
         encodeURIComponent(location.href)+'&title='+encodeURIComponent(document.title),\
         'deckle','width=420,height=200')}})()"
    )
}

/// `%XX` and `+` decoding of one query value.
fn decode(s: &str) -> String {
    let b = s.as_bytes();
    let mut out = Vec::with_capacity(b.len());
    let mut i = 0;
    while i < b.len() {
        match b[i] {
            b'+' => out.push(b' '),
            b'%' if i + 2 < b.len() => match u8::from_str_radix(&s[i + 1..i + 3], 16) {
                Ok(v) => {
                    out.push(v);
                    i += 2;
                }
                Err(_) => out.push(b'%'),
            },
            c => out.push(c),
        }
        i += 1;
    }
    String::from_utf8_lossy(&out).into_owned()
}

fn page(heading: &str, detail: &str) -> String {
    format!(
        "<!doctype html><html><head><meta charset=\"utf-8\"><title>Deckle</title>\
         <style>body{{font:16px Georgia,serif;margin:24px;color:#1f1b16;background:#f6f3ee}}\
         h1{{font-size:20px;margin:0 0 8px}}p{{margin:0 0 16px;color:#7a7169}}\
         button{{font:inherit;padding:4px 14px}}</style></head><body>\
         <h1>{}</h1><p>{}</p><button onclick=\"window.close()\">Close</button>\
         <script>setTimeout(()=>window.close(),2500)</script></body></html>",
        esc(heading),
        esc(detail)
    )
}

/// One request line (`GET /save?… HTTP/1.1`) → status and HTML body.
pub fn handle(
    request_line: &str,
    token: &str,
    queue: &ReadingQueue,
    now: DateTime<Utc>,
) -> (u16, String) {
    let mut parts = request_line.split_whitespace();
    let (Some("GET"), Some(target)) = (parts.next(), parts.next()) else {
        return (404, page("Not found", ""));
    };
    let Some(query) = target.strip_prefix("/save?") else {
        return (404, page("Not found", ""));
    };
    let param = |name: &str| {
        query
            .split('&')
            .find_map(|kv| kv.strip_prefix(&format!("{name}=")))
            .map(decode)
    };
    if param("token").as_deref() != Some(token) {
        return (
            403,
            page(
                "Not saved",
                "This bookmarklet is out of date: run `deckle save --bookmarklet` again.",
            ),
        );
    }
    let Some(url) = param("url").filter(|u| u.starts_with("http://") || u.starts_with("https://"))
    else {
        return (
            400,
            page(
                "Not saved",
                "That page has no web address Deckle can fetch.",
            ),
        );
    };
    let title = param("title");
    match queue.add_url(&url, title.as_deref(), "bookmarklet", now) {
        Ok(Added::New(_)) => (
            200,
            page(
                "Saved to tomorrow's paper",
                title.as_deref().unwrap_or(&url),
            ),
        ),
        Ok(Added::Duplicate(_)) => (200, page("Already saved", title.as_deref().unwrap_or(&url))),
        Err(e) => (500, page("Not saved", &e)),
    }
}

/// Serves the bookmarklet on `127.0.0.1:PORT`, one request per connection, until the
/// process ends. Errors only when the port can't be bound.
pub fn serve(queue: ReadingQueue, token: String) -> std::io::Result<()> {
    let listener = TcpListener::bind(("127.0.0.1", PORT))?;
    for stream in listener.incoming() {
        let Ok(mut stream) = stream else { continue };
        let _ = stream.set_read_timeout(Some(Duration::from_secs(5)));
        let mut line = String::new();
        let mut reader = BufReader::new((&stream).take(8 * 1024));
        if reader.read_line(&mut line).is_err() {
            continue;
        }
        let (status, body) = handle(line.trim_end(), &token, &queue, Utc::now());
        let reason = match status {
            200 => "OK",
            400 => "Bad Request",
            403 => "Forbidden",
            500 => "Internal Server Error",
            _ => "Not Found",
        };
        let _ = write!(
            stream,
            "HTTP/1.1 {status} {reason}\r\nContent-Type: text/html; charset=utf-8\r\n\
             Content-Length: {}\r\nConnection: close\r\n\r\n{body}",
            body.len()
        );
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    use crate::reading::ReadingQueue;

    fn now() -> chrono::DateTime<chrono::Utc> {
        "2026-10-20T14:03:11Z".parse().unwrap()
    }

    #[test]
    fn good_request_queues_and_says_saved() {
        let dir = tempfile::tempdir().unwrap();
        let q = ReadingQueue::new(dir.path().join("reading.jsonl"));
        let (status, body) = handle(
            "GET /save?token=abc&url=https%3A%2F%2Fex.com%2Fa%3Fx%3D1&title=A%20%26%20B HTTP/1.1",
            "abc",
            &q,
            now(),
        );
        assert_eq!(status, 200);
        assert!(body.contains("Saved to tomorrow&#39;s paper"), "{body}");
        assert!(body.contains("A &amp; B"), "title is escaped: {body}");
        let e = &q.load()[0];
        assert_eq!(e.url.as_deref(), Some("https://ex.com/a?x=1"));
        assert_eq!(e.source, "bookmarklet");
        let (_, again) = handle(
            "GET /save?token=abc&url=https%3A%2F%2Fex.com%2Fa%3Fx%3D1 HTTP/1.1",
            "abc",
            &q,
            now(),
        );
        assert!(again.contains("Already saved"), "{again}");
    }

    #[test]
    fn wrong_or_missing_token_is_403() {
        let dir = tempfile::tempdir().unwrap();
        let q = ReadingQueue::new(dir.path().join("reading.jsonl"));
        for line in [
            "GET /save?token=nope&url=https%3A%2F%2Fex.com HTTP/1.1",
            "GET /save?url=https%3A%2F%2Fex.com HTTP/1.1",
        ] {
            assert_eq!(handle(line, "abc", &q, now()).0, 403, "{line}");
        }
        assert!(q.load().is_empty());
    }

    #[test]
    fn non_get_or_other_path_is_404() {
        let dir = tempfile::tempdir().unwrap();
        let q = ReadingQueue::new(dir.path().join("reading.jsonl"));
        assert_eq!(
            handle("POST /save?token=abc HTTP/1.1", "abc", &q, now()).0,
            404
        );
        assert_eq!(handle("GET /other HTTP/1.1", "abc", &q, now()).0, 404);
        assert_eq!(handle("garbage", "abc", &q, now()).0, 404);
    }

    #[test]
    fn token_is_created_once_and_reused() {
        let dir = tempfile::tempdir().unwrap();
        let a = token(dir.path()).unwrap();
        assert_eq!(a.len(), 32);
        assert_eq!(token(dir.path()).unwrap(), a);
        assert!(javascript(&a).contains(&format!("127.0.0.1:47219/save?token={a}&url=")));
    }
}
