// SPDX-License-Identifier: GPL-3.0-or-later
//! PTP transactions over any bulk transport: command, optional data phase, response.

use std::any::Any;

use crate::ptp::{
    Container, ContainerKind, DeviceInfo, ObjectInfo, PtpError, StorageInfo, decode_u32_array, fmt,
    op, rsp,
};

#[derive(Debug, thiserror::Error)]
pub enum TransportError {
    #[error("USB transfer timed out")]
    Timeout,
    #[error("USB error: {0}")]
    Usb(String),
}

/// A bulk pipe to the device.
pub trait Transport: Any {
    fn write(&mut self, bytes: &[u8]) -> Result<(), TransportError>;
    /// Reads one container (or one chunk of a large data phase); returns bytes read.
    fn read(&mut self, buf: &mut [u8]) -> Result<usize, TransportError>;
    /// wMaxPacketSize of the bulk OUT endpoint (512 high speed, 1024 super speed).
    fn max_packet_out(&self) -> usize {
        512
    }
}

#[derive(Debug, thiserror::Error)]
pub enum MtpError {
    #[error(transparent)]
    Transport(#[from] TransportError),
    #[error(transparent)]
    Ptp(#[from] PtpError),
    #[error("device answered {op:#06x} with response code {code:#06x}")]
    Response { op: u16, code: u16 },
    #[error("unexpected container {0:?} during {1:#06x}")]
    Unexpected(ContainerKind, u16),
    #[error("device returned no {0}")]
    Missing(&'static str),
    #[error("the Kindle has no documents folder; nothing was written")]
    NoDocumentsFolder,
}

/// PTP response: a session is already open (a previous run did not close it).
pub const RSP_SESSION_ALREADY_OPEN: u16 = 0x201E;

/// Bulk reads of a data phase are at most this large per USB transfer.
const READ_CHUNK: usize = 1024 * 1024;
/// Bulk writes are split into this size so the device's buffers keep up.
const WRITE_CHUNK: usize = 512 * 1024;

pub struct Mtp {
    transport: Box<dyn Transport>,
    tx: u32,
}

impl Mtp {
    pub fn new(transport: Box<dyn Transport>) -> Self {
        Mtp { transport, tx: 0 }
    }

    /// Access the concrete transport (tests).
    pub fn transport_mut<T: Transport>(&mut self) -> Option<&mut T> {
        (self.transport.as_mut() as &mut dyn Any).downcast_mut::<T>()
    }

    fn next_tx(&mut self) -> u32 {
        self.tx = self.tx.wrapping_add(1);
        self.tx
    }

    fn read_container(&mut self) -> Result<(Container, Vec<u8>), MtpError> {
        let mut buf = vec![0u8; READ_CHUNK];
        let mut n = self.transport.read(&mut buf)?;
        // A zero-length packet ends a data phase whose size was a multiple of wMaxPacketSize;
        // the next read holds the real container.
        if n == 0 {
            n = self.transport.read(&mut buf)?;
        }
        buf.truncate(n);
        let (c, mut payload) = Container::decode(&buf)?;
        if c.kind == ContainerKind::Data {
            let total = u32::from_le_bytes(buf[..4].try_into().unwrap()) as usize;
            let mut have = buf.len();
            while have < total {
                let mut more = vec![0u8; READ_CHUNK.min(total - have)];
                let n = self.transport.read(&mut more)?;
                if n == 0 {
                    break;
                }
                payload.extend_from_slice(&more[..n]);
                have += n;
            }
        }
        Ok((c, payload))
    }

    /// Command with no outgoing data; returns (incoming data if any, response params).
    fn transact(
        &mut self,
        code: u16,
        params: &[u32],
        data_out: Option<&[u8]>,
    ) -> Result<(Option<Vec<u8>>, Vec<u32>), MtpError> {
        let tx = self.next_tx();
        let cmd = Container {
            kind: ContainerKind::Command,
            code,
            tx_id: tx,
            params: params.to_vec(),
        };
        self.transport.write(&cmd.encode(&[]))?;
        if let Some(data) = data_out {
            let header = Container {
                kind: ContainerKind::Data,
                code,
                tx_id: tx,
                params: vec![],
            };
            let bytes = header.encode(data);
            for chunk in bytes.chunks(WRITE_CHUNK) {
                self.transport.write(chunk)?;
            }
            // USB bulk: a data phase that is an exact multiple of wMaxPacketSize must be
            // terminated with a zero-length packet, or the device waits forever.
            let max = self.transport.max_packet_out();
            if max > 0 && bytes.len().is_multiple_of(max) {
                self.transport.write(&[])?;
            }
        }
        let mut data_in = None;
        loop {
            let (c, payload) = self.read_container()?;
            if c.kind != ContainerKind::Event && c.tx_id != tx {
                continue; // stale container from an aborted transaction
            }
            match c.kind {
                ContainerKind::Data => data_in = Some(payload),
                ContainerKind::Response => {
                    if c.code != rsp::OK {
                        return Err(MtpError::Response {
                            op: code,
                            code: c.code,
                        });
                    }
                    return Ok((data_in, c.params));
                }
                ContainerKind::Event => continue,
                other => return Err(MtpError::Unexpected(other, code)),
            }
        }
    }

    /// Opens session 1. PIMA 15740 §9.3.1: OpenSession uses TransactionID 0. A stale open
    /// session (crashed previous run) is closed and reopened.
    pub fn open_session(&mut self) -> Result<(), MtpError> {
        self.tx = u32::MAX; // next_tx wraps to 0 for OpenSession
        match self.transact(op::OPEN_SESSION, &[1], None) {
            Ok(_) => Ok(()),
            Err(MtpError::Response {
                code: RSP_SESSION_ALREADY_OPEN,
                ..
            }) => {
                self.tx = 0;
                let _ = self.transact(op::CLOSE_SESSION, &[], None);
                self.tx = u32::MAX;
                self.transact(op::OPEN_SESSION, &[1], None).map(|_| ())
            }
            Err(e) => Err(e),
        }
    }

    pub fn close_session(&mut self) -> Result<(), MtpError> {
        self.transact(op::CLOSE_SESSION, &[], None).map(|_| ())
    }

    /// GetDeviceInfo: the model and, in `device_version`, the firmware.
    pub fn device_info(&mut self) -> Result<DeviceInfo, MtpError> {
        let (data, _) = self.transact(op::GET_DEVICE_INFO, &[], None)?;
        Ok(DeviceInfo::decode(
            &data.ok_or(MtpError::Missing("device info"))?,
        )?)
    }

    pub fn storages(&mut self) -> Result<Vec<u32>, MtpError> {
        let (data, _) = self.transact(op::GET_STORAGE_IDS, &[], None)?;
        Ok(decode_u32_array(
            &data.ok_or(MtpError::Missing("storage ids"))?,
        )?)
    }

    /// GetStorageInfo: the storage's capacity and free space.
    pub fn storage_info(&mut self, storage: u32) -> Result<StorageInfo, MtpError> {
        let (data, _) = self.transact(op::GET_STORAGE_INFO, &[storage], None)?;
        Ok(StorageInfo::decode(
            &data.ok_or(MtpError::Missing("storage info"))?,
        )?)
    }

    /// Object handles directly under `parent` (0 = storage root).
    pub fn object_handles(&mut self, storage: u32, parent: u32) -> Result<Vec<u32>, MtpError> {
        let parent_param = if parent == 0 { 0xFFFF_FFFF } else { parent };
        let (data, _) = self.transact(op::GET_OBJECT_HANDLES, &[storage, 0, parent_param], None)?;
        Ok(decode_u32_array(
            &data.ok_or(MtpError::Missing("object handles"))?,
        )?)
    }

    pub fn object_info(&mut self, handle: u32) -> Result<ObjectInfo, MtpError> {
        let (data, _) = self.transact(op::GET_OBJECT_INFO, &[handle], None)?;
        Ok(ObjectInfo::decode(
            &data.ok_or(MtpError::Missing("object info"))?,
        )?)
    }

    /// Children of `parent` as (handle, info).
    pub fn list(&mut self, storage: u32, parent: u32) -> Result<Vec<(u32, ObjectInfo)>, MtpError> {
        let mut out = Vec::new();
        for h in self.object_handles(storage, parent)? {
            out.push((h, self.object_info(h)?));
        }
        Ok(out)
    }

    /// Handle of the child named `name` (case-insensitive) under `parent`.
    pub fn find_child(
        &mut self,
        storage: u32,
        parent: u32,
        name: &str,
    ) -> Result<Option<u32>, MtpError> {
        for (h, info) in self.list(storage, parent)? {
            if info.filename.eq_ignore_ascii_case(name) {
                return Ok(Some(h));
            }
        }
        Ok(None)
    }

    /// The Kindle's `documents` folder: at the storage root, or one level down inside one of
    /// the nameless root associations some firmwares expose (libmtp #436). Never created.
    pub fn find_documents(&mut self, storage: u32) -> Result<u32, MtpError> {
        let root = self.list(storage, 0)?;
        if let Some((h, _)) = root
            .iter()
            .find(|(_, i)| i.filename.eq_ignore_ascii_case("documents"))
        {
            return Ok(*h);
        }
        for (h, _) in root
            .iter()
            .filter(|(_, i)| i.association && i.filename.is_empty())
        {
            if let Some(d) = self.find_child(storage, *h, "documents")? {
                return Ok(d);
            }
        }
        Err(MtpError::NoDocumentsFolder)
    }

    /// Creates a folder and returns its handle.
    pub fn make_folder(&mut self, storage: u32, parent: u32, name: &str) -> Result<u32, MtpError> {
        let info = ObjectInfo {
            storage_id: storage,
            format: fmt::ASSOCIATION,
            size: 0,
            parent,
            association: true,
            filename: name.to_owned(),
        };
        let (_, params) = self.transact(
            op::SEND_OBJECT_INFO,
            &[storage, parent],
            Some(&info.encode()),
        )?;
        params
            .get(2)
            .copied()
            .ok_or(MtpError::Missing("new object handle"))
    }

    /// Finds or creates `name` under `parent`.
    pub fn ensure_folder(
        &mut self,
        storage: u32,
        parent: u32,
        name: &str,
    ) -> Result<u32, MtpError> {
        match self.find_child(storage, parent, name)? {
            Some(h) => Ok(h),
            None => self.make_folder(storage, parent, name),
        }
    }

    /// Sends a file (SendObjectInfo then SendObject) and returns its handle.
    pub fn send_file(
        &mut self,
        storage: u32,
        parent: u32,
        name: &str,
        bytes: &[u8],
    ) -> Result<u32, MtpError> {
        let info = ObjectInfo {
            storage_id: storage,
            format: fmt::UNDEFINED,
            size: bytes.len() as u32,
            parent,
            association: false,
            filename: name.to_owned(),
        };
        let (_, params) = self.transact(
            op::SEND_OBJECT_INFO,
            &[storage, parent],
            Some(&info.encode()),
        )?;
        let handle = params
            .get(2)
            .copied()
            .ok_or(MtpError::Missing("new object handle"))?;
        self.transact(op::SEND_OBJECT, &[], Some(bytes))?;
        Ok(handle)
    }

    /// Downloads a file (GetObject).
    pub fn get_object(&mut self, handle: u32) -> Result<Vec<u8>, MtpError> {
        let (data, _) = self.transact(op::GET_OBJECT, &[handle], None)?;
        data.ok_or(MtpError::Missing("object data"))
    }

    /// The folder that holds `documents` (and `system`): the storage root (0) or a nameless
    /// root association.
    pub fn content_root(&mut self, storage: u32) -> Result<u32, MtpError> {
        let docs = self.find_documents(storage)?;
        Ok(self.object_info(docs)?.parent)
    }

    /// Handle of `path` (e.g. `["system", "kll", "x.klld"]`) under `parent`, if every part exists.
    pub fn find_path(
        &mut self,
        storage: u32,
        parent: u32,
        path: &[&str],
    ) -> Result<Option<u32>, MtpError> {
        let mut at = parent;
        for part in path {
            match self.find_child(storage, at, part)? {
                Some(h) => at = h,
                None => return Ok(None),
            }
        }
        Ok(Some(at))
    }

    pub fn delete(&mut self, handle: u32) -> Result<(), MtpError> {
        self.transact(op::DELETE_OBJECT, &[handle, 0], None)
            .map(|_| ())
    }
}
