// SPDX-License-Identifier: GPL-3.0-or-later
//! Auto-sync on plug-in (auto-sync design): which of a device's actions a check of the watcher
//! runs, and running them over one connection.

use std::path::PathBuf;

use crate::clippings::read_on;
use crate::deliver::{Conn, DeliverError, Target, prune_named_on_except};
use crate::inventory::{DeviceBook, list_with_positions};
use crate::queue::{Finishing, FlushReport, Progress, Prune, Queue};
use crate::vocab::{VocabFiles, read_vocab_on};
use crate::yjf;

/// Whether opening an MTP session with nothing to deliver leaves the Kindle where it was. It
/// used to send the Scribe to its home screen (auto-sync design §4). Checked again on a Scribe
/// 2024 on 2026-10-22: plugging in already shows "connected to computer", a session changes
/// nothing, and the Kindle stays on the bus after it closes (no reset on close since `usb.rs`
/// stopped sending one). So clippings may open a session of their own, once per connection.
pub const IDLE_SESSION_OK: bool = true;

/// The actions a device has on (`[sync]` in `digest.toml`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Actions {
    pub push: bool,
    pub prune: bool,
    /// Read `My Clippings.txt` for the highlights export (#60).
    pub clippings: bool,
    /// Read each edition's `.yjf` for the readership report.
    pub readership: bool,
    /// Read the books' positions and the vocabulary for From Your Shelf.
    pub shelf: bool,
}

impl Default for Actions {
    /// An unconfigured device: what `digest watch` always did.
    fn default() -> Self {
        Actions {
            push: true,
            prune: true,
            clippings: false,
            readership: false,
            shelf: false,
        }
    }
}

/// What one check runs, all over one connection.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Plan {
    pub push: bool,
    pub prune: bool,
    pub clippings: bool,
    pub readership: bool,
    pub shelf: bool,
}

impl Plan {
    pub fn is_empty(&self) -> bool {
        !self.push && !self.prune && !self.clippings && !self.readership && !self.shelf
    }
}

/// What the shelf read found on the Kindle, for `deckle-shelf` to turn into a snapshot.
#[derive(Clone, Debug, Default)]
pub struct ShelfRead {
    /// The books, each KFX with its `.yjf` when it had one.
    pub books: Vec<DeviceBook>,
    /// `.yjf` left unread past the per-connection cap.
    pub skipped: usize,
    /// `system/vocabulary/vocab.db`, when the Kindle shares it.
    pub vocab: Option<VocabFiles>,
}

/// The key a device's `[sync.devices."<id>"]` uses: the USB serial for MTP, else the model and
/// the volume name (mass storage exposes no serial).
pub fn device_id(target: &Target) -> String {
    match target {
        Target::Mtp(m) => match m.serial.as_deref() {
            Some(s) if !s.trim().is_empty() => s.trim().to_owned(),
            _ => format!("{}/mtp", m.product.as_deref().unwrap_or("Kindle")),
        },
        Target::MassStorage(d) => {
            let volume = d.root.file_name().unwrap_or_default().to_string_lossy();
            match d.serial.as_deref().filter(|s| !s.is_empty()) {
                Some(serial) => format!("{}/{volume}#{serial}", d.model_hint),
                None => format!("{}/{volume}", d.model_hint),
            }
        }
        Target::Folder(dir) => dir.display().to_string(),
    }
}

/// The serial in a Kindle id: all of an MTP id, or what follows `#` in a mass-storage one.
fn id_serial(id: &str) -> Option<&str> {
    match id.rsplit_once('#') {
        Some((_, serial)) => Some(serial).filter(|s| !s.is_empty()),
        None if !id.contains('/') && !id.is_empty() => Some(id),
        None => None,
    }
}

/// Whether two Kindle ids name the same Kindle: equal, or the same serial (one Kindle seen over
/// MTP and over mass storage), or the same model and volume when either lacks the serial (an id
/// written before serials were read).
pub fn same_kindle(a: &str, b: &str) -> bool {
    if a.eq_ignore_ascii_case(b) {
        return true;
    }
    match (id_serial(a), id_serial(b)) {
        (Some(x), Some(y)) => x.eq_ignore_ascii_case(y),
        _ => {
            let base = |id: &str| id.split('#').next().unwrap_or(id).to_owned();
            a.contains('/') && b.contains('/') && base(a).eq_ignore_ascii_case(&base(b))
        }
    }
}

/// Remembers the current connection between checks, so each action runs once per connection.
#[derive(Debug, Default)]
pub struct Watcher {
    conn: Option<Connected>,
}

#[derive(Debug)]
struct Connected {
    id: String,
    /// The outbox as last pushed on this connection: a failed delivery waits for a reconnect
    /// or a new file instead of retrying every check.
    tried: Option<Vec<PathBuf>>,
    pruned: bool,
    clipped: bool,
    read: bool,
    shelved: bool,
}

impl Connected {
    /// The reads that are due on this connection, each once. The shelf needs the clippings, so
    /// it brings them when it is due.
    fn reads(&mut self, actions: Actions) -> Plan {
        let shelf = actions.shelf && !self.shelved;
        self.shelved |= shelf;
        let clippings = (actions.clippings && !self.clipped) || shelf;
        self.clipped |= clippings;
        let readership = actions.readership && !self.read;
        self.read |= readership;
        Plan {
            push: false,
            prune: false,
            clippings,
            readership,
            shelf,
        }
    }
}

impl Watcher {
    /// One check: `device` is the connected Kindle's id, `pending` the outbox.
    pub fn tick(&mut self, device: Option<&str>, pending: &[PathBuf], actions: Actions) -> Plan {
        let Some(id) = device else {
            self.conn = None;
            return Plan::default();
        };
        let conn = match &mut self.conn {
            Some(c) if c.id == id => c,
            slot => slot.insert(Connected {
                id: id.to_owned(),
                tried: None,
                pruned: false,
                clipped: false,
                read: false,
                shelved: false,
            }),
        };
        // The reads ride along with whatever opens a session, once per connection.
        if actions.push && !pending.is_empty() && conn.tried.as_deref() != Some(pending) {
            conn.tried = Some(pending.to_vec());
            conn.pruned |= actions.prune;
            return Plan {
                push: true,
                prune: actions.prune,
                ..conn.reads(actions)
            };
        }
        // With push on, prune rides along with a push: an idle plug-in opens no MTP session
        // (which would reset the Kindle to its home screen).
        if actions.prune && !actions.push && !conn.pruned {
            conn.pruned = true;
            return Plan {
                prune: true,
                ..conn.reads(actions)
            };
        }
        if IDLE_SESSION_OK {
            let plan = conn.reads(actions);
            if !plan.is_empty() {
                return plan;
            }
        }
        Plan::default()
    }
}

/// Runs `plan` over `conn` in one session: read the reading positions of the editions named
/// by `stems` (before prune removes any), push the outbox, prune, then read the shelf and the
/// clippings. The Kindle's own paper (`prune.prefix`) is left off the shelf.
pub fn run(
    queue: &Queue,
    conn: &mut Conn<'_>,
    plan: Plan,
    prunes: &[Prune],
    stems: &[String],
) -> Result<FlushReport, DeliverError> {
    run_with(
        queue,
        conn,
        plan,
        prunes,
        &Finishing::default(),
        &mut |_, _, _| {},
        stems,
        prunes.first().map_or("", |p| p.prefix.as_str()),
    )
}

/// [`run`], finishing prepared books for this Kindle with `finishing` (its profile) and
/// reporting each entry to `progress`. The Word Wise probe shares the session too. Books whose
/// file starts with `skip_prefix` (the editions) are left off the shelf. Each of `prunes` (one
/// per paper that goes to this Kindle) runs after the deliveries (many papers §4).
#[allow(clippy::too_many_arguments)]
pub fn run_with(
    queue: &Queue,
    conn: &mut Conn<'_>,
    plan: Plan,
    prunes: &[Prune],
    finishing: &Finishing,
    progress: &mut Progress<'_>,
    stems: &[String],
    skip_prefix: &str,
) -> Result<FlushReport, DeliverError> {
    // First: prune deletes an edition's `.sdr`, and its `.yjf` with it. A failed read doesn't
    // stop the delivery.
    let (positions, positions_error) = match plan.readership {
        true => match yjf::read_on(conn, stems) {
            Ok(p) => (p, None),
            Err(e) => (Vec::new(), Some(e.to_string())),
        },
        false => (Vec::new(), None),
    };
    let mut report = if plan.push {
        queue.flush_with(conn, None, finishing, progress)?
    } else {
        FlushReport::default()
    };
    if plan.prune {
        for p in prunes {
            report
                .pruned
                .extend(prune_named_on_except(conn, &p.prefix, p.keep, &p.exempt)?);
        }
    }
    // A failed read doesn't undo a delivery: it's reported beside it.
    if plan.shelf {
        // The same cache the Kindle tab keeps, so a plug-in doesn't re-read every KFX.
        let cache = finishing.word_wise.cache_dir.join("inventory");
        match list_with_positions(conn, Some(&cache), skip_prefix) {
            Ok((inventory, skipped)) => {
                let vocab = match read_vocab_on(conn) {
                    Ok(v) => v,
                    Err(e) => {
                        report.shelf_error = Some(format!("vocabulary: {e}"));
                        None
                    }
                };
                report.shelf = Some(ShelfRead {
                    books: inventory.books,
                    skipped,
                    vocab,
                });
            }
            Err(e) => report.shelf_error = Some(e.to_string()),
        }
    }
    if plan.clippings {
        match read_on(conn) {
            Ok(bytes) => report.clippings = bytes,
            Err(e) => report.clippings_error = Some(e.to_string()),
        }
    }
    report.positions = positions;
    report.positions_error = positions_error;
    Ok(report)
}

/// [`run`] for a reader on a drive (other readers design §8): each queued `.epub` goes over
/// into `Deckle/`, a KEPUB for a Kobo and as it is for a PocketBook (a book that won't convert
/// stays queued, with a warning), then the prune, then, on a Kobo, one read of its database for
/// the highlights and the shelf. Neither has a `.yjf`: `plan.readership` is ignored.
pub fn run_reader(
    queue: &Queue,
    reader: &crate::reader::Reader,
    plan: Plan,
    prune: &Prune,
    progress: &mut Progress<'_>,
) -> Result<FlushReport, DeliverError> {
    use crate::reader::{self, Reader};
    let mut report = FlushReport::default();
    if plan.push {
        let pending: Vec<PathBuf> = queue
            .pending()
            .into_iter()
            .filter(|p| p.extension().is_some_and(|x| x == "epub"))
            .collect();
        for (i, entry) in pending.iter().enumerate() {
            let io = |source| DeliverError::Io {
                path: entry.clone(),
                source,
            };
            let file = entry.file_name().unwrap_or_default().to_string_lossy();
            let name = reader.file_name(reader::stem(&file));
            progress(i + 1, pending.len(), &name);
            let bytes = std::fs::read(entry).map_err(io)?;
            match reader.prepare(&bytes) {
                Ok(ready) => {
                    let path = reader::deliver(reader, &ready, &name).map_err(io)?;
                    report.delivered.push(path);
                    std::fs::remove_file(entry).map_err(io)?;
                }
                Err(e) => report
                    .warnings
                    .push(format!("{file}: not sent, it doesn't read as an EPUB: {e}")),
            }
        }
    }
    if plan.prune {
        report.pruned =
            reader::prune(reader, &prune.prefix, prune.keep, &prune.exempt).map_err(|source| {
                DeliverError::Io {
                    path: reader.root().join(reader::FOLDER),
                    source,
                }
            })?;
    }
    // One read serves both; a failed read doesn't undo a delivery. A Kobo's database and
    // KOReader's sidecars are read together; a PocketBook has only KOReader's (KOReader reads
    // design §6).
    if plan.clippings || plan.shelf {
        let ko = crate::koreader::read(reader.root());
        report
            .warnings
            .extend(ko.warnings.iter().map(|w| format!("KOReader: {w}")));
        let read = match reader {
            Reader::Kobo(kobo) => crate::kobo::read(kobo).map(|k| crate::koreader::merge(k, ko)),
            Reader::PocketBook(_) => Ok(crate::koreader::merge(Default::default(), ko)),
        };
        match read {
            Ok(r) => {
                // The shelf prints a highlight too, so it brings the clippings, as on a Kindle.
                report.clippings = r.clippings;
                // A PocketBook without KOReader has nothing to show: no shelf for it.
                if plan.shelf && (matches!(reader, Reader::Kobo(_)) || !r.books.is_empty()) {
                    report.kobo_books = Some(r.books);
                }
            }
            Err(e) => {
                if plan.clippings {
                    report.clippings_error = Some(e.clone());
                }
                if plan.shelf {
                    report.shelf_error = Some(e);
                }
            }
        }
    }
    Ok(report)
}

/// What one push to a reMarkable did.
#[derive(Clone, Debug, Default, PartialEq, Eq, serde::Serialize)]
pub struct RemarkablePush {
    /// Files uploaded.
    pub uploaded: Vec<String>,
    /// Files the tablet already had a document of that name for: not sent twice.
    pub already: Vec<String>,
    /// `<file>: <why>` for those that failed; they stay queued.
    pub errors: Vec<String>,
}

/// Uploads what waits in `queue` to the reMarkable `r`, each once: a file whose name the
/// tablet already shows is taken as delivered (the USB web interface can't delete, so there
/// is no prune either).
pub fn run_remarkable(queue: &Queue, r: &crate::remarkable::Remarkable) -> RemarkablePush {
    use crate::remarkable;
    let mut out = RemarkablePush::default();
    let pending: Vec<PathBuf> = queue
        .pending()
        .into_iter()
        .filter(|p| p.extension().is_some_and(|x| x == "pdf" || x == "epub"))
        .collect();
    if pending.is_empty() {
        return out;
    }
    let shown: Vec<String> = match remarkable::list(r) {
        Ok(entries) => entries.into_iter().map(|e| e.name).collect(),
        Err(e) => {
            out.errors.push(e);
            return out;
        }
    };
    for entry in pending {
        let file = entry
            .file_name()
            .unwrap_or_default()
            .to_string_lossy()
            .into_owned();
        if shown.iter().any(|n| n == remarkable::shown_as(&file)) {
            let _ = std::fs::remove_file(&entry);
            out.already.push(file);
            continue;
        }
        let sent = std::fs::read(&entry)
            .map_err(|e| e.to_string())
            .and_then(|bytes| remarkable::upload(r, &file, &bytes));
        match sent {
            Ok(()) => {
                let _ = std::fs::remove_file(&entry);
                out.uploaded.push(file);
            }
            Err(e) => out.errors.push(format!("{file}: {e}")),
        }
    }
    out
}

#[cfg(test)]
mod remarkable_tests {
    use super::*;
    use crate::remarkable::tests::fake_tablet;

    #[test]
    fn the_outbox_goes_to_the_tablet_once() {
        let (base, uploads) = fake_tablet(
            "[{\"ID\":\"x\",\"VissibleName\":\"deckle-daily-2026-10-22\",\"Type\":\"DocumentType\"}]",
        );
        let r = crate::remarkable::detect_at(&base).unwrap();
        let tmp = tempfile::tempdir().unwrap();
        let q = Queue::new(tmp.path().join("outbox-remarkable"));
        std::fs::create_dir_all(&q.dir).unwrap();
        std::fs::write(q.dir.join("deckle-daily-2026-10-22.pdf"), b"%PDF-old").unwrap();
        std::fs::write(q.dir.join("deckle-daily-2026-10-23.pdf"), b"%PDF-new").unwrap();
        std::fs::write(q.dir.join("deckle-reads-a.epub"), b"EPUB").unwrap();
        let p = run_remarkable(&q, &r);
        assert_eq!(p.already, ["deckle-daily-2026-10-22.pdf"]);
        assert_eq!(
            p.uploaded,
            ["deckle-daily-2026-10-23.pdf", "deckle-reads-a.epub"]
        );
        assert!(p.errors.is_empty());
        assert_eq!(uploads.lock().unwrap().len(), 2);
        assert!(q.pending().is_empty());
    }

    #[test]
    fn a_tablet_that_stops_answering_keeps_the_outbox() {
        let free = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let r = crate::remarkable::Remarkable {
            base: format!("http://{}", free.local_addr().unwrap()),
        };
        drop(free);
        let tmp = tempfile::tempdir().unwrap();
        let q = Queue::new(tmp.path().join("outbox-remarkable"));
        std::fs::create_dir_all(&q.dir).unwrap();
        std::fs::write(q.dir.join("deckle-daily-2026-10-23.pdf"), b"%PDF").unwrap();
        let p = run_remarkable(&q, &r);
        assert_eq!(p.errors.len(), 1);
        assert_eq!(q.pending().len(), 1);
    }
}

#[cfg(test)]
mod tests {
    use std::path::Path;

    use deckle_transport::ptp::op;
    use deckle_transport::usb::MtpInfo;

    use super::*;
    use crate::deliver::Device;
    use crate::fake_mtp as fake;
    use crate::yjf::OnDevice;

    fn mtp(serial: Option<&str>, product: Option<&str>) -> Target {
        Target::Mtp(MtpInfo {
            vendor_id: deckle_transport::usb::AMAZON_VENDOR_ID,
            product_id: 1,
            serial: serial.map(Into::into),
            product: product.map(Into::into),
            bus: 1,
            address: 2,
        })
    }

    #[test]
    fn device_ids_use_the_serial_else_model_and_volume() {
        assert_eq!(device_id(&mtp(Some("G0001"), Some("Kindle"))), "G0001");
        assert_eq!(device_id(&mtp(Some(""), Some("Kindle"))), "Kindle/mtp");
        assert_eq!(device_id(&mtp(None, None)), "Kindle/mtp");
        let ms = Target::MassStorage(Device {
            root: PathBuf::from("/Volumes/Kindle"),
            model_hint: "Kindle Paperwhite".into(),
            thumbnails_writable: true,
            firmware: None,
            serial: None,
        });
        assert_eq!(device_id(&ms), "Kindle Paperwhite/Kindle");
        // Two Paperwhites on the same volume name are told apart by their USB serial.
        let Target::MassStorage(mut d) = ms else {
            unreachable!()
        };
        d.serial = Some("G0B0AAAA11112222".into());
        assert_eq!(
            device_id(&Target::MassStorage(d)),
            "Kindle Paperwhite/Kindle#G0B0AAAA11112222"
        );
    }

    fn files(names: &[&str]) -> Vec<PathBuf> {
        names.iter().map(PathBuf::from).collect()
    }

    const PUSH: Plan = Plan {
        push: true,
        prune: false,
        clippings: false,
        readership: false,
        shelf: false,
    };
    const PRUNE: Plan = Plan {
        push: false,
        prune: true,
        clippings: false,
        readership: false,
        shelf: false,
    };
    const BOTH: Plan = Plan {
        push: true,
        prune: true,
        clippings: false,
        readership: false,
        shelf: false,
    };
    const NONE: Plan = Plan {
        push: false,
        prune: false,
        clippings: false,
        readership: false,
        shelf: false,
    };

    #[test]
    fn a_connection_runs_only_the_enabled_actions_once() {
        let a = files(&["a.kfx"]);
        let push_only = Actions {
            push: true,
            prune: false,
            clippings: false,
            readership: false,
            shelf: false,
        };
        let mut w = Watcher::default();
        assert_eq!(w.tick(Some("K"), &a, push_only), PUSH);
        assert_eq!(
            w.tick(Some("K"), &a, push_only),
            NONE,
            "once per connection"
        );

        let prune_only = Actions {
            push: false,
            prune: true,
            clippings: false,
            readership: false,
            shelf: false,
        };
        let mut w = Watcher::default();
        assert_eq!(w.tick(Some("K"), &a, prune_only), PRUNE);
        assert_eq!(w.tick(Some("K"), &a, prune_only), NONE);
        assert_eq!(w.tick(None, &a, prune_only), NONE);
        assert_eq!(
            w.tick(Some("K"), &a, prune_only),
            PRUNE,
            "a reconnect runs again"
        );
        assert_eq!(
            w.tick(Some("other"), &a, prune_only),
            PRUNE,
            "another Kindle is another connection"
        );

        let off = Actions {
            push: false,
            prune: false,
            clippings: false,
            readership: false,
            shelf: false,
        };
        let mut w = Watcher::default();
        assert_eq!(w.tick(Some("K"), &a, off), NONE);
        assert_eq!(w.tick(Some("K"), &[], off), NONE);
    }

    /// The decisions of `digest watch` before auto-sync, verbatim but for the push itself.
    fn old_loop(tried: &mut Option<Vec<PathBuf>>, kindle: bool, pending: Vec<PathBuf>) -> bool {
        match kindle {
            false => *tried = None,
            true if pending.is_empty() || tried.as_ref() == Some(&pending) => {}
            true => {
                *tried = Some(pending);
                return true;
            }
        }
        false
    }

    #[test]
    fn an_unconfigured_device_behaves_as_watch_did() {
        let a = files(&["deckle-daily-2026-10-19.kfx"]);
        let b = files(&["deckle-daily-2026-10-20.kfx"]);
        let ab = files(&["deckle-daily-2026-10-20.kfx", "long.kfx"]);
        let script: Vec<(bool, Vec<PathBuf>)> = vec![
            (false, vec![]),
            (false, a.clone()),
            (true, vec![]),
            (true, a.clone()),
            (true, a.clone()),
            (true, b.clone()),
            (true, ab.clone()),
            (true, vec![]),
            (true, ab.clone()),
            (false, ab.clone()),
            (true, ab.clone()),
            (true, ab.clone()),
            (false, vec![]),
            (true, vec![]),
        ];
        let mut tried = None;
        let mut w = Watcher::default();
        for (i, (kindle, pending)) in script.into_iter().enumerate() {
            let pushed = old_loop(&mut tried, kindle, pending.clone());
            let plan = w.tick(kindle.then_some("K"), &pending, Actions::default());
            let want = if pushed { BOTH } else { NONE };
            assert_eq!(plan, want, "check {i}");
        }
    }

    #[test]
    fn push_without_prune_leaves_old_editions() {
        let tmp = tempfile::tempdir().unwrap();
        let (q, kindle) = outbox_and_folder(tmp.path());
        let target = Target::Folder(kindle.clone());
        let r = run(&q, &mut Conn::new(&target), PUSH, &[prune()], &[]).unwrap();
        assert_eq!(r.delivered.len(), 1);
        assert!(r.pruned.is_empty());
        assert!(kindle.join("deckle-daily-2026-10-17.kfx").exists());

        let r = run(&q, &mut Conn::new(&target), PRUNE, &[prune()], &[]).unwrap();
        assert!(r.delivered.is_empty());
        assert_eq!(r.pruned, ["deckle-daily-2026-10-17.kfx"]);
    }

    fn prune() -> Prune {
        Prune {
            prefix: "deckle-daily-".into(),
            keep: 2,
            ..Default::default()
        }
    }

    fn outbox_and_folder(tmp: &Path) -> (Queue, PathBuf) {
        let src = tmp.join("book.kfx");
        std::fs::write(&src, b"KFX").unwrap();
        let q = Queue::new(tmp.join("outbox"));
        q.add(&src, "deckle-daily-2026-10-20.kfx", None).unwrap();
        let kindle = tmp.join("kindle");
        std::fs::create_dir_all(&kindle).unwrap();
        for old in ["deckle-daily-2026-10-17.kfx", "deckle-daily-2026-10-18.kfx"] {
            std::fs::write(kindle.join(old), b"KFX").unwrap();
        }
        (q, kindle)
    }

    #[test]
    fn push_and_prune_share_one_mtp_session() {
        let tmp = tempfile::tempdir().unwrap();
        let (q, _) = outbox_and_folder(tmp.path());
        fake::reset(&["deckle-daily-2026-10-17.kfx", "deckle-daily-2026-10-18.kfx"]);
        let target = mtp(Some("G0001"), Some("Kindle"));
        let r = {
            let mut conn = Conn::with_opener(&target, fake::open);
            run(&q, &mut conn, BOTH, &[prune()], &[]).unwrap()
        };
        assert_eq!(
            r.delivered,
            [PathBuf::from(
                "mtp://G0001/documents/deckle-daily-2026-10-20.kfx"
            )]
        );
        assert_eq!(r.pruned, ["deckle-daily-2026-10-17.kfx"]);
        assert_eq!(
            fake::documents(),
            ["deckle-daily-2026-10-18.kfx", "deckle-daily-2026-10-20.kfx"]
        );
        let ops = fake::ops();
        assert_eq!(fake::opens(), 1, "one connection");
        assert_eq!(ops.iter().filter(|&&o| o == op::OPEN_SESSION).count(), 1);
        assert_eq!(ops.last(), Some(&op::CLOSE_SESSION));
        let sent = ops.iter().position(|&o| o == op::SEND_OBJECT).unwrap();
        let deleted = ops.iter().position(|&o| o == op::DELETE_OBJECT).unwrap();
        assert!(sent < deleted, "push, then prune");
    }

    #[test]
    fn a_prune_alone_is_one_session_and_sends_nothing() {
        let tmp = tempfile::tempdir().unwrap();
        let (q, _) = outbox_and_folder(tmp.path());
        fake::reset(&[
            "deckle-daily-2026-10-17.kfx",
            "deckle-daily-2026-10-18.kfx",
            "x.kfx",
        ]);
        let target = mtp(None, Some("Kindle"));
        let r = run(
            &q,
            &mut Conn::with_opener(&target, fake::open),
            PRUNE,
            &[prune()],
            &[],
        )
        .unwrap();
        assert!(r.delivered.is_empty());
        assert!(r.pruned.is_empty(), "two editions is within keep");
        assert_eq!(q.pending().len(), 1, "the outbox waits");
        assert_eq!(fake::opens(), 1);
        assert!(!fake::ops().contains(&op::SEND_OBJECT_INFO));
    }

    #[test]
    fn clippings_ride_along_with_a_push_once_per_connection() {
        let a = files(&["a.kfx"]);
        let on = Actions {
            push: true,
            prune: true,
            clippings: true,
            readership: false,
            shelf: false,
        };
        let mut w = Watcher::default();
        assert_eq!(
            w.tick(Some("K"), &a, on),
            Plan {
                push: true,
                prune: true,
                clippings: true,
                readership: false,
                shelf: false,
            }
        );
        let b = files(&["b.kfx"]);
        assert_eq!(w.tick(Some("K"), &b, on), BOTH, "once per connection");
        assert_eq!(w.tick(None, &b, on), NONE);
        assert!(
            w.tick(Some("K"), &b, on).clippings,
            "a reconnect exports again"
        );
    }

    #[test]
    fn clippings_ride_along_with_a_prune_alone() {
        let on = Actions {
            push: false,
            prune: true,
            clippings: true,
            readership: false,
            shelf: false,
        };
        let mut w = Watcher::default();
        assert_eq!(
            w.tick(Some("K"), &[], on),
            Plan {
                push: false,
                prune: true,
                clippings: true,
                readership: false,
                shelf: false,
            }
        );
    }

    #[test]
    fn an_idle_plug_in_exports_only_when_sessions_are_harmless() {
        let on = Actions {
            push: true,
            prune: true,
            clippings: true,
            readership: false,
            shelf: false,
        };
        let mut w = Watcher::default();
        let want = Plan {
            push: false,
            prune: false,
            clippings: IDLE_SESSION_OK,
            readership: false,
            shelf: false,
        };
        assert_eq!(w.tick(Some("K"), &[], on), want);
        assert_eq!(w.tick(Some("K"), &[], on), NONE);
    }

    #[test]
    fn clippings_share_the_push_session() {
        let tmp = tempfile::tempdir().unwrap();
        let (q, _) = outbox_and_folder(tmp.path());
        fake::reset(&[]);
        fake::put_bytes("My Clippings.txt", b"Walden (H)\n");
        let target = mtp(Some("G0001"), Some("Kindle"));
        let plan = Plan {
            push: true,
            prune: false,
            clippings: true,
            readership: false,
            shelf: false,
        };
        let r = run(
            &q,
            &mut Conn::with_opener(&target, fake::open),
            plan,
            &[prune()],
            &[],
        )
        .unwrap();
        assert_eq!(r.delivered.len(), 1);
        assert_eq!(r.clippings.as_deref(), Some(&b"Walden (H)\n"[..]));
        assert_eq!(fake::opens(), 1, "one connection");

        let alone = Plan {
            push: false,
            prune: false,
            clippings: true,
            readership: false,
            shelf: false,
        };
        fake::reset(&[]);
        let r = run(
            &q,
            &mut Conn::with_opener(&target, fake::open),
            alone,
            &[prune()],
            &[],
        )
        .unwrap();
        assert_eq!(r.clippings, None, "no file: nothing, and no error");
        assert_eq!(fake::opens(), 1);
    }

    #[test]
    fn a_failed_clippings_read_keeps_the_delivery() {
        let tmp = tempfile::tempdir().unwrap();
        let (q, _) = outbox_and_folder(tmp.path());
        fake::reset(&[]);
        fake::put_bytes("My Clippings.txt", b"Walden (H)\n");
        fake::fail_get_object();
        let target = mtp(Some("G0001"), Some("Kindle"));
        let plan = Plan {
            push: true,
            prune: false,
            clippings: true,
            readership: false,
            shelf: false,
        };
        let r = run(
            &q,
            &mut Conn::with_opener(&target, fake::open),
            plan,
            &[prune()],
            &[],
        )
        .expect("the push succeeded");
        assert_eq!(r.delivered.len(), 1);
        assert_eq!(r.clippings, None);
        assert!(r.clippings_error.as_deref().unwrap().contains("MTP"));
    }

    #[test]
    fn readership_rides_along_with_a_push_once_per_connection() {
        let a = files(&["a.kfx"]);
        let on = Actions {
            push: true,
            prune: true,
            clippings: false,
            readership: true,
            shelf: false,
        };
        let mut w = Watcher::default();
        assert!(w.tick(Some("K"), &a, on).readership);
        let b = files(&["b.kfx"]);
        assert!(!w.tick(Some("K"), &b, on).readership, "once per connection");
        assert_eq!(w.tick(None, &b, on), NONE);
        assert!(
            w.tick(Some("K"), &b, on).readership,
            "a reconnect reads again"
        );
    }

    #[test]
    fn readership_runs_alone_on_an_idle_plug_in() {
        let on = Actions {
            push: true,
            prune: true,
            clippings: false,
            readership: true,
            shelf: false,
        };
        let mut w = Watcher::default();
        let p = w.tick(Some("K"), &[], on);
        assert_eq!(
            (p.push, p.prune, p.readership),
            (false, false, IDLE_SESSION_OK)
        );
        assert_eq!(w.tick(Some("K"), &[], on), NONE);
    }

    #[test]
    fn positions_are_read_before_prune_removes_the_edition() {
        let tmp = tempfile::tempdir().unwrap();
        let (q, _) = outbox_and_folder(tmp.path());
        fake::reset(&["deckle-daily-2026-10-17.kfx", "deckle-daily-2026-10-18.kfx"]);
        fake::put_bytes(
            "deckle-daily-2026-10-17.sdr/deckle-daily-2026-10-179d6e5f44.yjf",
            b"YJF17",
        );
        let target = mtp(Some("G0001"), Some("Kindle"));
        let plan = Plan {
            push: true,
            prune: true,
            clippings: false,
            readership: true,
            shelf: false,
        };
        let stems = vec![
            "deckle-daily-2026-10-18".to_owned(),
            "deckle-daily-2026-10-17".to_owned(),
        ];
        let r = run(
            &q,
            &mut Conn::with_opener(&target, fake::open),
            plan,
            &[prune()],
            &stems,
        )
        .unwrap();
        assert_eq!(
            r.positions,
            [
                ("deckle-daily-2026-10-18".to_owned(), OnDevice::Unopened),
                (
                    "deckle-daily-2026-10-17".to_owned(),
                    OnDevice::Opened(b"YJF17".to_vec())
                ),
            ]
        );
        assert!(
            !fake::documents().contains(&"deckle-daily-2026-10-17.kfx".to_owned()),
            "pruned after the read: {:?}",
            fake::documents()
        );
        assert_eq!(fake::opens(), 1, "one connection");
    }

    #[test]
    fn a_failed_yjf_read_keeps_the_delivery() {
        let tmp = tempfile::tempdir().unwrap();
        let (q, _) = outbox_and_folder(tmp.path());
        fake::reset(&["deckle-daily-2026-10-18.kfx"]);
        fake::put_bytes("deckle-daily-2026-10-18.sdr/x.yjf", b"YJF");
        fake::fail_get_object();
        let target = mtp(Some("G0001"), Some("Kindle"));
        let plan = Plan {
            push: true,
            prune: false,
            clippings: false,
            readership: true,
            shelf: false,
        };
        let r = run(
            &q,
            &mut Conn::with_opener(&target, fake::open),
            plan,
            &[prune()],
            &["deckle-daily-2026-10-18".to_owned()],
        )
        .expect("the push succeeded");
        assert_eq!(r.delivered.len(), 1);
        assert!(r.positions.is_empty());
        assert!(r.positions_error.as_deref().unwrap().contains("MTP"));
    }

    #[test]
    fn shelf_rides_with_push_once_per_connection() {
        let a = files(&["a.kfx"]);
        let on = Actions {
            push: true,
            prune: true,
            clippings: false,
            readership: false,
            shelf: true,
        };
        let mut w = Watcher::default();
        let p = w.tick(Some("K"), &a, on);
        assert!(p.shelf && p.clippings, "the shelf reads the clippings too");
        let b = files(&["b.kfx"]);
        let p = w.tick(Some("K"), &b, on);
        assert!(!p.shelf && !p.clippings, "once per connection");
        assert_eq!(w.tick(None, &b, on), NONE);
        assert!(w.tick(Some("K"), &b, on).shelf, "a reconnect reads again");
    }

    #[test]
    fn shelf_runs_alone_on_an_idle_plug_in() {
        let on = Actions {
            push: true,
            prune: true,
            clippings: false,
            readership: false,
            shelf: true,
        };
        let mut w = Watcher::default();
        let p = w.tick(Some("K"), &[], on);
        assert_eq!((p.push, p.prune, p.shelf), (false, false, IDLE_SESSION_OK));
    }

    #[test]
    fn shelf_read_in_the_same_session() {
        let tmp = tempfile::tempdir().unwrap();
        let (q, _) = outbox_and_folder(tmp.path());
        fake::reset(&["Book.kfx", "deckle-longread-walden.kfx"]);
        fake::put_bytes("Book.sdr/Book9d6e.yjf", b"YJF");
        fake::put_bytes("deckle-longread-walden.sdr/w9d6e.yjf", b"LONG");
        let target = mtp(Some("G0001"), Some("Kindle"));
        let plan = Plan {
            push: true,
            prune: false,
            clippings: true,
            readership: false,
            shelf: true,
        };
        let r = run(
            &q,
            &mut Conn::with_opener(&target, fake::open),
            plan,
            &[prune()],
            &[],
        )
        .unwrap();
        assert_eq!(r.delivered.len(), 1);
        assert_eq!(fake::opens(), 1, "one connection");
        let shelf = r.shelf.expect("the shelf was read");
        let yjf = |f: &str| {
            shelf
                .books
                .iter()
                .find(|b| b.file == f)
                .and_then(|b| b.yjf.clone())
        };
        assert_eq!(yjf("Book.kfx").as_deref(), Some(&b"YJF"[..]));
        assert_eq!(
            yjf("deckle-longread-walden.kfx").as_deref(),
            Some(&b"LONG"[..])
        );
        assert_eq!(
            yjf("deckle-daily-2026-10-20.kfx"),
            None,
            "the paper itself is skipped"
        );
        assert_eq!(r.shelf_error, None);
    }

    #[test]
    fn a_bad_yjf_fetch_costs_one_book_not_the_shelf() {
        let tmp = tempfile::tempdir().unwrap();
        let (q, _) = outbox_and_folder(tmp.path());
        fake::reset(&["Book.kfx"]);
        fake::put_bytes("Book.sdr/Book9d6e.yjf", b"YJF");
        fake::fail_get_object();
        let target = mtp(Some("G0001"), Some("Kindle"));
        let plan = Plan {
            push: true,
            prune: false,
            clippings: false,
            readership: false,
            shelf: true,
        };
        let r = run(
            &q,
            &mut Conn::with_opener(&target, fake::open),
            plan,
            &[prune()],
            &[],
        )
        .expect("the push succeeded");
        assert_eq!(r.delivered.len(), 1);
        let shelf = r.shelf.expect("the listing survives one bad sidecar");
        let book = shelf.books.iter().find(|b| b.file == "Book.kfx").unwrap();
        assert_eq!(book.yjf, None);
    }

    #[test]
    fn nothing_to_do_opens_nothing() {
        let tmp = tempfile::tempdir().unwrap();
        let (q, _) = outbox_and_folder(tmp.path());
        fake::reset(&[]);
        let target = mtp(None, None);
        run(
            &q,
            &mut Conn::with_opener(&target, fake::open),
            NONE,
            &[prune()],
            &[],
        )
        .unwrap();
        assert_eq!(fake::opens(), 0);
    }
}

#[cfg(test)]
mod kobo_tests {
    use super::*;
    use crate::kobo::tests::fake_kobo;
    use crate::reader::Reader;

    fn epub(text: &str) -> Vec<u8> {
        use std::io::Write;
        let mut z = zip::ZipWriter::new(std::io::Cursor::new(Vec::new()));
        let o = zip::write::SimpleFileOptions::default();
        for (n, d) in [
            ("mimetype", "application/epub+zip".to_owned()),
            (
                "META-INF/container.xml",
                "<container xmlns=\"urn:oasis:names:tc:opendocument:xmlns:container\"><rootfiles>\
                 <rootfile full-path=\"c.opf\"/></rootfiles></container>"
                    .to_owned(),
            ),
            (
                "c.opf",
                "<package xmlns=\"http://www.idpf.org/2007/opf\"><manifest><item id=\"a\" \
                 href=\"a.xhtml\" media-type=\"application/xhtml+xml\"/></manifest></package>"
                    .to_owned(),
            ),
            (
                "a.xhtml",
                format!("<html><body><p>{text}</p></body></html>"),
            ),
        ] {
            z.start_file(n, o).unwrap();
            z.write_all(d.as_bytes()).unwrap();
        }
        z.finish().unwrap().into_inner()
    }

    const ALL: Plan = Plan {
        push: true,
        prune: true,
        clippings: true,
        readership: true,
        shelf: true,
    };

    #[test]
    fn a_kobo_gets_the_epub_outbox_as_kepubs_then_the_prune_and_its_reads() {
        let tmp = tempfile::tempdir().unwrap();
        let kobo = Reader::Kobo(fake_kobo(
            &tmp.path().join("KOBOeReader"),
            "INSERT INTO content VALUES ('file:///mnt/onboard/x.epub', '6', 'X', 'A', 10, \
             '2026-10-20T08:00:00Z', 1, NULL);",
        ));
        std::fs::create_dir_all(tmp.path().join("KOBOeReader/Deckle")).unwrap();
        for d in ["2026-10-01", "2026-10-02"] {
            std::fs::write(
                tmp.path()
                    .join(format!("KOBOeReader/Deckle/deckle-daily-{d}.kepub.epub")),
                b"old",
            )
            .unwrap();
        }
        let q = Queue::new(tmp.path().join("outbox-epub"));
        std::fs::create_dir_all(&q.dir).unwrap();
        std::fs::write(q.dir.join("deckle-daily-2026-10-03.epub"), epub("Today.")).unwrap();
        std::fs::write(q.dir.join("broken.epub"), b"not a zip").unwrap();
        let prune = Prune {
            prefix: "deckle-daily-".into(),
            keep: 2,
            exempt: Vec::new(),
        };
        let r = run_reader(&q, &kobo, ALL, &prune, &mut |_, _, _| {}).unwrap();
        assert_eq!(
            r.delivered,
            [tmp.path()
                .join("KOBOeReader/Deckle/deckle-daily-2026-10-03.kepub.epub")]
        );
        let sent = std::fs::read(&r.delivered[0]).unwrap();
        assert!(
            crate::kepub::convert(&sent).unwrap() == sent,
            "already a KEPUB"
        );
        assert_eq!(r.pruned, ["deckle-daily-2026-10-01.kepub.epub"]);
        // The broken one stays queued with a warning; the sent one is gone from the outbox.
        let left: Vec<_> = q
            .pending()
            .iter()
            .map(|p| p.file_name().unwrap().to_owned())
            .collect();
        assert_eq!(left, ["broken.epub"]);
        assert!(r.warnings[0].starts_with("broken.epub: not sent"));
        assert_eq!(r.kobo_books.as_ref().unwrap()[0].title, "X");
        assert_eq!(r.clippings, None);
        assert!(r.positions.is_empty());
    }

    #[test]
    fn a_failed_read_is_reported_beside_the_delivery() {
        let tmp = tempfile::tempdir().unwrap();
        let kobo = Reader::Kobo(fake_kobo(tmp.path(), ""));
        std::fs::write(tmp.path().join(".kobo/KoboReader.sqlite"), b"garbage").unwrap();
        let q = Queue::new(tmp.path().join("outbox-epub"));
        let r = run_reader(&q, &kobo, ALL, &Prune::default(), &mut |_, _, _| {}).unwrap();
        assert!(r.clippings_error.is_some() && r.shelf_error.is_some());
    }
}

#[cfg(test)]
mod pocketbook_tests {
    use super::*;
    use crate::reader::{PocketBook, Reader};

    #[test]
    fn a_pocketbook_gets_the_epub_as_it_is_and_is_not_read() {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path().join("PB");
        std::fs::create_dir_all(&root).unwrap();
        let pb = Reader::PocketBook(PocketBook { root: root.clone() });
        let q = Queue::new(tmp.path().join("outbox-epub"));
        std::fs::create_dir_all(&q.dir).unwrap();
        let epub = {
            use std::io::Write;
            let mut z = zip::ZipWriter::new(std::io::Cursor::new(Vec::new()));
            z.start_file("mimetype", zip::write::SimpleFileOptions::default())
                .unwrap();
            z.write_all(b"application/epub+zip").unwrap();
            z.finish().unwrap().into_inner()
        };
        std::fs::write(q.dir.join("deckle-daily-2026-10-03.epub"), &epub).unwrap();
        let plan = Plan {
            push: true,
            prune: true,
            clippings: true,
            readership: false,
            shelf: true,
        };
        let prune = Prune {
            prefix: "deckle-daily-".into(),
            keep: 3,
            exempt: Vec::new(),
        };
        let r = run_reader(&q, &pb, plan, &prune, &mut |_, _, _| {}).unwrap();
        let sent = root.join("Deckle/deckle-daily-2026-10-03.epub");
        assert_eq!(r.delivered, std::slice::from_ref(&sent));
        assert_eq!(std::fs::read(sent).unwrap(), epub);
        assert!(r.kobo_books.is_none() && r.clippings.is_none() && r.clippings_error.is_none());
    }

    #[test]
    fn a_pocketbook_running_koreader_is_read_for_highlights_and_books() {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path().join("PB");
        std::fs::create_dir_all(root.join("Books/Walden.sdr")).unwrap();
        std::fs::write(
            root.join("Books/Walden.sdr/metadata.epub.lua"),
            "return { [\"annotations\"] = { [1] = { [\"datetime\"] = \"2026-10-21 08:15:03\", \
             [\"drawer\"] = \"lighten\", [\"text\"] = \"Simplify.\" } }, \
             [\"doc_props\"] = { [\"title\"] = \"Walden\" }, [\"percent_finished\"] = 0.5 }",
        )
        .unwrap();
        let pb = Reader::PocketBook(PocketBook { root });
        let q = Queue::new(tmp.path().join("outbox-epub"));
        let plan = Plan {
            push: false,
            prune: false,
            clippings: true,
            readership: false,
            shelf: true,
        };
        let prune = Prune {
            prefix: "deckle-daily-".into(),
            keep: 3,
            exempt: Vec::new(),
        };
        let r = run_reader(&q, &pb, plan, &prune, &mut |_, _, _| {}).unwrap();
        let text = String::from_utf8(r.clippings.unwrap()).unwrap();
        assert!(
            text.starts_with("Walden\n- Your Highlight | Added on"),
            "{text}"
        );
        let books = r.kobo_books.unwrap();
        assert_eq!(books.len(), 1);
        assert_eq!(books[0].content_id, "Books/Walden.epub");
        assert_eq!(books[0].percent, 50.0);
    }
}

#[cfg(test)]
mod same_kindle_tests {
    use super::same_kindle;

    #[test]
    fn one_kindle_by_id_serial_or_older_id() {
        // Equal, ignoring case.
        assert!(same_kindle("G000AAAA", "g000aaaa"));
        // MTP serial and the same Kindle on mass storage.
        assert!(same_kindle("G000AAAA", "Kindle/Kindle#G000AAAA"));
        // An id written before serials were read.
        assert!(same_kindle("Kindle/Kindle", "Kindle/Kindle#G000AAAA"));
        // Different serials, volumes or kinds.
        assert!(!same_kindle(
            "Kindle/Kindle#G000AAAA",
            "Kindle/Kindle#G000BBBB"
        ));
        assert!(!same_kindle("G000AAAA", "G000BBBB"));
        assert!(!same_kindle("Kindle/Mom", "Kindle/Dad"));
        assert!(!same_kindle("G000AAAA", "Kindle/Kindle"));
    }
}
