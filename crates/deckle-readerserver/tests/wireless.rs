// SPDX-License-Identifier: GPL-3.0-or-later
//! The wireless server against a stand-in for KOReader's calibre plugin
//! (`plugins/calibre.koplugin/wireless.lua`), answering as it does.

use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};
use std::path::{Path, PathBuf};
use std::sync::mpsc;
use std::time::{Duration, Instant};

use deckle_readerserver::wireless::frame::{self, Op};
use deckle_readerserver::wireless::{Library, Settings, Store, serve_on, session};
use serde_json::{Value, json};

/// KOReader's half of the protocol, after `wireless.lua`.
struct Koreader {
    inbox: PathBuf,
    password: Option<String>,
    store_uuid: Option<String>,
    books: Vec<Value>,
    /// Books it says no to (as when it's out of space).
    refuse: Vec<String>,
    /// What the server asked, by opcode.
    asked: Vec<u64>,
    /// `NOOP {priKey}` requests.
    pri_keys: usize,
    told_bad_password: bool,
    busy: Option<String>,
    ejected: bool,
}

impl Koreader {
    fn new(inbox: &Path) -> Self {
        Koreader {
            inbox: inbox.to_path_buf(),
            password: None,
            store_uuid: None,
            books: Vec::new(),
            refuse: Vec::new(),
            asked: Vec::new(),
            pri_keys: 0,
            told_bad_password: false,
            busy: None,
            ejected: false,
        }
    }

    fn ok(s: &mut TcpStream, v: Value) {
        frame::send(s, Op::Ok, &v).unwrap();
    }

    /// Talks until the server closes, or `until` says to hang up (then sends a NOOP and goes,
    /// as KOReader's disconnect does).
    fn run(&mut self, addr: std::net::SocketAddr, until: impl Fn(&Koreader) -> bool) {
        let mut s = TcpStream::connect(addr).unwrap();
        s.set_read_timeout(Some(Duration::from_millis(200)))
            .unwrap();
        let deadline = Instant::now() + Duration::from_secs(20);
        loop {
            assert!(Instant::now() < deadline, "the session never ended");
            if until(self) {
                frame::send(&mut s, Op::Noop, &json!({})).unwrap();
                return;
            }
            let (code, arg) = match frame::read(&mut s) {
                Ok(m) => m,
                Err(e)
                    if matches!(
                        e.kind(),
                        std::io::ErrorKind::WouldBlock | std::io::ErrorKind::TimedOut
                    ) =>
                {
                    continue;
                }
                Err(_) => return,
            };
            self.asked.push(code);
            match Op::from_code(code).unwrap() {
                Op::GetInitializationInfo => {
                    let challenge = arg["passwordChallenge"].as_str().unwrap_or("");
                    let hash = match (&self.password, challenge) {
                        (Some(p), c) if !c.is_empty() => session::password_hash(p, c),
                        _ => String::new(),
                    };
                    Self::ok(
                        &mut s,
                        json!({
                            "appName": "KOReader",
                            "acceptedExtensions": ["epub", "pdf", "txt", "cbz"],
                            "cacheUsesLpaths": true,
                            "canAcceptLibraryInfo": true,
                            "canDeleteMultipleBooks": true,
                            "canReceiveBookBinary": true,
                            "canSendOkToSendbook": true,
                            "canStreamBooks": true,
                            "canStreamMetadata": true,
                            "canUseCachedMetadata": true,
                            "ccVersionNumber": "v2026.10",
                            "coverHeight": 240,
                            "deviceKind": "Kobo_frost",
                            "deviceName": "KOReader (Kobo_frost)",
                            "extensionPathLengths": {"epub": 4},
                            "passwordHash": hash,
                            "maxBookContentPacketLen": 4096,
                            "useUuidFileNames": false,
                            "versionOK": true,
                        }),
                    );
                }
                Op::GetDeviceInformation => Self::ok(
                    &mut s,
                    json!({
                        "device_info": {"device_store_uuid": self.store_uuid, "device_name": "KOReader (Kobo_frost)"},
                        "version": "v2026.10",
                        "device_version": "v2026.10",
                    }),
                ),
                Op::SetCalibreDeviceInfo => {
                    self.store_uuid = arg["device_store_uuid"].as_str().map(str::to_owned);
                    Self::ok(&mut s, json!({}));
                }
                Op::FreeSpace => Self::ok(&mut s, json!({"free_space_on_device": 123456789u64})),
                Op::SetLibraryInfo => Self::ok(&mut s, json!({})),
                Op::GetBookCount => {
                    Self::ok(
                        &mut s,
                        json!({"willStream": true, "willScan": true, "count": self.books.len()}),
                    );
                    for (i, b) in self.books.iter().enumerate() {
                        Self::ok(
                            &mut s,
                            json!({"priKey": i + 1, "uuid": b["uuid"], "lpath": b["lpath"], "last_modified": b["last_modified"]}),
                        );
                    }
                }
                Op::Noop => {
                    if arg["ejecting"].as_bool() == Some(true) {
                        Self::ok(&mut s, json!({}));
                        self.ejected = true;
                        return;
                    } else if arg.get("count").is_some() {
                    } else if let Some(k) = arg["priKey"].as_u64() {
                        self.pri_keys += 1;
                        Self::ok(&mut s, self.books[k as usize - 1].clone());
                    } else {
                        Self::ok(&mut s, json!({}));
                    }
                }
                Op::SendBook => {
                    let lpath = arg["lpath"].as_str().unwrap().to_owned();
                    assert!(!lpath.starts_with('/') && !lpath.split('/').any(|c| c == ".."));
                    let len = arg["length"].as_u64().unwrap() as usize;
                    // Refused, KOReader answers ERROR and stays reading JSON: no bytes may follow
                    // (calibre stops there too).
                    if self.refuse.iter().any(|r| lpath.contains(r.as_str())) {
                        frame::send(
                            &mut s,
                            Op::Error,
                            &json!({"message": "No space left on device"}),
                        )
                        .unwrap();
                        continue;
                    }
                    Self::ok(&mut s, json!({}));
                    let mut bytes = vec![0u8; len];
                    s.set_read_timeout(Some(Duration::from_secs(5))).unwrap();
                    s.read_exact(&mut bytes).unwrap();
                    s.set_read_timeout(Some(Duration::from_millis(200)))
                        .unwrap();
                    let to = self.inbox.join(&lpath);
                    std::fs::create_dir_all(to.parent().unwrap()).unwrap();
                    std::fs::write(&to, &bytes).unwrap();
                    let mut m = arg["metadata"].clone();
                    m["lpath"] = json!(lpath);
                    self.books.push(m);
                }
                Op::DeleteBook => {
                    Self::ok(&mut s, json!({}));
                    for l in arg["lpaths"].as_array().unwrap() {
                        let l = l.as_str().unwrap();
                        let uuid = self
                            .books
                            .iter()
                            .find(|b| b["lpath"] == l)
                            .map(|b| b["uuid"].clone())
                            .unwrap_or(Value::Null);
                        let _ = std::fs::remove_file(self.inbox.join(l));
                        self.books.retain(|b| b["lpath"] != l);
                        Self::ok(&mut s, json!({"uuid": uuid}));
                    }
                }
                Op::DisplayMessage => {
                    if arg["messageKind"] == 1 {
                        self.told_bad_password = true;
                    }
                }
                Op::CalibreBusy => {
                    self.busy = arg["otherDevice"].as_str().map(str::to_owned);
                    return;
                }
                other => panic!("KOReader doesn't expect {other:?}"),
            }
        }
    }
}

struct Server {
    addr: std::net::SocketAddr,
    stop: Option<mpsc::Sender<()>>,
    handle: Option<std::thread::JoinHandle<()>>,
    store: Store,
}

impl Drop for Server {
    fn drop(&mut self) {
        drop(self.stop.take());
        if let Some(h) = self.handle.take() {
            let _ = h.join();
        }
    }
}

fn server(dir: &Path, password: Option<&str>, idle: Duration) -> Server {
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
        keepalive: Duration::from_millis(300),
        idle,
        poll: Duration::from_millis(30),
    };
    let (stop, rx) = mpsc::channel();
    let handle = std::thread::spawn(move || serve_on(listener, settings, rx).unwrap());
    Server {
        addr,
        stop: Some(stop),
        handle: Some(handle),
        store: Store::new(dir),
    }
}

/// Waits for the server to finish with the last session, so the next connection isn't told
/// it's busy.
fn wait_disconnected(store: &Store) {
    let deadline = Instant::now() + Duration::from_secs(10);
    while Instant::now() < deadline {
        let r = store.readers();
        if !r.readers.is_empty()
            && r.readers.values().all(|s| !s.connected)
            && r.log.last().is_some_and(|l| l.contains("disconnected"))
        {
            std::thread::sleep(Duration::from_millis(50));
            return;
        }
        std::thread::sleep(Duration::from_millis(20));
    }
    panic!("the session never ended: {:?}", store.readers().log);
}

fn epub(path: &Path, title: &str, author: &str, calibre_id: Option<&str>) {
    let mut z = zip::ZipWriter::new(std::fs::File::create(path).unwrap());
    let stored =
        zip::write::SimpleFileOptions::default().compression_method(zip::CompressionMethod::Stored);
    z.start_file("mimetype", stored).unwrap();
    z.write_all(b"application/epub+zip").unwrap();
    z.start_file("META-INF/container.xml", stored).unwrap();
    z.write_all(br#"<?xml version="1.0"?><container version="1.0" xmlns="urn:oasis:names:tc:opendocument:xmlns:container"><rootfiles><rootfile full-path="OEBPS/content.opf" media-type="application/oebps-package+xml"/></rootfiles></container>"#).unwrap();
    let id = calibre_id
        .map(|u| {
            format!(r#"<dc:identifier opf:scheme="calibre" id="calibre_id">{u}</dc:identifier>"#)
        })
        .unwrap_or_default();
    z.start_file("OEBPS/content.opf", stored).unwrap();
    write!(
        z,
        r#"<?xml version="1.0"?><package xmlns="http://www.idpf.org/2007/opf" xmlns:opf="http://www.idpf.org/2007/opf" version="2.0" unique-identifier="id"><metadata xmlns:dc="http://purl.org/dc/elements/1.1/"><dc:title>{title}</dc:title><dc:creator>{author}</dc:creator><dc:language>en</dc:language><dc:identifier id="id">urn:x</dc:identifier>{id}<dc:subject>Sci-Fi</dc:subject></metadata><manifest><item id="c" href="c.xhtml" media-type="application/xhtml+xml"/></manifest><spine><itemref idref="c"/></spine></package>"#
    )
    .unwrap();
    z.start_file("OEBPS/c.xhtml", stored).unwrap();
    z.write_all(b"<html xmlns=\"http://www.w3.org/1999/xhtml\"><body><p>Hi</p></body></html>")
        .unwrap();
    z.finish().unwrap();
}

#[test]
fn a_queued_book_lands_in_koreaders_inbox_and_the_reader_is_recorded() {
    let tmp = tempfile::tempdir().unwrap();
    let inbox = tmp.path().join("inbox");
    std::fs::create_dir_all(&inbox).unwrap();
    let book = tmp.path().join("dune.epub");
    epub(&book, "Dune", "Frank Herbert", Some("aaaa-calibre-uuid"));
    let srv = server(&tmp.path().join("state"), None, Duration::from_secs(30));
    srv.store.add("any", Some(book.clone()), None).unwrap();

    let mut ko = Koreader::new(&inbox);
    let landed = inbox.join("Frank Herbert/Dune - Frank Herbert.epub");
    ko.run(srv.addr, |_| landed.is_file());

    assert_eq!(
        std::fs::read(&landed).unwrap(),
        std::fs::read(&book).unwrap()
    );
    let sent = &ko.books[0];
    assert_eq!(sent["uuid"], "aaaa-calibre-uuid");
    assert_eq!(sent["title"], "Dune");
    assert_eq!(sent["authors"], json!(["Frank Herbert"]));
    assert_eq!(sent["tags"], json!(["Sci-Fi"]));
    assert!(srv.store.jobs().is_empty());
    // The reader keeps the id Deckle gave its storage.
    let uuid = ko.store_uuid.clone().unwrap();
    wait_disconnected(&srv.store);
    let r = srv.store.readers();
    let seen = &r.readers[&uuid];
    assert_eq!(seen.name, "KOReader (Kobo_frost)");
    assert_eq!(seen.free, Some(123456789));
    assert!(!seen.connected, "it hung up");
    assert!(
        r.log.iter().any(|l| l.contains("sent Dune to KOReader")),
        "{:?}",
        r.log
    );
    assert!(r.log.iter().any(|l| l.contains("hung up")), "{:?}", r.log);
}

#[test]
fn a_second_connection_while_the_first_is_reconnecting_reads_details_only_for_new_books() {
    let tmp = tempfile::tempdir().unwrap();
    let inbox = tmp.path().join("inbox");
    let srv = server(&tmp.path().join("state"), None, Duration::from_secs(30));
    let mut ko = Koreader::new(&inbox);
    ko.books.push(json!({"uuid": "u1", "lpath": "A/One - A.epub", "title": "One", "authors": ["A"], "last_modified": "2026-01-01T00:00:00+00:00"}));
    ko.run(srv.addr, |k| k.asked.contains(&(Op::FreeSpace as u64)));
    assert_eq!(ko.pri_keys, 1, "the first time, its details are asked for");
    wait_disconnected(&srv.store);
    let r = srv.store.readers();
    assert_eq!(
        r.readers.values().next().unwrap().books[0].title.as_deref(),
        Some("One")
    );

    wait_disconnected(&srv.store);
    ko.asked.clear();
    ko.books.push(json!({"uuid": "u2", "lpath": "B/Two - B.epub", "title": "Two", "authors": ["B"], "last_modified": "2026-01-01T00:00:00+00:00"}));
    ko.run(srv.addr, |k| k.asked.contains(&(Op::FreeSpace as u64)));
    assert_eq!(ko.pri_keys, 2, "only the new book's details");
    wait_disconnected(&srv.store);
    let books = srv
        .store
        .readers()
        .readers
        .values()
        .next()
        .unwrap()
        .books
        .clone();
    let titles: Vec<_> = books.iter().map(|b| b.title.clone().unwrap()).collect();
    assert_eq!(titles, ["One", "Two"]);
}

#[test]
fn another_reader_is_told_calibre_is_busy() {
    let tmp = tempfile::tempdir().unwrap();
    let srv = server(&tmp.path().join("state"), None, Duration::from_secs(30));
    let first_inbox = tmp.path().join("a");
    let addr = srv.addr;
    let (tx, rx) = mpsc::channel::<()>();
    let first = std::thread::spawn(move || {
        let mut ko = Koreader::new(&first_inbox);
        ko.run(addr, |_| rx.try_recv().is_ok());
    });
    std::thread::sleep(Duration::from_millis(500));
    let mut second = Koreader::new(&tmp.path().join("b"));
    second.run(addr, |_| false);
    assert_eq!(second.busy.as_deref(), Some("KOReader (Kobo_frost)"));
    tx.send(()).unwrap();
    first.join().unwrap();
}

#[test]
fn a_wrong_password_is_told_to_the_reader_and_nothing_else_happens() {
    let tmp = tempfile::tempdir().unwrap();
    let srv = server(
        &tmp.path().join("state"),
        Some("secret"),
        Duration::from_secs(30),
    );
    let mut ko = Koreader::new(&tmp.path().join("inbox"));
    ko.password = Some("wrong".into());
    ko.run(srv.addr, |_| false);
    assert!(ko.told_bad_password);
    assert!(!ko.asked.contains(&(Op::GetBookCount as u64)));
    std::thread::sleep(Duration::from_millis(200));
    assert!(
        srv.store
            .readers()
            .log
            .iter()
            .any(|l| l.contains("password"))
    );

    let mut ok = Koreader::new(&tmp.path().join("inbox"));
    ok.password = Some("secret".into());
    ok.run(srv.addr, |k| k.asked.contains(&(Op::FreeSpace as u64)));
    assert!(!ok.told_bad_password);
}

#[test]
fn a_refused_book_is_logged_and_the_next_still_goes_then_removal_and_idle_eject() {
    let tmp = tempfile::tempdir().unwrap();
    let inbox = tmp.path().join("inbox");
    let big = tmp.path().join("big.epub");
    let small = tmp.path().join("small.epub");
    epub(&big, "Big", "Ann", None);
    epub(&small, "Small", "Bo", None);
    let srv = server(&tmp.path().join("state"), None, Duration::from_millis(1500));
    srv.store
        .add("KOReader (Kobo_frost)", Some(big), None)
        .unwrap();
    srv.store
        .add("KOReader (Kobo_frost)", Some(small), None)
        .unwrap();
    srv.store
        .add("someone else", Some(tmp.path().join("nope.epub")), None)
        .unwrap();

    let mut ko = Koreader::new(&inbox);
    ko.refuse.push("Big".into());
    ko.run(srv.addr, |_| false);
    assert!(ko.ejected, "nothing to do: the server ejects");
    assert!(inbox.join("Bo/Small - Bo.epub").is_file());
    assert!(!inbox.join("Ann/Big - Ann.epub").exists());
    let log = srv.store.readers().log;
    assert!(
        log.iter()
            .any(|l| l.contains("refused Big") && l.contains("No space")),
        "{log:?}"
    );
    assert_eq!(srv.store.jobs().len(), 1, "another reader's job waits");

    // Next time, take it off again.
    srv.store
        .add("any", None, Some("Bo/Small - Bo.epub".into()))
        .unwrap();
    ko.ejected = false;
    ko.run(srv.addr, |_| false);
    assert!(!inbox.join("Bo/Small - Bo.epub").exists());
    assert!(ko.books.is_empty());
}

#[test]
fn a_reader_that_isnt_calibres_kind_is_closed_without_a_word() {
    let tmp = tempfile::tempdir().unwrap();
    let srv = server(&tmp.path().join("state"), None, Duration::from_secs(30));
    let mut s = TcpStream::connect(srv.addr).unwrap();
    // Read the server's first request, then answer with junk.
    s.set_read_timeout(Some(Duration::from_secs(5))).unwrap();
    let (op, _) = frame::read(&mut s).unwrap();
    assert_eq!(op, Op::GetInitializationInfo as u64);
    s.write_all(b"HTTP/1.1 400 Bad Request\r\n\r\n").unwrap();
    let mut rest = Vec::new();
    let _ = s.read_to_end(&mut rest);
    std::thread::sleep(Duration::from_millis(200));
    assert!(
        srv.store
            .readers()
            .log
            .iter()
            .any(|l| l.contains("couldn't connect"))
    );
}
