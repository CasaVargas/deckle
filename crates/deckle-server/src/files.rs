// SPDX-License-Identifier: GPL-3.0-or-later
//! The file browser, uploads and downloads. A browser can't hand the server a path of its own,
//! so the web UI picks server folders here and uploads files from the user's computer. Browsing
//! and downloads stay inside the roots, after symlinks are resolved.

use std::cmp::Ordering;
use std::path::{Component, Path, PathBuf};

use axum::Json;
use axum::body::Body;
use axum::extract::{Multipart, Query, Request, State};
use axum::http::header::CONTENT_DISPOSITION;
use axum::http::{HeaderValue, StatusCode};
use axum::response::{IntoResponse, Response};
use serde::{Deserialize, Serialize};
use tokio::io::AsyncWriteExt;
use tower_http::services::ServeFile;

use crate::Shared;
use crate::invoke::error;

/// The most one uploaded file may hold.
pub(crate) const UPLOAD_LIMIT: u64 = 512 * 1024 * 1024;

#[derive(Deserialize)]
pub(crate) struct PathQuery {
    path: Option<String>,
}

#[derive(Serialize)]
struct Root {
    path: PathBuf,
}

#[derive(Serialize)]
struct Listing {
    path: PathBuf,
    parent: Option<PathBuf>,
    entries: Vec<Entry>,
}

#[derive(Serialize)]
struct Entry {
    name: String,
    path: PathBuf,
    dir: bool,
    size: u64,
}

/// The roots as they really are; any that don't exist are left out.
fn canonical_roots(roots: &[PathBuf]) -> Vec<PathBuf> {
    let mut out: Vec<PathBuf> = Vec::new();
    for r in roots {
        if let Ok(c) = r.canonicalize()
            && !out.contains(&c)
        {
            out.push(c);
        }
    }
    out
}

/// `path` resolved (`..` and symlinks gone), if that lies inside one of `roots`. Both sides are
/// canonical, so Windows' `\\?\` forms compare with each other. `path` must exist.
pub(crate) fn confine(roots: &[PathBuf], path: &Path) -> Result<PathBuf, StatusCode> {
    // A relative path would resolve against the server's working folder.
    if !path.is_absolute() {
        return Err(StatusCode::BAD_REQUEST);
    }
    let real = path.canonicalize().map_err(|_| StatusCode::NOT_FOUND)?;
    if canonical_roots(roots).iter().any(|r| real.starts_with(r)) {
        Ok(real)
    } else {
        Err(StatusCode::FORBIDDEN)
    }
}

/// A canonical path as the web UI shows it: without Windows' `\\?\` before a drive letter.
fn shown(p: &Path) -> PathBuf {
    let s = p.to_string_lossy();
    match s.strip_prefix(r"\\?\") {
        Some(rest) if !rest.starts_with(r"UNC\") => PathBuf::from(rest),
        _ => p.to_path_buf(),
    }
}

/// Why a path was turned away, for [`error`].
fn refused(status: StatusCode, path: &str) -> (StatusCode, String) {
    let why = match status {
        StatusCode::BAD_REQUEST => "not an absolute path",
        StatusCode::NOT_FOUND => "no such file or folder",
        _ => "outside the folders the web UI may show",
    };
    (status, format!("{path}: {why}"))
}

/// The query's `path`, confined, off the async workers (it touches the disk).
async fn confined(server: &Shared, q: PathQuery) -> Result<PathBuf, (StatusCode, String)> {
    let Some(path) = q.path.filter(|p| !p.is_empty()) else {
        return Err((StatusCode::BAD_REQUEST, "path is required".into()));
    };
    let roots = server.cfg.roots.clone();
    // Taken literally: the query was percent-decoded once already, and that's all.
    let p = PathBuf::from(&path);
    match tokio::task::spawn_blocking(move || confine(&roots, &p)).await {
        Ok(Ok(real)) => Ok(real),
        Ok(Err(status)) => Err(refused(status, &path)),
        Err(e) => Err((StatusCode::INTERNAL_SERVER_ERROR, e.to_string())),
    }
}

/// `GET /api/fs/roots`: where browsing starts.
pub(crate) async fn roots(State(server): State<Shared>) -> Response {
    let roots = server.cfg.roots.clone();
    match tokio::task::spawn_blocking(move || canonical_roots(&roots)).await {
        Ok(rs) => Json(
            rs.iter()
                .map(|r| Root { path: shown(r) })
                .collect::<Vec<_>>(),
        )
        .into_response(),
        Err(e) => error(StatusCode::INTERNAL_SERVER_ERROR, e.to_string()),
    }
}

/// `GET /api/fs/list?path=`: a folder's entries, folders first. Hidden ones are left out.
pub(crate) async fn list(State(server): State<Shared>, Query(q): Query<PathQuery>) -> Response {
    let dir = match confined(&server, q).await {
        Ok(d) => d,
        Err((status, e)) => return error(status, e),
    };
    let roots = server.cfg.roots.clone();
    let read = move || -> Result<Listing, String> {
        if !dir.is_dir() {
            return Err(format!("{}: not a folder", shown(&dir).display()));
        }
        let mut entries = Vec::new();
        for e in std::fs::read_dir(&dir).map_err(|e| format!("{}: {e}", dir.display()))? {
            let Ok(e) = e else { continue };
            // A name that isn't UTF-8 couldn't be sent back as a path.
            let Ok(name) = e.file_name().into_string() else {
                continue;
            };
            if name.starts_with('.') {
                continue;
            }
            // What a symlink points at; one that points nowhere is left out. Following it is
            // confined again on the next call.
            let Ok(meta) = std::fs::metadata(e.path()) else {
                continue;
            };
            entries.push(Entry {
                path: shown(&dir.join(&name)),
                dir: meta.is_dir(),
                size: if meta.is_dir() { 0 } else { meta.len() },
                name,
            });
        }
        entries.sort_by(|a, b| match (a.dir, b.dir) {
            (true, false) => Ordering::Less,
            (false, true) => Ordering::Greater,
            _ => a
                .name
                .to_lowercase()
                .cmp(&b.name.to_lowercase())
                .then_with(|| a.name.cmp(&b.name)),
        });
        // A root's parent is outside the roots.
        let parent = if canonical_roots(&roots).contains(&dir) {
            None
        } else {
            dir.parent().map(shown)
        };
        Ok(Listing {
            path: shown(&dir),
            parent,
            entries,
        })
    };
    match tokio::task::spawn_blocking(read).await {
        Ok(Ok(l)) => Json(l).into_response(),
        Ok(Err(e)) => error(StatusCode::BAD_REQUEST, e),
        Err(e) => error(StatusCode::INTERNAL_SERVER_ERROR, e.to_string()),
    }
}

/// `GET /api/fs/download?path=`: the file, as an attachment.
pub(crate) async fn download(
    State(server): State<Shared>,
    Query(q): Query<PathQuery>,
    req: Request,
) -> Response {
    let file = match confined(&server, q).await {
        Ok(f) => f,
        Err((status, e)) => return error(status, e),
    };
    if file.is_dir() {
        return error(
            StatusCode::BAD_REQUEST,
            format!("{}: a folder, not a file", shown(&file).display()),
        );
    }
    let name = file
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_default();
    // ServeFile streams it, with a content type from the extension and range requests.
    let mut res = match ServeFile::new(&file).try_call(req).await {
        Ok(r) => r.map(Body::new),
        Err(e) => return error(StatusCode::INTERNAL_SERVER_ERROR, e.to_string()),
    };
    if res.status().is_success()
        && let Ok(v) = HeaderValue::from_str(&attachment(&name))
    {
        res.headers_mut().insert(CONTENT_DISPOSITION, v);
    }
    res
}

/// `attachment; filename="…"`, ASCII with `"`, `\` and anything else unsafe as `_`; a name
/// that isn't plain ASCII also gets RFC 6266's `filename*`.
fn attachment(name: &str) -> String {
    let plain: String = name
        .chars()
        .map(|c| match c {
            ' '..='~' if c != '"' && c != '\\' => c,
            _ => '_',
        })
        .collect();
    let mut v = format!("attachment; filename=\"{plain}\"");
    if plain != name {
        v.push_str("; filename*=UTF-8''");
        for b in name.bytes() {
            if b.is_ascii_alphanumeric() || b"!#$&+-.^_`|~".contains(&b) {
                v.push(b as char);
            } else {
                v.push_str(&format!("%{b:02X}"));
            }
        }
    }
    v
}

/// Where uploads go: `<root>/Deckle/Uploads`, under the first root named `data` (`/data` in the
/// image), else the last root.
fn uploads_dir(roots: &[PathBuf]) -> PathBuf {
    roots
        .iter()
        .find(|r| r.ends_with("data"))
        .or(roots.last())
        .cloned()
        .unwrap_or_else(|| PathBuf::from("/data"))
        .join("Deckle")
        .join("Uploads")
}

/// An uploaded file's name, reduced to one plain component: its last part, without control or
/// Windows-reserved characters, without leading dots (so it can't climb or hide) and without
/// the leading or trailing spaces and dots Windows would drop. Control characters go first so
/// they can't shield a dot; anything still not a single normal component becomes `upload`.
fn upload_name(sent: &str) -> String {
    let last = sent.rsplit(['/', '\\']).next().unwrap_or("");
    let kept: String = last
        .chars()
        .filter(|c| !c.is_control() && !"<>:\"|?*".contains(*c))
        .collect();
    let name = kept
        .trim_start_matches(['.', ' '])
        .trim_end_matches(['.', ' ']);
    let mut parts = Path::new(name).components();
    match (parts.next(), parts.next()) {
        (Some(Component::Normal(_)), None) => name.to_owned(),
        _ => "upload".into(),
    }
}

fn random_id() -> Result<String, getrandom::Error> {
    let mut bytes = [0u8; 16];
    getrandom::fill(&mut bytes)?;
    Ok(bytes.iter().map(|b| format!("{b:02x}")).collect())
}

/// The folders this request made. Unless kept, they go when it ends — on an error, the limit,
/// or the client hanging up mid-upload.
#[derive(Default)]
struct Made {
    dirs: Vec<PathBuf>,
    keep: bool,
}

impl Drop for Made {
    fn drop(&mut self) {
        if !self.keep {
            for d in &self.dirs {
                let _ = std::fs::remove_dir_all(d);
            }
        }
    }
}

fn too_large(name: &str, limit: u64) -> Response {
    const MIB: u64 = 1024 * 1024;
    let limit = if limit >= MIB {
        format!("{} MiB", limit / MIB)
    } else {
        format!("{limit} bytes")
    };
    error(
        StatusCode::PAYLOAD_TOO_LARGE,
        format!("{name}: larger than the {limit} upload limit"),
    )
}

/// `POST /api/fs/upload`: multipart files, each streamed to `<uploads>/<id>/<name>`. Answers
/// the saved paths; on any failure nothing from the request is kept.
pub(crate) async fn upload(State(server): State<Shared>, mut form: Multipart) -> Response {
    let internal =
        |e: &dyn std::fmt::Display| error(StatusCode::INTERNAL_SERVER_ERROR, e.to_string());
    let base = uploads_dir(&server.cfg.roots);
    let mut made = Made::default();
    let mut saved = Vec::new();
    loop {
        let mut field = match form.next_field().await {
            Ok(Some(f)) => f,
            Ok(None) => break,
            Err(e) => return error(e.status(), e.body_text()),
        };
        let Some(name) = field.file_name().map(upload_name) else {
            return error(StatusCode::BAD_REQUEST, "only files may be uploaded".into());
        };
        let id = match random_id() {
            Ok(id) => id,
            Err(e) => return internal(&e),
        };
        let dir = base.join(id);
        if let Err(e) = tokio::fs::create_dir_all(&dir).await {
            return internal(&format_args!("{}: {e}", dir.display()));
        }
        made.dirs.push(dir.clone());
        // Resolved and confined like a browsed path: a symlinked `Deckle` or `Uploads` mustn't
        // send the file outside the roots. Written through the resolved path from here on.
        let roots = server.cfg.roots.clone();
        let check = dir.clone();
        let dir = match tokio::task::spawn_blocking(move || confine(&roots, &check)).await {
            Ok(Ok(real)) => real,
            Ok(Err(_)) => {
                return error(
                    StatusCode::FORBIDDEN,
                    format!(
                        "{}: the uploads folder is outside the folders the web UI may use",
                        base.display()
                    ),
                );
            }
            Err(e) => return internal(&e),
        };
        // A temp name until it's whole, so nothing half-written looks finished.
        let part = dir.join(format!("{name}.part"));
        let mut out = match tokio::fs::File::create(&part).await {
            Ok(f) => f,
            Err(e) => return internal(&format_args!("{}: {e}", part.display())),
        };
        let mut size = 0u64;
        loop {
            match field.chunk().await {
                Ok(Some(chunk)) => {
                    size += chunk.len() as u64;
                    if size > server.upload_limit {
                        return too_large(&name, server.upload_limit);
                    }
                    if let Err(e) = out.write_all(&chunk).await {
                        return internal(&format_args!("{}: {e}", part.display()));
                    }
                }
                Ok(None) => break,
                Err(e) => return error(e.status(), e.body_text()),
            }
        }
        if let Err(e) = out.flush().await {
            return internal(&format_args!("{}: {e}", part.display()));
        }
        // Closed before the rename: Windows won't rename an open file.
        drop(out);
        let file = dir.join(&name);
        if let Err(e) = tokio::fs::rename(&part, &file).await {
            return internal(&format_args!("{}: {e}", file.display()));
        }
        // Canonical, like the browser's paths, so the two compare.
        let real = tokio::fs::canonicalize(&file).await.unwrap_or(file);
        saved.push(shown(&real));
    }
    if saved.is_empty() {
        return error(StatusCode::BAD_REQUEST, "no files sent".into());
    }
    made.keep = true;
    Json(saved).into_response()
}
