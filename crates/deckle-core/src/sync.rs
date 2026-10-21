// SPDX-License-Identifier: GPL-3.0-or-later
//! Auto-sync on plug-in (auto-sync design): which of a device's actions a check of the watcher
//! runs, and running them over one connection.

use std::path::PathBuf;

use crate::deliver::{Conn, DeliverError, Target, prune_named_on};
use crate::queue::{FlushReport, Prune, Queue};

/// The actions a device has on (`[sync]` in `digest.toml`). Clippings (#60) isn't one yet.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Actions {
    pub push: bool,
    pub prune: bool,
}

impl Default for Actions {
    /// An unconfigured device: what `digest watch` always did.
    fn default() -> Self {
        Actions {
            push: true,
            prune: true,
        }
    }
}

/// What one check runs, both over one connection.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Plan {
    pub push: bool,
    pub prune: bool,
}

impl Plan {
    pub fn is_empty(&self) -> bool {
        !self.push && !self.prune
    }
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
            }),
        };
        if actions.push && !pending.is_empty() && conn.tried.as_deref() != Some(pending) {
            conn.tried = Some(pending.to_vec());
            conn.pruned |= actions.prune;
            return Plan {
                push: true,
                prune: actions.prune,
            };
        }
        // With push on, prune rides along with a push: an idle plug-in opens no MTP session
        // (which would reset the Kindle to its home screen).
        if actions.prune && !actions.push && !conn.pruned {
            conn.pruned = true;
            return Plan {
                push: false,
                prune: true,
            };
        }
        Plan::default()
    }
}

/// Runs `plan` over `conn`: push the outbox, then prune, in one session.
pub fn run(
    queue: &Queue,
    conn: &mut Conn<'_>,
    plan: Plan,
    prune: &Prune,
) -> Result<FlushReport, DeliverError> {
    if plan.push {
        return queue.flush_on(conn, plan.prune.then_some(prune));
    }
    let mut report = FlushReport::default();
    if plan.prune {
        report.pruned = prune_named_on(conn, &prune.prefix, prune.keep)?;
    }
    Ok(report)
}

#[cfg(test)]
mod tests {
    use std::path::Path;

    use deckle_transport::ptp::op;
    use deckle_transport::usb::MtpInfo;

    use super::*;
    use crate::deliver::Device;

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
    };
    const PRUNE: Plan = Plan {
        push: false,
        prune: true,
    };
    const BOTH: Plan = Plan {
        push: true,
        prune: true,
    };
    const NONE: Plan = Plan {
        push: false,
        prune: false,
    };

    #[test]
    fn a_connection_runs_only_the_enabled_actions_once() {
        let a = files(&["a.kfx"]);
        let push_only = Actions {
            push: true,
            prune: false,
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
        let r = run(&q, &mut Conn::new(&target), PUSH, &prune()).unwrap();
        assert_eq!(r.delivered.len(), 1);
        assert!(r.pruned.is_empty());
        assert!(kindle.join("deckle-daily-2026-10-17.kfx").exists());

        let r = run(&q, &mut Conn::new(&target), PRUNE, &prune()).unwrap();
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
            run(&q, &mut conn, BOTH, &prune()).unwrap()
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
        )
        .unwrap();
        assert!(r.delivered.is_empty());
        assert!(r.pruned.is_empty(), "two editions is within keep");
        assert_eq!(q.pending().len(), 1, "the outbox waits");
        assert_eq!(fake::opens(), 1);
        assert!(!fake::ops().contains(&op::SEND_OBJECT_INFO));
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
        )
        .unwrap();
        assert_eq!(fake::opens(), 0);
    }

    /// A Kindle on the far side of an MTP session: a root with a `documents` folder, answering
    /// the operations delivery and pruning use, and logging every operation code.
    mod fake {
        use std::cell::RefCell;
        use std::collections::VecDeque;

        use deckle_transport::ptp::{Container, ContainerKind, ObjectInfo, fmt, op, rsp};
        use deckle_transport::session::{Mtp, Transport, TransportError};
        use deckle_transport::usb::MtpInfo;

        const STORAGE: u32 = 0x0001_0001;
        const DOCUMENTS: u32 = 1;

        #[derive(Default)]
        struct Kindle {
            objects: Vec<(u32, ObjectInfo)>,
            ops: Vec<u16>,
            opens: u32,
            next: u32,
        }

        thread_local! {
            static KINDLE: RefCell<Kindle> = RefCell::new(Kindle::default());
        }

        fn info(parent: u32, name: &str, folder: bool) -> ObjectInfo {
            ObjectInfo {
                storage_id: STORAGE,
                format: if folder {
                    fmt::ASSOCIATION
                } else {
                    fmt::UNDEFINED
                },
                size: 0,
                parent,
                association: folder,
                filename: name.into(),
            }
        }

        pub fn reset(docs: &[&str]) {
            KINDLE.with(|k| {
                let mut k = k.borrow_mut();
                *k = Kindle::default();
                k.objects.push((DOCUMENTS, info(0, "documents", true)));
                for (i, n) in docs.iter().enumerate() {
                    k.objects.push((10 + i as u32, info(DOCUMENTS, n, false)));
                }
                k.next = 100;
            });
        }

        pub fn documents() -> Vec<String> {
            KINDLE.with(|k| {
                let mut names: Vec<String> = k
                    .borrow()
                    .objects
                    .iter()
                    .filter(|(_, i)| i.parent == DOCUMENTS)
                    .map(|(_, i)| i.filename.clone())
                    .collect();
                names.sort();
                names
            })
        }

        pub fn ops() -> Vec<u16> {
            KINDLE.with(|k| k.borrow().ops.clone())
        }

        pub fn opens() -> u32 {
            KINDLE.with(|k| k.borrow().opens)
        }

        pub fn open(_: &MtpInfo) -> Result<Mtp, TransportError> {
            KINDLE.with(|k| k.borrow_mut().opens += 1);
            Ok(Mtp::new(Box::new(Wire::default())))
        }

        #[derive(Default)]
        struct Wire {
            replies: VecDeque<Vec<u8>>,
            /// A command whose data phase is still arriving, with the bytes so far.
            awaiting: Option<(Container, Vec<u8>)>,
        }

        fn ok(tx: u32, params: Vec<u32>) -> Vec<u8> {
            Container {
                kind: ContainerKind::Response,
                code: rsp::OK,
                tx_id: tx,
                params,
            }
            .encode(&[])
        }

        fn data(code: u16, tx: u32, payload: &[u8]) -> Vec<u8> {
            Container {
                kind: ContainerKind::Data,
                code,
                tx_id: tx,
                params: Vec::new(),
            }
            .encode(payload)
        }

        fn u32s(v: &[u32]) -> Vec<u8> {
            let mut out = (v.len() as u32).to_le_bytes().to_vec();
            for x in v {
                out.extend_from_slice(&x.to_le_bytes());
            }
            out
        }

        impl Wire {
            fn answer(&mut self, c: Container, payload: Vec<u8>) {
                let tx = c.tx_id;
                KINDLE.with(|k| {
                    let mut k = k.borrow_mut();
                    match c.code {
                        op::GET_STORAGE_IDS => {
                            self.replies.push_back(data(c.code, tx, &u32s(&[STORAGE])));
                        }
                        op::GET_OBJECT_HANDLES => {
                            let parent = match c.params[2] {
                                0xFFFF_FFFF => 0,
                                p => p,
                            };
                            let hs: Vec<u32> = k
                                .objects
                                .iter()
                                .filter(|(_, i)| i.parent == parent)
                                .map(|(h, _)| *h)
                                .collect();
                            self.replies.push_back(data(c.code, tx, &u32s(&hs)));
                        }
                        op::GET_OBJECT_INFO => {
                            let (_, i) = k.objects.iter().find(|(h, _)| *h == c.params[0]).unwrap();
                            self.replies.push_back(data(c.code, tx, &i.encode()));
                        }
                        op::DELETE_OBJECT => {
                            let h = c.params[0];
                            k.objects.retain(|(o, i)| *o != h && i.parent != h);
                        }
                        op::SEND_OBJECT_INFO => {
                            let mut i = ObjectInfo::decode(&payload).unwrap();
                            i.parent = c.params[1];
                            let h = k.next;
                            k.next += 1;
                            k.objects.push((h, i));
                            self.replies
                                .push_back(ok(tx, vec![STORAGE, c.params[1], h]));
                            return;
                        }
                        _ => {}
                    }
                    self.replies.push_back(ok(tx, Vec::new()));
                });
            }
        }

        impl Transport for Wire {
            fn write(&mut self, bytes: &[u8]) -> Result<(), TransportError> {
                if bytes.is_empty() {
                    return Ok(()); // the zero-length packet ending a data phase
                }
                if let Some((_, buf)) = &mut self.awaiting {
                    buf.extend_from_slice(bytes);
                    let total = u32::from_le_bytes(buf[..4].try_into().unwrap()) as usize;
                    if buf.len() >= total {
                        let (c, buf) = self.awaiting.take().unwrap();
                        let (_, payload) = Container::decode(&buf).unwrap();
                        self.answer(c, payload);
                    }
                    return Ok(());
                }
                let (c, _) = Container::decode(bytes).unwrap();
                KINDLE.with(|k| k.borrow_mut().ops.push(c.code));
                if matches!(c.code, op::SEND_OBJECT_INFO | op::SEND_OBJECT) {
                    self.awaiting = Some((c, Vec::new()));
                } else {
                    self.answer(c, Vec::new());
                }
                Ok(())
            }

            fn read(&mut self, buf: &mut [u8]) -> Result<usize, TransportError> {
                let r = self.replies.pop_front().ok_or(TransportError::Timeout)?;
                buf[..r.len()].copy_from_slice(&r);
                Ok(r.len())
            }
        }
    }
}
