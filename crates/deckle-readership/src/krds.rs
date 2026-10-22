// SPDX-License-Identifier: GPL-3.0-or-later
//! The Kindle's reader data store (KRDS) sidecars, `.yjf` here: just enough to read the furthest
//! and last reading positions.
//!
//! Ported from `krds.py` (<https://github.com/K-R-D-S/KRDS>, © 2019 John Howell, GPL-3.0): an
//! 8-byte signature, a header value, a count, then that many named objects of typed values.
//! Objects this module doesn't know are skipped, as are values newer firmware appends.

use chrono::{TimeZone, Utc};

const SIGNATURE: [u8; 8] = [0, 0, 0, 0, 0, 0x1a, 0xb1, 0x26];
/// Real files nest two or three objects deep; anything past this is refused, not recursed into.
const MAX_DEPTH: usize = 32;

/// Where the reader is in a book, as the Kindle recorded it.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Positions {
    /// `fpr`: the furthest position reached (a contents jump counts).
    pub furthest: Option<i64>,
    /// `lpr`: the last position read.
    pub last: Option<i64>,
    /// When `last` was recorded, UTC as `YYYY-MM-DDTHH:MM:SSZ`.
    pub last_read: Option<String>,
}

#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum KrdsError {
    #[error("not a Kindle reader data file")]
    BadSignature,
    #[error("file ends early (at byte {at})")]
    Truncated { at: usize },
    #[error("unknown value type {byte} at byte {at}")]
    BadType { byte: u8, at: usize },
    #[error("objects nested too deep (at byte {at})")]
    TooDeep { at: usize },
}

#[derive(Debug)]
enum Value {
    Int(i64),
    Str(String),
    Object(String, Vec<Value>),
    Other,
}

struct Reader<'a> {
    b: &'a [u8],
    at: usize,
}

impl Reader<'_> {
    fn take(&mut self, n: usize) -> Result<&[u8], KrdsError> {
        let end = self.at.checked_add(n).filter(|&e| e <= self.b.len());
        let end = end.ok_or(KrdsError::Truncated { at: self.b.len() })?;
        let s = &self.b[self.at..end];
        self.at = end;
        Ok(s)
    }

    fn byte(&mut self) -> Result<u8, KrdsError> {
        Ok(self.take(1)?[0])
    }

    fn peek(&self) -> Result<u8, KrdsError> {
        self.b
            .get(self.at)
            .copied()
            .ok_or(KrdsError::Truncated { at: self.at })
    }

    fn utf(&mut self) -> Result<String, KrdsError> {
        // A boolean "is empty", then (when it isn't) a 2-byte length and UTF-8.
        if self.byte()? != 0 {
            return Ok(String::new());
        }
        let n = u16::from_be_bytes(self.take(2)?.try_into().unwrap()) as usize;
        Ok(String::from_utf8_lossy(self.take(n)?).into_owned())
    }

    fn value(&mut self, depth: usize) -> Result<Value, KrdsError> {
        let at = self.at;
        if depth > MAX_DEPTH {
            return Err(KrdsError::TooDeep { at });
        }
        let ty = self.byte()?;
        Ok(match ty as i8 {
            0 | 9 => {
                self.take(1)?;
                Value::Other
            }
            7 => Value::Int(self.byte()? as i8 as i64),
            1 => Value::Int(i32::from_be_bytes(self.take(4)?.try_into().unwrap()) as i64),
            2 => Value::Int(i64::from_be_bytes(self.take(8)?.try_into().unwrap())),
            3 => Value::Str(self.utf()?),
            4 => {
                self.take(8)?;
                Value::Other
            }
            5 => Value::Int(i16::from_be_bytes(self.take(2)?.try_into().unwrap()) as i64),
            6 => {
                self.take(4)?;
                Value::Other
            }
            -2 => {
                let name = self.utf()?;
                let mut values = Vec::new();
                while self.peek()? as i8 != -1 {
                    values.push(self.value(depth + 1)?);
                }
                self.byte()?;
                Value::Object(name, values)
            }
            _ => return Err(KrdsError::BadType { byte: ty, at }),
        })
    }
}

/// The number after the last `:` of a position string (`"AQULAAB1AgAA:54797"` → 54797): the
/// KFX position id.
pub fn position_id(s: &str) -> Option<i64> {
    s.rsplit_once(':')?.1.parse().ok()
}

fn utc_time(ms: i64) -> Option<String> {
    if ms < 0 {
        return None;
    }
    let t = Utc.timestamp_millis_opt(ms).single()?;
    Some(t.format("%Y-%m-%dT%H:%M:%SZ").to_string())
}

/// The reading positions in a `.yjf`.
pub fn parse_yjf(bytes: &[u8]) -> Result<Positions, KrdsError> {
    let mut r = Reader { b: bytes, at: 0 };
    if r.take(SIGNATURE.len())? != SIGNATURE {
        return Err(KrdsError::BadSignature);
    }
    r.value(0)?; // header value (1)
    let count = match r.value(0)? {
        Value::Int(n) => n.max(0) as usize,
        _ => return Err(KrdsError::BadType { byte: 0, at: r.at }),
    };
    let mut p = Positions::default();
    for _ in 0..count {
        let Value::Object(name, values) = r.value(0)? else {
            continue;
        };
        match (name.as_str(), values.as_slice()) {
            ("fpr", [Value::Str(pos), ..]) => p.furthest = position_id(pos),
            // Old files: a bare position string.
            ("lpr", [Value::Str(pos), ..]) => p.last = position_id(pos),
            ("lpr", [Value::Int(version), Value::Str(pos), rest @ ..]) if *version <= 2 => {
                p.last = position_id(pos);
                if let Some(Value::Int(ms)) = rest.first() {
                    p.last_read = utc_time(*ms);
                }
            }
            _ => {}
        }
    }
    Ok(p)
}
