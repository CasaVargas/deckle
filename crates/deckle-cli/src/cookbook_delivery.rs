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
//!     queue/              a private Queue holding one prepared entry, DEVICE_NAME, tagged
//!                         with its build id (`build_id` inside it); prepared in `queue.next/`
//!                         and swapped in whole
//!     history             build ids, oldest first, the newest 50
//!     build_id            the newest build's id; written last
//!     error               why the last build failed; gone after a good one
//!     .lock               held while building (the watcher and `cookbook send` take turns)
//! ```

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::time::SystemTime;

use deckle_core::deliver::Target;
use deckle_core::pipeline::{RunOptions, XRayMode, prepare};
use deckle_core::queue::{FlushReport, Queue};
use deckle_core::sync::same_kindle;
use deckle_digest::config::SyncConfig;
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

fn error_path(store: &Store) -> PathBuf {
    build_dir(store).join("error")
}

/// Takes the build lock (`build/.lock`): `wait` blocks until it is free; without it, `None`
/// means someone else is building. Held until the returned file is dropped.
fn build_lock(store: &Store, wait: bool) -> Result<Option<std::fs::File>, String> {
    let dir = build_dir(store);
    std::fs::create_dir_all(&dir).map_err(|e| format!("{}: {e}", dir.display()))?;
    let path = dir.join(".lock");
    let file = std::fs::OpenOptions::new()
        .create(true)
        .truncate(false)
        .write(true)
        .open(&path)
        .map_err(|e| format!("{}: {e}", path.display()))?;
    if wait {
        file.lock()
            .map_err(|e| format!("{}: {e}", path.display()))?;
        return Ok(Some(file));
    }
    match file.try_lock() {
        Ok(()) => Ok(Some(file)),
        Err(std::fs::TryLockError::WouldBlock) => Ok(None),
        Err(std::fs::TryLockError::Error(e)) => Err(format!("{}: {e}", path.display())),
    }
}

/// The private queue the prepared cookbook waits in.
fn private_queue(store: &Store) -> Queue {
    Queue::new(build_dir(store).join("queue"))
}

/// The id `sync::device_id` gives `target` when it's a Kindle; `None` for a folder, which is an
/// export (`push --to <folder>`), not a Kindle the cookbook stands on.
pub(crate) fn kindle_id(target: &Target) -> Option<String> {
    matches!(target, Target::Mtp(_) | Target::MassStorage(_))
        .then(|| deckle_core::sync::device_id(target))
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

/// The newest build's EPUB (`build/cookbook.epub`); there once [`current_build`] is `Some`.
pub(crate) fn epub_path(store: &Store) -> PathBuf {
    build_dir(store).join("cookbook.epub")
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

    /// Every Kindle id with a record.
    pub(crate) fn ids(&self) -> impl Iterator<Item = &str> {
        self.0.keys().map(String::as_str)
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

/// Removes the folder `dir` if it's there.
fn remove_dir(dir: &Path) -> Result<(), String> {
    match std::fs::remove_dir_all(dir) {
        Err(e) if e.kind() != std::io::ErrorKind::NotFound => {
            Err(format!("{}: {e}", dir.display()))
        }
        _ => Ok(()),
    }
}

/// Puts the folder `next` in place of `live`: the old one is renamed aside first, put back if
/// the new one can't be moved in, and removed once it is.
fn swap_in(next: &Path, live: &Path) -> Result<(), String> {
    let mut old = live.as_os_str().to_owned();
    old.push(".old");
    let old = PathBuf::from(old);
    remove_dir(&old)?;
    let had_live = live.exists();
    if had_live {
        std::fs::rename(live, &old).map_err(|e| format!("{}: {e}", live.display()))?;
    }
    if let Err(e) = std::fs::rename(next, live) {
        if had_live {
            let _ = std::fs::rename(&old, live);
        }
        return Err(format!("{}: {e}", live.display()));
    }
    // The new build is in; a leftover is cleared before the next swap.
    let _ = remove_dir(&old);
    Ok(())
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

/// Whether the book has changes no build has picked up ([`due`]), and why the last attempt failed
/// (only said while changes wait).
pub(crate) fn unbuilt(store: &Store) -> (bool, Option<String>) {
    let pending = due(store);
    (
        pending,
        read_trimmed(&error_path(store)).filter(|_| pending),
    )
}

/// Rebuilds the cookbook when it changed ([`due`]): the EPUB to `build/cookbook.epub`, prepared
/// (Kindle Previewer at `previewer`, else found as `deckle run` finds it) into the private queue,
/// its id appended to the history and written to `build/build_id` last. `Some` when there's a
/// new build; `None` when nothing changed, the book came out the same, or there are no recipes.
/// This is the watcher's: when another process is building (the build lock is taken) it does
/// nothing this time, and the next tick looks again.
///
/// `dirty` is cleared before the store is read, so a recipe saved during the build marks it
/// again; a failed build marks it again too, so the next check retries, and says why in
/// `build/error`.
pub(crate) fn rebuild_if_dirty(
    store: &Store,
    previewer: Option<&Path>,
) -> Result<Option<Built>, String> {
    rebuild_when_due(store, previewer, false)
}

/// [`rebuild_if_dirty`] for `cookbook send`: waits its turn when another process is building.
pub(crate) fn rebuild_waiting(
    store: &Store,
    previewer: Option<&Path>,
) -> Result<Option<Built>, String> {
    rebuild_when_due(store, previewer, true)
}

fn rebuild_when_due(
    store: &Store,
    previewer: Option<&Path>,
    wait: bool,
) -> Result<Option<Built>, String> {
    if !due(store) {
        return Ok(None);
    }
    let Some(_lock) = build_lock(store, wait)? else {
        return Ok(None);
    };
    // Whoever held the lock may have built it already.
    if !due(store) {
        return Ok(None);
    }
    // Anything edited from here on is newer than the build: `build_id` gets this time.
    let started = SystemTime::now();
    store.clear_dirty()?;
    let r = rebuild(store, previewer, started);
    match &r {
        Ok(_) => {
            let _ = std::fs::remove_file(error_path(store));
        }
        Err(e) => {
            store.mark_dirty()?;
            // Best effort: the build's own error is what's returned.
            let _ = write_atomic(&error_path(store), e);
        }
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
    let epub = epub_path(store);
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
    // Prepared and tagged beside the live queue, then swapped in whole: a failure anywhere
    // leaves the old entry and `build_id` as they were.
    let next = Queue::new(dir.join("queue.next"));
    remove_dir(&next.dir)?;
    let tagged = next
        .add_prepared(&prepared, DEVICE_NAME, None)
        .map_err(|e| format!("{}: {e}", next.dir.display()))?;
    write_atomic(&tagged.join(ENTRY_BUILD), &built.build_id)?;
    swap_in(&next.dir, &queue.dir)?;
    let ids = appended(history(store), &built.build_id);
    write_atomic(&history_path(store), &(ids.join("\n") + "\n"))?;
    write_atomic(&id_path, &built.build_id)?;
    stamp(&id_path)?;
    Ok(Some(built))
}

/// Queues the newest build in `outbox` for the Kindle `kindle_id` when `on` (its `cookbook`
/// switch) and it hasn't taken that build yet (or an older build still waits for it). `true`
/// when it was queued now; `false` when it's off, there's no build, the Kindle is up to date,
/// or the newest is already queued for it. An entry of
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
        // Marked changed, so the next check rebuilds it instead of waiting for an edit.
        let marked = match store.mark_dirty() {
            Ok(()) => "it's rebuilt on the next check".to_owned(),
            Err(e) => format!("it couldn't be marked for a rebuild: {e}"),
        };
        return Err(format!(
            "{}: the cookbook's build is missing; {marked}",
            entry.display()
        ));
    }
    let up_to_date = Delivered::load(store)?.get(kindle_id) == Some(current.as_str());
    let queued = entry_in(outbox);
    let queued_for_it = Queue::owner(&queued).is_some_and(|o| same_kindle(&o, kindle_id));
    let queued_build = read_trimmed(&queued.join(ENTRY_BUILD));
    if queued_for_it && queued_build.as_deref() == Some(current.as_str()) {
        return Ok(false);
    }
    // An older build still waiting for an up-to-date Kindle would take it back a version; it's
    // replaced by the newest instead.
    if up_to_date && !queued_for_it {
        return Ok(false);
    }
    outbox
        .adopt_for(&entry, kindle_id)
        .map_err(|e| format!("{}: {e}", outbox.dir.display()))?;
    Ok(true)
}

/// The build the cookbook entry in `outbox` was made from, when the next push to `kindle_id`
/// would deliver it (it's meant for that Kindle, or for anyone). Read before the push: the
/// entry is gone after it.
pub(crate) fn queued_build(outbox: &Queue, kindle_id: &str) -> Option<String> {
    let entry = entry_in(outbox);
    let goes = match Queue::owner(&entry) {
        Some(owner) => same_kindle(&owner, kindle_id),
        None => true,
    };
    goes.then(|| read_trimmed(&entry.join(ENTRY_BUILD)))
        .flatten()
}

/// After a push to `kindle_id`: when the cookbook was among what it delivered, records that the
/// Kindle has `pushed`, the build [`queued_build`] read before the push. Not the newest build:
/// another process may have rebuilt since. Without a tag nothing is recorded.
pub(crate) fn record(
    store: &Store,
    kindle_id: &str,
    report: &FlushReport,
    pushed: Option<&str>,
) -> Result<(), String> {
    let sent = report
        .delivered
        .iter()
        .any(|p| p.file_name().is_some_and(|n| n == DEVICE_NAME));
    let Some(pushed) = pushed.filter(|_| sent) else {
        return Ok(());
    };
    let mut delivered = Delivered::load(store)?;
    if delivered.get(kindle_id) == Some(pushed) {
        return Ok(());
    }
    delivered.set(kindle_id, pushed);
    delivered.save(store)
}

/// Where a Kindle stands with the cookbook.
#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "snake_case")]
pub enum KindleState {
    UpToDate,
    Behind,
    NeverSent,
    /// Left out with `cookbook = false`.
    Off,
}

/// One Kindle's line on the Delivery list.
#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize)]
pub struct KindleStatus {
    pub name: String,
    pub id: String,
    /// "Up to date", "<n> changes behind", "Behind", "Never sent" or "Off".
    pub status: String,
    pub state: KindleState,
    /// Builds behind: 0 when up to date; `None` when it never took one, is left out, or the
    /// build it has has aged out of the history.
    pub behind: Option<usize>,
}

/// How far `delivered` is behind `current`, given the build history (oldest first).
fn status_of(
    delivered: Option<&str>,
    current: Option<&str>,
    history: &[String],
) -> (KindleState, Option<usize>, String) {
    let Some(delivered) = delivered else {
        return (KindleState::NeverSent, None, "Never sent".to_owned());
    };
    if current.is_none_or(|c| c == delivered) {
        return (KindleState::UpToDate, Some(0), "Up to date".to_owned());
    }
    match history.iter().rposition(|b| b == delivered) {
        Some(at) => match history.len() - 1 - at {
            0 => (KindleState::Behind, None, "Behind".to_owned()),
            1 => (KindleState::Behind, Some(1), "1 change behind".to_owned()),
            n => (KindleState::Behind, Some(n), format!("{n} changes behind")),
        },
        None => (KindleState::Behind, None, "Behind".to_owned()),
    }
}

/// Each Kindle in `kindles` ((name, id)) with how its cookbook stands; one that `sync` leaves
/// out (`cookbook = false`) is Off. A record that can't be read is an error, not "Never sent".
pub(crate) fn status(
    store: &Store,
    sync: &SyncConfig,
    kindles: &[(String, String)],
) -> Result<Vec<KindleStatus>, String> {
    let delivered = Delivered::load(store)?;
    let current = current_build(store);
    let history = history(store);
    Ok(kindles
        .iter()
        .map(|(name, id)| {
            let (state, behind, status) = if sync.for_device(id).cookbook {
                status_of(delivered.get(id), current.as_deref(), &history)
            } else {
                (KindleState::Off, None, "Off".to_owned())
            };
            KindleStatus {
                name: name.clone(),
                id: id.clone(),
                status,
                state,
                behind,
            }
        })
        .collect())
}

/// Every Kindle the cookbook knows, as `status` gives them, sorted by name: the ones named in
/// `sync` and any with a delivery on record (called by its id when it has no name).
pub(crate) fn statuses(store: &Store, sync: &SyncConfig) -> Result<Vec<KindleStatus>, String> {
    let mut kindles: Vec<(String, String)> = sync
        .named()
        .map(|(id, name)| (name.to_owned(), id.to_owned()))
        .collect();
    let delivered = Delivered::load(store)?;
    for id in delivered.ids() {
        if !kindles.iter().any(|(_, known)| same_kindle(known, id)) {
            kindles.push((
                sync.device_name(id).unwrap_or_else(|| id.to_owned()),
                id.to_owned(),
            ));
        }
    }
    let mut all = status(store, sync, &kindles)?;
    all.sort_by_key(|k| (k.name.to_lowercase(), k.id.clone()));
    Ok(all)
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

    /// Sets a file's or a folder's modified time. Unix: the owner may, with any handle. Windows
    /// needs the attribute-writing right, and a folder opens only with backup semantics.
    fn set_mtime(path: &Path, t: SystemTime) {
        let mut opts = std::fs::File::options();
        #[cfg(windows)]
        {
            use std::os::windows::fs::OpenOptionsExt;
            const FILE_WRITE_ATTRIBUTES: u32 = 0x0100;
            const FILE_FLAG_BACKUP_SEMANTICS: u32 = 0x0200_0000;
            opts.access_mode(FILE_WRITE_ATTRIBUTES)
                .custom_flags(FILE_FLAG_BACKUP_SEMANTICS);
        }
        #[cfg(not(windows))]
        opts.read(true);
        opts.open(path).unwrap().set_modified(t).unwrap();
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
        let sync = SyncConfig::default();
        let lines: Vec<String> = status(&store, &sync, &kindles)
            .unwrap()
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
        let all = status(&store, &sync, &kindles).unwrap();
        assert_eq!(all[1].status, "1 change behind");
        // The machine fields: how many behind, and where it stands.
        assert_eq!(
            (all[0].state, all[0].behind),
            (KindleState::UpToDate, Some(0))
        );
        assert_eq!(
            (all[1].state, all[1].behind),
            (KindleState::Behind, Some(1))
        );
        assert_eq!(
            (all[2].state, all[2].behind),
            (KindleState::NeverSent, None)
        );
        assert_eq!((all[3].state, all[3].behind), (KindleState::Behind, None));

        // A Kindle left out is Off, whatever it took before.
        let mut off = SyncConfig::default();
        off.devices.insert(
            MOM.into(),
            deckle_digest::config::DeviceSync {
                cookbook: Some(false),
                ..Default::default()
            },
        );
        let all = status(&store, &off, &kindles).unwrap();
        assert_eq!(
            (all[0].status.as_str(), all[0].state, all[0].behind),
            ("Off", KindleState::Off, None)
        );
        assert_eq!(all[1].state, KindleState::Behind);

        // A record that won't parse is an error, not "Never sent".
        std::fs::write(Delivered::path(&store), "{ nope").unwrap();
        assert!(status(&store, &sync, &kindles).is_err());
        assert!(statuses(&store, &sync).is_err());
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
        fake_build(&store, "b2");
        let b1 = Some("b1");
        record(&store, MOM, &flushed(&["deckle-daily-2026-10-27.kfx"]), b1).unwrap();
        assert_eq!(Delivered::load(&store).unwrap().get(MOM), None);
        record(&store, MOM, &flushed(&[]), b1).unwrap();
        assert_eq!(Delivered::load(&store).unwrap().get(MOM), None);
        // No tag: what went can't be told, so nothing is recorded.
        record(&store, MOM, &flushed(&[DEVICE_NAME]), None).unwrap();
        assert_eq!(Delivered::load(&store).unwrap().get(MOM), None);
        // The build that was pushed, not the newest.
        record(&store, MOM, &flushed(&["x.kfx", DEVICE_NAME]), b1).unwrap();
        assert_eq!(Delivered::load(&store).unwrap().get(MOM), Some("b1"));
    }

    #[test]
    fn queued_build_is_the_tag_of_the_entry_this_kindle_takes() {
        let (tmp, store) = store();
        let outbox = Queue::new(tmp.path().join("outbox"));
        assert_eq!(queued_build(&outbox, MOM), None);
        fake_build(&store, "b1");
        offer(&store, &outbox, MOM, true).unwrap();
        assert_eq!(queued_build(&outbox, MOM).as_deref(), Some("b1"));
        assert_eq!(queued_build(&outbox, DAD), None, "it waits for Mom");
    }

    #[test]
    fn an_up_to_date_kindle_with_an_older_build_waiting_gets_the_newest() {
        let (tmp, store) = store();
        let outbox = Queue::new(tmp.path().join("outbox"));
        fake_build(&store, "b1");
        assert!(offer(&store, &outbox, MOM, true).unwrap());
        // Mom took b2 by another route while b1 still waited for her.
        fake_build(&store, "b2");
        let mut d = Delivered::default();
        d.set(MOM, "b2");
        d.save(&store).unwrap();
        assert!(offer(&store, &outbox, MOM, true).unwrap());
        assert_eq!(queued_build(&outbox, MOM).as_deref(), Some("b2"));
        assert!(!offer(&store, &outbox, MOM, true).unwrap());
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

        assert!(!store.is_dirty());
        let err = offer(&store, &outbox, DAD, true).unwrap_err();
        assert!(err.contains("missing"), "{err}");
        assert!(store.is_dirty(), "marked, so the next check rebuilds it");
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
        // Why, for whoever lists the book.
        let (pending, why) = unbuilt(&store);
        assert!(pending);
        assert_eq!(why.as_deref(), Some(err.as_str()));

        // A recipe file that won't parse fails the build the same way.
        std::fs::write(store.dir.join("recipes/0123456789ab.json"), "{").unwrap();
        let err = rebuild_if_dirty(&store, None).unwrap_err();
        assert!(err.contains("0123456789ab.json"), "{err}");
        assert!(store.is_dirty());
    }

    #[cfg(unix)]
    #[test]
    fn a_good_build_clears_the_error_and_the_pending_flag() {
        let (tmp, store) = store();
        add(&store, "lemon-bars");
        assert_eq!(unbuilt(&store), (true, None));
        rebuild_if_dirty(&store, Some(&tmp.path().join("no-previewer"))).unwrap_err();
        assert!(unbuilt(&store).1.is_some());
        rebuild_if_dirty(&store, Some(&fake_pv())).unwrap().unwrap();
        assert_eq!(unbuilt(&store), (false, None));
        assert!(!error_path(&store).exists());
    }

    #[cfg(unix)]
    #[test]
    fn the_watcher_skips_a_tick_while_someone_else_builds() {
        let (_tmp, store) = store();
        add(&store, "lemon-bars");
        let held = build_lock(&store, true).unwrap().unwrap();

        // The watcher doesn't wait, build or touch the flag.
        assert!(
            rebuild_if_dirty(&store, Some(&fake_pv()))
                .unwrap()
                .is_none()
        );
        assert!(current_build(&store).is_none());
        assert!(store.is_dirty());
        assert!(!build_dir(&store).join("queue.next").exists());

        // Once the lock is free, the next tick builds.
        drop(held);
        let built = rebuild_if_dirty(&store, Some(&fake_pv())).unwrap().unwrap();
        assert_eq!(current_build(&store), Some(built.build_id));
        assert!(!store.is_dirty());
    }

    #[cfg(unix)]
    #[test]
    fn send_waits_for_the_lock_then_finds_the_book_built() {
        let (_tmp, store) = store();
        add(&store, "lemon-bars");
        let held = build_lock(&store, true).unwrap().unwrap();
        let (tx, rx) = std::sync::mpsc::channel();
        let waiting = {
            let dir = store.dir.clone();
            std::thread::spawn(move || {
                let store = Store::new(dir);
                tx.send(()).unwrap();
                rebuild_waiting(&store, Some(&fake_pv()))
            })
        };
        rx.recv().unwrap();
        std::thread::sleep(Duration::from_millis(300));
        assert!(current_build(&store).is_none(), "still waiting");
        // The holder built it meanwhile (here: nothing is due any more).
        store.clear_dirty().unwrap();
        fake_build(&store, "b1");
        drop(held);
        assert!(
            waiting.join().unwrap().unwrap().is_none(),
            "due is re-checked"
        );
        assert_eq!(current_build(&store).as_deref(), Some("b1"));
    }

    /// The fake Previewer is a shell script, so only the Unix tests use it.
    #[cfg(unix)]
    fn fake_pv() -> PathBuf {
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../deckle-core/tests/fake_previewer/ok.sh")
    }

    /// Another process rebuilds between the offer and the push: the Kindle got the build that
    /// was queued, and its record says so.
    #[cfg(unix)]
    #[test]
    fn rebuild_between_offer_and_push_records_the_pushed_build() {
        let (tmp, store) = store();
        let outbox = Queue::new(tmp.path().join("outbox"));
        let kindle = tmp.path().join("kindle");
        std::fs::create_dir_all(&kindle).unwrap();
        let target = Target::Folder(kindle.clone());
        let id = deckle_core::sync::device_id(&target);
        add(&store, "lemon-bars");
        let first = rebuild_if_dirty(&store, Some(&fake_pv())).unwrap().unwrap();
        assert!(offer(&store, &outbox, &id, true).unwrap());
        let pushed = queued_build(&outbox, &id);
        assert_eq!(pushed.as_deref(), Some(first.build_id.as_str()));

        add(&store, "plum-cake");
        let second = rebuild_if_dirty(&store, Some(&fake_pv())).unwrap().unwrap();
        assert_ne!(first.build_id, second.build_id);
        let report = outbox.flush(&target, None).unwrap();
        assert!(kindle.join(DEVICE_NAME).is_file());
        record(&store, &id, &report, pushed.as_deref()).unwrap();
        assert_eq!(
            Delivered::load(&store).unwrap().get(&id),
            Some(first.build_id.as_str())
        );
        let kindles = [("Kitchen".to_owned(), id.clone())];
        let all = status(&store, &SyncConfig::default(), &kindles).unwrap();
        assert_eq!(all[0].status, "1 change behind");
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
        assert!(!build_dir(&store).join("queue.next").exists());
        assert!(!build_dir(&store).join("queue.old").exists());
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

    /// A build that fails after the Previewer (here: the old queue can't be cleared away) or in
    /// it leaves the last good build in place: its entry, its tag and `build_id`.
    #[cfg(unix)]
    #[test]
    fn a_failed_build_keeps_the_last_good_one() {
        use std::os::unix::fs::PermissionsExt;
        let (_tmp, store) = store();
        add(&store, "lemon-bars");
        let first = rebuild_if_dirty(&store, Some(&fake_pv())).unwrap().unwrap();
        let entry = entry_in(&private_queue(&store));
        let intact = |store: &Store| {
            assert_eq!(current_build(store), Some(first.build_id.clone()));
            assert_eq!(
                read_trimmed(&entry.join(ENTRY_BUILD)),
                Some(first.build_id.clone())
            );
            assert!(entry.join("book.kpf").is_file());
            assert_eq!(history(store), std::slice::from_ref(&first.build_id));
            assert!(store.is_dirty(), "tried again on the next check");
        };

        // The Previewer fails.
        add(&store, "plum-cake");
        let failed = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../deckle-core/tests/fake_previewer/failed.sh");
        assert!(rebuild_if_dirty(&store, Some(&failed)).is_err());
        intact(&store);

        // Prepared, but the swap can't start: a leftover `queue.old` that won't go.
        let stuck = build_dir(&store).join("queue.old/stuck");
        std::fs::create_dir_all(&stuck).unwrap();
        std::fs::write(stuck.join("f"), b"x").unwrap();
        std::fs::set_permissions(&stuck, std::fs::Permissions::from_mode(0o555)).unwrap();
        let err = rebuild_if_dirty(&store, Some(&fake_pv())).unwrap_err();
        std::fs::set_permissions(&stuck, std::fs::Permissions::from_mode(0o755)).unwrap();
        assert!(err.contains("queue.old"), "{err}");
        intact(&store);

        // Once it can, the next check builds, clearing the leftovers.
        let second = rebuild_if_dirty(&store, Some(&fake_pv())).unwrap().unwrap();
        assert_eq!(current_build(&store), Some(second.build_id.clone()));
        assert_eq!(
            read_trimmed(&entry.join(ENTRY_BUILD)),
            Some(second.build_id)
        );
        assert!(!build_dir(&store).join("queue.next").exists());
        assert!(!build_dir(&store).join("queue.old").exists());
    }
}
