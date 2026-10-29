// SPDX-License-Identifier: GPL-3.0-or-later
//! The bookmarklet (save-to-paper design §4): `deckle digest watch` listens on
//! `127.0.0.1:47219`; the bookmarklet opens `/save?token=…&url=…&title=…` in a small popup,
//! which avoids the browser's mixed-content and CORS rules. The token keeps other local pages
//! from adding to the queue blindly.
//!
//! A second bookmarklet, **Save recipe**, carries the recipe from the page you're looking at
//! (cookbook design §8.3, amended): the page's recipe data travels in the fragment of a popup
//! at `/recipe`, whose own script POSTs it back to this listener, and the listener writes a
//! saved page into the Inbox's `Recipes/` folder. So a site that refuses apps (bot protection)
//! still works, and the listener never waits on the network. A page with no recipe data falls
//! back to `/save?…&kind=recipe`, a `.url` link file the Inbox scan fetches.

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

/// What the bookmarklets and the origin check use when `DECKLE_BOOKMARKLET_URL` is unset.
fn loopback_origin() -> String {
    format!("http://127.0.0.1:{PORT}")
}

/// The address the listener binds: `DECKLE_BOOKMARKLET_BIND`, else loopback only. A container
/// sets `0.0.0.0`; nothing else ever exposes the listener beyond this machine.
pub fn bind_address() -> String {
    bind_from(std::env::var("DECKLE_BOOKMARKLET_BIND").ok().as_deref())
}

fn bind_from(var: Option<&str>) -> String {
    match var.map(str::trim) {
        Some(b) if !b.is_empty() => b.to_owned(),
        _ => "127.0.0.1".to_owned(),
    }
}

/// The public origin the bookmarklets talk to: `DECKLE_BOOKMARKLET_URL` (e.g.
/// `https://deckle.home.lan`) without a trailing `/`, else `http://127.0.0.1:47219`.
pub fn origin() -> String {
    origin_from(std::env::var("DECKLE_BOOKMARKLET_URL").ok().as_deref())
}

fn origin_from(var: Option<&str>) -> String {
    match var.map(|u| u.trim().trim_end_matches('/')) {
        Some(u) if !u.is_empty() => u.to_owned(),
        _ => loopback_origin(),
    }
}

/// The origins the `/recipe` POST accepts: the public one, and loopback so setting the URL
/// doesn't lock out local use. The token check is separate and always applies.
fn accepted_origins(public: &str) -> Vec<String> {
    let mut v = vec![public.to_owned()];
    if public != loopback_origin() {
        v.push(loopback_origin());
    }
    v
}

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
fn script(origin: &str, token: &str, kind: &str) -> String {
    format!(
        "javascript:(()=>{{window.open('{origin}/save?token={token}{kind}&url='+\
         encodeURIComponent(location.href)+'&title='+encodeURIComponent(document.title),\
         'deckle','width=420,height=200')}})()"
    )
}

/// The bookmarklet to drag to the bookmarks bar.
pub fn javascript(token: &str) -> String {
    script(&origin(), token, "")
}

/// **Save recipe**: the page's `ld+json` blocks that mention `Recipe`, with its address, title,
/// canonical, `og:image` and `og:site_name`, as JSON in the fragment of a `/recipe` popup.
/// Nothing else leaves the page. With no such block, the address alone (`kind=recipe`).
pub fn recipe_javascript(token: &str) -> String {
    recipe_script(&origin(), token)
}

fn recipe_script(origin: &str, token: &str) -> String {
    format!(
        "javascript:(()=>{{const q=s=>document.querySelector(s),u=encodeURIComponent,\
         o='{origin}',b=[...document.querySelectorAll('script[type=\"application/ld+json\"]')]\
         .map(s=>s.textContent).filter(t=>t.includes('Recipe'));\
         window.open(b.length?o+'/recipe?token={token}\\x23'+u(JSON.stringify({{url:location.href,\
         title:document.title,canonical:(q('link[rel=canonical]')||{{}}).href||'',\
         image:(q('meta[property=\"og:image\"]')||{{}}).content||'',\
         site:(q('meta[property=\"og:site_name\"]')||{{}}).content||'',blocks:b}})):\
         o+'/save?token={token}&kind=recipe&url='+u(location.href)+'&title='+u(document.title),\
         'deckle','width=420,height=200')}})()"
    )
}

/// The most a carried recipe may weigh, and the most blocks it may bring.
const MAX_CARRIED: usize = 4 * 1024 * 1024;
const MAX_BLOCKS: usize = 20;

/// What **Save recipe** carries from the page (all but `url` may be empty).
#[derive(serde::Deserialize)]
struct Carried {
    url: String,
    #[serde(default)]
    title: String,
    #[serde(default)]
    canonical: String,
    #[serde(default)]
    image: String,
    #[serde(default)]
    site: String,
    #[serde(default)]
    blocks: Vec<String>,
}

/// An address Deckle can keep: `http(s)`, no control characters (a line break would smuggle a
/// second address into a link file).
fn web_address(u: &str) -> bool {
    (u.starts_with("http://") || u.starts_with("https://")) && !u.chars().any(char::is_control)
}

/// The `/recipe` popup: reads the recipe from its own fragment, clears it, POSTs it here and
/// shows the answer. `token` is 32 hex characters, so it needs no escaping.
fn recipe_popup(token: &str) -> String {
    format!(
        "<!doctype html><html><head><meta charset=\"utf-8\"><title>Deckle</title>\
         <style>body{{font:16px Georgia,serif;margin:24px;color:#1f1b16;background:#f6f3ee}}\
         h1{{font-size:20px;margin:0 0 8px}}p{{margin:0 0 16px;color:#7a7169}}\
         button{{font:inherit;padding:4px 14px}}</style></head><body>\
         <h1 id=\"h\">Saving…</h1><p id=\"d\"></p><button onclick=\"window.close()\">Close</button>\
         <script>const show=(h,d,ok)=>{{document.getElementById('h').textContent=h;\
         document.getElementById('d').textContent=d;if(ok)setTimeout(()=>window.close(),2500)}};\
         const f=location.hash.slice(1);history.replaceState(null,'',location.pathname+location.search);\
         if(!f)show('Not saved','The page sent nothing: run `deckle save --bookmarklet` again.');\
         else fetch('/recipe?token={token}',{{method:'POST',headers:{{'Content-Type':'application/json'}},\
         body:decodeURIComponent(f)}}).then(r=>r.json().then(m=>show(m.heading,m.detail,r.ok)))\
         .catch(()=>show('Not saved','Deckle didn\\'t answer: is `deckle digest watch` running?'))\
         </script></body></html>"
    )
}

/// `{{"heading", "detail"}}` for the popup.
fn answer(heading: &str, detail: &str) -> String {
    serde_json::json!({ "heading": heading, "detail": detail }).to_string()
}

/// The saved page a carried recipe becomes: the blocks re-serialized with every `<` escaped
/// (nothing can close the script), the rest escaped, and `deckle:source` naming the address.
fn carried_page(c: &Carried, blocks: &[serde_json::Value]) -> String {
    let mut head = format!(
        "<meta charset=\"utf-8\">\n<meta name=\"deckle:source\" content=\"{}\">\n<title>{}</title>\n",
        esc(&c.url),
        esc(c.title.trim())
    );
    let canonical = if web_address(&c.canonical) {
        &c.canonical
    } else {
        &c.url
    };
    head.push_str(&format!(
        "<link rel=\"canonical\" href=\"{}\">\n",
        esc(canonical)
    ));
    if !c.image.trim().is_empty() {
        head.push_str(&format!(
            "<meta property=\"og:image\" content=\"{}\">\n",
            esc(c.image.trim())
        ));
    }
    if !c.site.trim().is_empty() {
        head.push_str(&format!(
            "<meta property=\"og:site_name\" content=\"{}\">\n",
            esc(c.site.trim())
        ));
    }
    for b in blocks {
        head.push_str(&format!(
            "<script type=\"application/ld+json\">{}</script>\n",
            b.to_string().replace('<', "\\u003c")
        ));
    }
    format!("<!doctype html>\n<html><head>\n{head}</head><body></body></html>\n")
}

/// Writes `name` into `recipes_inbox` whole (written aside, then renamed), under a free name.
fn write_whole(recipes_inbox: &Path, name: &str, body: &str) -> Result<(), String> {
    std::fs::create_dir_all(recipes_inbox).map_err(|e| e.to_string())?;
    let dest = free_name(recipes_inbox, name);
    let part = recipes_inbox.join(format!(".{}.part", rand::random::<u64>()));
    std::fs::write(&part, body)
        .and_then(|()| std::fs::rename(&part, &dest))
        .map_err(|e| {
            let _ = std::fs::remove_file(&part);
            e.to_string()
        })
}

/// One `/recipe` request → status, content type and body. `GET` is the popup (token checked);
/// `POST` carries the recipe as JSON, from the listener's own origin only, and becomes
/// `<slug>.html` in `recipes_inbox` for the Inbox scan. Nothing is fetched here.
pub fn handle_recipe(
    method: &str,
    query: &str,
    origin: Option<&str>,
    body: &[u8],
    token: &str,
    recipes_inbox: &Path,
) -> (u16, &'static str, String) {
    handle_recipe_for(
        method,
        query,
        origin,
        body,
        token,
        recipes_inbox,
        &accepted_origins(&loopback_origin()),
    )
}

/// [`handle_recipe`] with the origins a `POST` may come from.
pub fn handle_recipe_for(
    method: &str,
    query: &str,
    origin: Option<&str>,
    body: &[u8],
    token: &str,
    recipes_inbox: &Path,
    accepted: &[String],
) -> (u16, &'static str, String) {
    const HTML: &str = "text/html; charset=utf-8";
    const JSON: &str = "application/json";
    let ok_token = query
        .split('&')
        .find_map(|kv| kv.strip_prefix("token="))
        .map(decode)
        .as_deref()
        == Some(token);
    let stale = "This bookmarklet is out of date: run `deckle save --bookmarklet` again.";
    match method {
        "GET" if ok_token => (200, HTML, recipe_popup(token)),
        "GET" => (403, HTML, page("Not saved", stale)),
        "POST" => {
            if !ok_token {
                return (403, JSON, answer("Not saved", stale));
            }
            if !origin.is_some_and(|o| accepted.iter().any(|a| a == o)) {
                return (
                    403,
                    JSON,
                    answer("Not saved", "Only Deckle's own page can save here."),
                );
            }
            let unreadable = || {
                (
                    400,
                    JSON,
                    answer("Not saved", "Deckle couldn't read what the page sent."),
                )
            };
            if body.len() > MAX_CARRIED {
                return (
                    413,
                    JSON,
                    answer("Not saved", "That page's recipe data is too big to carry."),
                );
            }
            let Ok(c) = serde_json::from_slice::<Carried>(body) else {
                return unreadable();
            };
            if !web_address(&c.url) {
                return (
                    400,
                    JSON,
                    answer("Not saved", "That page has no web address Deckle can keep."),
                );
            }
            let blocks: Vec<serde_json::Value> = c
                .blocks
                .iter()
                .take(MAX_BLOCKS)
                .filter_map(|b| serde_json::from_str(b).ok())
                .collect();
            if blocks.is_empty() {
                return unreadable();
            }
            let name = format!("{}.html", recipe_slug(&c.url, Some(c.title.as_str())));
            match write_whole(recipes_inbox, &name, &carried_page(&c, &blocks)) {
                Ok(()) => (
                    200,
                    JSON,
                    answer("Sent to your cookbook", "It appears after the next scan."),
                ),
                Err(e) => (500, JSON, answer("Not saved", &e)),
            }
        }
        _ => (404, HTML, page("Not found", "")),
    }
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
    write_whole(
        recipes_inbox,
        &format!("{}.url", recipe_slug(url, title)),
        &format!("[InternetShortcut]\r\nURL={url}\r\n"),
    )
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
                    page("Sent to your cookbook", "It appears after the next scan."),
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

/// One request as the listener reads it: the method, the target, the `Origin` header and the
/// body (`Content-Length` bytes, at most [`MAX_CARRIED`] + 1, so an oversize body is noticed
/// without reading it all).
struct Request {
    line: String,
    origin: Option<String>,
    body: Vec<u8>,
}

fn read_request(stream: &std::net::TcpStream) -> Option<Request> {
    let mut reader = BufReader::new(stream.take(MAX_CARRIED as u64 + 64 * 1024));
    let mut line = String::new();
    reader.read_line(&mut line).ok()?;
    let mut origin = None;
    let mut length = 0usize;
    loop {
        let mut h = String::new();
        if reader.read_line(&mut h).ok()? == 0 || h.trim_end().is_empty() {
            break;
        }
        if let Some((name, value)) = h.split_once(':') {
            match name.trim().to_ascii_lowercase().as_str() {
                "origin" => origin = Some(value.trim().to_owned()),
                "content-length" => length = value.trim().parse().unwrap_or(0),
                _ => {}
            }
        }
    }
    let mut body = Vec::new();
    let _ = (&mut reader)
        .take(length.min(MAX_CARRIED + 1) as u64)
        .read_to_end(&mut body);
    Some(Request {
        line: line.trim_end().to_owned(),
        origin,
        body,
    })
}

/// Serves the bookmarklets on `bind_address():PORT`, one request per connection, until the
/// process ends. Errors only when the port can't be bound.
pub fn serve(queue: ReadingQueue, token: String, recipes_inbox: PathBuf) -> std::io::Result<()> {
    let listener = TcpListener::bind((bind_address(), PORT))?;
    let accepted = accepted_origins(&origin());
    for stream in listener.incoming() {
        let Ok(mut stream) = stream else { continue };
        let _ = stream.set_read_timeout(Some(Duration::from_secs(5)));
        let Some(req) = read_request(&stream) else {
            continue;
        };
        let mut parts = req.line.split_whitespace();
        let (method, target) = (parts.next().unwrap_or(""), parts.next().unwrap_or(""));
        let (path, query) = target.split_once('?').unwrap_or((target, ""));
        let (status, kind, body) = if path == "/recipe" {
            handle_recipe_for(
                method,
                query,
                req.origin.as_deref(),
                &req.body,
                &token,
                &recipes_inbox,
                &accepted,
            )
        } else {
            let (status, body) = handle(&req.line, &token, &queue, &recipes_inbox, Utc::now());
            (status, "text/html; charset=utf-8", body)
        };
        let reason = match status {
            200 => "OK",
            400 => "Bad Request",
            403 => "Forbidden",
            413 => "Payload Too Large",
            500 => "Internal Server Error",
            _ => "Not Found",
        };
        let _ = write!(
            stream,
            "HTTP/1.1 {status} {reason}\r\nContent-Type: {kind}\r\n\
             Content-Length: {}\r\nCache-Control: no-store\r\nConnection: close\r\n\r\n{body}",
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
        assert!(body.contains("Sent to your cookbook"), "{body}");
        assert!(body.contains("It appears after the next scan."), "{body}");
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
    fn recipe_javascript_carries_the_recipe_or_falls_back_to_the_address() {
        let js = recipe_javascript("abc");
        assert!(js.starts_with("javascript:"), "{js}");
        assert!(js.contains("127.0.0.1:47219"), "{js}");
        assert!(
            js.contains(r"/recipe?token=abc\x23'+u("),
            "the data rides in the fragment: {js}"
        );
        // A `javascript:` bookmark is a URL: a literal `#` would end it, a `%` would be decoded.
        assert!(!js.contains('#') && !js.contains('%'), "{js}");
        assert!(js.contains(r#"script[type="application/ld+json"]"#), "{js}");
        assert!(
            js.contains("t.includes('Recipe')"),
            "only blocks that mention a recipe: {js}"
        );
        assert!(
            js.contains("/save?token=abc&kind=recipe&url="),
            "the fallback: {js}"
        );
        for never in [
            "innerHTML",
            "outerHTML",
            "innerText",
            "cookie",
            "localStorage",
            "fetch(",
        ] {
            assert!(!js.contains(never), "{never}: {js}");
        }
        assert!(!js.contains('\n'), "one line, so it pastes as a bookmark");
        assert!(
            !javascript("abc").contains("kind="),
            "the paper's is unchanged"
        );
    }

    const ORIGIN: Option<&str> = Some("http://127.0.0.1:47219");

    #[test]
    fn javascript_uses_public_url() {
        let o = origin_from(Some("https://deckle.home.lan/"));
        assert_eq!(o, "https://deckle.home.lan", "trailing slash stripped");
        assert!(script(&o, "abc", "").contains("https://deckle.home.lan/save?token=abc&url="));
        assert!(recipe_script(&o, "abc").contains("o='https://deckle.home.lan',"));
    }

    #[test]
    fn unset_or_blank_env_keeps_loopback() {
        assert_eq!(origin_from(None), "http://127.0.0.1:47219");
        assert_eq!(origin_from(Some("  ")), "http://127.0.0.1:47219");
        assert_eq!(bind_from(None), "127.0.0.1");
        assert_eq!(bind_from(Some("")), "127.0.0.1");
        assert_eq!(bind_from(Some("0.0.0.0")), "0.0.0.0");
    }

    #[test]
    fn origin_check_accepts_public_url_and_loopback_only() {
        let dir = tempfile::tempdir().unwrap();
        let accepted = accepted_origins(&origin_from(Some("https://deckle.home.lan/")));
        let body = carried("https://ex.test/pie", &["{\"@type\":\"Recipe\"}"]);
        let post = |o: Option<&str>, token: &str| {
            handle_recipe_for(
                "POST",
                &format!("token={token}"),
                o,
                &body,
                "abc",
                dir.path(),
                &accepted,
            )
            .0
        };
        assert_eq!(post(Some("https://deckle.home.lan"), "abc"), 200);
        assert_eq!(post(ORIGIN, "abc"), 200, "loopback still works");
        assert_eq!(post(Some("https://evil.test"), "abc"), 403);
        assert_eq!(post(None, "abc"), 403);
        assert_eq!(
            post(Some("https://deckle.home.lan"), "nope"),
            403,
            "token still required"
        );
    }

    fn carried(url: &str, blocks: &[&str]) -> Vec<u8> {
        serde_json::json!({"url": url, "title": "Pie & <b>Co</b>", "canonical": "javascript:alert(1)",
            "image": "https://img.test/p.jpg\"><x", "site": "Ex", "blocks": blocks})
        .to_string()
        .into_bytes()
    }

    const PIE: &str =
        r#"{"@type":"Recipe","name":"Pie","description":"</script><script>alert(1)</script>"}"#;

    #[test]
    fn recipe_popup_needs_the_token() {
        let dir = tempfile::tempdir().unwrap();
        let (status, kind, body) = handle_recipe("GET", "token=abc", None, b"", "abc", dir.path());
        assert_eq!((status, kind), (200, "text/html; charset=utf-8"));
        assert!(body.contains("fetch('/recipe?token=abc'"), "{body}");
        assert!(body.contains("location.hash"), "{body}");
        assert_eq!(
            handle_recipe("GET", "token=nope", None, b"", "abc", dir.path()).0,
            403
        );
        assert_eq!(
            handle_recipe("PUT", "token=abc", ORIGIN, b"", "abc", dir.path()).0,
            404
        );
    }

    #[test]
    fn a_carried_recipe_becomes_a_saved_page() {
        let dir = tempfile::tempdir().unwrap();
        let inbox = dir.path().join("Recipes");
        let body = carried("https://ex.com/pie", &["not json", PIE]);
        let (status, kind, answer) =
            handle_recipe("POST", "token=abc", ORIGIN, &body, "abc", &inbox);
        assert_eq!((status, kind), (200, "application/json"), "{answer}");
        assert!(answer.contains("Sent to your cookbook"), "{answer}");
        assert_eq!(recipe_files(&inbox), ["pie-b-co-b.html"]);
        let html = std::fs::read_to_string(inbox.join("pie-b-co-b.html")).unwrap();
        assert!(
            html.contains(r#"<meta name="deckle:source" content="https://ex.com/pie">"#),
            "{html}"
        );
        // A canonical that isn't a web address is replaced by the page's own.
        assert!(
            html.contains(r#"<link rel="canonical" href="https://ex.com/pie">"#),
            "{html}"
        );
        assert!(
            html.contains("<title>Pie &amp; &lt;b&gt;Co&lt;/b&gt;</title>"),
            "{html}"
        );
        assert!(
            html.contains(r#"content="https://img.test/p.jpg&quot;&gt;&lt;x""#),
            "{html}"
        );
        // The block is re-serialized, every `<` escaped; the one that wasn't JSON is dropped.
        assert_eq!(html.matches("<script").count(), 1, "{html}");
        assert!(
            html.contains(r#"\u003c/script>\u003cscript>alert(1)\u003c/script>"#),
            "{html}"
        );
        assert!(!html.contains("not json"), "{html}");
    }

    #[test]
    fn a_carried_recipe_is_refused_unless_it_is_whole_and_from_deckle() {
        let dir = tempfile::tempdir().unwrap();
        let inbox = dir.path().join("Recipes");
        let good = carried("https://ex.com/pie", &[PIE]);
        let cases: [(&str, Option<&str>, Vec<u8>, u16); 8] = [
            ("token=nope", ORIGIN, good.clone(), 403),
            ("", ORIGIN, good.clone(), 403),
            ("token=abc", None, good.clone(), 403),
            ("token=abc", Some("https://evil.test"), good.clone(), 403),
            ("token=abc", ORIGIN, b"{not json".to_vec(), 400),
            ("token=abc", ORIGIN, carried("about:blank", &[PIE]), 400),
            (
                "token=abc",
                ORIGIN,
                carried("https://ex.com/\r\nURL=https://evil.test", &[PIE]),
                400,
            ),
            (
                "token=abc",
                ORIGIN,
                carried("https://ex.com/pie", &["nope"]),
                400,
            ),
        ];
        for (query, origin, body, want) in cases {
            let (status, _, answer) = handle_recipe("POST", query, origin, &body, "abc", &inbox);
            assert_eq!(status, want, "{query} {origin:?}: {answer}");
        }
        let big = vec![b' '; MAX_CARRIED + 1];
        assert_eq!(
            handle_recipe("POST", "token=abc", ORIGIN, &big, "abc", &inbox).0,
            413
        );
        assert!(recipe_files(&inbox).is_empty(), "nothing written");
    }
}
