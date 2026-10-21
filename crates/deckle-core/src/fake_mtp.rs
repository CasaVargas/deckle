// SPDX-License-Identifier: GPL-3.0-or-later
//! A Kindle on the far side of an MTP session, for tests: a root with a `documents` folder,
//! answering the operations delivery, pruning and the Word Wise probe use, and logging every
//! operation code. Open it with `Conn::with_opener(&target, fake_mtp::open)`.

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
    children(DOCUMENTS)
}

/// The names in `documents/<folder>` (a book's `.sdr`), sorted; empty when it doesn't exist.
#[cfg(unix)]
pub fn in_documents(folder: &str) -> Vec<String> {
    let handle = KINDLE.with(|k| {
        k.borrow()
            .objects
            .iter()
            .find(|(_, i)| i.parent == DOCUMENTS && i.filename == folder)
            .map(|(h, _)| *h)
    });
    handle.map(children).unwrap_or_default()
}

fn children(parent: u32) -> Vec<String> {
    KINDLE.with(|k| {
        let mut names: Vec<String> = k
            .borrow()
            .objects
            .iter()
            .filter(|(_, i)| i.parent == parent)
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
