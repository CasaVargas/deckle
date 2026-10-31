// SPDX-License-Identifier: GPL-3.0-or-later
//! Deckle's wireless server against KOReader's own calibre client (calibre bridge design §8),
//! run under LuaJIT by tests/koreader/client.lua. The client's files come from
//! `tools/fetch-koreader-calibre.sh`; without them (or without luajit) this test says so and
//! passes, unless `DECKLE_REQUIRE_KOREADER` is set, as CI sets it on Linux.

use std::io::{BufRead, BufReader, Read, Write};
use std::net::{TcpListener, TcpStream};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::mpsc;
use std::time::Duration;

use deckle_readerserver::wireless::{Library, Settings, Store, serve_on};

fn koreader_dir() -> Option<PathBuf> {
    let dir = std::env::var_os("DECKLE_KOREADER_CALIBRE")
        .map(PathBuf::from)
        .unwrap_or_else(|| {
            Path::new(env!("CARGO_MANIFEST_DIR")).join("../../target/koreader-calibre")
        });
    let have_lua = Command::new("luajit")
        .arg("-v")
        .output()
        .is_ok_and(|o| o.status.success());
    if dir.join("wireless.lua").is_file() && have_lua {
        return Some(dir);
    }
    assert!(
        std::env::var_os("DECKLE_REQUIRE_KOREADER").is_none(),
        "KOReader's calibre client isn't in {} (or luajit is missing): run tools/fetch-koreader-calibre.sh",
        dir.display()
    );
    eprintln!("skipped: no KOReader calibre client (tools/fetch-koreader-calibre.sh) or no luajit");
    None
}

/// Runs the client against `addr` until the server ends the session; returns its stderr.
fn run_client(
    dir: &Path,
    addr: std::net::SocketAddr,
    inbox: &Path,
    password: Option<&str>,
) -> String {
    let script = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/koreader/client.lua");
    let mut cmd = Command::new("luajit");
    cmd.arg(script).arg(dir).arg(inbox);
    if let Some(p) = password {
        cmd.arg(p);
    }
    let mut child = cmd
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    let tcp = TcpStream::connect(addr).unwrap();
    let mut to_server = tcp.try_clone().unwrap();
    let mut from_server = tcp;
    let mut to_lua = child.stdin.take().unwrap();
    let lua_out = child.stdout.take().unwrap();

    // Deckle → KOReader: each read, framed for the client's stdin; closed when Deckle closes.
    let down = std::thread::spawn(move || {
        let mut buf = vec![0u8; 8192];
        loop {
            match from_server.read(&mut buf) {
                Ok(0) | Err(_) => break,
                Ok(n) => {
                    if to_lua.write_all(format!("{n}\n").as_bytes()).is_err()
                        || to_lua.write_all(&buf[..n]).is_err()
                        || to_lua.flush().is_err()
                    {
                        break;
                    }
                }
            }
        }
    });
    // KOReader → Deckle.
    let up = std::thread::spawn(move || {
        let mut r = BufReader::new(lua_out);
        loop {
            let mut line = String::new();
            if r.read_line(&mut line).unwrap_or(0) == 0 {
                break;
            }
            let n: usize = line.trim().parse().unwrap();
            let mut data = vec![0u8; n];
            r.read_exact(&mut data).unwrap();
            if to_server.write_all(&data).is_err() {
                break;
            }
        }
    });
    down.join().unwrap();
    let mut err = String::new();
    child
        .stderr
        .take()
        .unwrap()
        .read_to_string(&mut err)
        .unwrap();
    child.wait().unwrap();
    up.join().unwrap();
    err
}

fn server(
    dir: &Path,
    password: Option<&str>,
) -> (
    std::net::SocketAddr,
    mpsc::Sender<()>,
    std::thread::JoinHandle<()>,
) {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let addr = listener.local_addr().unwrap();
    let settings = Settings {
        bind: "127.0.0.1".into(),
        port: addr.port(),
        discovery: false,
        library: Library {
            name: "Deckle".into(),
            uuid: "11111111-2222-4333-8444-555555555555".into(),
        },
        password: password.map(str::to_owned),
        content_port: 0,
        dir: dir.to_path_buf(),
        keepalive: Duration::from_millis(500),
        idle: Duration::from_millis(1500),
        poll: Duration::from_millis(50),
    };
    let (stop, rx) = mpsc::channel();
    let h = std::thread::spawn(move || serve_on(listener, settings, rx).unwrap());
    (addr, stop, h)
}

fn epub(path: &Path) {
    let mut z = zip::ZipWriter::new(std::fs::File::create(path).unwrap());
    let o =
        zip::write::SimpleFileOptions::default().compression_method(zip::CompressionMethod::Stored);
    z.start_file("mimetype", o).unwrap();
    z.write_all(b"application/epub+zip").unwrap();
    z.start_file("META-INF/container.xml", o).unwrap();
    z.write_all(br#"<?xml version="1.0"?><container version="1.0" xmlns="urn:oasis:names:tc:opendocument:xmlns:container"><rootfiles><rootfile full-path="content.opf" media-type="application/oebps-package+xml"/></rootfiles></container>"#).unwrap();
    z.start_file("content.opf", o).unwrap();
    z.write_all(r#"<?xml version="1.0"?><package xmlns="http://www.idpf.org/2007/opf" xmlns:opf="http://www.idpf.org/2007/opf" version="2.0" unique-identifier="id"><metadata xmlns:dc="http://purl.org/dc/elements/1.1/"><dc:title>Café Stories</dc:title><dc:creator opf:file-as="Ng, Celeste">Celeste Ng</dc:creator><dc:language>en</dc:language><dc:identifier id="id">urn:x</dc:identifier><dc:identifier opf:scheme="calibre">c0ffee00-0000-4000-8000-000000000001</dc:identifier></metadata><manifest><item id="c" href="c.xhtml" media-type="application/xhtml+xml"/></manifest><spine><itemref idref="c"/></spine></package>"#.as_bytes()).unwrap();
    z.start_file("c.xhtml", o).unwrap();
    z.write_all(&vec![b'x'; 20_000]).unwrap();
    z.finish().unwrap();
}

#[test]
fn koreaders_own_client_receives_a_book_and_records_it_for_calibre_search() {
    let Some(dir) = koreader_dir() else { return };
    let tmp = tempfile::tempdir().unwrap();
    let inbox = tmp.path().join("inbox");
    let book = tmp.path().join("cafe.epub");
    epub(&book);
    let state = tmp.path().join("state");
    let (addr, stop, h) = server(&state, None);
    let store = Store::new(&state);
    store.add("any", Some(book.clone()), None).unwrap();

    let said = run_client(&dir, addr, &inbox, None);
    let landed = inbox.join("Celeste Ng/Café Stories - Celeste Ng.epub");
    assert!(landed.is_file(), "{said}");
    assert_eq!(
        std::fs::read(&landed).unwrap(),
        std::fs::read(&book).unwrap()
    );
    assert!(said.contains("ejected"), "{said}");

    // KOReader's calibre search reads this file.
    let meta: serde_json::Value =
        serde_json::from_slice(&std::fs::read(inbox.join(".metadata.calibre")).unwrap()).unwrap();
    let b = &meta[0];
    assert_eq!(b["uuid"], "c0ffee00-0000-4000-8000-000000000001");
    assert_eq!(b["title"], "Café Stories");
    assert_eq!(b["authors"], serde_json::json!(["Celeste Ng"]));
    assert_eq!(b["author_sort"], "Ng, Celeste");

    // Connecting again: the book is listed back, by its details, and nothing is sent twice.
    let said = run_client(&dir, addr, &inbox, None);
    assert!(said.contains("ejected"), "{said}");
    std::thread::sleep(Duration::from_millis(200));
    let r = store.readers();
    let seen = r.readers.values().next().unwrap();
    assert_eq!(seen.books.len(), 1);
    assert_eq!(seen.books[0].title.as_deref(), Some("Café Stories"));
    assert_eq!(seen.name, "KOReader (Kobo_frost)");

    // And off again.
    store
        .add(
            "any",
            None,
            Some("Celeste Ng/Café Stories - Celeste Ng.epub".into()),
        )
        .unwrap();
    run_client(&dir, addr, &inbox, None);
    assert!(!landed.exists());

    drop(stop);
    h.join().unwrap();
}

#[test]
fn koreaders_client_gives_the_password_deckle_asks_for() {
    let Some(dir) = koreader_dir() else { return };
    let tmp = tempfile::tempdir().unwrap();
    let state = tmp.path().join("state");
    let (addr, stop, h) = server(&state, Some("hunter2"));
    let said = run_client(&dir, addr, &tmp.path().join("a"), Some("wrong"));
    assert!(said.contains("invalid password"), "{said}");
    let said = run_client(&dir, addr, &tmp.path().join("b"), Some("hunter2"));
    assert!(
        !said.contains("invalid password") && said.contains("ejected"),
        "{said}"
    );
    drop(stop);
    h.join().unwrap();
}
