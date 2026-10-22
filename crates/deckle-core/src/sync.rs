// SPDX-License-Identifier: GPL-3.0-or-later
//! Auto-sync on plug-in (auto-sync design): which of a device's actions a check of the watcher
//! runs, and running them over one connection.

use std::path::PathBuf;

use crate::clippings::read_on;
use crate::deliver::{Conn, DeliverError, Target, prune_named_on};
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
            format!("{}/{volume}", d.model_hint)
        }
        Target::Folder(dir) => dir.display().to_string(),
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
    prune: &Prune,
    stems: &[String],
) -> Result<FlushReport, DeliverError> {
    run_with(
        queue,
        conn,
        plan,
        prune,
        &Finishing::default(),
        &mut |_, _, _| {},
        stems,
        &prune.prefix,
    )
}

/// [`run`], finishing prepared books for this Kindle with `finishing` (its profile) and
/// reporting each entry to `progress`. The Word Wise probe shares the session too. Books whose
/// file starts with `skip_prefix` (the editions) are left off the shelf.
#[allow(clippy::too_many_arguments)]
pub fn run_with(
    queue: &Queue,
    conn: &mut Conn<'_>,
    plan: Plan,
    prune: &Prune,
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
        queue.flush_with(conn, plan.prune.then_some(prune), finishing, progress)?
    } else {
        FlushReport::default()
    };
    if plan.prune && !plan.push {
        report.pruned = prune_named_on(conn, &prune.prefix, prune.keep)?;
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
        });
        assert_eq!(device_id(&ms), "Kindle Paperwhite/Kindle");
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
        let r = run(&q, &mut Conn::new(&target), PUSH, &prune(), &[]).unwrap();
        assert_eq!(r.delivered.len(), 1);
        assert!(r.pruned.is_empty());
        assert!(kindle.join("deckle-daily-2026-10-17.kfx").exists());

        let r = run(&q, &mut Conn::new(&target), PRUNE, &prune(), &[]).unwrap();
        assert!(r.delivered.is_empty());
        assert_eq!(r.pruned, ["deckle-daily-2026-10-17.kfx"]);
    }

    fn prune() -> Prune {
        Prune {
            prefix: "deckle-daily-".into(),
            keep: 2,
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
            run(&q, &mut conn, BOTH, &prune(), &[]).unwrap()
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
            &prune(),
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
            &prune(),
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
            &prune(),
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
            &prune(),
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
            &prune(),
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
            &prune(),
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
            &prune(),
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
            &prune(),
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
            &prune(),
            &[],
        )
        .unwrap();
        assert_eq!(fake::opens(), 0);
    }
}
