// SPDX-License-Identifier: GPL-3.0-or-later
//! The file browser, uploads and downloads, confined to the roots.

use std::path::{Path, PathBuf};

use axum::Router;
use axum::body::Body;
use axum::http::{Request, StatusCode};
use serde_json::Value;

use crate::{Reply, config, ctx, login, send};

/// Roots `<tmp>/config` and `<tmp>/data` (the real ones can't be made in a test), canonical so
/// the answers compare: macOS's temp dir is behind a symlink.
fn roots(tmp: &Path) -> (PathBuf, PathBuf) {
    let config = tmp.join("config");
    let data = tmp.join("data");
    std::fs::create_dir_all(&config).unwrap();
    std::fs::create_dir_all(&data).unwrap();
    (shown(&config), shown(&data))
}

/// A path as the server shows it: canonical, without Windows' `\\?\`.
fn shown(p: &Path) -> PathBuf {
    let c = p.canonicalize().unwrap().to_string_lossy().into_owned();
    PathBuf::from(c.strip_prefix(r"\\?\").unwrap_or(&c))
}

fn files_app(tmp: &Path, limit: Option<u64>) -> Router {
    let mut cfg = config(tmp, false);
    cfg.roots = vec![tmp.join("config"), tmp.join("data")];
    match limit {
        Some(l) => deckle_server::router_with_upload_limit(cfg, ctx(tmp), l),
        None => deckle_server::router(cfg, ctx(tmp)),
    }
}

async fn get(app: &Router, uri: &str, cookie: &str) -> Reply {
    let req = Request::get(uri)
        .header("cookie", cookie)
        .body(Body::empty())
        .unwrap();
    send(app, req).await
}

fn q(p: &Path) -> String {
    let mut s = String::new();
    for b in p.to_string_lossy().bytes() {
        if b.is_ascii_alphanumeric() || b"-._~".contains(&b) {
            s.push(b as char);
        } else {
            s.push_str(&format!("%{b:02X}"));
        }
    }
    s
}

const BOUNDARY: &str = "deckleboundary";

/// A multipart upload of `(file name, contents)`.
fn upload(files: &[(&str, &[u8])], cookie: &str) -> Request<Body> {
    let mut body = Vec::new();
    for (name, data) in files {
        body.extend_from_slice(
            format!(
                "--{BOUNDARY}\r\nContent-Disposition: form-data; name=\"file\"; \
                 filename=\"{name}\"\r\nContent-Type: application/octet-stream\r\n\r\n"
            )
            .as_bytes(),
        );
        body.extend_from_slice(data);
        body.extend_from_slice(b"\r\n");
    }
    body.extend_from_slice(format!("--{BOUNDARY}--\r\n").as_bytes());
    Request::post("/api/fs/upload")
        .header("cookie", cookie)
        .header("x-deckle", "1")
        .header(
            "content-type",
            format!("multipart/form-data; boundary={BOUNDARY}"),
        )
        .body(Body::from(body))
        .unwrap()
}

/// Every file under `dir`, recursively.
fn tree(dir: &Path) -> Vec<PathBuf> {
    let mut out = Vec::new();
    for e in std::fs::read_dir(dir).unwrap() {
        let p = e.unwrap().path();
        out.push(p.clone());
        if p.is_dir() {
            out.extend(tree(&p));
        }
    }
    out
}

#[tokio::test]
async fn fs_routes_need_a_session_and_csrf() {
    let tmp = tempfile::tempdir().unwrap();
    roots(tmp.path());
    let app = files_app(tmp.path(), None);
    let r = send(
        &app,
        Request::get("/api/fs/roots").body(Body::empty()).unwrap(),
    )
    .await;
    assert_eq!(r.status, StatusCode::UNAUTHORIZED);
    let cookie = login(&app).await;
    let mut req = upload(&[("a.epub", b"x")], &cookie);
    req.headers_mut().remove("x-deckle");
    assert_eq!(send(&app, req).await.status, StatusCode::FORBIDDEN);
}

#[tokio::test]
async fn fs_lists_root_dirs_first() {
    let tmp = tempfile::tempdir().unwrap();
    let (config, data) = roots(tmp.path());
    std::fs::write(data.join("a.epub"), b"12345").unwrap();
    std::fs::create_dir(data.join("zines")).unwrap();
    std::fs::create_dir(data.join("Books")).unwrap();
    std::fs::write(data.join(".hidden"), b"").unwrap();
    let app = files_app(tmp.path(), None);
    let cookie = login(&app).await;

    let r = get(&app, "/api/fs/roots", &cookie).await;
    assert_eq!(r.status, StatusCode::OK, "{}", r.body);
    let v: Value = serde_json::from_str(&r.body).unwrap();
    let got: Vec<&str> = v
        .as_array()
        .unwrap()
        .iter()
        .map(|r| r["path"].as_str().unwrap())
        .collect();
    assert_eq!(got, [config.to_str().unwrap(), data.to_str().unwrap()]);

    let r = get(&app, &format!("/api/fs/list?path={}", q(&data)), &cookie).await;
    assert_eq!(r.status, StatusCode::OK, "{}", r.body);
    let v: Value = serde_json::from_str(&r.body).unwrap();
    assert_eq!(v["path"], data.to_str().unwrap());
    assert!(v["parent"].is_null(), "a root has no parent: {v}");
    let names: Vec<&str> = v["entries"]
        .as_array()
        .unwrap()
        .iter()
        .map(|e| e["name"].as_str().unwrap())
        .collect();
    assert_eq!(names, ["Books", "zines", "a.epub"]);
    let file = &v["entries"][2];
    assert_eq!(file["dir"], false);
    assert_eq!(file["size"], 5);
    assert_eq!(file["path"], data.join("a.epub").to_str().unwrap());
    assert_eq!(v["entries"][0]["dir"], true);

    // A folder below a root has the root as its parent.
    let r = get(
        &app,
        &format!("/api/fs/list?path={}", q(&data.join("Books"))),
        &cookie,
    )
    .await;
    let v: Value = serde_json::from_str(&r.body).unwrap();
    assert_eq!(v["parent"], data.to_str().unwrap());

    // No path is a bad request: the roots are where browsing starts.
    for uri in ["/api/fs/list", "/api/fs/list?path="] {
        assert_eq!(
            get(&app, uri, &cookie).await.status,
            StatusCode::BAD_REQUEST
        );
    }
}

#[tokio::test]
async fn fs_refuses_paths_outside_roots_including_symlinks() {
    let tmp = tempfile::tempdir().unwrap();
    let (_, data) = roots(tmp.path());
    let outside = tmp.path().join("outside");
    std::fs::create_dir(&outside).unwrap();
    std::fs::write(outside.join("secret.txt"), b"the-secret-contents").unwrap();
    let app = files_app(tmp.path(), None);
    let cookie = login(&app).await;

    let dotdot = data.join("..").join("outside");
    let refused = |r: &Reply| {
        assert!(
            [
                StatusCode::FORBIDDEN,
                StatusCode::BAD_REQUEST,
                StatusCode::NOT_FOUND
            ]
            .contains(&r.status),
            "{} {}",
            r.status,
            r.body
        );
        assert!(!r.body.contains("the-secret-contents"), "{}", r.body);
    };
    let mut cases = vec![
        q(&dotdot),
        q(&outside),
        q(&data.join("..").join("outside").join("secret.txt")),
        // `..` sent encoded: decoded once, then resolved like any other.
        q(&data).replace("data", "data%2F%2e%2e%2Foutside"),
        // Encoded twice: a folder literally named `%2e%2e`, which isn't there.
        format!("{}%2F%252e%252e%2Foutside", q(&data)),
        "outside".into(),
        "..".into(),
    ];
    if cfg!(unix) {
        cases.push("%2Fetc".into());
    } else {
        cases.push("C%3A%5CWindows".into());
    }
    for path in &cases {
        for route in ["list", "download"] {
            let r = get(&app, &format!("/api/fs/{route}?path={path}"), &cookie).await;
            refused(&r);
        }
    }

    // Symlinks only on unix: Windows needs a privilege to make one, which CI doesn't have.
    #[cfg(unix)]
    {
        std::os::unix::fs::symlink(&outside, data.join("link")).unwrap();
        std::os::unix::fs::symlink(outside.join("secret.txt"), data.join("secret.txt")).unwrap();
        // Listed, marked by what they point at…
        let r = get(&app, &format!("/api/fs/list?path={}", q(&data)), &cookie).await;
        let v: Value = serde_json::from_str(&r.body).unwrap();
        assert_eq!(v["entries"][0]["name"], "link");
        assert_eq!(v["entries"][0]["dir"], true);
        // …but not followed out of the root.
        for p in [data.join("link"), data.join("link/secret.txt")] {
            refused(&get(&app, &format!("/api/fs/list?path={}", q(&p)), &cookie).await);
        }
        for p in [data.join("secret.txt"), data.join("link/secret.txt")] {
            refused(&get(&app, &format!("/api/fs/download?path={}", q(&p)), &cookie).await);
        }
    }
}

#[tokio::test]
async fn upload_saves_under_uploads_and_returns_path() {
    let tmp = tempfile::tempdir().unwrap();
    let (_, data) = roots(tmp.path());
    let app = files_app(tmp.path(), None);
    let cookie = login(&app).await;
    let r = send(
        &app,
        upload(&[("Dune.epub", b"spice"), ("Émile.pdf", b"pdf")], &cookie),
    )
    .await;
    assert_eq!(r.status, StatusCode::OK, "{}", r.body);
    let v: Vec<String> = serde_json::from_str(&r.body).unwrap();
    assert_eq!(v.len(), 2);
    let uploads = data.join("Deckle").join("Uploads");
    for (path, (name, contents)) in v.iter().zip([("Dune.epub", "spice"), ("Émile.pdf", "pdf")]) {
        let p = Path::new(path);
        assert_eq!(p.file_name().unwrap(), name);
        let uuid = p.parent().unwrap();
        assert_eq!(uuid.parent().unwrap(), uploads);
        let id = uuid.file_name().unwrap().to_str().unwrap();
        assert_eq!(id.len(), 32, "{id}");
        assert!(id.bytes().all(|b| b.is_ascii_hexdigit()), "{id}");
        assert_eq!(std::fs::read_to_string(p).unwrap(), contents);
    }
    // No temp files left behind.
    assert!(
        tree(&uploads)
            .iter()
            .all(|p| !p.to_string_lossy().ends_with(".part")),
        "{:?}",
        tree(&uploads)
    );
}

#[tokio::test]
async fn upload_name_cannot_traverse() {
    let tmp = tempfile::tempdir().unwrap();
    let (_, data) = roots(tmp.path());
    let app = files_app(tmp.path(), None);
    let cookie = login(&app).await;
    let uploads = data.join("Deckle").join("Uploads");
    for (sent, saved) in [
        ("../../x.epub", "x.epub"),
        (r"..\..\y.epub", "y.epub"),
        ("/etc/z.epub", "z.epub"),
        ("..", "upload"),
        (".bashrc", "bashrc"),
        // Control characters go before the dots are trimmed, so they can't shield them.
        ("\t..", "upload"),
        ("\t.bashrc", "bashrc"),
        ("\u{85}..", "upload"),
        // A drive prefix would make Windows' `join` replace the whole path.
        ("C:evil.epub", "Cevil.epub"),
        ("..\\..\\x2.epub", "x2.epub"),
        (" . name. ", "name"),
    ] {
        let r = send(&app, upload(&[(sent, b"x")], &cookie)).await;
        assert_eq!(r.status, StatusCode::OK, "{sent}: {}", r.body);
        let v: Vec<String> = serde_json::from_str(&r.body).unwrap();
        let p = Path::new(&v[0]);
        assert_eq!(p.file_name().unwrap(), saved, "{sent}");
        assert_eq!(p.parent().unwrap().parent().unwrap(), uploads, "{sent}");
        assert!(p.is_file(), "{sent}");
    }
    // Nothing escaped the uploads folder.
    for p in tree(&shown(tmp.path())) {
        if p.is_file() && p.extension().is_some_and(|e| e == "epub") {
            assert!(p.starts_with(&uploads), "{p:?}");
        }
    }
}

/// A symlinked uploads folder can't send uploads outside the roots. Unix only: Windows needs a
/// privilege to make a symlink, which CI doesn't have.
#[cfg(unix)]
#[tokio::test]
async fn upload_refuses_symlinked_uploads_dir() {
    let tmp = tempfile::tempdir().unwrap();
    let (_, data) = roots(tmp.path());
    let outside = tmp.path().join("outside");
    std::fs::create_dir(&outside).unwrap();
    std::fs::create_dir(data.join("Deckle")).unwrap();
    std::os::unix::fs::symlink(&outside, data.join("Deckle").join("Uploads")).unwrap();
    let app = files_app(tmp.path(), None);
    let cookie = login(&app).await;
    let r = send(&app, upload(&[("x.epub", b"x")], &cookie)).await;
    assert_eq!(r.status, StatusCode::FORBIDDEN, "{}", r.body);
    assert!(r.body.contains("error"), "{}", r.body);
    assert_eq!(tree(&outside), Vec::<PathBuf>::new());
}

#[tokio::test]
async fn upload_over_limit_is_413_and_leaves_nothing() {
    let tmp = tempfile::tempdir().unwrap();
    let (_, data) = roots(tmp.path());
    let app = files_app(tmp.path(), Some(1024));
    let cookie = login(&app).await;
    // Exactly at the limit is fine.
    let r = send(&app, upload(&[("ok.epub", &[7u8; 1024])], &cookie)).await;
    assert_eq!(r.status, StatusCode::OK, "{}", r.body);
    let uploads = data.join("Deckle").join("Uploads");
    let before = tree(&uploads);
    // One byte over, after a file that fitted: the whole request leaves nothing.
    let r = send(
        &app,
        upload(&[("small.epub", b"x"), ("big.epub", &[7u8; 1025])], &cookie),
    )
    .await;
    assert_eq!(r.status, StatusCode::PAYLOAD_TOO_LARGE, "{}", r.body);
    assert!(r.body.contains("error"), "{}", r.body);
    assert_eq!(tree(&uploads), before);
}

#[tokio::test]
async fn download_sets_attachment() {
    let tmp = tempfile::tempdir().unwrap();
    let (_, data) = roots(tmp.path());
    std::fs::write(data.join("Dune.epub"), b"spice").unwrap();
    std::fs::write(data.join("Émile.epub"), b"rousseau").unwrap();
    let app = files_app(tmp.path(), None);
    let cookie = login(&app).await;

    let p = data.join("Dune.epub");
    let r = get(&app, &format!("/api/fs/download?path={}", q(&p)), &cookie).await;
    assert_eq!(r.status, StatusCode::OK, "{}", r.body);
    assert_eq!(r.body, "spice");
    assert_eq!(
        r.headers["content-disposition"],
        "attachment; filename=\"Dune.epub\""
    );
    assert_eq!(r.headers["content-type"], "application/epub+zip");

    // A quote can't break out of `filename="…"`. Unix only: Windows forbids `"` in names.
    #[cfg(unix)]
    {
        let p = data.join("Dune \"1\".epub");
        std::fs::write(&p, b"spice").unwrap();
        let r = get(&app, &format!("/api/fs/download?path={}", q(&p)), &cookie).await;
        assert_eq!(r.status, StatusCode::OK, "{}", r.body);
        assert_eq!(
            r.headers["content-disposition"],
            "attachment; filename=\"Dune _1_.epub\"; filename*=UTF-8''Dune%20%221%22.epub"
        );
    }

    let p = data.join("Émile.epub");
    let r = get(&app, &format!("/api/fs/download?path={}", q(&p)), &cookie).await;
    assert_eq!(r.status, StatusCode::OK, "{}", r.body);
    assert_eq!(
        r.headers["content-disposition"],
        "attachment; filename=\"_mile.epub\"; filename*=UTF-8''%C3%89mile.epub"
    );

    // A folder isn't a download.
    let r = get(
        &app,
        &format!("/api/fs/download?path={}", q(&data)),
        &cookie,
    )
    .await;
    assert_eq!(r.status, StatusCode::BAD_REQUEST, "{}", r.body);
}
