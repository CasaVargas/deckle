// SPDX-License-Identifier: GPL-3.0-or-later
//! calibre's wireless framing (calibre bridge design §1): each message is its JSON's length in
//! decimal, then the JSON, `[opcode, {args}]`, with nothing between. Book bytes go raw, outside
//! the framing.

use std::io::{self, Read, Write};

use serde_json::Value;

/// The largest JSON message taken from a device.
pub const MAX_JSON: usize = 16 * 1024 * 1024;

/// calibre's opcodes (`driver.py`'s `opcodes`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u8)]
pub enum Op {
    Ok = 0,
    SetCalibreDeviceInfo = 1,
    SetCalibreDeviceName = 2,
    GetDeviceInformation = 3,
    TotalSpace = 4,
    FreeSpace = 5,
    GetBookCount = 6,
    SendBooklists = 7,
    SendBook = 8,
    GetInitializationInfo = 9,
    BookDone = 11,
    Noop = 12,
    DeleteBook = 13,
    GetBookFileSegment = 14,
    GetBookMetadata = 15,
    SendBookMetadata = 16,
    DisplayMessage = 17,
    CalibreBusy = 18,
    SetLibraryInfo = 19,
    Error = 20,
    GetCollections = 21,
    UpdateCollections = 22,
}

impl Op {
    pub fn from_code(code: u64) -> Option<Op> {
        use Op::*;
        Some(match code {
            0 => Ok,
            1 => SetCalibreDeviceInfo,
            2 => SetCalibreDeviceName,
            3 => GetDeviceInformation,
            4 => TotalSpace,
            5 => FreeSpace,
            6 => GetBookCount,
            7 => SendBooklists,
            8 => SendBook,
            9 => GetInitializationInfo,
            11 => BookDone,
            12 => Noop,
            13 => DeleteBook,
            14 => GetBookFileSegment,
            15 => GetBookMetadata,
            16 => SendBookMetadata,
            17 => DisplayMessage,
            18 => CalibreBusy,
            19 => SetLibraryInfo,
            20 => Error,
            21 => GetCollections,
            22 => UpdateCollections,
            _ => return None,
        })
    }
}

/// One message, framed. The JSON is ASCII, non-ASCII escaped as calibre's `json.dumps` does, so
/// its length in bytes and in characters agree.
pub fn encode(op: Op, arg: &Value) -> Vec<u8> {
    let json = ascii(&Value::Array(vec![Value::from(op as u8), arg.clone()]).to_string());
    let mut out = json.len().to_string().into_bytes();
    out.extend_from_slice(json.as_bytes());
    out
}

/// Writes one message.
pub fn send(w: &mut impl Write, op: Op, arg: &Value) -> io::Result<()> {
    w.write_all(&encode(op, arg))?;
    w.flush()
}

fn ascii(json: &str) -> String {
    let mut out = String::with_capacity(json.len());
    for c in json.chars() {
        if c.is_ascii() {
            out.push(c);
        } else {
            let mut units = [0u16; 2];
            for u in c.encode_utf16(&mut units) {
                out.push_str(&format!("\\u{u:04x}"));
            }
        }
    }
    out
}

fn bad(why: impl Into<String>) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, why.into())
}

/// Reads one message: its opcode (as sent, known or not) and its argument.
pub fn read(r: &mut impl Read) -> io::Result<(u64, Value)> {
    let mut digits = Vec::new();
    let mut byte = [0u8; 1];
    loop {
        r.read_exact(&mut byte)?;
        match byte[0] {
            b'[' => break,
            b'0'..=b'9' if digits.len() < 10 => digits.push(byte[0]),
            _ => return Err(bad("not a calibre message (bad length)")),
        }
    }
    let len: usize = std::str::from_utf8(&digits)
        .ok()
        .and_then(|d| d.parse().ok())
        .ok_or_else(|| bad("not a calibre message (no length)"))?;
    if len == 0 || len > MAX_JSON {
        return Err(bad(format!("a message of {len} bytes")));
    }
    let mut json = vec![0u8; len];
    json[0] = b'[';
    r.read_exact(&mut json[1..])?;
    let v: Value = serde_json::from_slice(&json).map_err(|e| bad(format!("bad JSON: {e}")))?;
    match v {
        Value::Array(mut a) if a.len() == 2 => {
            let arg = a.pop().unwrap_or_default();
            let code = a[0]
                .as_u64()
                .ok_or_else(|| bad("the opcode isn't a number"))?;
            Ok((code, arg))
        }
        _ => Err(bad("not [opcode, argument]")),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn a_message_is_its_length_then_its_json() {
        let m = encode(Op::Ok, &json!({"key": 1}));
        assert_eq!(m, b"13[0,{\"key\":1}]");
        let (op, arg) = read(&mut &m[..]).unwrap();
        assert_eq!((op, arg), (0, json!({"key": 1})));
    }

    #[test]
    fn non_ascii_is_escaped_so_bytes_and_characters_agree() {
        let m = encode(Op::Ok, &json!({"t": "Café 📚"}));
        let text = std::str::from_utf8(&m).unwrap();
        assert!(text.is_ascii(), "{text}");
        assert!(
            text.contains(concat!("Caf", "\\", "u00e9 ", "\\", "ud83d", "\\", "udcda")),
            "{text}"
        );
        assert_eq!(read(&mut &m[..]).unwrap().1, json!({"t": "Café 📚"}));
    }

    #[test]
    fn two_messages_back_to_back_read_one_at_a_time() {
        let mut both = encode(Op::Noop, &json!({}));
        both.extend(encode(Op::Ok, &json!({"a": [1, 2]})));
        let mut r = &both[..];
        assert_eq!(read(&mut r).unwrap().0, 12);
        assert_eq!(read(&mut r).unwrap().1, json!({"a": [1, 2]}));
        assert!(r.is_empty());
    }

    #[test]
    fn junk_and_huge_lengths_are_refused() {
        assert!(read(&mut &b"GET / HTTP/1.1"[..]).is_err());
        assert!(read(&mut &b"99999999999["[..]).is_err());
        assert!(read(&mut &b"999999999[0,{}]"[..]).is_err());
        assert!(read(&mut &b"5[0,{"[..]).is_err());
        assert!(read(&mut &b"7{\"a\":1}"[..]).is_err());
    }
}
