// SPDX-License-Identifier: GPL-3.0-or-later
//! A Kindle on the far side of an MTP session, for tests: a root with a `documents` folder,
//! answering the operations delivery, pruning and the Word Wise probe use, and logging every
//! operation code. Open it with `Conn::with_opener(&target, fake_mtp::open)`.
//!
//! Like a real one it refuses what it doesn't know: an operation it doesn't list, and a second
//! object with a name already in that folder. It renames by SetObjectPropValue unless told not
//! to ([`no_rename`]), and a send can be made to fail half-way ([`fail_next_send`]).

use std::cell::RefCell;
use std::collections::{HashMap, VecDeque};

use deckle_transport::ptp::{
    Container, ContainerKind, DeviceInfo, ObjectInfo, StorageInfo, decode_string, encode_string,
    fmt, op, prop, rsp,
};
use deckle_transport::session::{Mtp, Transport, TransportError};
use deckle_transport::usb::MtpInfo;

const STORAGE: u32 = 0x0001_0001;
const DOCUMENTS: u32 = 1;
/// What GetStorageInfo reports.
pub const CAPACITY: u64 = 31_000_000_000;
pub const FREE: u64 = 12_000_000_000;

struct Kindle {
    objects: Vec<(u32, ObjectInfo)>,
    /// What GetObject returns: a file put with [`put_bytes`], or sent.
    contents: HashMap<u32, Vec<u8>>,
    /// GetObject answers with an error (a read that times out on a real Kindle).
    fail_get_object: bool,
    /// The next SendObject keeps half the bytes and answers with an error.
    fail_send: bool,
    /// SetObjectPropValue and GetObjectPropDesc are listed and answered.
    rename: bool,
    free: u64,
    /// The object SendObjectInfo made, which the next SendObject fills.
    sending: Option<u32>,
    ops: Vec<u16>,
    opens: u32,
    next: u32,
}

impl Default for Kindle {
    fn default() -> Self {
        Kindle {
            objects: Vec::new(),
            contents: HashMap::new(),
            fail_get_object: false,
            fail_send: false,
            rename: true,
            free: FREE,
            sending: None,
            ops: Vec::new(),
            opens: 0,
            next: 0,
        }
    }
}

/// The operations this Kindle lists in GetDeviceInfo and answers.
fn supported(rename: bool) -> Vec<u16> {
    let mut ops = vec![
        op::GET_DEVICE_INFO,
        op::OPEN_SESSION,
        op::CLOSE_SESSION,
        op::GET_STORAGE_IDS,
        op::GET_STORAGE_INFO,
        op::GET_OBJECT_HANDLES,
        op::GET_OBJECT_INFO,
        op::GET_OBJECT,
        op::DELETE_OBJECT,
        op::SEND_OBJECT_INFO,
        op::SEND_OBJECT,
    ];
    if rename {
        ops.extend([op::GET_OBJECT_PROP_DESC, op::SET_OBJECT_PROP_VALUE]);
    }
    ops
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
        // Past the handles given to `docs`, which would collide with 100+ for a long list.
        k.next = 100.max(10 + docs.len() as u32);
    });
}

/// Puts `path` under `documents/` (`a.kfx`, `a.sdr/XRAY.entities.B0X.asc`, `Sub/b.pdf`),
/// making its folders; a file gets `size` bytes.
pub fn put(path: &str, size: u32) {
    KINDLE.with(|k| {
        let mut k = k.borrow_mut();
        let mut parent = DOCUMENTS;
        let parts: Vec<&str> = path.split('/').collect();
        for (i, part) in parts.iter().enumerate() {
            let folder = i + 1 < parts.len();
            let found = k
                .objects
                .iter()
                .find(|(_, o)| o.parent == parent && o.filename == *part)
                .map(|(h, _)| *h);
            parent = match found {
                Some(h) => h,
                None => {
                    let h = k.next;
                    k.next += 1;
                    let mut o = info(parent, part, folder);
                    if !folder {
                        o.size = size;
                    }
                    k.objects.push((h, o));
                    h
                }
            };
        }
    });
}

/// [`put`] with these bytes as the file's contents.
pub fn put_bytes(path: &str, bytes: &[u8]) {
    put(path, bytes.len() as u32);
    KINDLE.with(|k| {
        let mut k = k.borrow_mut();
        let name = path.rsplit('/').next().unwrap_or(path);
        let h = k
            .objects
            .iter()
            .rev()
            .find(|(_, o)| o.filename == name && !o.association)
            .map(|(h, _)| *h)
            .unwrap();
        k.contents.insert(h, bytes.to_vec());
    });
}

/// Makes every GetObject fail until the next [`reset`].
pub fn fail_get_object() {
    KINDLE.with(|k| k.borrow_mut().fail_get_object = true);
}

/// The next SendObject stops half-way: half the bytes stay in the object and the Kindle
/// answers with an error, as when a cable is pulled or the Kindle fills up.
pub fn fail_next_send() {
    KINDLE.with(|k| k.borrow_mut().fail_send = true);
}

/// A Kindle that neither lists nor answers SetObjectPropValue: it can't rename, until the next
/// [`reset`].
pub fn no_rename() {
    KINDLE.with(|k| k.borrow_mut().rename = false);
}

/// What GetStorageInfo reports as free, until the next [`reset`].
pub fn set_free(bytes: u64) {
    KINDLE.with(|k| k.borrow_mut().free = bytes);
}

/// The bytes of the file at `path` under `documents/`, as put or sent.
pub fn bytes(path: &str) -> Option<Vec<u8>> {
    KINDLE.with(|k| {
        let k = k.borrow();
        let mut parent = DOCUMENTS;
        let mut found = None;
        for part in path.split('/') {
            let (h, _) = k
                .objects
                .iter()
                .find(|(_, o)| o.parent == parent && o.filename == part)?;
            parent = *h;
            found = Some(*h);
        }
        k.contents.get(&found?).cloned()
    })
}

/// Every path under `documents/`, folders with a trailing `/`, sorted.
pub fn paths() -> Vec<String> {
    fn walk(k: &Kindle, parent: u32, prefix: &str, out: &mut Vec<String>) {
        for (h, o) in k.objects.iter().filter(|(_, o)| o.parent == parent) {
            if o.association {
                out.push(format!("{prefix}{}/", o.filename));
                walk(k, *h, &format!("{prefix}{}/", o.filename), out);
            } else {
                out.push(format!("{prefix}{}", o.filename));
            }
        }
    }
    let mut out = Vec::new();
    KINDLE.with(|k| walk(&k.borrow(), DOCUMENTS, "", &mut out));
    out.sort();
    out
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
    response(rsp::OK, tx, params)
}

fn response(code: u16, tx: u32, params: Vec<u32>) -> Vec<u8> {
    Container {
        kind: ContainerKind::Response,
        code,
        tx_id: tx,
        params,
    }
    .encode(&[])
}

impl Kindle {
    /// Another object in `parent` already has `name` (the Kindle compares without case).
    fn taken(&self, parent: u32, name: &str, except: u32) -> bool {
        self.objects.iter().any(|(h, o)| {
            *h != except && o.parent == parent && o.filename.eq_ignore_ascii_case(name)
        })
    }
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
            if !supported(k.rename).contains(&c.code) {
                self.replies
                    .push_back(response(rsp::OPERATION_NOT_SUPPORTED, tx, Vec::new()));
                return;
            }
            match c.code {
                op::GET_DEVICE_INFO => {
                    let info = DeviceInfo {
                        manufacturer: "Amazon".into(),
                        model: "Kindle Scribe".into(),
                        device_version: "5.18.1.0.1".into(),
                        serial_number: "FAKE".into(),
                        operations: supported(k.rename),
                    };
                    self.replies.push_back(data(c.code, tx, &info.encode()));
                }
                op::GET_OBJECT_PROP_DESC => {
                    let mut desc = Vec::new();
                    desc.extend_from_slice(&prop::OBJECT_FILE_NAME.to_le_bytes());
                    desc.extend_from_slice(&0xFFFFu16.to_le_bytes()); // a string
                    desc.push(1); // get and set
                    desc.extend(encode_string(""));
                    desc.extend_from_slice(&0u32.to_le_bytes());
                    desc.push(0);
                    self.replies.push_back(data(c.code, tx, &desc));
                }
                op::SET_OBJECT_PROP_VALUE => {
                    let (h, code) = (c.params[0], c.params[1]);
                    let name = decode_string(&payload).map(|(s, _)| s);
                    let parent = k
                        .objects
                        .iter()
                        .find(|(o, _)| *o == h)
                        .map(|(_, i)| i.parent);
                    match (u16::try_from(code), name, parent) {
                        (Ok(prop::OBJECT_FILE_NAME), Ok(name), Some(parent))
                            if !k.taken(parent, &name, h) =>
                        {
                            let (_, i) = k.objects.iter_mut().find(|(o, _)| *o == h).unwrap();
                            i.filename = name;
                        }
                        _ => {
                            self.replies
                                .push_back(response(rsp::GENERAL_ERROR, tx, Vec::new()));
                            return;
                        }
                    }
                }
                op::GET_STORAGE_IDS => {
                    self.replies.push_back(data(c.code, tx, &u32s(&[STORAGE])));
                }
                op::GET_STORAGE_INFO => {
                    let s = StorageInfo {
                        max_capacity: CAPACITY,
                        free_bytes: k.free,
                        description: "Internal Storage".into(),
                        volume_label: "Kindle".into(),
                    };
                    self.replies.push_back(data(c.code, tx, &s.encode()));
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
                op::GET_OBJECT if k.fail_get_object => {
                    self.replies.push_back(
                        Container {
                            kind: ContainerKind::Response,
                            code: rsp::INVALID_DATASET,
                            tx_id: tx,
                            params: Vec::new(),
                        }
                        .encode(&[]),
                    );
                    return;
                }
                op::GET_OBJECT => {
                    let bytes = k.contents.get(&c.params[0]).cloned().unwrap_or_default();
                    self.replies.push_back(data(c.code, tx, &bytes));
                }
                op::DELETE_OBJECT => {
                    let h = c.params[0];
                    k.objects.retain(|(o, i)| *o != h && i.parent != h);
                    k.contents.remove(&h);
                }
                op::SEND_OBJECT_INFO => {
                    let mut i = ObjectInfo::decode(&payload).unwrap();
                    i.parent = c.params[1];
                    // The Kindle rejects two objects with one name in a folder.
                    if k.taken(i.parent, &i.filename, 0) {
                        self.replies
                            .push_back(response(rsp::GENERAL_ERROR, tx, Vec::new()));
                        return;
                    }
                    let h = k.next;
                    k.next += 1;
                    k.objects.push((h, i));
                    k.sending = Some(h);
                    self.replies
                        .push_back(ok(tx, vec![STORAGE, c.params[1], h]));
                    return;
                }
                op::SEND_OBJECT => {
                    let Some(h) = k.sending.take() else {
                        self.replies
                            .push_back(response(rsp::GENERAL_ERROR, tx, Vec::new()));
                        return;
                    };
                    if std::mem::take(&mut k.fail_send) {
                        k.contents.insert(h, payload[..payload.len() / 2].to_vec());
                        self.replies
                            .push_back(response(rsp::INCOMPLETE_TRANSFER, tx, Vec::new()));
                        return;
                    }
                    if let Some((_, i)) = k.objects.iter_mut().find(|(o, _)| *o == h) {
                        i.size = payload.len() as u32;
                    }
                    k.contents.insert(h, payload);
                }
                // Open and close session: nothing to do.
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
        if matches!(
            c.code,
            op::SEND_OBJECT_INFO | op::SEND_OBJECT | op::SET_OBJECT_PROP_VALUE
        ) {
            self.awaiting = Some((c, Vec::new()));
        } else {
            self.answer(c, Vec::new());
        }
        Ok(())
    }

    fn read(&mut self, buf: &mut [u8]) -> Result<usize, TransportError> {
        let mut r = self.replies.pop_front().ok_or(TransportError::Timeout)?;
        // A reply longer than the reader's buffer arrives over several reads, as over USB.
        if r.len() > buf.len() {
            self.replies.push_front(r.split_off(buf.len()));
        }
        buf[..r.len()].copy_from_slice(&r);
        Ok(r.len())
    }
}
