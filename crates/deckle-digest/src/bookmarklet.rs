// SPDX-License-Identifier: GPL-3.0-or-later
//! The bookmarklet (save-to-paper design §4): `deckle digest watch` listens on
//! `127.0.0.1:47219`; the bookmarklet opens `/save?token=…&url=…&title=…` in a small popup,
//! which avoids the browser's mixed-content and CORS rules. The token keeps other local pages
//! from adding to the queue blindly.
//!
//! A second bookmarklet, **Save recipe**, adds `kind=recipe`: the handler drops a `.url` link
//! file in the Inbox's `Recipes/` folder and the Inbox scan does the fetching, so the listener
//! never waits on the network. It sends the address only, never the page (cookbook design §8.3).

use std::io::{BufRead, BufReader, Read, Write};
use std::net::TcpListener;
use std::path::{Path, PathBuf};
use std::time::Duration;

use chrono::{DateTime, Utc};

use crate::esc;
use crate::inbox::free_name;
use crate::reading::{Added, ReadingQueue};
use crate::sources::feeds::host;
use crate::sources::reading::slug;

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

/// The popup script; `kind` is an extra query parameter (with its leading `&`), or empty.
fn script(token: &str, kind: &str) -> String {
    format!(
        "javascript:(()=>{{window.open('http://127.0.0.1:{PORT}/save?token={token}{kind}&url='+\
         encodeURIComponent(location.href)+'&title='+encodeURIComponent(document.title),\
         'deckle','width=420,height=200')}})()"
    )
}

/// The bookmarklet to drag to the bookmarks bar.
pub fn javascript(token: &str) -> String {
    script(token, "")
}

/// **Save recipe**: like [`javascript`], for the cookbook (`kind=recipe`).
pub fn recipe_javascript(token: &str) -> String {
    script(token, "&kind=recipe")
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

/// The link file's name: the title as a slug, else the address's host.
fn recipe_slug(url: &str, title: Option<&str>) -> String {
    match title.filter(|t| t.chars().any(char::is_alphanumeric)) {
        Some(t) => slug(t),
        None => slug(host(url)),
    }
}

/// Writes `<recipes_inbox>/<slug>.url` for the Inbox scan to pick up, under a name that is
/// free. The file appears whole (written aside, then renamed), so a scan never reads half.
fn write_recipe_link(recipes_inbox: &Path, url: &str, title: Option<&str>) -> Result<(), String> {
    std::fs::create_dir_all(recipes_inbox).map_err(|e| e.to_string())?;
    let dest = free_name(recipes_inbox, &format!("{}.url", recipe_slug(url, title)));
    let part = recipes_inbox.join(format!(".{}.part", rand::random::<u64>()));
    std::fs::write(&part, format!("[InternetShortcut]\r\nURL={url}\r\n"))
        .and_then(|()| std::fs::rename(&part, &dest))
        .map_err(|e| {
            let _ = std::fs::remove_file(&part);
            e.to_string()
        })
}

/// One request line (`GET /save?… HTTP/1.1`) → status and HTML body. An article is queued
/// for the paper; with `kind=recipe` a link file goes into `recipes_inbox` instead, and
/// nothing is fetched here.
pub fn handle(
    request_line: &str,
    token: &str,
    queue: &ReadingQueue,
    recipes_inbox: &Path,
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
    match param("kind").as_deref() {
        None | Some("") | Some("article") => {}
        Some("recipe") => {
            // A line break would put a second address in the link file.
            if url.chars().any(char::is_control) {
                return (
                    400,
                    page(
                        "Not saved",
                        "That page has no web address Deckle can fetch.",
                    ),
                );
            }
            return match write_recipe_link(recipes_inbox, &url, title.as_deref()) {
                Ok(()) => (
                    200,
                    page("Saved to your cookbook", title.as_deref().unwrap_or(&url)),
                ),
                Err(e) => (500, page("Not saved", &e)),
            };
        }
        Some(_) => {
            return (
                400,
                page(
                    "Not saved",
                    "This bookmarklet is out of date: run `deckle save --bookmarklet` again.",
                ),
            );
        }
    }
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

/// Serves the bookmarklets on `127.0.0.1:PORT`, one request per connection, until the
/// process ends. Errors only when the port can't be bound.
pub fn serve(queue: ReadingQueue, token: String, recipes_inbox: PathBuf) -> std::io::Result<()> {
    let listener = TcpListener::bind(("127.0.0.1", PORT))?;
    for stream in listener.incoming() {
        let Ok(mut stream) = stream else { continue };
        let _ = stream.set_read_timeout(Some(Duration::from_secs(5)));
        let mut line = String::new();
        let mut reader = BufReader::new((&stream).take(8 * 1024));
        if reader.read_line(&mut line).is_err() {
            continue;
        }
        let (status, body) = handle(line.trim_end(), &token, &queue, &recipes_inbox, Utc::now());
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
            dir.path(),
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
            dir.path(),
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
            assert_eq!(handle(line, "abc", &q, dir.path(), now()).0, 403, "{line}");
        }
        assert!(q.load().is_empty());
    }

    #[test]
    fn non_get_or_other_path_is_404() {
        let dir = tempfile::tempdir().unwrap();
        let q = ReadingQueue::new(dir.path().join("reading.jsonl"));
        assert_eq!(
            handle(
                "POST /save?token=abc HTTP/1.1",
                "abc",
                &q,
                dir.path(),
                now()
            )
            .0,
            404
        );
        assert_eq!(
            handle("GET /other HTTP/1.1", "abc", &q, dir.path(), now()).0,
            404
        );
        assert_eq!(handle("garbage", "abc", &q, dir.path(), now()).0, 404);
    }

    #[test]
    fn token_is_created_once_and_reused() {
        let dir = tempfile::tempdir().unwrap();
        let a = token(dir.path()).unwrap();
        assert_eq!(a.len(), 32);
        assert_eq!(token(dir.path()).unwrap(), a);
        assert!(javascript(&a).contains(&format!("127.0.0.1:47219/save?token={a}&url=")));
    }

    fn recipe_files(inbox: &Path) -> Vec<String> {
        let mut names: Vec<String> = std::fs::read_dir(inbox)
            .map(|d| {
                d.flatten()
                    .map(|e| e.file_name().to_string_lossy().into_owned())
                    .collect()
            })
            .unwrap_or_default();
        names.sort();
        names
    }

    #[test]
    fn recipe_kind_writes_url_file() {
        let dir = tempfile::tempdir().unwrap();
        let q = ReadingQueue::new(dir.path().join("reading.jsonl"));
        let inbox = dir.path().join("Inbox/Recipes");
        let line = "GET /save?token=abc&kind=recipe&url=https%3A%2F%2Fex.com%2Fr%3Fx%3D1&title=Mom%27s%20Best%20Pie%20%26%20More HTTP/1.1";
        let (status, body) = handle(line, "abc", &q, &inbox, now());
        assert_eq!(status, 200);
        assert!(body.contains("Saved to your cookbook"), "{body}");
        assert!(body.contains("Mom&#39;s Best Pie &amp; More"), "{body}");
        assert_eq!(recipe_files(&inbox), ["mom-s-best-pie-more.url"]);
        // Never fetched, never queued for the paper.
        assert!(q.load().is_empty());
        // Exactly what the Inbox scan reads back.
        let file = inbox.join("mom-s-best-pie-more.url");
        assert_eq!(
            std::fs::read_to_string(&file).unwrap(),
            "[InternetShortcut]\r\nURL=https://ex.com/r?x=1\r\n"
        );
        assert_eq!(
            crate::inbox::urls_in(&file, "url").unwrap(),
            ["https://ex.com/r?x=1"]
        );
        // The same title again gets its own name.
        handle(line, "abc", &q, &inbox, now());
        assert_eq!(
            recipe_files(&inbox),
            ["mom-s-best-pie-more (2).url", "mom-s-best-pie-more.url"]
        );
    }

    #[test]
    fn recipe_file_is_named_for_the_host_without_a_title() {
        let dir = tempfile::tempdir().unwrap();
        let q = ReadingQueue::new(dir.path().join("reading.jsonl"));
        for line in [
            "GET /save?token=abc&kind=recipe&url=https%3A%2F%2Fwww.ex.com%2Fr HTTP/1.1",
            "GET /save?token=abc&kind=recipe&url=https%3A%2F%2Fwww.ex.com%2Fr&title=%20%21%21 HTTP/1.1",
        ] {
            assert_eq!(handle(line, "abc", &q, dir.path(), now()).0, 200);
        }
        assert_eq!(recipe_files(dir.path()), ["ex-com (2).url", "ex-com.url"]);
    }

    #[test]
    fn recipe_kind_needs_token() {
        let dir = tempfile::tempdir().unwrap();
        let q = ReadingQueue::new(dir.path().join("reading.jsonl"));
        let inbox = dir.path().join("Recipes");
        for line in [
            "GET /save?token=nope&kind=recipe&url=https%3A%2F%2Fex.com HTTP/1.1",
            "GET /save?kind=recipe&url=https%3A%2F%2Fex.com HTTP/1.1",
        ] {
            assert_eq!(handle(line, "abc", &q, &inbox, now()).0, 403, "{line}");
        }
        assert!(!inbox.exists(), "nothing written");
    }

    #[test]
    fn recipe_kind_refuses_what_cannot_be_a_link() {
        let dir = tempfile::tempdir().unwrap();
        let q = ReadingQueue::new(dir.path().join("reading.jsonl"));
        let inbox = dir.path().join("Recipes");
        for line in [
            "GET /save?token=abc&kind=recipe&url=about%3Ablank HTTP/1.1",
            // a line break would add a second address to the link file
            "GET /save?token=abc&kind=recipe&url=https%3A%2F%2Fex.com%2F%0D%0AURL%3Dhttps%3A%2F%2Fevil.test HTTP/1.1",
            "GET /save?token=abc&kind=cake&url=https%3A%2F%2Fex.com HTTP/1.1",
        ] {
            assert_eq!(handle(line, "abc", &q, &inbox, now()).0, 400, "{line}");
        }
        assert!(recipe_files(&inbox).is_empty());
        assert!(q.load().is_empty());
    }

    #[test]
    fn plain_save_unchanged() {
        let dir = tempfile::tempdir().unwrap();
        let q = ReadingQueue::new(dir.path().join("reading.jsonl"));
        let inbox = dir.path().join("Recipes");
        let (status, body) = handle(
            "GET /save?token=abc&url=https%3A%2F%2Fex.com%2Fa HTTP/1.1",
            "abc",
            &q,
            &inbox,
            now(),
        );
        assert_eq!(status, 200);
        assert!(body.contains("Saved to tomorrow&#39;s paper"), "{body}");
        assert_eq!(q.load().len(), 1);
        assert!(
            !inbox.exists(),
            "an article never touches the cookbook's folder"
        );
    }

    #[test]
    fn recipe_javascript_has_kind_recipe() {
        let js = recipe_javascript("abc");
        assert!(js.contains("kind=recipe"), "{js}");
        assert!(js.contains("127.0.0.1:47219/save?token=abc&"), "{js}");
        assert!(js.contains("encodeURIComponent(location.href)"), "{js}");
        assert!(js.contains("encodeURIComponent(document.title)"), "{js}");
        assert!(
            !js.contains("outerHTML") && !js.contains("innerHTML"),
            "the address only, never the page: {js}"
        );
        assert!(
            !javascript("abc").contains("kind="),
            "the paper's is unchanged"
        );
    }
}
