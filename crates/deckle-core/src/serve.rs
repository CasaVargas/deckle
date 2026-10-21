// SPDX-License-Identifier: GPL-3.0-or-later
//! `deckle serve` (Wi-Fi delivery design): the outbox, read-only, over HTTP on one address the
//! user chose, for a Kindle's browser to download from. Every path sits under `/t/<token>/`;
//! a file is served only when its name matches one the listing produced, so there is nothing
//! for `..` to reach.

use std::io::{BufRead, BufReader, Read, Write};
use std::net::{SocketAddr, TcpListener, TcpStream};
use std::path::{Path, PathBuf};
use std::time::Duration;

use crate::queue::Queue;

/// The default port; the bookmarklet has 47219.
pub const PORT: u16 = 47220;

/// The extension every queued book is also offered under, for Kindle browsers that only
/// download the old Amazon types (design §7). Same bytes.
const ALIAS: &str = "azw";

/// What to send back.
#[derive(Debug, PartialEq, Eq)]
pub enum Body {
    Html(String),
    /// The file's bytes, streamed by [`serve`].
    File(PathBuf),
    /// `HEAD`: headers only.
    Empty,
}

#[derive(Debug)]
pub struct Response {
    pub status: u16,
    pub headers: Vec<(&'static str, String)>,
    pub body: Body,
}

impl Response {
    fn html(status: u16, body: String, head: bool) -> Self {
        Response {
            status,
            headers: vec![
                ("Content-Type", "text/html; charset=utf-8".into()),
                ("Content-Length", body.len().to_string()),
            ],
            body: if head { Body::Empty } else { Body::Html(body) },
        }
    }
}

/// The token in `serve.token` in `dir`, created on first use (or replaced when `rotate`):
/// 32 hex characters.
pub fn token(dir: &Path, rotate: bool) -> std::io::Result<String> {
    let path = dir.join("serve.token");
    if !rotate && let Ok(t) = std::fs::read_to_string(&path) {
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

/// The index URL to type into the Kindle's browser.
pub fn url(addr: SocketAddr, token: &str) -> String {
    format!("http://{addr}/t/{token}/")
}

/// Constant-time string compare, for the token.
fn same(a: &str, b: &str) -> bool {
    a.len() == b.len()
        && a.bytes()
            .zip(b.bytes())
            .fold(0u8, |acc, (x, y)| acc | (x ^ y))
            == 0
}

/// Strict `%XX` decoding of one path segment; `None` on a bad escape or non-UTF-8.
fn decode(s: &str) -> Option<String> {
    let b = s.as_bytes();
    let mut out = Vec::with_capacity(b.len());
    let mut i = 0;
    while i < b.len() {
        if b[i] == b'%' {
            let hex = s.get(i + 1..i + 3)?;
            out.push(u8::from_str_radix(hex, 16).ok()?);
            i += 3;
        } else {
            out.push(b[i]);
            i += 1;
        }
    }
    String::from_utf8(out).ok()
}

fn encode(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for b in s.bytes() {
        if b.is_ascii_alphanumeric() || matches!(b, b'-' | b'_' | b'.' | b'~') {
            out.push(b as char);
        } else {
            out.push_str(&format!("%{b:02X}"));
        }
    }
    out
}

fn esc(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}

fn content_type(name: &str) -> &'static str {
    match name.rsplit_once('.').map(|(_, x)| x.to_ascii_lowercase()) {
        Some(x) if x == ALIAS => "application/vnd.amazon.ebook",
        _ => "application/octet-stream",
    }
}

fn disposition(name: &str) -> String {
    let plain: String = name
        .chars()
        .map(|c| {
            if c.is_ascii() && !c.is_ascii_control() && !matches!(c, '"' | '\\') {
                c
            } else {
                '_'
            }
        })
        .collect();
    if plain == name {
        format!("attachment; filename=\"{plain}\"")
    } else {
        format!(
            "attachment; filename=\"{plain}\"; filename*=UTF-8''{}",
            encode(name)
        )
    }
}

fn size(bytes: u64) -> String {
    if bytes >= 1 << 20 {
        format!("{:.1} MB", bytes as f64 / f64::from(1 << 20))
    } else {
        format!("{} KB", bytes.div_ceil(1024))
    }
}

fn index(token: &str, books: &[(String, PathBuf)]) -> String {
    let mut items = String::new();
    for (name, path) in books {
        let len = std::fs::metadata(path).map(|m| m.len()).unwrap_or(0);
        let stem = name.strip_suffix(".kfx").unwrap_or(name);
        let alias = format!("{stem}.{ALIAS}");
        items.push_str(&format!(
            "<li><a href=\"/t/{token}/{}\">{}</a> <span>{}</span><br>\
             <a class=\"alt\" href=\"/t/{token}/{}\">as .{ALIAS} (if the download is \
             refused)</a></li>",
            encode(name),
            esc(name),
            size(len),
            encode(&alias),
        ));
    }
    let list = if books.is_empty() {
        "<p>Nothing waiting in the outbox.</p>".to_owned()
    } else {
        format!("<ul>{items}</ul>")
    };
    format!(
        "<!doctype html><html><head><meta charset=\"utf-8\"><title>Deckle</title>\
         <meta name=\"viewport\" content=\"width=device-width\">\
         <style>body{{font:20px Georgia,serif;margin:24px;color:#000;background:#fff}}\
         h1{{font-size:28px}}ul{{list-style:none;padding:0}}li{{margin:0 0 28px}}\
         a{{color:#000}}span,.alt,p.note{{font-size:16px;color:#444}}</style></head><body>\
         <h1>Deckle outbox</h1>{list}\
         <p class=\"note\">Wi-Fi delivers the book only. X-Ray and Word Wise need USB \
         (<code>deckle digest push</code>).</p></body></html>"
    )
}

/// One request line (`GET /t/<token>/ HTTP/1.1`) → the response. Reads the outbox, never
/// writes it.
pub fn handle(request_line: &str, token: &str, queue: &Queue) -> Response {
    let forbidden = || Response::html(403, "<!doctype html><p>Forbidden</p>".into(), false);
    let mut parts = request_line.split_whitespace();
    let (Some(method), Some(target)) = (parts.next(), parts.next()) else {
        return forbidden();
    };
    let path = target.split(['?', '#']).next().unwrap_or("");
    let Some(rest) = path.strip_prefix("/t/") else {
        return forbidden();
    };
    let (given, file) = rest.split_once('/').unwrap_or((rest, ""));
    if !same(given, token) {
        return forbidden();
    }
    let head = match method {
        "GET" => false,
        "HEAD" => true,
        _ => {
            return Response::html(
                405,
                "<!doctype html><p>Method not allowed</p>".into(),
                false,
            );
        }
    };
    let books: Vec<(String, PathBuf)> = queue
        .pending()
        .into_iter()
        .filter(|p| p.is_file())
        .filter_map(|p| Some((p.file_name()?.to_str()?.to_owned(), p)))
        .collect();
    if file.is_empty() {
        return Response::html(200, index(token, &books), head);
    }
    let not_found = || Response::html(404, "<!doctype html><p>Not found</p>".into(), head);
    let Some(name) = decode(file) else {
        return not_found();
    };
    let found = books.iter().find(|(n, _)| *n == name).or_else(|| {
        let stem = name.strip_suffix(&format!(".{ALIAS}"))?;
        books
            .iter()
            .find(|(n, _)| n.strip_suffix(".kfx") == Some(stem))
    });
    let Some((_, path)) = found else {
        return not_found();
    };
    let Ok(meta) = std::fs::metadata(path) else {
        return not_found();
    };
    Response {
        status: 200,
        headers: vec![
            ("Content-Type", content_type(&name).into()),
            ("Content-Length", meta.len().to_string()),
            ("Content-Disposition", disposition(&name)),
        ],
        body: if head {
            Body::Empty
        } else {
            Body::File(path.clone())
        },
    }
}

fn reason(status: u16) -> &'static str {
    match status {
        200 => "OK",
        403 => "Forbidden",
        405 => "Method Not Allowed",
        _ => "Not Found",
    }
}

fn respond(stream: TcpStream, token: &str, queue: &Queue) -> std::io::Result<()> {
    stream.set_read_timeout(Some(Duration::from_secs(10)))?;
    stream.set_write_timeout(Some(Duration::from_secs(60)))?;
    let mut reader = BufReader::new((&stream).take(8 * 1024));
    let mut line = String::new();
    reader.read_line(&mut line)?;
    // Drain the headers so the client isn't reset mid-request; their content is ignored.
    let mut h = String::new();
    while reader.read_line(&mut h)? > 0 && !h.trim_end().is_empty() {
        h.clear();
    }
    let r = handle(line.trim_end(), token, queue);
    let mut out = std::io::BufWriter::new(&stream);
    write!(out, "HTTP/1.1 {} {}\r\n", r.status, reason(r.status))?;
    for (k, v) in &r.headers {
        write!(out, "{k}: {v}\r\n")?;
    }
    write!(out, "Connection: close\r\n\r\n")?;
    match r.body {
        Body::Html(s) => out.write_all(s.as_bytes())?,
        Body::File(p) => {
            std::io::copy(&mut std::fs::File::open(p)?, &mut out)?;
        }
        Body::Empty => {}
    }
    out.flush()
}

/// Serves `queue` on `listener`, one thread per connection, until the process ends.
pub fn serve(listener: TcpListener, queue: Queue, token: String) -> std::io::Result<()> {
    for stream in listener.incoming() {
        let Ok(stream) = stream else { continue };
        let (queue, token) = (queue.clone(), token.clone());
        std::thread::spawn(move || {
            let _ = respond(stream, &token, &queue);
        });
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    const TOKEN: &str = "0123456789abcdef0123456789abcdef";

    fn outbox() -> (tempfile::TempDir, Queue) {
        let tmp = tempfile::tempdir().unwrap();
        let q = Queue::new(tmp.path().join("outbox"));
        std::fs::create_dir_all(&q.dir).unwrap();
        std::fs::write(q.dir.join("deckle-daily-2026-10-20.kfx"), b"KFX-BYTES").unwrap();
        std::fs::write(q.dir.join("my book.kfx"), b"OTHER").unwrap();
        std::fs::create_dir_all(q.dir.join("deckle-daily-2026-10-20.sdr")).unwrap();
        std::fs::write(
            q.dir.join("deckle-daily-2026-10-20.sdr/XRAY.entities.asc"),
            b"SIDECAR",
        )
        .unwrap();
        std::fs::write(q.dir.join("notes.txt"), b"not a book").unwrap();
        (tmp, q)
    }

    fn header<'a>(r: &'a Response, name: &str) -> Option<&'a str> {
        r.headers
            .iter()
            .find(|(k, _)| k.eq_ignore_ascii_case(name))
            .map(|(_, v)| v.as_str())
    }

    fn html(r: &Response) -> &str {
        match &r.body {
            Body::Html(s) => s,
            other => panic!("not html: {other:?}"),
        }
    }

    #[test]
    fn without_the_token_is_403() {
        let (_t, q) = outbox();
        for line in [
            "GET / HTTP/1.1",
            "GET /favicon.ico HTTP/1.1",
            "GET /deckle-daily-2026-10-20.kfx HTTP/1.1",
            "GET /t/ HTTP/1.1",
            "GET /t/ffffffffffffffffffffffffffffffff/ HTTP/1.1",
            "GET /t/0123456789abcdef0123456789abcdef0/ HTTP/1.1",
            "GET /t/0123456789abcdef0123456789abcdeX/deckle-daily-2026-10-20.kfx HTTP/1.1",
            "POST / HTTP/1.1",
            "garbage",
            "",
        ] {
            assert_eq!(handle(line, TOKEN, &q).status, 403, "{line:?}");
        }
    }

    #[test]
    fn other_methods_with_the_token_are_405() {
        let (_t, q) = outbox();
        let r = handle(
            &format!("DELETE /t/{TOKEN}/my%20book.kfx HTTP/1.1"),
            TOKEN,
            &q,
        );
        assert_eq!(r.status, 405);
        assert!(q.dir.join("my book.kfx").exists());
    }

    #[test]
    fn index_lists_the_books_and_says_sidecars_need_usb() {
        let (_t, q) = outbox();
        for path in [format!("/t/{TOKEN}/"), format!("/t/{TOKEN}")] {
            let r = handle(&format!("GET {path} HTTP/1.1"), TOKEN, &q);
            assert_eq!(r.status, 200);
            assert!(header(&r, "content-type").unwrap().starts_with("text/html"));
            let body = html(&r);
            assert!(
                body.contains(&format!("href=\"/t/{TOKEN}/deckle-daily-2026-10-20.kfx\"")),
                "{body}"
            );
            assert!(
                body.contains(&format!("href=\"/t/{TOKEN}/my%20book.kfx\"")),
                "{body}"
            );
            assert!(
                body.contains(&format!("href=\"/t/{TOKEN}/my%20book.azw\"")),
                "{body}"
            );
            assert!(!body.contains(".sdr"), "{body}");
            assert!(!body.contains("notes.txt"), "{body}");
            assert!(body.contains("X-Ray and Word Wise need USB"), "{body}");
            assert_eq!(
                header(&r, "content-length").unwrap(),
                body.len().to_string()
            );
        }
    }

    #[test]
    fn index_of_an_empty_outbox_says_so() {
        let tmp = tempfile::tempdir().unwrap();
        let q = Queue::new(tmp.path().join("missing"));
        let r = handle(&format!("GET /t/{TOKEN}/ HTTP/1.1"), TOKEN, &q);
        assert_eq!(r.status, 200);
        assert!(html(&r).contains("Nothing waiting"), "{}", html(&r));
    }

    #[test]
    fn a_listed_book_is_served_as_an_attachment() {
        let (_t, q) = outbox();
        let r = handle(
            &format!("GET /t/{TOKEN}/deckle-daily-2026-10-20.kfx?x=1 HTTP/1.1"),
            TOKEN,
            &q,
        );
        assert_eq!(r.status, 200);
        assert_eq!(
            r.body,
            Body::File(q.dir.join("deckle-daily-2026-10-20.kfx"))
        );
        assert_eq!(header(&r, "content-length"), Some("9"));
        assert_eq!(header(&r, "content-type"), Some("application/octet-stream"));
        assert_eq!(
            header(&r, "content-disposition"),
            Some("attachment; filename=\"deckle-daily-2026-10-20.kfx\"")
        );

        let r = handle(&format!("GET /t/{TOKEN}/my%20book.kfx HTTP/1.1"), TOKEN, &q);
        assert_eq!(r.body, Body::File(q.dir.join("my book.kfx")));
    }

    #[test]
    fn the_azw_alias_serves_the_same_bytes_under_that_name() {
        let (_t, q) = outbox();
        let r = handle(&format!("GET /t/{TOKEN}/my%20book.azw HTTP/1.1"), TOKEN, &q);
        assert_eq!(r.status, 200);
        assert_eq!(r.body, Body::File(q.dir.join("my book.kfx")));
        assert_eq!(header(&r, "content-length"), Some("5"));
        assert_eq!(
            header(&r, "content-type"),
            Some("application/vnd.amazon.ebook")
        );
        assert_eq!(
            header(&r, "content-disposition"),
            Some("attachment; filename=\"my book.azw\"")
        );
    }

    #[test]
    fn head_sends_headers_only() {
        let (_t, q) = outbox();
        let r = handle(
            &format!("HEAD /t/{TOKEN}/my%20book.kfx HTTP/1.1"),
            TOKEN,
            &q,
        );
        assert_eq!(r.status, 200);
        assert_eq!(r.body, Body::Empty);
        assert_eq!(header(&r, "content-length"), Some("5"));
    }

    #[test]
    fn unknown_names_traversal_and_sidecars_are_404() {
        let (tmp, q) = outbox();
        std::fs::write(tmp.path().join("secret.kfx"), b"SECRET").unwrap();
        for name in [
            "nope.kfx",
            "notes.txt",
            "notes.azw",
            "../secret.kfx",
            "..%2fsecret.kfx",
            "%2e%2e%2fsecret.kfx",
            "%2e%2e/secret.kfx",
            "..\\secret.kfx",
            "deckle-daily-2026-10-20.sdr",
            "deckle-daily-2026-10-20.sdr/XRAY.entities.asc",
            "deckle-daily-2026-10-20.sdr%2fXRAY.entities.asc",
            "./my%20book.kfx",
            "sub/my%20book.kfx",
            "my%20book.kfx/",
            "%ZZ",
        ] {
            let r = handle(&format!("GET /t/{TOKEN}/{name} HTTP/1.1"), TOKEN, &q);
            assert_eq!(r.status, 404, "{name}");
            assert!(!matches!(r.body, Body::File(_)), "{name}");
        }
    }

    #[test]
    fn token_is_created_once_reused_and_rotated_on_request() {
        let dir = tempfile::tempdir().unwrap();
        let a = token(dir.path(), false).unwrap();
        assert_eq!(a.len(), 32);
        assert!(a.chars().all(|c| c.is_ascii_hexdigit()));
        assert_eq!(token(dir.path(), false).unwrap(), a);
        let b = token(dir.path(), true).unwrap();
        assert_ne!(a, b);
        assert_eq!(token(dir.path(), false).unwrap(), b);
    }

    #[test]
    fn url_puts_the_token_in_the_path() {
        let addr: std::net::SocketAddr = "192.168.1.20:47220".parse().unwrap();
        assert_eq!(url(addr, "abc"), "http://192.168.1.20:47220/t/abc/");
        let v6: std::net::SocketAddr = "[fd7a:115c::1]:8080".parse().unwrap();
        assert_eq!(url(v6, "abc"), "http://[fd7a:115c::1]:8080/t/abc/");
    }
}
