// SPDX-License-Identifier: GPL-3.0-or-later
//! The Kitchen Book's standing delivery (cookbook design §9): every Kindle gets the newest build
//! on its next plug-in. An ordinary outbox entry goes to the first Kindle that takes it, so the
//! cookbook keeps its own prepared copy and offers it to each Kindle in turn, recording per
//! Kindle the build it last took.
//!
//! ```text
//! cookbook/
//!   delivered.json        sync id → build id last delivered
//!   build/
//!     cookbook.epub       the newest build
//!     queue/              a private Queue holding one prepared entry, DEVICE_NAME
//!     history             build ids, oldest first, the newest 50
//!     build_id            the newest build's id; written last
//! ```

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::time::SystemTime;

use deckle_core::deliver::Target;
use deckle_core::pipeline::{RunOptions, XRayMode, prepare};
use deckle_core::queue::{FlushReport, Queue};
use deckle_core::sync::same_kindle;
use deckle_digest::cookbook::book::{self, Built, DEVICE_NAME};
use deckle_digest::cookbook::store::Store;

/// The build ids kept in `build/history`.
const HISTORY: usize = 50;
/// The build id a prepared entry was made from, inside the entry (the queue ignores it).
const ENTRY_BUILD: &str = "build_id";

fn build_dir(store: &Store) -> PathBuf {
    store.dir.join("build")
}

fn build_id_path(store: &Store) -> PathBuf {
    build_dir(store).join("build_id")
}

fn history_path(store: &Store) -> PathBuf {
    build_dir(store).join("history")
}

/// The private queue the prepared cookbook waits in.
fn private_queue(store: &Store) -> Queue {
    Queue::new(build_dir(store).join("queue"))
}

/// The prepared entry in a queue: `Deckle Kitchen Book.book`.
fn entry_in(queue: &Queue) -> PathBuf {
    let stem = DEVICE_NAME.strip_suffix(".kfx").unwrap_or(DEVICE_NAME);
    queue.dir.join(format!("{stem}.book"))
}

fn read_trimmed(path: &Path) -> Option<String> {
    std::fs::read_to_string(path)
        .ok()
        .map(|s| s.trim().to_owned())
        .filter(|s| !s.is_empty())
}

/// Writes `text` aside and renames it into place.
fn write_atomic(path: &Path, text: &str) -> Result<(), String> {
    let mut tmp = path.as_os_str().to_owned();
    tmp.push(".tmp");
    let tmp = PathBuf::from(tmp);
    std::fs::write(&tmp, text).map_err(|e| format!("{}: {e}", tmp.display()))?;
    std::fs::rename(&tmp, path).map_err(|e| format!("{}: {e}", path.display()))
}

/// The newest build's id; `None` before the first build (or after the last recipe went).
pub(crate) fn current_build(store: &Store) -> Option<String> {
    read_trimmed(&build_id_path(store))
}

/// `cookbook/delivered.json`: per Kindle sync id, the build id it last took.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct Delivered(BTreeMap<String, String>);

impl Delivered {
    fn path(store: &Store) -> PathBuf {
        store.dir.join("delivered.json")
    }

    /// The record; empty when there's none yet. A file that doesn't parse is an error, so a
    /// slip never sends every Kindle the book again.
    pub(crate) fn load(store: &Store) -> Result<Self, String> {
        let path = Self::path(store);
        match std::fs::read(&path) {
            Ok(bytes) => serde_json::from_slice(&bytes)
                .map(Delivered)
                .map_err(|e| format!("{}: {e}", path.display())),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(Self::default()),
            Err(e) => Err(format!("{}: {e}", path.display())),
        }
    }

    pub(crate) fn save(&self, store: &Store) -> Result<(), String> {
        std::fs::create_dir_all(&store.dir).map_err(|e| format!("{}: {e}", store.dir.display()))?;
        let text = serde_json::to_string_pretty(&self.0).map_err(|e| e.to_string())?;
        write_atomic(&Self::path(store), &text)
    }

    /// The build `kindle` last took: under its id, else under another id for the same Kindle
    /// (seen over MTP and over mass storage).
    pub(crate) fn get(&self, kindle: &str) -> Option<&str> {
        self.0
            .get(kindle)
            .or_else(|| {
                self.0
                    .iter()
                    .find(|(id, _)| same_kindle(id, kindle))
                    .map(|(_, b)| b)
            })
            .map(String::as_str)
    }

    /// Records that `kindle` took `build`, under the id it has now.
    pub(crate) fn set(&mut self, kindle: &str, build: &str) {
        self.0.retain(|id, _| !same_kindle(id, kindle));
        self.0.insert(kindle.to_owned(), build.to_owned());
    }
}

/// Build ids, oldest first.
fn history(store: &Store) -> Vec<String> {
    std::fs::read_to_string(history_path(store))
        .unwrap_or_default()
        .lines()
        .map(str::trim)
        .filter(|l| !l.is_empty())
        .map(str::to_owned)
        .collect()
}

/// `ids` with `id` appended (unless it's already the newest), keeping the newest [`HISTORY`].
fn appended(mut ids: Vec<String>, id: &str) -> Vec<String> {
    if ids.last().map(String::as_str) != Some(id) {
        ids.push(id.to_owned());
    }
    let over = ids.len().saturating_sub(HISTORY);
    ids.drain(..over);
    ids
}

/// When `path` (a file, or a folder and what's in it) last changed.
fn newest_change(path: &Path) -> Option<SystemTime> {
    let meta = std::fs::metadata(path).ok()?;
    let mut newest = meta.modified().ok();
    if meta.is_dir()
        && let Ok(entries) = std::fs::read_dir(path)
    {
        for e in entries.flatten() {
            let t = e.metadata().ok().and_then(|m| m.modified().ok());
            newest = newest.max(t);
        }
    }
    newest
}

/// Whether a recipe file is saved (the book has something to print).
fn has_recipes(store: &Store) -> bool {
    std::fs::read_dir(store.dir.join("recipes"))
        .map(|rd| {
            rd.flatten()
                .any(|e| e.path().extension().is_some_and(|x| x == "json"))
        })
        .unwrap_or(false)
}

/// The book needs building: `dirty` is set, or the owner edited the folder by hand since the
/// last build (`cookbook.toml`, a recipe file or a picture changed or went), or there's no
/// build yet and there are recipes.
fn due(store: &Store) -> bool {
    if store.is_dirty() {
        return true;
    }
    let built = std::fs::metadata(build_id_path(store)).and_then(|m| m.modified());
    let Ok(built) = built else {
        return has_recipes(store);
    };
    ["cookbook.toml", "recipes", "images"]
        .iter()
        .filter_map(|p| newest_change(&store.dir.join(p)))
        .any(|t| t > built)
}

/// The Inbox book's preparation (`inbox::pipeline_with`), without X-Ray or Word Wise (one
/// Previewer run a build), with page numbers as an ordinary book gets them.
fn run_options(previewer: PathBuf) -> RunOptions {
    RunOptions {
        preset: None,
        cover: Default::default(),
        cde_type: deckle_kfx::convert::CdeType::Pdoc,
        cde_override: None,
        amazon_match: false,
        generated_cover: Some(None),
        metadata_fill: false,
        typography: false,
        popup_notes: false,
        verified_real_asin: false,
        asin: None,
        pages: Some(None),
        // `prepare` delivers nothing: the Kindle is known at push.
        target: Target::Folder(std::env::temp_dir()),
        previewer: Some(previewer),
        keep_work_dir: false,
        xray: XRayMode::Off,
        llm: None,
        wikipedia: false,
        word_wise: None,
        replaces: Vec::new(),
        originals: None,
    }
}

/// Removes the build, so nothing is offered (the last recipe went).
fn forget_build(store: &Store) -> Result<(), String> {
    let path = build_id_path(store);
    match std::fs::remove_file(&path) {
        Err(e) if e.kind() != std::io::ErrorKind::NotFound => {
            return Err(format!("{}: {e}", path.display()));
        }
        _ => {}
    }
    let entry = entry_in(&private_queue(store));
    if entry.exists() {
        std::fs::remove_dir_all(&entry).map_err(|e| format!("{}: {e}", entry.display()))?;
    }
    Ok(())
}

/// Rebuilds the cookbook when it changed ([`due`]): the EPUB to `build/cookbook.epub`, prepared
/// (Kindle Previewer at `previewer`, else found as `deckle run` finds it) into the private queue,
/// its id appended to the history and written to `build/build_id` last. `Some` when there's a
/// new build; `None` when nothing changed, the book came out the same, or there are no recipes.
///
/// `dirty` is cleared before the store is read, so a recipe saved during the build marks it
/// again; a failed build marks it again too, so the next check retries.
pub(crate) fn rebuild_if_dirty(
    store: &Store,
    previewer: Option<&Path>,
) -> Result<Option<Built>, String> {
    if !due(store) {
        return Ok(None);
    }
    // Anything edited from here on is newer than the build: `build_id` gets this time.
    let started = SystemTime::now();
    store.clear_dirty()?;
    let r = rebuild(store, previewer, started);
    if r.is_err() {
        store.mark_dirty()?;
    }
    r
}

fn rebuild(
    store: &Store,
    previewer: Option<&Path>,
    started: SystemTime,
) -> Result<Option<Built>, String> {
    if store.all()?.is_empty() {
        forget_build(store)?;
        return Ok(None);
    }
    let pv = deckle_core::previewer::locate(previewer)
        .map_err(|e| format!("can't build the cookbook yet: {e}"))?
        .path;
    let dir = build_dir(store);
    std::fs::create_dir_all(&dir).map_err(|e| format!("{}: {e}", dir.display()))?;
    let epub = dir.join("cookbook.epub");
    let part = dir.join("cookbook.part.epub");
    let mut built = book::build(store, &part)?;
    std::fs::rename(&part, &epub).map_err(|e| format!("{}: {e}", epub.display()))?;
    built.path = epub.clone();

    let id_path = build_id_path(store);
    let queue = private_queue(store);
    let entry = entry_in(&queue);
    let stamp = |path: &Path| -> Result<(), String> {
        std::fs::File::options()
            .write(true)
            .open(path)
            .and_then(|f| f.set_modified(started))
            .map_err(|e| format!("{}: {e}", path.display()))
    };
    let unchanged = current_build(store).as_deref() == Some(built.build_id.as_str())
        && read_trimmed(&entry.join(ENTRY_BUILD)).as_deref() == Some(built.build_id.as_str());
    if unchanged {
        // Touched but not changed (a file saved as it was): no Previewer run, no new build.
        stamp(&id_path)?;
        return Ok(None);
    }

    let mut prepared = prepare(&epub, &run_options(pv), &mut |_| {}).map_err(|r| {
        r.error
            .unwrap_or_else(|| "the cookbook wasn't converted".into())
    })?;
    prepared.name = Some(DEVICE_NAME.into());
    let entry = queue
        .add_prepared(&prepared, DEVICE_NAME, None)
        .map_err(|e| format!("{}: {e}", queue.dir.display()))?;
    write_atomic(&entry.join(ENTRY_BUILD), &built.build_id)?;
    let ids = appended(history(store), &built.build_id);
    write_atomic(&history_path(store), &(ids.join("\n") + "\n"))?;
    write_atomic(&id_path, &built.build_id)?;
    stamp(&id_path)?;
    Ok(Some(built))
}

/// Queues the newest build in `outbox` for the Kindle `kindle_id` when `on` (its `cookbook`
/// switch) and it hasn't taken that build yet. `true` when it was queued now; `false` when it's
/// off, there's no build, the Kindle is up to date, or it's already queued for it. An entry of
/// the same name meant for another Kindle (a push cut short) is replaced: that Kindle's record
/// never moved, so it's offered again on its next plug-in.
pub(crate) fn offer(
    store: &Store,
    outbox: &Queue,
    kindle_id: &str,
    on: bool,
) -> Result<bool, String> {
    if !on {
        return Ok(false);
    }
    let Some(current) = current_build(store) else {
        return Ok(false);
    };
    let entry = entry_in(&private_queue(store));
    if !entry.is_dir() {
        return Err(format!(
            "{}: the cookbook's build is missing; it's rebuilt on the next change",
            entry.display()
        ));
    }
    if Delivered::load(store)?.get(kindle_id) == Some(current.as_str()) {
        return Ok(false);
    }
    let queued = entry_in(outbox);
    let already = Queue::owner(&queued).is_some_and(|o| same_kindle(&o, kindle_id))
        && read_trimmed(&queued.join(ENTRY_BUILD)).as_deref() == Some(current.as_str());
    if already {
        return Ok(false);
    }
    outbox
        .adopt_for(&entry, kindle_id)
        .map_err(|e| format!("{}: {e}", outbox.dir.display()))?;
    Ok(true)
}

/// After a push to `kindle_id`: when the cookbook was among what it delivered, records that the
/// Kindle has the newest build.
pub(crate) fn record(store: &Store, kindle_id: &str, report: &FlushReport) -> Result<(), String> {
    let sent = report
        .delivered
        .iter()
        .any(|p| p.file_name().is_some_and(|n| n == DEVICE_NAME));
    let Some(current) = current_build(store).filter(|_| sent) else {
        return Ok(());
    };
    let mut delivered = Delivered::load(store)?;
    if delivered.get(kindle_id) == Some(current.as_str()) {
        return Ok(());
    }
    delivered.set(kindle_id, &current);
    delivered.save(store)
}

/// One Kindle's line on the Delivery list.
#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize)]
pub(crate) struct KindleStatus {
    pub name: String,
    pub id: String,
    /// "Up to date", "<n> changes behind", "Behind" or "Never sent".
    pub status: String,
}

/// How far `delivered` is behind `current`, given the build history (oldest first).
fn status_of(delivered: Option<&str>, current: Option<&str>, history: &[String]) -> String {
    let Some(delivered) = delivered else {
        return "Never sent".to_owned();
    };
    if current.is_none_or(|c| c == delivered) {
        return "Up to date".to_owned();
    }
    match history.iter().rposition(|b| b == delivered) {
        Some(at) => match history.len() - 1 - at {
            0 => "Behind".to_owned(),
            1 => "1 change behind".to_owned(),
            n => format!("{n} changes behind"),
        },
        None => "Behind".to_owned(),
    }
}

/// Each Kindle in `kindles` ((name, id)) with how its cookbook stands.
#[allow(dead_code)] // the `cookbook list` footer and the desktop's Delivery (Task 3.4)
pub(crate) fn status(store: &Store, kindles: &[(String, String)]) -> Vec<KindleStatus> {
    let delivered = Delivered::load(store).unwrap_or_default();
    let current = current_build(store);
    let history = history(store);
    kindles
        .iter()
        .map(|(name, id)| KindleStatus {
            name: name.clone(),
            id: id.clone(),
            status: status_of(delivered.get(id), current.as_deref(), &history),
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use deckle_digest::chrono::{TimeZone, Utc};
    use deckle_digest::cookbook::Recipe;
    use std::time::Duration;

    fn store() -> (tempfile::TempDir, Store) {
        let tmp = tempfile::tempdir().unwrap();
        let store = Store::new(tmp.path().join("cookbook"));
        std::fs::create_dir_all(&store.dir).unwrap();
        (tmp, store)
    }

    fn add(store: &Store, title: &str) {
        let recipe = Recipe {
            title: title.to_owned(),
            description: None,
            yield_: None,
            prep: None,
            cook: None,
            total: None,
            ingredients: vec![],
            steps: vec![],
            category: vec![],
            cuisine: None,
            author: None,
            image_url: None,
            source_url: format!("https://example.com/{title}"),
            site_name: "example.com".to_owned(),
            structured: true,
            body_xhtml: None,
        };
        let key = recipe.source_url.clone();
        let at = Utc.with_ymd_and_hms(2026, 10, 1, 12, 0, 0).unwrap();
        store.put(recipe, &key, None, None, at).unwrap();
    }

    /// A build as `rebuild` leaves one, without the Previewer.
    fn fake_build(store: &Store, id: &str) {
        let entry = entry_in(&private_queue(store));
        std::fs::create_dir_all(&entry).unwrap();
        std::fs::write(entry.join("book.kpf"), id).unwrap();
        std::fs::write(entry.join(ENTRY_BUILD), id).unwrap();
        let ids = appended(history(store), id);
        std::fs::write(history_path(store), ids.join("\n")).unwrap();
        std::fs::write(build_id_path(store), id).unwrap();
    }

    fn flushed(names: &[&str]) -> FlushReport {
        FlushReport {
            delivered: names
                .iter()
                .map(|n| PathBuf::from("/Volumes/Kindle/documents").join(n))
                .collect(),
            ..Default::default()
        }
    }

    /// Sets a file's or a folder's modified time (the owner may, with any handle).
    fn set_mtime(path: &Path, t: SystemTime) {
        std::fs::File::open(path).unwrap().set_modified(t).unwrap();
    }

    const MOM: &str = "Kindle 5.16.2.1.1 (4184940002)/Mom";
    const DAD: &str = "Kindle 5.16.2.1.1 (4184940002)/Dad";

    #[test]
    fn history_keeps_the_newest_fifty_without_repeats() {
        let mut ids = Vec::new();
        for i in 0..60 {
            ids = appended(ids, &format!("b{i}"));
            ids = appended(ids, &format!("b{i}"));
        }
        assert_eq!(ids.len(), HISTORY);
        assert_eq!(ids.first().map(String::as_str), Some("b10"));
        assert_eq!(ids.last().map(String::as_str), Some("b59"));
        // A build that comes back after another is a new change.
        let ids = appended(appended(vec!["a".into()], "b"), "a");
        assert_eq!(ids, ["a", "b", "a"]);
    }

    #[test]
    fn status_lines() {
        let (_tmp, store) = store();
        let kindles: Vec<(String, String)> = [
            ("Mom's Kindle", MOM),
            ("Dad's Kindle", DAD),
            ("Guest", "G000GUEST"),
            ("Old", "G000OLD"),
        ]
        .iter()
        .map(|(n, i)| (n.to_string(), i.to_string()))
        .collect();
        for b in ["b1", "b2", "b3", "b4"] {
            fake_build(&store, b);
        }
        let mut d = Delivered::default();
        d.set(MOM, "b4");
        d.set(DAD, "b1");
        d.set("G000OLD", "gone");
        d.save(&store).unwrap();
        let lines: Vec<String> = status(&store, &kindles)
            .into_iter()
            .map(|s| format!("{}: {}", s.name, s.status))
            .collect();
        assert_eq!(
            lines,
            [
                "Mom's Kindle: Up to date",
                "Dad's Kindle: 3 changes behind",
                "Guest: Never sent",
                "Old: Behind",
            ]
        );
        d.set(DAD, "b3");
        d.save(&store).unwrap();
        assert_eq!(status(&store, &kindles)[1].status, "1 change behind");
    }

    #[test]
    fn delivered_round_trips_and_knows_a_kindle_by_its_serial() {
        let (_tmp, store) = store();
        assert_eq!(Delivered::load(&store).unwrap(), Delivered::default());
        let mut d = Delivered::default();
        d.set("Kindle/Mom#G000ABC", "b1");
        d.save(&store).unwrap();
        let d = Delivered::load(&store).unwrap();
        // The same Kindle over MTP is known by its serial alone.
        assert_eq!(d.get("G000ABC"), Some("b1"));
        assert_eq!(d.get("G000XYZ"), None);
        let mut d = d;
        d.set("G000ABC", "b2");
        assert_eq!(d.0.len(), 1, "one record per Kindle");
        assert_eq!(d.get("Kindle/Mom#G000ABC"), Some("b2"));

        std::fs::write(store.dir.join("delivered.json"), "{ nope").unwrap();
        assert!(Delivered::load(&store).is_err());
    }

    #[test]
    fn record_needs_the_cookbook_among_what_was_delivered() {
        let (_tmp, store) = store();
        fake_build(&store, "b1");
        record(&store, MOM, &flushed(&["deckle-daily-2026-10-27.kfx"])).unwrap();
        assert_eq!(Delivered::load(&store).unwrap().get(MOM), None);
        record(&store, MOM, &flushed(&[])).unwrap();
        assert_eq!(Delivered::load(&store).unwrap().get(MOM), None);
        record(&store, MOM, &flushed(&["x.kfx", DEVICE_NAME])).unwrap();
        assert_eq!(Delivered::load(&store).unwrap().get(MOM), Some("b1"));
    }

    #[test]
    fn offer_queues_the_build_for_each_kindle_once() {
        let (tmp, store) = store();
        let outbox = Queue::new(tmp.path().join("outbox"));
        // Off, or nothing built: nothing.
        assert!(!offer(&store, &outbox, MOM, true).unwrap());
        fake_build(&store, "b1");
        assert!(!offer(&store, &outbox, MOM, false).unwrap());
        assert!(outbox.pending().is_empty());

        assert!(offer(&store, &outbox, MOM, true).unwrap());
        let queued = entry_in(&outbox);
        assert_eq!(outbox.pending(), std::slice::from_ref(&queued));
        assert_eq!(Queue::owner(&queued).as_deref(), Some(MOM));
        // Already queued for Mom with this build: not copied again.
        std::fs::write(queued.join("marker"), b"x").unwrap();
        assert!(!offer(&store, &outbox, MOM, true).unwrap());
        assert!(queued.join("marker").exists());

        // Dad plugs in before Mom took it: the entry is his.
        assert!(offer(&store, &outbox, DAD, true).unwrap());
        assert_eq!(Queue::owner(&queued).as_deref(), Some(DAD));
        assert!(!queued.join("marker").exists());

        // Up to date: nothing.
        let mut d = Delivered::default();
        d.set(MOM, "b1");
        d.save(&store).unwrap();
        assert!(!offer(&store, &outbox, MOM, true).unwrap());
        assert_eq!(Queue::owner(&queued).as_deref(), Some(DAD));

        // A newer build goes to Mom again.
        fake_build(&store, "b2");
        assert!(offer(&store, &outbox, MOM, true).unwrap());
        assert_eq!(
            std::fs::read_to_string(queued.join(ENTRY_BUILD)).unwrap(),
            "b2"
        );
    }

    #[test]
    fn offer_without_the_build_entry_touches_nothing() {
        let (tmp, store) = store();
        let outbox = Queue::new(tmp.path().join("outbox"));
        fake_build(&store, "b1");
        assert!(offer(&store, &outbox, MOM, true).unwrap());
        let before = std::fs::read_to_string(entry_in(&outbox).join(ENTRY_BUILD)).unwrap();
        std::fs::remove_dir_all(entry_in(&private_queue(&store))).unwrap();
        std::fs::write(build_id_path(&store), "b2").unwrap();

        let err = offer(&store, &outbox, DAD, true).unwrap_err();
        assert!(err.contains("missing"), "{err}");
        let queued = entry_in(&outbox);
        assert_eq!(Queue::owner(&queued).as_deref(), Some(MOM));
        assert_eq!(
            std::fs::read_to_string(queued.join(ENTRY_BUILD)).unwrap(),
            before
        );
    }

    #[test]
    fn hand_edits_make_the_book_due() {
        let (_tmp, store) = store();
        assert!(!due(&store), "nothing saved, nothing to build");
        add(&store, "lemon-bars");
        store.clear_dirty().unwrap();
        assert!(due(&store), "recipes and no build yet");
        fake_build(&store, "b1");
        let past = SystemTime::now() - Duration::from_secs(60);
        set_mtime(&build_id_path(&store), past + Duration::from_secs(30));
        for p in std::fs::read_dir(store.dir.join("recipes"))
            .unwrap()
            .flatten()
        {
            set_mtime(&p.path(), past);
        }
        set_mtime(&store.dir.join("recipes"), past);
        assert!(!due(&store));

        // The owner edits cookbook.toml by hand.
        std::fs::write(store.dir.join("cookbook.toml"), "title = \"Ours\"\n").unwrap();
        assert!(due(&store));
        set_mtime(&store.dir.join("cookbook.toml"), past);
        assert!(!due(&store));

        // ...or deletes a recipe file: the folder changed.
        let file = std::fs::read_dir(store.dir.join("recipes"))
            .unwrap()
            .flatten()
            .next()
            .unwrap()
            .path();
        std::fs::remove_file(file).unwrap();
        assert!(due(&store));
        set_mtime(&store.dir.join("recipes"), past);

        // ...or drops a picture in.
        std::fs::create_dir_all(store.dir.join("images")).unwrap();
        set_mtime(&store.dir.join("images"), past);
        assert!(!due(&store));
        std::fs::write(store.dir.join("images/abc.jpg"), b"jpg").unwrap();
        assert!(due(&store));
    }

    #[test]
    fn an_empty_cookbook_builds_nothing() {
        let (_tmp, store) = store();
        store.mark_dirty().unwrap();
        assert!(rebuild_if_dirty(&store, None).unwrap().is_none());
        assert!(current_build(&store).is_none());
        assert!(!store.is_dirty());

        // The last recipe deleted: the old build goes, so nothing is offered.
        fake_build(&store, "b1");
        store.mark_dirty().unwrap();
        assert!(rebuild_if_dirty(&store, None).unwrap().is_none());
        assert!(current_build(&store).is_none());
        assert!(!entry_in(&private_queue(&store)).exists());
    }

    #[test]
    fn a_failed_build_marks_it_dirty_again() {
        let (tmp, store) = store();
        add(&store, "lemon-bars");
        let err = rebuild_if_dirty(&store, Some(&tmp.path().join("no-previewer"))).unwrap_err();
        assert!(err.contains("can't build the cookbook yet"), "{err}");
        assert!(store.is_dirty());
        assert!(current_build(&store).is_none());

        // A recipe file that won't parse fails the build the same way.
        std::fs::write(store.dir.join("recipes/0123456789ab.json"), "{").unwrap();
        let err = rebuild_if_dirty(&store, None).unwrap_err();
        assert!(err.contains("0123456789ab.json"), "{err}");
        assert!(store.is_dirty());
    }

    #[cfg(unix)]
    #[test]
    fn rebuild_prepares_one_entry_and_writes_the_id_last() {
        let pv =
            Path::new(env!("CARGO_MANIFEST_DIR")).join("../deckle-core/tests/fake_previewer/ok.sh");
        let (_tmp, store) = store();
        add(&store, "lemon-bars");
        let built = rebuild_if_dirty(&store, Some(&pv)).unwrap().unwrap();
        assert!(!store.is_dirty());
        assert_eq!(built.path, build_dir(&store).join("cookbook.epub"));
        assert!(built.path.is_file());
        assert!(!build_dir(&store).join("cookbook.part.epub").exists());
        assert_eq!(current_build(&store), Some(built.build_id.clone()));
        let queue = private_queue(&store);
        assert_eq!(queue.pending(), [entry_in(&queue)]);
        assert_eq!(
            read_trimmed(&entry_in(&queue).join(ENTRY_BUILD)),
            Some(built.build_id.clone())
        );
        assert_eq!(history(&store), std::slice::from_ref(&built.build_id));
        assert!(!due(&store));
        assert!(rebuild_if_dirty(&store, Some(&pv)).unwrap().is_none());

        // Saved again unchanged: due, but the same book, so no new build.
        let saved = store.all().unwrap().remove(0);
        store.update(&saved).unwrap();
        assert!(due(&store));
        assert!(rebuild_if_dirty(&store, Some(&pv)).unwrap().is_none());
        assert!(!due(&store));
        assert_eq!(history(&store).len(), 1);

        // A real change is a second build.
        add(&store, "plum-cake");
        let second = rebuild_if_dirty(&store, Some(&pv)).unwrap().unwrap();
        assert_ne!(second.build_id, built.build_id);
        assert_eq!(history(&store), [built.build_id, second.build_id.clone()]);
        assert_eq!(queue.pending().len(), 1);
    }
}
