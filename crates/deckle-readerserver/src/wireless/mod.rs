// SPDX-License-Identifier: GPL-3.0-or-later
//! The wireless server (docs/superpowers/specs/2026-10-31-calibre-bridge-design.md part A):
//! calibre's "smart device" protocol, so KOReader's built-in *calibre wireless connection*
//! finds Deckle on the home network and takes books from it. A plain TCP listener and a UDP
//! responder, on threads of their own; one reader at a time.

pub mod book;
pub mod frame;
pub mod session;
pub mod store;

use std::io;
use std::net::{IpAddr, SocketAddr, TcpListener, TcpStream, UdpSocket};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use serde::{Deserialize, Serialize};

pub use session::{Device, Library, OnReader, Session};
pub use store::{Job, Readers, Seen, Store};

/// The ports a reader broadcasts to, looking for calibre (`driver.py`'s `BROADCAST_PORTS`).
pub const BROADCAST_PORTS: [u16; 5] = [54982, 48123, 39001, 44044, 59678];
/// calibre's default port for the wireless connection.
pub const DEFAULT_PORT: u16 = 9090;
/// What every broadcast reply starts with: KOReader matches it.
pub const CLIENT_STRING: &str = "calibre wireless device client";

/// How the server runs.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Settings {
    pub bind: String,
    pub port: u16,
    pub discovery: bool,
    pub library: Library,
    pub password: Option<String>,
    /// The other number in a broadcast reply (calibre's content server port): KOReader
    /// needs digits there, though it doesn't use them. The reader server's port, else 0.
    pub content_port: u16,
    /// Where the queue and `readers.json` are.
    pub dir: PathBuf,
    /// A keep-alive is sent after this long without traffic (KOReader gives up at 5 minutes).
    pub keepalive: Duration,
    /// With nothing to do for this long, the session ends.
    pub idle: Duration,
    /// How often the queue is looked at during a session.
    pub poll: Duration,
}

impl Settings {
    pub fn store(&self) -> Store {
        Store::new(&self.dir)
    }
}

/// The broadcast reply: `calibre wireless device client (on <name>);<content port>,<port>`.
pub fn discovery_reply(name: &str, content_port: u16, port: u16) -> String {
    let name: String = name
        .chars()
        .filter(|c| !matches!(c, ')' | ';' | ','))
        .collect();
    format!("{CLIENT_STRING} (on {name});{content_port},{port}")
}

/// Whether `ip` is on a home network: private, link-local or loopback. Anything else is
/// refused (§7).
pub fn is_private(ip: IpAddr) -> bool {
    match ip {
        IpAddr::V4(v4) => v4.is_private() || v4.is_loopback() || v4.is_link_local(),
        IpAddr::V6(v6) => {
            if let Some(v4) = v6.to_ipv4_mapped() {
                return is_private(IpAddr::V4(v4));
            }
            let first = v6.segments()[0];
            v6.is_loopback() || (first & 0xfe00) == 0xfc00 || (first & 0xffc0) == 0xfe80
        }
    }
}

/// `wireless.json` beside `digest.toml`, owner-only: the password a reader must give (kept as
/// is, since calibre's challenge needs it) and the id Deckle's library goes by.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct Secrets {
    pub password: Option<String>,
    pub library_uuid: String,
}

pub fn secrets_path(config: &Path) -> PathBuf {
    config.with_file_name("wireless.json")
}

/// The saved secrets, with a library id made and kept the first time.
pub fn load_secrets(path: &Path) -> Secrets {
    let mut s: Secrets = std::fs::read(path)
        .ok()
        .and_then(|b| serde_json::from_slice(&b).ok())
        .unwrap_or_default();
    if s.library_uuid.is_empty() {
        s.library_uuid = uuid::Uuid::new_v4().to_string();
        if let Err(e) = save_secrets(path, &s) {
            eprintln!("wireless: {}: {e}", path.display());
        }
    }
    s
}

pub fn save_secrets(path: &Path, s: &Secrets) -> io::Result<()> {
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir)?;
    }
    let tmp = path.with_extension("json.tmp");
    crate::auth::write_private(&tmp, &serde_json::to_vec_pretty(s)?)?;
    std::fs::rename(tmp, path)
}

/// Answers broadcasts on every port it can bind until `stop`. Returns how many it bound;
/// with none (calibre holds them), there's no thread.
fn discovery(settings: &Settings, stop: Arc<AtomicBool>) -> usize {
    let sockets: Vec<UdpSocket> = BROADCAST_PORTS
        .iter()
        .filter_map(|p| UdpSocket::bind(("0.0.0.0", *p)).ok())
        .filter(|s| s.set_nonblocking(true).is_ok())
        .collect();
    let n = sockets.len();
    if n == 0 {
        return 0;
    }
    let reply = discovery_reply(&settings.library.name, settings.content_port, settings.port);
    std::thread::spawn(move || {
        let mut buf = [0u8; 128];
        while !stop.load(Ordering::Relaxed) {
            let mut quiet = true;
            for s in &sockets {
                if let Ok((_, from)) = s.recv_from(&mut buf) {
                    quiet = false;
                    if is_private(from.ip()) {
                        let _ = s.send_to(reply.as_bytes(), from);
                    }
                }
            }
            if quiet {
                std::thread::sleep(Duration::from_millis(100));
            }
        }
    });
    n
}

/// Serves until `stop` is sent (or dropped): the watcher's thread and `deckle wireless serve`.
pub fn serve_blocking(settings: Settings, stop: std::sync::mpsc::Receiver<()>) -> io::Result<()> {
    let addr: SocketAddr = format!("{}:{}", settings.bind, settings.port)
        .parse()
        .map_err(|e| io::Error::new(io::ErrorKind::InvalidInput, e))?;
    let listener = TcpListener::bind(addr)?;
    serve_on(listener, settings, stop)
}

/// [`serve_blocking`] on a listener already bound (tests bind port 0).
pub fn serve_on(
    listener: TcpListener,
    settings: Settings,
    stop: std::sync::mpsc::Receiver<()>,
) -> io::Result<()> {
    listener.set_nonblocking(true)?;
    let store = settings.store();
    let stopping = Arc::new(AtomicBool::new(false));
    let answered = settings.discovery && discovery(&settings, stopping.clone()) > 0;
    store.update(|r| {
        r.discovery = answered;
        for seen in r.readers.values_mut() {
            seen.connected = false;
        }
    });
    if settings.discovery && !answered {
        store.log(format!(
            "can't answer broadcasts (is calibre's wireless server on?): type this computer's \
             address and port {} into the reader",
            settings.port
        ));
    }
    let active: Arc<Mutex<Option<String>>> = Arc::new(Mutex::new(None));
    let mut sessions: Vec<std::thread::JoinHandle<()>> = Vec::new();
    while let Err(std::sync::mpsc::TryRecvError::Empty) = stop.try_recv() {
        match listener.accept() {
            Ok((mut stream, peer)) => {
                if !is_private(peer.ip()) {
                    continue;
                }
                let busy = active.lock().map(|a| a.clone()).unwrap_or(None);
                if let Some(other) = busy {
                    let _ = stream.set_nonblocking(false);
                    let _ = session::busy(&mut stream, &other);
                    continue;
                }
                if let Ok(mut a) = active.lock() {
                    *a = Some("a reader".into());
                }
                let (s, active, stopping) = (settings.clone(), active.clone(), stopping.clone());
                sessions.push(std::thread::spawn(move || {
                    run(stream, &s, &active, &stopping);
                    free(&active);
                }));
                sessions.retain(|h| !h.is_finished());
            }
            Err(e) if e.kind() == io::ErrorKind::WouldBlock => {
                std::thread::sleep(Duration::from_millis(50));
            }
            Err(e) => {
                store.log(format!("accept: {e}"));
                std::thread::sleep(Duration::from_millis(500));
            }
        }
    }
    stopping.store(true, Ordering::Relaxed);
    for h in sessions {
        let _ = h.join();
    }
    Ok(())
}

fn now() -> String {
    chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Secs, false)
}

/// One reader's session: handshake, what it holds, then its jobs until it goes or goes quiet.
fn run(
    stream: TcpStream,
    settings: &Settings,
    active: &Mutex<Option<String>>,
    stopping: &AtomicBool,
) {
    let store = settings.store();
    let _ = stream.set_nonblocking(false);
    let _ = stream.set_read_timeout(Some(Duration::from_secs(60)));
    let _ = stream.set_write_timeout(Some(Duration::from_secs(60)));
    let peer = stream
        .peer_addr()
        .map(|a| a.ip().to_string())
        .unwrap_or_default();
    let mut s = Session::new(stream);
    let t = now();
    let device = match s.handshake(&settings.library, settings.password.as_deref(), &t, &t) {
        Ok(d) => d.clone(),
        Err(e) => {
            free(active);
            store.log(format!("a reader at {peer} couldn't connect: {e}"));
            return;
        }
    };
    if let Ok(mut a) = active.lock() {
        *a = Some(device.name.clone());
    }
    store.log(format!("{} connected from {peer}", device.name));
    let ended = match session_loop(&mut s, &device, settings, &store, stopping) {
        Ok(why) => why.to_owned(),
        Err(session::Error::HungUp) => "the reader hung up".to_owned(),
        // A reader that closes mid-exchange (Wi-Fi off, sleep) is hanging up too.
        Err(session::Error::Io(e))
            if matches!(
                e.kind(),
                io::ErrorKind::ConnectionReset
                    | io::ErrorKind::ConnectionAborted
                    | io::ErrorKind::BrokenPipe
                    | io::ErrorKind::UnexpectedEof
            ) =>
        {
            "the reader hung up".to_owned()
        }
        Err(e) => format!("lost: {e}"),
    };
    // Another reader may connect from here on; the connection itself closes when `s` goes.
    free(active);
    store.update(|r| {
        if let Some(seen) = r.readers.get_mut(&device.store_uuid) {
            seen.connected = false;
            seen.last_seen = now();
        }
    });
    store.log(format!("{} disconnected ({ended})", device.name));
}

fn free(active: &Mutex<Option<String>>) {
    if let Ok(mut a) = active.lock() {
        *a = None;
    }
}

/// Reads what the reader holds and records it.
fn refresh(
    s: &mut Session<TcpStream>,
    device: &Device,
    store: &Store,
) -> Result<(), session::Error> {
    let before = store.readers().readers.get(&device.store_uuid).cloned();
    let known = |b: &OnReader| {
        before.as_ref().is_some_and(|p| {
            p.books
                .iter()
                .any(|x| x.lpath == b.lpath && x.title.is_some())
        })
    };
    let mut books = s.books(known)?;
    // Fill in what was already known about books whose details weren't asked for again.
    if let Some(p) = &before {
        for b in books.iter_mut().filter(|b| b.title.is_none()) {
            if let Some(old) = p.books.iter().find(|x| x.lpath == b.lpath) {
                *b = OnReader {
                    lpath: b.lpath.clone(),
                    uuid: b.uuid.clone().or(old.uuid.clone()),
                    last_modified: b.last_modified.clone().or(old.last_modified.clone()),
                    ..old.clone()
                };
            }
        }
    }
    books.sort_by(|a, b| a.lpath.cmp(&b.lpath));
    let free = s.free_space().ok();
    store.update(|r| {
        // What was learned before (its inbox, Deckle's collections) stays.
        let kept = r.readers.remove(&device.store_uuid).unwrap_or_default();
        r.readers.insert(
            device.store_uuid.clone(),
            Seen {
                name: device.name.clone(),
                kind: device.kind.clone(),
                app: device.app.clone(),
                connected: true,
                last_seen: now(),
                free,
                books,
                ..kept
            },
        );
    });
    Ok(())
}

fn session_loop(
    s: &mut Session<TcpStream>,
    device: &Device,
    settings: &Settings,
    store: &Store,
    stopping: &AtomicBool,
) -> Result<&'static str, session::Error> {
    refresh(s, device, store)?;
    shelve(s, device, store)?;
    let mut last_traffic = Instant::now();
    let mut last_work = Instant::now();
    loop {
        if stopping.load(Ordering::Relaxed) {
            s.eject()?;
            return Ok("Deckle stopped");
        }
        let jobs: Vec<Job> = store
            .jobs()
            .into_iter()
            .filter(|j| j.is_for(device))
            .collect();
        if !jobs.is_empty() {
            do_jobs(s, device, store, &jobs)?;
            refresh(s, device, store)?;
            last_traffic = Instant::now();
            last_work = Instant::now();
            continue;
        }
        if last_work.elapsed() >= settings.idle {
            s.eject()?;
            return Ok("nothing to do");
        }
        if last_traffic.elapsed() >= settings.keepalive {
            s.noop()?;
            last_traffic = Instant::now();
        }
        // The reader speaks unasked only to hang up (a NOOP, then it closes).
        if hung_up(s.stream(), settings.poll)? {
            return Ok("the reader hung up");
        }
    }
}

/// Waits up to `wait` for the reader to say anything; true when it hung up.
fn hung_up(stream: &mut TcpStream, wait: Duration) -> io::Result<bool> {
    stream.set_read_timeout(Some(wait.max(Duration::from_millis(1))))?;
    let mut b = [0u8; 1];
    let r = stream.peek(&mut b);
    stream.set_read_timeout(Some(Duration::from_secs(60)))?;
    match r {
        Ok(_) => Ok(true),
        Err(e)
            if matches!(
                e.kind(),
                io::ErrorKind::WouldBlock | io::ErrorKind::TimedOut
            ) =>
        {
            Ok(false)
        }
        Err(e) => Err(e),
    }
}

/// How long a reader has to answer for its collections: a KOReader too old to know the
/// request never does.
const COLLECTIONS_WAIT: Duration = Duration::from_secs(5);

/// The inbox a reader's collections show: the start of a path that ends in a known `lpath`.
fn inbox_from(collections: &serde_json::Value, books: &[OnReader]) -> Option<String> {
    let lpaths: Vec<&str> = books.iter().map(|b| b.lpath.as_str()).collect();
    collections
        .as_object()?
        .values()
        .flat_map(|v| v.as_array().into_iter().flatten())
        .find_map(|p| {
            let p = p.as_str()?;
            lpaths.iter().find_map(|l| {
                p.strip_suffix(l)
                    .filter(|pre| pre.ends_with('/'))
                    .map(str::to_owned)
            })
        })
}

/// calibre's shelves as KOReader collections (calibre bridge design §6.2), when the watcher
/// has rules and the reader can say what it has.
fn shelve(
    s: &mut Session<TcpStream>,
    device: &Device,
    store: &Store,
) -> Result<(), session::Error> {
    use deckle_core::shelves::{OnDrive, Shelves, diff, wanted};
    let sets = store.shelves();
    let Some(seen) = store.readers().readers.get(&device.store_uuid).cloned() else {
        return Ok(());
    };
    if sets.is_empty() || seen.no_collections {
        return Ok(());
    }
    s.stream().set_read_timeout(Some(COLLECTIONS_WAIT))?;
    let asked = s.collections();
    s.stream().set_read_timeout(Some(Duration::from_secs(60)))?;
    let collections = match asked {
        Ok(c) => c,
        Err(session::Error::Io(e))
            if matches!(
                e.kind(),
                io::ErrorKind::WouldBlock | io::ErrorKind::TimedOut
            ) =>
        {
            store.update(|r| {
                if let Some(x) = r.readers.get_mut(&device.store_uuid) {
                    x.no_collections = true;
                }
            });
            store.log(format!(
                "{} is too old for collections over Wi-Fi",
                device.name
            ));
            // It may still answer late, so this session can't be trusted to stay in step.
            return Err(session::Error::Protocol("no answer for collections".into()));
        }
        Err(e) => return Err(e),
    };
    let Some(inbox) = inbox_from(&collections, &seen.books).or(seen.inbox.clone()) else {
        store.log(format!(
            "{}: can't tell where KOReader's calibre folder is yet; add any book Deckle sent to a \
             collection once, and the shelves follow",
            device.name
        ));
        return Ok(());
    };
    let key = format!("koreader/{}", device.store_uuid);
    let books: Vec<OnDrive> = seen
        .books
        .iter()
        .filter_map(|b| {
            Some(OnDrive {
                path: b.lpath.clone(),
                calibre_id: b.uuid.clone()?,
            })
        })
        .collect();
    let want = wanted(&sets, &key, &books);
    let mut current = Shelves::new();
    for (name, files) in collections.as_object().into_iter().flatten() {
        let list = current.entry(name.clone()).or_default();
        for f in files
            .as_array()
            .into_iter()
            .flatten()
            .filter_map(|f| f.as_str())
        {
            if let Some(l) = f.strip_prefix(&inbox) {
                list.insert(l.to_owned());
            }
        }
    }
    let d = diff(&want, &current, &seen.collections);
    if !d.is_empty() {
        let abs = |m: &Shelves| -> std::collections::BTreeMap<String, Vec<String>> {
            m.iter()
                .map(|(n, b)| (n.clone(), b.iter().map(|l| format!("{inbox}{l}")).collect()))
                .collect()
        };
        s.update_collections(&d.create, &d.delete, &abs(&d.add), &abs(&d.remove))?;
        let (added, removed) = d.counts();
        store.log(format!(
            "{}: collections from calibre, {added} added, {removed} taken off",
            device.name
        ));
    }
    store.update(|r| {
        if let Some(x) = r.readers.get_mut(&device.store_uuid) {
            x.inbox = Some(inbox.clone());
            let mut ours = x.collections.clone();
            ours.extend(d.create.iter().cloned());
            for g in &d.delete {
                ours.remove(g);
            }
            ours.retain(|n| want.contains_key(n));
            x.collections = ours;
        }
    });
    Ok(())
}

fn do_jobs(
    s: &mut Session<TcpStream>,
    device: &Device,
    store: &Store,
    jobs: &[Job],
) -> Result<(), session::Error> {
    let removes: Vec<&Job> = jobs.iter().filter(|j| j.remove.is_some()).collect();
    if !removes.is_empty() {
        let lpaths: Vec<String> = removes.iter().filter_map(|j| j.remove.clone()).collect();
        s.delete(&lpaths)?;
        for j in &removes {
            store.finish(j);
        }
        store.log(format!("took {} off {}", lpaths.join(", "), device.name));
    }
    let sends: Vec<&Job> = jobs.iter().filter(|j| j.send.is_some()).collect();
    let total = sends.len();
    for (i, j) in sends.into_iter().enumerate() {
        let Some(path) = &j.send else { continue };
        let book = match book::from_file(path, &now()) {
            Ok(b) => b,
            Err(e) => {
                store.finish(j);
                store.log(format!("can't send {}: {e}", path.display()));
                continue;
            }
        };
        let ext = book.lpath.rsplit('.').next().unwrap_or("").to_owned();
        if !device.extensions.contains(&ext) {
            store.finish(j);
            store.log(format!(
                "{} doesn't take .{ext} files: {} not sent",
                device.name, book.title
            ));
            continue;
        }
        match s.send_book(&book.lpath, book.metadata.clone(), &book.bytes, i, total) {
            Ok(at) => {
                store.finish(j);
                store.log(format!("sent {} to {} ({at})", book.title, device.name));
            }
            // The reader refused this book (space, a name it won't take): the next can go.
            Err(session::Error::Protocol(why)) => {
                store.finish(j);
                store.log(format!("{} refused {}: {why}", device.name, book.title));
            }
            Err(e) => return Err(e),
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_broadcast_reply_is_what_koreader_matches() {
        // KOReader: dgram:match("calibre wireless device client %(on (.-)%);(%d+),(%d+)$")
        assert_eq!(
            discovery_reply("Deckle", 47220, 9090),
            "calibre wireless device client (on Deckle);47220,9090"
        );
        assert_eq!(
            discovery_reply("a;b,c)", 0, 9090),
            "calibre wireless device client (on abc);0,9090"
        );
    }

    #[test]
    fn only_home_network_peers_are_taken() {
        for ok in [
            "192.168.1.5",
            "10.0.0.2",
            "172.16.4.4",
            "127.0.0.1",
            "169.254.1.1",
            "::1",
            "fd00::1",
            "fe80::1",
            "::ffff:192.168.1.9",
        ] {
            assert!(is_private(ok.parse().unwrap()), "{ok}");
        }
        for no in ["8.8.8.8", "2001:4860::8888", "::ffff:8.8.8.8", "100.64.0.1"] {
            assert!(!is_private(no.parse().unwrap()), "{no}");
        }
    }
}
