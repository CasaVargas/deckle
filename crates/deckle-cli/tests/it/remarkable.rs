// SPDX-License-Identifier: GPL-3.0-or-later
//! A reMarkable (other readers design §9): the paper as a PDF over its USB web interface, here a
//! stand-in on 127.0.0.1 through `DECKLE_REMARKABLE_URL`, with no Kindle Previewer.

use std::collections::BTreeMap;
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

/// `[remarkable] address` reaches a tablet off the USB link (fixed-layout spec §12.4): with the
/// test env var blank, the configured address is where the paper goes; `deckle devices` shows
/// the tablet, and says when none answers.
#[test]
fn the_configured_address_stands_in_for_the_usb_link_and_devices_lists_the_tablet() {
    let (base, uploads) = tablet("[]");
    let (_tmp, home) = setup(&format!("address = \"{base}\"\n"));
    let o = deckle(&home, "").args(["devices"]).output().unwrap();
    let out = String::from_utf8_lossy(&o.stdout);
    assert!(o.status.success(), "{out}");
    assert!(
        out.contains(&format!(
            "reMarkable at {base}  USB web interface  sync id \"remarkable/usb\""
        )),
        "{out}"
    );
    let o = deckle(&home, "")
        .args(["digest", "run", "--date", "2026-10-23"])
        .output()
        .unwrap();
    assert!(o.status.success(), "{}", String::from_utf8_lossy(&o.stderr));
    assert_eq!(uploads.lock().unwrap().len(), 1);

    // No tablet at the configured address: `devices` says so.
    let free = TcpListener::bind("127.0.0.1:0").unwrap();
    let gone = format!("http://{}", free.local_addr().unwrap());
    drop(free);
    let (_tmp, home) = setup(&format!("address = \"{gone}\"\n"));
    let o = deckle(&home, "").args(["devices"]).output().unwrap();
    let out = String::from_utf8_lossy(&o.stdout);
    assert!(o.status.success(), "{out}");
    assert!(out.contains("no reMarkable answered"), "{out}");
}

/// The reMarkable outbox under the test home's cache dir.
fn outbox(home: &Path) -> PathBuf {
    let cache = if cfg!(target_os = "macos") {
        home.join("Library/Caches")
    } else {
        home.join("cache")
    };
    cache.join("deckle/outbox-remarkable")
}

/// A stand-in tablet over SSH (fixed-layout spec §12.5): the in-process server from
/// `deckle_core::remarkable_ssh::fake` with a file map standing in for xochitl's folder and
/// the few commands Deckle runs. Returns its port, the files, and how often it "restarted".
/// The stand-in tablet's document folder.
type Files = Arc<Mutex<BTreeMap<String, Vec<u8>>>>;

fn ssh_tablet() -> (u16, Files, Arc<Mutex<usize>>) {
    const DIR: &str = deckle_core::remarkable_ssh::DIR;
    let files: Files = Default::default();
    let restarts = Arc::new(Mutex::new(0usize));
    let (f, r) = (files.clone(), restarts.clone());
    let (_host, port, _fp) = deckle_core::remarkable_ssh::fake::dropbear(move |cmd, stdin| {
        let prefix = format!("{DIR}/");
        let mut files = f.lock().unwrap();
        if cmd == "date +%s" {
            return (b"1700000000".to_vec(), 0);
        }
        if cmd.starts_with("cd ") && cmd.contains("*.metadata") {
            let mut out = Vec::new();
            for (name, bytes) in files.iter() {
                if name.ends_with(".metadata") {
                    out.extend_from_slice(format!("== {name}\n").as_bytes());
                    out.extend_from_slice(bytes);
                    out.push(b'\n');
                }
            }
            return (out, 0);
        }
        if let Some(rest) = cmd.strip_prefix("mkdir -p ") {
            let write = rest.split_once(" && ").unwrap().1;
            let file = write
                .strip_prefix("cat > ")
                .unwrap()
                .strip_prefix(&prefix)
                .unwrap();
            files.insert(file.to_owned(), stdin.to_vec());
            return (Vec::new(), 0);
        }
        if let Some(file) = cmd.strip_prefix("cat > ") {
            files.insert(
                file.strip_prefix(&prefix).unwrap().to_owned(),
                stdin.to_vec(),
            );
            return (Vec::new(), 0);
        }
        if let Some(file) = cmd.strip_prefix("cat ") {
            return match files.get(file.strip_prefix(&prefix).unwrap()) {
                Some(b) => (b.clone(), 0),
                None => (b"No such file".to_vec(), 1),
            };
        }
        if cmd.starts_with("systemctl reset-failed xochitl.service; systemctl restart") {
            *r.lock().unwrap() += 1;
            return (Vec::new(), 0);
        }
        (format!("unknown: {cmd}").into_bytes(), 127)
    });
    (port, files, restarts)
}

/// The paper over SSH: into a Deckle folder on the tablet, the host key pinned on first use,
/// xochitl restarted once, and not sent twice. The password comes from the environment, as
/// in a container (no keychain).
#[test]
fn the_paper_goes_over_ssh_into_a_deckle_folder_and_the_host_key_is_pinned() {
    let (port, files, restarts) = ssh_tablet();
    let (tmp, home) = setup(&format!(
        "transport = \"ssh\"\nssh_host = \"127.0.0.1\"\nssh_port = {port}\n"
    ));
    let run = |args: &[&str]| {
        deckle(&home, "")
            .env("DECKLE_DAEMON", "1")
            .env("DECKLE_REMARKABLE_PASSWORD", "pw")
            .args(args)
            .output()
            .unwrap()
    };
    let o = run(&["digest", "run", "--date", "2026-10-23"]);
    let out = String::from_utf8_lossy(&o.stdout);
    assert!(
        o.status.success(),
        "{out}\n{}",
        String::from_utf8_lossy(&o.stderr)
    );
    assert!(
        out.contains("reMarkable -> deckle-daily-2026-10-23.pdf"),
        "{out}"
    );
    assert!(out.contains("xochitl restarted"), "{out}");
    {
        let files = files.lock().unwrap();
        let folder = files
            .iter()
            .find(|(n, b)| {
                n.ends_with(".metadata") && b.windows(14).any(|w| w == b"CollectionType")
            })
            .map(|(n, _)| n.trim_end_matches(".metadata").to_owned())
            .expect("a Deckle folder");
        let folder_meta: serde_json::Value =
            serde_json::from_slice(&files[&format!("{folder}.metadata")]).unwrap();
        assert_eq!(folder_meta["visibleName"], "Deckle");
        let (pdf_name, pdf) = files
            .iter()
            .find(|(n, _)| n.ends_with(".pdf"))
            .expect("the PDF");
        assert!(pdf.starts_with(b"%PDF-"));
        let uuid = pdf_name.trim_end_matches(".pdf");
        let meta: serde_json::Value =
            serde_json::from_slice(&files[&format!("{uuid}.metadata")]).unwrap();
        assert_eq!(meta["visibleName"], "deckle-daily-2026-10-23");
        assert_eq!(meta["parent"], folder);
        assert_eq!(
            files[&format!("{uuid}.content")],
            b"{\"fileType\": \"pdf\"}"
        );
    }
    assert_eq!(*restarts.lock().unwrap(), 1);
    // The host key is pinned beside digest.toml.
    let pin = std::fs::read_to_string(tmp.path().join(if cfg!(target_os = "macos") {
        "home/Library/Application Support/deckle/remarkable-hostkey.json"
    } else {
        "home/deckle/remarkable-hostkey.json"
    }))
    .expect("the pin file");
    assert!(pin.contains("\"fingerprint\": \"SHA256:"), "{pin}");
    // The same edition again: already there, no second copy, no second restart.
    std::fs::write(
        outbox(&home).join("deckle-daily-2026-10-23.pdf"),
        b"%PDF-again",
    )
    .unwrap();
    let o = run(&["digest", "push"]);
    let out = String::from_utf8_lossy(&o.stdout);
    assert!(
        o.status.success(),
        "{out}\n{}",
        String::from_utf8_lossy(&o.stderr)
    );
    assert!(
        out.contains("reMarkable already has deckle-daily-2026-10-23.pdf"),
        "{out}"
    );
    assert_eq!(
        files
            .lock()
            .unwrap()
            .values()
            .filter(|b| b.starts_with(b"%PDF-"))
            .count(),
        1
    );
    assert_eq!(*restarts.lock().unwrap(), 1);
    // `devices` names the tablet by its host key.
    let o = run(&["devices"]);
    let out = String::from_utf8_lossy(&o.stdout);
    assert!(
        out.contains(&format!(
            "reMarkable over SSH at 127.0.0.1:{port}  host key SHA256:"
        )),
        "{out}"
    );
}

/// A one-page CBZ at `<dir>/<name>`.
fn cbz(dir: &Path, name: &str) -> PathBuf {
    let img = image::RgbImage::from_pixel(600, 800, image::Rgb([200, 40, 40]));
    let mut png = std::io::Cursor::new(Vec::new());
    img.write_to(&mut png, image::ImageFormat::Png).unwrap();
    let path = dir.join(name);
    let mut zw = zip::ZipWriter::new(std::fs::File::create(&path).unwrap());
    zw.start_file("1.png", zip::write::SimpleFileOptions::default())
        .unwrap();
    zw.write_all(png.get_ref()).unwrap();
    zw.finish().unwrap();
    path
}

fn fixture(name: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join(format!("../../fixtures/{name}"))
}

fn has(hay: &[u8], needle: &[u8]) -> bool {
    hay.windows(needle.len()).any(|w| w == needle)
}

/// `run --to remarkable` (fixed-layout spec §12.1): a comic goes as an image PDF, a book as an
/// EPUB, both uploaded at once when the tablet answers.
#[test]
fn run_to_remarkable_sends_a_comic_as_a_pdf_and_a_book_as_an_epub() {
    let (base, uploads) = tablet("[]");
    let (tmp, home) = setup("");
    let comic = cbz(tmp.path(), "Ink.cbz");
    let o = deckle(&home, &base)
        .args(["run", "--to", "remarkable"])
        .arg(&comic)
        .arg(fixture("mini/deckle-mini.epub"))
        .output()
        .unwrap();
    let out = String::from_utf8_lossy(&o.stdout);
    assert!(
        o.status.success(),
        "{out}\n{}",
        String::from_utf8_lossy(&o.stderr)
    );
    assert!(
        out.contains("queued for the reMarkable as Ink.pdf"),
        "{out}"
    );
    assert!(
        out.contains("queued for the reMarkable as deckle-mini.epub"),
        "{out}"
    );
    assert!(out.contains("reMarkable -> Ink.pdf"), "{out}");
    assert!(out.contains("reMarkable -> deckle-mini.epub"), "{out}");
    let got = uploads.lock().unwrap();
    assert_eq!(got.len(), 2);
    let pdf = got
        .iter()
        .find(|b| has(b, b"filename=\"Ink.pdf\""))
        .expect("the comic's PDF");
    assert!(has(pdf, b"Content-Type: application/pdf"));
    assert!(has(pdf, b"%PDF-"));
    let epub = got
        .iter()
        .find(|b| has(b, b"filename=\"deckle-mini.epub\""))
        .expect("the book's EPUB");
    assert!(has(epub, b"Content-Type: application/epub+zip"));
}

#[test]
fn a_publishers_fixed_layout_book_is_refused_for_a_remarkable() {
    let (base, uploads) = tablet("[]");
    let (_tmp, home) = setup("");
    let o = deckle(&home, &base)
        .args(["run", "--to", "remarkable"])
        .arg(fixture("comic/fixed-layout-rtl.epub"))
        .output()
        .unwrap();
    assert!(!o.status.success());
    let out = String::from_utf8_lossy(&o.stdout);
    assert!(out.contains("publisher's fixed-layout book"), "{out}");
    assert!(uploads.lock().unwrap().is_empty());
}

/// Books wait for the tablet and go with the next push, with no `[remarkable]` section at all:
/// `enabled` is about the paper.
#[test]
fn books_wait_for_the_tablet_and_push_sends_them_without_the_paper_set_up() {
    let tmp = tempfile::tempdir().unwrap();
    let home = tmp.path().join("home");
    let dir = if cfg!(target_os = "macos") {
        home.join("Library/Application Support/deckle")
    } else {
        home.join("deckle")
    };
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(dir.join("digest.toml"), "sections = [\"horoscope\"]\n").unwrap();
    let comic = cbz(tmp.path(), "Ink.cbz");
    let free = TcpListener::bind("127.0.0.1:0").unwrap();
    let gone = format!("http://{}", free.local_addr().unwrap());
    drop(free);
    let o = deckle(&home, &gone)
        .args(["run", "--to", "remarkable"])
        .arg(&comic)
        .output()
        .unwrap();
    let out = String::from_utf8_lossy(&o.stdout);
    assert!(
        o.status.success(),
        "{out}\n{}",
        String::from_utf8_lossy(&o.stderr)
    );
    assert!(out.contains("no reMarkable answered"), "{out}");
    let (base, uploads) = tablet("[]");
    let o = deckle(&home, &base)
        .args(["digest", "push"])
        .output()
        .unwrap();
    let out = String::from_utf8_lossy(&o.stdout);
    assert!(
        o.status.success(),
        "{out}\n{}",
        String::from_utf8_lossy(&o.stderr)
    );
    assert!(out.contains("reMarkable -> Ink.pdf"), "{out}");
    assert_eq!(uploads.lock().unwrap().len(), 1);
}
