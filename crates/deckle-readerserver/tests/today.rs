// SPDX-License-Identifier: GPL-3.0-or-later
//! What the Deckle plugin asks (KOReader plugins design §5.3, §6).
use std::io::Write;
use std::path::Path;
use std::sync::Arc;

use axum::body::Body;
use axum::http::{Request, StatusCode};
use chrono::NaiveDate;
use deckle_readerserver::{Catalog, Ctx, Hooks, Paper, Settings, auth, router};
use http_body_util::BodyExt;
use sha2::{Digest, Sha256};
use tower::ServiceExt;

/// A one-page EPUB with a JPEG cover.
fn epub(path: &Path) {
    let mut z = zip::ZipWriter::new(std::fs::File::create(path).unwrap());
    let o = zip::write::SimpleFileOptions::default();
    for (name, body) in [
        ("mimetype", "application/epub+zip"),
        (
            "META-INF/container.xml",
            "<?xml version=\"1.0\"?><container version=\"1.0\" \
             xmlns=\"urn:oasis:names:tc:opendocument:xmlns:container\"><rootfiles><rootfile \
             full-path=\"OEBPS/content.opf\" media-type=\"application/oebps-package+xml\"/>\
             </rootfiles></container>",
        ),
        (
            "OEBPS/content.opf",
            "<?xml version=\"1.0\"?><package xmlns=\"http://www.idpf.org/2007/opf\" version=\"3.0\" \
             unique-identifier=\"id\"><metadata xmlns:dc=\"http://purl.org/dc/elements/1.1/\">\
             <dc:identifier id=\"id\">x</dc:identifier><dc:title>Paper</dc:title>\
             <dc:language>en</dc:language><meta name=\"cover\" content=\"cov\"/></metadata>\
             <manifest><item id=\"p\" href=\"p.xhtml\" media-type=\"application/xhtml+xml\"/>\
             <item id=\"cov\" href=\"cover.jpg\" media-type=\"image/jpeg\" properties=\"cover-image\"/>\
             </manifest><spine><itemref idref=\"p\"/></spine></package>",
        ),
        (
            "OEBPS/p.xhtml",
            "<html xmlns=\"http://www.w3.org/1999/xhtml\"><body><p>Rain today. Sun tomorrow.</p>\
             </body></html>",
        ),
        ("OEBPS/cover.jpg", "\u{ff}JPEGBYTES"),
    ] {
        z.start_file(name, o).unwrap();
        z.write_all(body.as_bytes()).unwrap();
    }
    z.finish().unwrap();
}

fn ctx(catalog: Catalog, dir: &Path) -> Arc<Ctx> {
    Arc::new(Ctx::new(
        Settings {
            bind: "127.0.0.1".into(),
            port: 0,
            username: "deckle".into(),
            key: auth::md5_hex(b"hunter2"),
        },
        Hooks {
            source: Arc::new(move || catalog.clone()),
            served: Arc::new(|_, _| {}),
            progress: Arc::new(|_| {}),
            sync_store: dir.join("koreader-sync.json"),
        },
    ))
}

/// A GET signed in as the plugin is: KOReader's sync header pair, no password.
async fn get(ctx: &Arc<Ctx>, uri: &str, signed_in: bool) -> (StatusCode, Vec<u8>) {
    let mut req = Request::builder().uri(uri);
    if signed_in {
        req = req
            .header("x-auth-user", "deckle")
            .header("x-auth-key", auth::md5_hex(b"hunter2"));
    }
    let res = router(ctx.clone())
        .oneshot(req.body(Body::empty()).unwrap())
        .await
        .unwrap();
    let status = res.status();
    (
        status,
        res.into_body().collect().await.unwrap().to_bytes().to_vec(),
    )
}

fn paper(dir: &Path, stem: &str, date: (i32, u32, u32), main: bool) -> Paper {
    let path = dir.join(format!("{stem}.epub"));
    epub(&path);
    Paper {
        date: NaiveDate::from_ymd_opt(date.0, date.1, date.2).unwrap(),
        title: "Paper".into(),
        epub: path,
        stem: stem.into(),
        main,
    }
}

#[tokio::test]
async fn today_needs_the_account_and_says_when_theres_no_paper() {
    let tmp = tempfile::tempdir().unwrap();
    let c = ctx(Catalog::default(), tmp.path());
    assert_eq!(
        get(&c, "/deckle/v1/today", false).await.0,
        StatusCode::UNAUTHORIZED
    );
    assert_eq!(
        get(&c, "/deckle/v1/today", true).await.0,
        StatusCode::NO_CONTENT
    );
}

#[tokio::test]
async fn today_is_the_main_papers_newest_and_downloads_as_described() {
    let tmp = tempfile::tempdir().unwrap();
    let catalog = Catalog {
        papers: vec![
            paper(tmp.path(), "kids-2026-10-31", (2026, 10, 31), false),
            paper(tmp.path(), "deckle-daily-2026-10-30", (2026, 10, 30), true),
            paper(tmp.path(), "deckle-daily-2026-10-29", (2026, 10, 29), true),
        ],
        books: Vec::new(),
    };
    let c = ctx(catalog, tmp.path());
    for (query, name) in [
        ("", "deckle-daily-2026-10-30.epub"),
        ("?format=kepub", "deckle-daily-2026-10-30.kepub.epub"),
    ] {
        let (status, body) = get(&c, &format!("/deckle/v1/today{query}"), true).await;
        assert_eq!(status, StatusCode::OK);
        let t: serde_json::Value = serde_json::from_slice(&body).unwrap();
        assert_eq!(t["date"], "2026-10-30");
        assert_eq!(t["file"], name);
        let (status, bytes) = get(&c, t["url"].as_str().unwrap(), true).await;
        assert_eq!(status, StatusCode::OK, "{query}");
        assert_eq!(t["size"].as_u64().unwrap() as usize, bytes.len(), "{query}");
        let sha: String = Sha256::digest(&bytes)
            .iter()
            .map(|b| format!("{b:02x}"))
            .collect();
        assert_eq!(t["sha256"], sha, "{query}");
    }
}

#[tokio::test]
async fn with_no_main_paper_today_takes_any_papers_newest() {
    let tmp = tempfile::tempdir().unwrap();
    let catalog = Catalog {
        papers: vec![paper(tmp.path(), "kids-2026-10-31", (2026, 10, 31), false)],
        books: Vec::new(),
    };
    let c = ctx(catalog, tmp.path());
    let (_, body) = get(&c, "/deckle/v1/today", true).await;
    let t: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(t["file"], "kids-2026-10-31.epub");
}
