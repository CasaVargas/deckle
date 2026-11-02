// SPDX-License-Identifier: GPL-3.0-or-later
//! A reMarkable over its USB web interface (docs/superpowers/specs/
//! 2026-10-23-other-readers-design.md §9, phase 4): plugged in, with *Settings → Storage → USB
//! web interface* turned on, the tablet answers at `http://10.11.99.1`. Deckle lists what's on
//! it and uploads the paper there; no account, no cloud.
//!
//! The interface is the tablet's own and not documented by reMarkable: `GET /documents/` lists
//! the top folder as JSON (`VissibleName`, sic, and `Type`), `POST /upload` takes one file as
//! multipart form data. It can't delete, so Deckle can't prune a reMarkable. Spike 4 (§11)
//! checks all of it on a device.

use std::time::Duration;

use serde::Deserialize;

/// The tablet's address on its USB network.
pub const ADDRESS: &str = "http://10.11.99.1";

/// `DECKLE_REMARKABLE_URL` stands in for [`ADDRESS`] (tests, or a tablet reached otherwise).
pub fn base() -> String {
    std::env::var("DECKLE_REMARKABLE_URL")
        .ok()
        .filter(|u| !u.is_empty())
        .unwrap_or_else(|| ADDRESS.to_owned())
}

/// A reMarkable that answered.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Remarkable {
    pub base: String,
}

impl Remarkable {
    /// The key `[sync.devices."<id>"]` uses.
    pub fn id(&self) -> String {
        "remarkable/usb".to_owned()
    }
}

/// One document or folder in the tablet's top folder.
#[derive(Clone, Debug, Deserialize, PartialEq, Eq)]
pub struct Entry {
    #[serde(rename = "ID", default)]
    pub id: String,
    #[serde(rename = "VissibleName", alias = "VisibleName", default)]
    pub name: String,
    #[serde(rename = "Type", default)]
    pub kind: String,
}

fn agent(timeout: Duration) -> ureq::Agent {
    ureq::Agent::config_builder()
        .timeout_global(Some(timeout))
        // The tablet is on its own USB link: never through a proxy.
        .proxy(None)
        .http_status_as_error(true)
        .build()
        .new_agent()
}

/// The reMarkable at `base`, when it answers within a second.
pub fn detect_at(base: &str) -> Option<Remarkable> {
    let ok = agent(Duration::from_secs(1))
        .get(format!("{base}/documents/"))
        .call()
        .is_ok();
    ok.then(|| Remarkable {
        base: base.to_owned(),
    })
}

/// The reMarkable plugged in now, when its USB web interface answers.
pub fn detect() -> Option<Remarkable> {
    detect_at(&base())
}

/// What's in the tablet's top folder.
pub fn list(r: &Remarkable) -> Result<Vec<Entry>, String> {
    let mut resp = agent(Duration::from_secs(10))
        .get(format!("{}/documents/", r.base))
        .call()
        .map_err(|e| format!("reMarkable: {e}"))?;
    let body = resp
        .body_mut()
        .read_to_vec()
        .map_err(|e| format!("reMarkable: {e}"))?;
    serde_json::from_slice(&body).map_err(|e| format!("reMarkable: the list didn't read: {e}"))
}

/// Uploads `bytes` as `name` (`.pdf` or `.epub`; the tablet names the document after it).
pub fn upload(r: &Remarkable, name: &str, bytes: &[u8]) -> Result<(), String> {
    let kind = if name.ends_with(".pdf") {
        "application/pdf"
    } else {
        "application/epub+zip"
    };
    let boundary = format!("deckle{:016x}", fastish_id(bytes));
    let mut body = Vec::with_capacity(bytes.len() + 512);
    body.extend_from_slice(
        format!(
            "--{boundary}\r\nContent-Disposition: form-data; name=\"file\"; filename=\"{}\"\r\n\
             Content-Type: {kind}\r\n\r\n",
            name.replace('"', "'")
        )
        .as_bytes(),
    );
    body.extend_from_slice(bytes);
    body.extend_from_slice(format!("\r\n--{boundary}--\r\n").as_bytes());
    agent(Duration::from_secs(120))
        .post(format!("{}/upload", r.base))
        .header(
            "Content-Type",
            format!("multipart/form-data; boundary={boundary}"),
        )
        .send(&body[..])
        .map(|_| ())
        .map_err(|e| format!("reMarkable: upload of {name} failed: {e}"))
}

/// A boundary that won't occur in the file: a hash of it.
fn fastish_id(bytes: &[u8]) -> u64 {
    use sha2::{Digest, Sha256};
    let h = Sha256::digest(bytes);
    u64::from_be_bytes(h[..8].try_into().expect("8 bytes"))
}

/// A book made ready for the reMarkable's outbox (fixed-layout spec §12.1).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Book {
    pub bytes: Vec<u8>,
    /// `.pdf` for a comic, `.epub` for anything else.
    pub ext: &'static str,
    /// A comic's page images that wouldn't decode and were left out.
    pub skipped: usize,
}

/// The book at `source` as a reMarkable takes it: Deckle's own comic (a CBZ, CBR, page folder,
/// scanned PDF, or the EPUB made from one) as an image-only PDF fitted to `screen` on pages
/// `page` points big; any other book as the EPUB [`crate::reader::book_epub`] makes, which the
/// tablet's own reader reflows. A publisher's fixed-layout EPUB is refused: its pages would
/// need rendering, and the tablet would reflow them.
pub fn book(
    source: &std::path::Path,
    opts: &crate::reader::BookOptions,
    screen: deckle_comic::pages::Screen,
    page: (f32, f32),
) -> Result<Book, String> {
    let epub = crate::reader::book_epub(source, opts)?;
    match crate::reader::fixed_layout_of(&epub) {
        Some(layout) if layout.deckle_comic => {
            let made = deckle_comic::pdf::from_epub(&epub, screen, page)?;
            Ok(Book {
                bytes: made.pdf,
                ext: ".pdf",
                skipped: made.skipped,
            })
        }
        Some(_) => Err(
            "a publisher's fixed-layout book can't go to a reMarkable yet (its \
                        reader would reflow the pages); a CBZ, CBR or folder of page images can"
                .into(),
        ),
        None => Ok(Book {
            bytes: epub,
            ext: ".epub",
            skipped: 0,
        }),
    }
}

/// The document name a file shows as: its name without the extension.
pub fn shown_as(file: &str) -> &str {
    file.strip_suffix(".pdf")
        .or_else(|| file.strip_suffix(".epub"))
        .unwrap_or(file)
}

#[cfg(test)]
pub mod tests {
    use super::*;
    use std::io::{BufRead, BufReader, Read, Write};
    use std::net::TcpListener;
    use std::sync::{Arc, Mutex};

    /// A stand-in for the tablet: answers `GET /documents/` with `listing` and records every
    /// upload's body. Returns its base URL and the uploads.
    pub fn fake_tablet(listing: &'static str) -> (String, Arc<Mutex<Vec<Vec<u8>>>>) {
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
                    r.read_line(&mut h).unwrap();
                    if h == "\r\n" || h.is_empty() {
                        break;
                    }
                    if let Some(v) = h.to_ascii_lowercase().strip_prefix("content-length:") {
                        len = v.trim().parse().unwrap_or(0);
                    }
                }
                let mut body = vec![0; len];
                r.read_exact(&mut body).unwrap();
                let (status, reply) = if line.starts_with("GET /documents/") {
                    ("200 OK", listing.to_owned())
                } else if line.starts_with("POST /upload") {
                    seen.lock().unwrap().push(body);
                    ("201 Created", "{}".to_owned())
                } else {
                    ("404 Not Found", String::new())
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

    #[test]
    fn a_tablet_that_answers_is_found_and_listed() {
        let (base, _) = fake_tablet(
            "[{\"ID\":\"a1\",\"VissibleName\":\"deckle-daily-2026-10-22\",\"Type\":\"DocumentType\"}]",
        );
        let r = detect_at(&base).unwrap();
        assert_eq!(r.id(), "remarkable/usb");
        assert_eq!(
            list(&r).unwrap(),
            [Entry {
                id: "a1".into(),
                name: "deckle-daily-2026-10-22".into(),
                kind: "DocumentType".into()
            }]
        );
    }

    #[test]
    fn no_tablet_is_none_quickly() {
        let start = std::time::Instant::now();
        // A port nothing listens on.
        let free = TcpListener::bind("127.0.0.1:0").unwrap();
        let base = format!("http://{}", free.local_addr().unwrap());
        drop(free);
        assert_eq!(detect_at(&base), None);
        assert!(start.elapsed() < Duration::from_secs(3));
    }

    #[test]
    fn an_upload_is_one_file_as_form_data() {
        let (base, uploads) = fake_tablet("[]");
        let r = detect_at(&base).unwrap();
        upload(&r, "deckle-daily-2026-10-23.pdf", b"%PDF-1.7 the paper").unwrap();
        let got = uploads.lock().unwrap();
        let body = String::from_utf8_lossy(&got[0]);
        assert!(body.contains("name=\"file\"; filename=\"deckle-daily-2026-10-23.pdf\""));
        assert!(body.contains("Content-Type: application/pdf"));
        assert!(body.contains("%PDF-1.7 the paper"));
    }

    #[test]
    fn a_comic_becomes_a_pdf_a_book_an_epub_and_a_publishers_fixed_layout_is_refused() {
        let tmp = tempfile::tempdir().unwrap();
        let img = image::RgbImage::from_pixel(600, 800, image::Rgb([200, 40, 40]));
        let mut png = std::io::Cursor::new(Vec::new());
        img.write_to(&mut png, image::ImageFormat::Png).unwrap();
        let cbz = tmp.path().join("c.cbz");
        let mut zw = zip::ZipWriter::new(std::fs::File::create(&cbz).unwrap());
        zw.start_file("1.png", zip::write::SimpleFileOptions::default())
            .unwrap();
        zw.write_all(png.get_ref()).unwrap();
        zw.finish().unwrap();
        let screen = deckle_comic::pages::Screen {
            width: 351,
            height: 468,
            color: false,
        };
        let page = (111.8, 149.1);
        let opts = crate::reader::BookOptions::default();

        let comic = book(&cbz, &opts, screen, page).unwrap();
        assert_eq!((comic.ext, comic.skipped), (".pdf", 0));
        assert!(comic.bytes.starts_with(b"%PDF-"));

        let mini = deckle_kfx::fixtures::path("deckle-mini.epub");
        let text = book(&mini, &opts, screen, page).unwrap();
        assert_eq!(text.ext, ".epub");
        assert_eq!(&text.bytes[..2], b"PK");

        let publisher = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../../fixtures/comic/fixed-layout-rtl.epub");
        let err = book(&publisher, &opts, screen, page).unwrap_err();
        assert!(err.contains("publisher's fixed-layout"), "{err}");
    }

    #[test]
    fn names_as_the_tablet_shows_them() {
        assert_eq!(
            shown_as("deckle-daily-2026-10-23.pdf"),
            "deckle-daily-2026-10-23"
        );
        assert_eq!(shown_as("a.epub"), "a");
    }
}
