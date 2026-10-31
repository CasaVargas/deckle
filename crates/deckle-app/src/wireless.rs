// SPDX-License-Identifier: GPL-3.0-or-later
//! calibre's wireless connection in the app (calibre bridge design §4.4): the readers that have
//! connected, what they hold, what's queued for them, and the password. The on switch, port and
//! paper setting are `[wireless]`, saved with the Daily settings; the watcher serves it.

use std::path::{Path, PathBuf};

use deckle_readerserver::wireless::{self, Job, OnReader, Store};
use serde::Serialize;

/// The queue and `readers.json`: the CLI's `wireless::store()`, beside the readership state.
fn store() -> Store {
    Store::new(deckle_readership::state_path().with_file_name("wireless"))
}

/// A reader as last seen.
#[derive(Clone, Debug, Serialize)]
pub struct ReaderView {
    /// Its `store_uuid`: what a send names it by.
    pub id: String,
    pub name: String,
    pub kind: String,
    pub connected: bool,
    pub last_seen: String,
    pub free: Option<u64>,
    pub books: Vec<OnReader>,
}

#[derive(Clone, Debug, Serialize)]
pub struct WirelessView {
    /// `[wireless] enabled`.
    pub enabled: bool,
    pub password_set: bool,
    /// What to type into a reader that doesn't find Deckle itself: `address:port`.
    pub address: String,
    /// Broadcasts are answered this run (false: calibre has the ports, or it isn't running).
    pub discovery: bool,
    /// Connected first, then the most lately seen.
    pub readers: Vec<ReaderView>,
    pub queue: Vec<Job>,
    /// The last few things that happened, newest last.
    pub log: Vec<String>,
}

fn view_at(main: &Path, store: &Store) -> WirelessView {
    let cfg = deckle_digest::Config::load(main)
        .map(|c| c.wireless)
        .unwrap_or_default();
    let port = cfg.port;
    let secrets = wireless::load_secrets(&wireless::secrets_path(main));
    let r = store.readers();
    let mut readers: Vec<ReaderView> = r
        .readers
        .into_iter()
        .map(|(id, s)| ReaderView {
            id,
            name: s.name,
            kind: s.kind,
            connected: s.connected,
            last_seen: s.last_seen,
            free: s.free,
            books: s.books,
        })
        .collect();
    readers.sort_by(|a, b| {
        b.connected
            .cmp(&a.connected)
            .then_with(|| b.last_seen.cmp(&a.last_seen))
    });
    let host = deckle_readerserver::lan_address()
        .map(|ip| ip.to_string())
        .unwrap_or_else(|| "this computer's address".into());
    let keep = r.log.len().saturating_sub(10);
    WirelessView {
        enabled: cfg.enabled,
        password_set: secrets.password.is_some_and(|p| !p.is_empty()),
        address: format!("{host}:{port}"),
        discovery: r.discovery,
        readers,
        queue: store.jobs(),
        log: r.log[keep..].to_vec(),
    }
}

fn view() -> WirelessView {
    view_at(&deckle_digest::config::default_path(), &store())
}

pub fn wireless_status() -> WirelessView {
    view()
}

/// Sets the password a reader must give, or clears it (`None` or empty).
pub fn wireless_set_password(password: Option<String>) -> Result<WirelessView, String> {
    let path = wireless::secrets_path(&deckle_digest::config::default_path());
    let mut s = wireless::load_secrets(&path);
    s.password = password.filter(|p| !p.is_empty());
    wireless::save_secrets(&path, &s).map_err(|e| e.to_string())?;
    Ok(view())
}

/// Queues books (files from the Books tab's library) for `reader`: made ready as for a Kobo,
/// then sent when it connects, or at once if it's connected.
pub fn wireless_queue(reader: String, paths: Vec<String>) -> Result<WirelessView, String> {
    queue_at(&store(), &reader, &paths)?;
    Ok(view())
}

fn queue_at(store: &Store, reader: &str, paths: &[String]) -> Result<(), String> {
    if reader.trim().is_empty() {
        return Err("which reader?".into());
    }
    for p in paths {
        let path = PathBuf::from(p);
        if !path.is_file() {
            return Err(format!("{p} isn't there any more"));
        }
        store.add_book(reader, path).map_err(|e| e.to_string())?;
    }
    Ok(())
}

/// Takes books off `reader` (by their paths on it) when it next connects, or at once.
pub fn wireless_remove(reader: String, lpaths: Vec<String>) -> Result<WirelessView, String> {
    let store = store();
    for l in lpaths {
        store
            .add(&reader, None, Some(l))
            .map_err(|e| e.to_string())?;
    }
    Ok(view())
}

/// Takes a job off the queue before it's done.
pub fn wireless_cancel(id: String) -> Result<WirelessView, String> {
    if !store().cancel(&id) {
        return Err("it has already gone, or was never queued".into());
    }
    Ok(view())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_view_lists_connected_readers_first_and_queues_by_reader() {
        let tmp = tempfile::tempdir().unwrap();
        let store = Store::new(tmp.path().join("wireless"));
        store.update(|r| {
            for (id, name, connected, seen) in [
                ("a", "Old Kobo", false, "2026-10-01T00:00:00Z"),
                ("b", "KOReader (Kobo_frost)", true, "2026-10-30T00:00:00Z"),
                ("c", "PocketBook", false, "2026-10-29T00:00:00Z"),
            ] {
                r.readers.insert(
                    id.into(),
                    wireless::Seen {
                        name: name.into(),
                        connected,
                        last_seen: seen.into(),
                        ..Default::default()
                    },
                );
            }
            r.log = (0..15).map(|i| format!("line {i}")).collect();
        });
        let book = tmp.path().join("dune.epub");
        std::fs::write(&book, b"x").unwrap();
        queue_at(&store, "b", &[book.to_string_lossy().into_owned()]).unwrap();
        assert!(queue_at(&store, "b", &["/nope.epub".into()]).is_err());
        assert!(queue_at(&store, " ", &[]).is_err());

        let v = view_at(&tmp.path().join("digest.toml"), &store);
        let names: Vec<_> = v.readers.iter().map(|r| r.name.as_str()).collect();
        assert_eq!(names, ["KOReader (Kobo_frost)", "PocketBook", "Old Kobo"]);
        assert_eq!(v.queue.len(), 1);
        assert!(v.queue[0].build && v.queue[0].reader == "b");
        assert_eq!(v.log.len(), 10);
        assert_eq!(v.log.last().unwrap(), "line 14");
        assert!(v.address.ends_with(":9090"));
        assert!(!v.password_set);
    }
}
