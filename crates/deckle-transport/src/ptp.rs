// SPDX-License-Identifier: GPL-3.0-or-later
//! PTP (ISO 15740) / MTP wire format: containers, strings, arrays, ObjectInfo.

/// Operation codes.
pub mod op {
    pub const GET_DEVICE_INFO: u16 = 0x1001;
    pub const OPEN_SESSION: u16 = 0x1002;
    pub const CLOSE_SESSION: u16 = 0x1003;
    pub const GET_STORAGE_IDS: u16 = 0x1004;
    pub const GET_STORAGE_INFO: u16 = 0x1005;
    pub const GET_OBJECT_HANDLES: u16 = 0x1007;
    pub const GET_OBJECT_INFO: u16 = 0x1008;
    pub const GET_OBJECT: u16 = 0x1009;
    pub const DELETE_OBJECT: u16 = 0x100B;
    pub const SEND_OBJECT_INFO: u16 = 0x100C;
    pub const SEND_OBJECT: u16 = 0x100D;
}

/// Response codes.
pub mod rsp {
    pub const OK: u16 = 0x2001;
    pub const INVALID_DATASET: u16 = 0xA806;
}

/// Object formats.
pub mod fmt {
    pub const UNDEFINED: u16 = 0x3000;
    pub const ASSOCIATION: u16 = 0x3001;
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u16)]
pub enum ContainerKind {
    Command = 1,
    Data = 2,
    Response = 3,
    Event = 4,
}

#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum PtpError {
    #[error("PTP container is truncated")]
    Truncated,
    #[error("unknown PTP container type {0}")]
    UnknownKind(u16),
    #[error("malformed PTP string")]
    BadString,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Container {
    pub kind: ContainerKind,
    pub code: u16,
    pub tx_id: u32,
    pub params: Vec<u32>,
}

impl Container {
    /// Header + params + payload. Data containers carry the payload; others take `&[]`.
    pub fn encode(&self, payload: &[u8]) -> Vec<u8> {
        let len = 12 + 4 * self.params.len() + payload.len();
        let mut out = Vec::with_capacity(len);
        out.extend_from_slice(&(len as u32).to_le_bytes());
        out.extend_from_slice(&(self.kind as u16).to_le_bytes());
        out.extend_from_slice(&self.code.to_le_bytes());
        out.extend_from_slice(&self.tx_id.to_le_bytes());
        for p in &self.params {
            out.extend_from_slice(&p.to_le_bytes());
        }
        out.extend_from_slice(payload);
        out
    }

    /// Parses a container; for data containers the remainder is the payload, for the others the
    /// remainder is parsed as u32 parameters.
    pub fn decode(bytes: &[u8]) -> Result<(Container, Vec<u8>), PtpError> {
        if bytes.len() < 12 {
            return Err(PtpError::Truncated);
        }
        let kind = match u16::from_le_bytes([bytes[4], bytes[5]]) {
            1 => ContainerKind::Command,
            2 => ContainerKind::Data,
            3 => ContainerKind::Response,
            4 => ContainerKind::Event,
            k => return Err(PtpError::UnknownKind(k)),
        };
        let code = u16::from_le_bytes([bytes[6], bytes[7]]);
        let tx_id = u32::from_le_bytes(bytes[8..12].try_into().unwrap());
        let rest = &bytes[12..];
        if kind == ContainerKind::Data {
            return Ok((
                Container {
                    kind,
                    code,
                    tx_id,
                    params: vec![],
                },
                rest.to_vec(),
            ));
        }
        let params = rest
            .as_chunks::<4>()
            .0
            .iter()
            .map(|c| u32::from_le_bytes(*c))
            .collect();
        Ok((
            Container {
                kind,
                code,
                tx_id,
                params,
            },
            Vec::new(),
        ))
    }
}

/// PTP string: u8 count of UTF-16 code units including the terminator, then UTF-16LE.
pub fn encode_string(s: &str) -> Vec<u8> {
    if s.is_empty() {
        return vec![0];
    }
    let units: Vec<u16> = s.encode_utf16().chain(std::iter::once(0)).collect();
    let mut out = vec![units.len() as u8];
    for u in units {
        out.extend_from_slice(&u.to_le_bytes());
    }
    out
}

/// Returns the string and the number of bytes consumed.
pub fn decode_string(bytes: &[u8]) -> Result<(String, usize), PtpError> {
    let count = *bytes.first().ok_or(PtpError::BadString)? as usize;
    if count == 0 {
        return Ok((String::new(), 1));
    }
    let need = 1 + count * 2;
    if bytes.len() < need {
        return Err(PtpError::BadString);
    }
    let units: Vec<u16> = bytes[1..need]
        .as_chunks::<2>()
        .0
        .iter()
        .map(|c| u16::from_le_bytes(*c))
        .collect();
    let text: Vec<u16> = units.into_iter().take_while(|u| *u != 0).collect();
    Ok((String::from_utf16_lossy(&text), need))
}

pub fn decode_u32_array(bytes: &[u8]) -> Result<Vec<u32>, PtpError> {
    if bytes.len() < 4 {
        return Err(PtpError::Truncated);
    }
    let n = u32::from_le_bytes(bytes[..4].try_into().unwrap()) as usize;
    if bytes.len() < 4 + 4 * n {
        return Err(PtpError::Truncated);
    }
    Ok(bytes[4..4 + 4 * n]
        .as_chunks::<4>()
        .0
        .iter()
        .map(|c| u32::from_le_bytes(*c))
        .collect())
}

/// The ObjectInfo dataset, always in the 32-bit ObjectCompressedSize layout. libmtp #436: the
/// Kindle's nameless root folders fool libmtp into the 64-bit layout and the device then
/// rejects every SendObjectInfo with 0xA806; we never switch layouts.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ObjectInfo {
    pub storage_id: u32,
    pub format: u16,
    pub size: u32,
    pub parent: u32,
    pub association: bool,
    pub filename: String,
}

impl ObjectInfo {
    pub fn encode(&self) -> Vec<u8> {
        let mut o = Vec::with_capacity(64 + self.filename.len() * 2);
        o.extend_from_slice(&self.storage_id.to_le_bytes());
        o.extend_from_slice(&self.format.to_le_bytes());
        o.extend_from_slice(&0u16.to_le_bytes()); // protection status
        o.extend_from_slice(&self.size.to_le_bytes()); // ObjectCompressedSize (32-bit)
        o.extend_from_slice(&0u16.to_le_bytes()); // thumb format
        o.extend_from_slice(&0u32.to_le_bytes()); // thumb size
        o.extend_from_slice(&0u32.to_le_bytes()); // thumb width
        o.extend_from_slice(&0u32.to_le_bytes()); // thumb height
        o.extend_from_slice(&0u32.to_le_bytes()); // image width
        o.extend_from_slice(&0u32.to_le_bytes()); // image height
        o.extend_from_slice(&0u32.to_le_bytes()); // image bit depth
        o.extend_from_slice(&self.parent.to_le_bytes());
        o.extend_from_slice(&(if self.association { 1u16 } else { 0u16 }).to_le_bytes()); // association type (generic folder)
        o.extend_from_slice(&0u32.to_le_bytes()); // association desc
        o.extend_from_slice(&0u32.to_le_bytes()); // sequence number
        o.extend(encode_string(&self.filename));
        o.push(0); // capture date
        o.push(0); // modification date
        o.push(0); // keywords
        o
    }

    pub fn decode(bytes: &[u8]) -> Result<ObjectInfo, PtpError> {
        if bytes.len() < 53 {
            return Err(PtpError::Truncated);
        }
        let u32_at = |i: usize| u32::from_le_bytes(bytes[i..i + 4].try_into().unwrap());
        let u16_at = |i: usize| u16::from_le_bytes([bytes[i], bytes[i + 1]]);
        let format = u16_at(4);
        let (filename, _) = decode_string(&bytes[52..])?;
        Ok(ObjectInfo {
            storage_id: u32_at(0),
            format,
            size: u32_at(8),
            parent: u32_at(38),
            association: format == fmt::ASSOCIATION,
            filename,
        })
    }
}
