// SPDX-License-Identifier: GPL-3.0-or-later
//! The catalog over HTTP (KOReader hub design §A.3).
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use axum::body::Body;
use axum::http::{Request, StatusCode};
use base64::Engine;
use chrono::NaiveDate;
use deckle_readerserver::{Catalog, Ctx, Hooks, Paper, Settings, auth, opds, router};
use http_body_util::BodyExt;
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

struct Fixture {
    _tmp: tempfile::TempDir,
    paper: PathBuf,
    book: PathBuf,
    ctx: Arc<Ctx>,
}

fn fixture() -> Fixture {
    let tmp = tempfile::tempdir().unwrap();
    let paper = tmp.path().join("2026-10-30.epub");
    let older = tmp.path().join("2026-10-29.epub");
    let book = tmp.path().join("Walden.epub");
    for p in [&paper, &older, &book] {
        epub(p);
    }
    let catalog = Catalog {
        papers: vec![
            Paper {
                date: NaiveDate::from_ymd_opt(2026, 10, 30).unwrap(),
                title: "The Deckle Daily".into(),
                epub: paper.clone(),
                stem: "deckle-daily-2026-10-30".into(),
            },
            Paper {
                date: NaiveDate::from_ymd_opt(2026, 10, 29).unwrap(),
                title: "The Deckle Daily".into(),
                epub: older,
                stem: "deckle-daily-2026-10-29".into(),
            },
        ],
        books: vec![book.clone()],
    };
    let sync_store = tmp.path().join("koreader-sync.json");
    let ctx = Arc::new(Ctx::new(
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
            sync_store,
        },
    ));
    Fixture {
        _tmp: tmp,
        paper,
        book,
        ctx,
    }
}

async fn get(ctx: &Arc<Ctx>, uri: &str, signed_in: bool) -> (StatusCode, String, Vec<u8>) {
    let mut req = Request::builder().uri(uri);
    if signed_in {
        let b = base64::engine::general_purpose::STANDARD.encode("deckle:hunter2");
        req = req.header("authorization", format!("Basic {b}"));
    }
    let res = router(ctx.clone())
        .oneshot(req.body(Body::empty()).unwrap())
        .await
        .unwrap();
    let status = res.status();
    let ct = res
        .headers()
        .get("content-type")
        .map(|v| v.to_str().unwrap().to_owned())
        .unwrap_or_default();
    let body = res.into_body().collect().await.unwrap().to_bytes().to_vec();
    (status, ct, body)
}

#[tokio::test]
async fn every_route_needs_the_account() {
    let f = fixture();
    let id = opds::file_id(&f.paper);
    for uri in ["/opds", "/opds/today", "/opds/papers", "/opds/books"] {
        let (status, _, _) = get(&f.ctx, uri, false).await;
        assert_eq!(status, StatusCode::UNAUTHORIZED, "{uri}");
    }
    let (status, _, _) = get(&f.ctx, &format!("/opds/file/{id}"), false).await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn the_feeds_are_opds_atom() {
    let f = fixture();
    let (status, ct, body) = get(&f.ctx, "/opds", true).await;
    assert_eq!(status, StatusCode::OK);
    assert!(ct.contains("kind=navigation"), "{ct}");
    let xml = String::from_utf8(body).unwrap();
    let doc = roxmltree::Document::parse(&xml).unwrap();
    let atom = "http://www.w3.org/2005/Atom";
    assert_eq!(doc.root_element().tag_name().namespace(), Some(atom));
    let subsections = doc
        .descendants()
        .filter(|n| n.attribute("rel") == Some("subsection"))
        .count();
    assert_eq!(subsections, 3);

    let (_, ct, body) = get(&f.ctx, "/opds/today", true).await;
    assert!(ct.contains("kind=acquisition"), "{ct}");
    let xml = String::from_utf8(body).unwrap();
    let doc = roxmltree::Document::parse(&xml).unwrap();
    let entries: Vec<_> = doc
        .descendants()
        .filter(|n| n.tag_name().name() == "entry")
        .collect();
    assert_eq!(entries.len(), 1, "only the newest: {xml}");
    assert!(
        xml.contains("The Deckle Daily — Friday, October 30, 2026"),
        "{xml}"
    );
    let acq = doc
        .descendants()
        .filter(|n| n.attribute("rel") == Some("http://opds-spec.org/acquisition"))
        .count();
    assert_eq!(acq, 2, "EPUB and KEPUB");

    let (_, _, body) = get(&f.ctx, "/opds/papers", true).await;
    let xml = String::from_utf8(body).unwrap();
    assert_eq!(xml.matches("<entry>").count(), 2);
    let (_, _, body) = get(&f.ctx, "/opds/books", true).await;
    let xml = String::from_utf8(body).unwrap();
    assert!(xml.contains("<title>Walden</title>"), "{xml}");
}

#[tokio::test]
async fn files_come_by_id_only_and_a_kobo_can_have_a_kepub() {
    let f = fixture();
    let id = opds::file_id(&f.paper);
    let (status, ct, body) = get(&f.ctx, &format!("/opds/file/{id}"), true).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(ct, "application/epub+zip");
    let (_, _, feed) = get(&f.ctx, "/opds/today", true).await;
    assert!(
        String::from_utf8(feed)
            .unwrap()
            .contains("deckle-daily-2026-10-30.epub")
    );
    assert_eq!(body, std::fs::read(&f.paper).unwrap());

    let (status, ct, body) = get(&f.ctx, &format!("/opds/file/{id}?format=kepub"), true).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(ct, "application/kepub+zip");
    let mut z = zip::ZipArchive::new(std::io::Cursor::new(body)).unwrap();
    let mut page = String::new();
    std::io::Read::read_to_string(&mut z.by_name("OEBPS/p.xhtml").unwrap(), &mut page).unwrap();
    assert!(page.contains("koboSpan"), "{page}");

    let book = opds::file_id(&f.book);
    let (status, _, _) = get(&f.ctx, &format!("/opds/file/{book}"), true).await;
    assert_eq!(status, StatusCode::OK);
    for bad in [
        "0000",
        "..%2F..%2Fetc%2Fpasswd",
        &opds::file_id(Path::new("/etc/passwd")),
    ] {
        let (status, _, _) = get(&f.ctx, &format!("/opds/file/{bad}"), true).await;
        assert_eq!(status, StatusCode::NOT_FOUND, "{bad}");
    }
}

#[tokio::test]
async fn the_cover_is_the_epubs_own() {
    let f = fixture();
    let id = opds::file_id(&f.paper);
    let (status, ct, body) = get(&f.ctx, &format!("/opds/cover/{id}"), true).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(ct, "image/jpeg");
    assert!(body.ends_with(b"JPEGBYTES"));
}
