// SPDX-License-Identifier: GPL-3.0-or-later
//! Ion binary decoder, a port of the deserialize side of kfxlib's `ion_binary.py`.

use super::{BVM, Decimal, IonError, Struct, Sym, Timestamp, Value};
use crate::symtab::SymbolTable;

const VARIABLE_LEN_FLAG: u8 = 14;
const NULL_FLAG: u8 = 15;

pub(crate) struct Cursor<'a> {
    data: &'a [u8],
    pos: usize,
}

impl<'a> Cursor<'a> {
    fn new(data: &'a [u8]) -> Self {
        Cursor { data, pos: 0 }
    }
    fn remaining(&self) -> usize {
        self.data.len() - self.pos
    }
    fn peek(&self) -> Option<u8> {
        self.data.get(self.pos).copied()
    }
    fn u8(&mut self) -> Result<u8, IonError> {
        let b = *self.data.get(self.pos).ok_or(IonError::UnexpectedEof)?;
        self.pos += 1;
        Ok(b)
    }
    fn take(&mut self, n: usize) -> Result<&'a [u8], IonError> {
        if self.remaining() < n {
            return Err(IonError::UnexpectedEof);
        }
        let s = &self.data[self.pos..self.pos + n];
        self.pos += n;
        Ok(s)
    }
    fn rest(&mut self) -> &'a [u8] {
        let s = &self.data[self.pos..];
        self.pos = self.data.len();
        s
    }
}

/// Reads a VarUInt (7 bits per byte, high bit set on the final byte).
pub(crate) fn var_uint(c: &mut Cursor) -> Result<u64, IonError> {
    let mut value: u64 = 0;
    loop {
        let b = c.u8()?;
        value = (value << 7) | u64::from(b & 0x7f);
        if b & 0x80 != 0 {
            return Ok(value);
        }
        if value > 0x7f_ffff_ffff_ffff {
            return Err(IonError::Malformed("VarUInt too large".into()));
        }
    }
}

/// Reads a VarInt (sign bit is 0x40 of the first byte). `None` is the "-0" offset value.
pub(crate) fn var_int(c: &mut Cursor, allow_minus_zero: bool) -> Result<Option<i64>, IonError> {
    let first = c.u8()?;
    let mut bytes = Vec::new();
    let mut b = first & 0xbf;
    if b != 0 {
        bytes.push(b);
    }
    while b & 0x80 == 0 {
        b = c.u8()?;
        bytes.push(b);
    }
    let magnitude = var_uint(&mut Cursor::new(&bytes))? as i64;
    if first & 0x40 != 0 {
        if magnitude != 0 {
            Ok(Some(-magnitude))
        } else if allow_minus_zero {
            Ok(None)
        } else {
            Err(IonError::Malformed("unexpected -0 VarInt".into()))
        }
    } else {
        Ok(Some(magnitude))
    }
}

fn unsigned_int(data: &[u8]) -> Result<u128, IonError> {
    if data.len() > 16 {
        return Err(IonError::Malformed("integer too large".into()));
    }
    Ok(data
        .iter()
        .fold(0u128, |acc, b| (acc << 8) | u128::from(*b)))
}

/// Sign-and-magnitude Int (high bit of first byte is the sign).
fn signed_int(data: &[u8]) -> Result<i128, IonError> {
    if data.is_empty() {
        return Ok(0);
    }
    let negative = data[0] & 0x80 != 0;
    let mut first = vec![data[0] & 0x7f];
    first.extend_from_slice(&data[1..]);
    let m = unsigned_int(&first)? as i128;
    Ok(if negative { -m } else { m })
}

pub(crate) struct Decoder<'s> {
    symtab: &'s SymbolTable,
}

impl<'s> Decoder<'s> {
    pub(crate) fn new(symtab: &'s SymbolTable) -> Self {
        Decoder { symtab }
    }

    /// Resolves a symbol id; unknown ids become `$N` placeholders, as kfxlib does.
    fn symbol(&self, id: u64) -> Result<Sym, IonError> {
        let id = u32::try_from(id).map_err(|_| IonError::UnknownSymbolId(u32::MAX))?;
        Ok(match self.symtab.symbol_of(id) {
            Some(text) => Sym::new(text),
            None => Sym(format!("${id}")),
        })
    }

    /// Decodes all top-level values of a stream that starts with the BVM. NOP padding is skipped.
    pub(crate) fn values(&self, data: &[u8]) -> Result<Vec<Value>, IonError> {
        if data.len() < 4 || data[..4] != BVM {
            return Err(IonError::BadSignature);
        }
        let mut c = Cursor::new(&data[4..]);
        let mut out = Vec::new();
        while c.remaining() > 0 {
            if c.peek() == Some(0xE0) {
                let sig = c.take(4)?;
                if sig != BVM {
                    return Err(IonError::BadSignature);
                }
                continue;
            }
            if let Some(v) = self.value(&mut c)? {
                out.push(v);
            }
        }
        Ok(out)
    }

    /// Decodes one value; `None` for NOP padding.
    fn value(&self, c: &mut Cursor) -> Result<Option<Value>, IonError> {
        let descriptor = c.u8()?;
        if descriptor == 0xE0 {
            return Err(IonError::Malformed(
                "version marker inside value stream".into(),
            ));
        }
        let signature = descriptor >> 4;
        let flag = descriptor & 0x0f;

        // null of any type
        if flag == NULL_FLAG && signature != 0 {
            return Ok(Some(Value::Null));
        }
        match signature {
            0 => {
                if flag == NULL_FLAG {
                    return Ok(Some(Value::Null));
                }
                let len = self.length(c, flag)?;
                c.take(len)?;
                Ok(None)
            }
            1 => match flag {
                0 => Ok(Some(Value::Bool(false))),
                1 => Ok(Some(Value::Bool(true))),
                _ => Err(IonError::Malformed(format!("bool flag {flag}"))),
            },
            13 => {
                let flag = if flag == 1 { VARIABLE_LEN_FLAG } else { flag };
                let len = self.length(c, flag)?;
                let body = c.take(len)?;
                Ok(Some(Value::Struct(self.struct_body(body)?)))
            }
            _ => {
                let len = self.length(c, flag)?;
                let body = c.take(len)?;
                self.typed(signature, body).map(Some)
            }
        }
    }

    fn length(&self, c: &mut Cursor, flag: u8) -> Result<usize, IonError> {
        if flag == VARIABLE_LEN_FLAG {
            Ok(var_uint(c)? as usize)
        } else {
            Ok(usize::from(flag))
        }
    }

    fn typed(&self, signature: u8, body: &[u8]) -> Result<Value, IonError> {
        Ok(match signature {
            2 => Value::Int(
                i64::try_from(unsigned_int(body)?)
                    .map_err(|_| IonError::Malformed("integer exceeds i64".into()))?,
            ),
            3 => {
                if body.is_empty() || body[0] == 0 {
                    return Err(IonError::Malformed("bad negative int".into()));
                }
                Value::Int(
                    -i64::try_from(unsigned_int(body)?)
                        .map_err(|_| IonError::Malformed("integer exceeds i64".into()))?,
                )
            }
            4 => match body.len() {
                0 => Value::Float(0.0),
                4 => Value::Float(f64::from(f32::from_be_bytes(body.try_into().unwrap()))),
                8 => Value::Float(f64::from_be_bytes(body.try_into().unwrap())),
                n => return Err(IonError::Malformed(format!("float length {n}"))),
            },
            5 => {
                if body.is_empty() {
                    Value::Decimal(Decimal {
                        coefficient: 0,
                        exponent: 0,
                    })
                } else {
                    let mut c = Cursor::new(body);
                    let exponent = var_int(&mut c, false)?.unwrap_or(0) as i32;
                    let coefficient = signed_int(c.rest())?;
                    Value::Decimal(Decimal {
                        coefficient,
                        exponent,
                    })
                }
            }
            6 => Value::Timestamp(self.timestamp(body)?),
            7 => Value::Symbol(self.symbol(unsigned_int(body)? as u64)?),
            8 => Value::String(String::from_utf8(body.to_vec()).map_err(|_| IonError::Utf8)?),
            9 => Value::Clob(body.to_vec()),
            10 => Value::Blob(body.to_vec()),
            11 => Value::List(self.sequence(body)?),
            12 => Value::SExp(self.sequence(body)?),
            14 => {
                let mut c = Cursor::new(body);
                let alen = var_uint(&mut c)? as usize;
                let abytes = c.take(alen)?;
                let mut ac = Cursor::new(abytes);
                let mut annots = Vec::new();
                while ac.remaining() > 0 {
                    annots.push(self.symbol(var_uint(&mut ac)?)?);
                }
                if annots.is_empty() {
                    return Err(IonError::Malformed("annotation without annotations".into()));
                }
                let value = self
                    .value(&mut c)?
                    .ok_or_else(|| IonError::Malformed("annotated NOP".into()))?;
                if c.remaining() > 0 {
                    return Err(IonError::Malformed("annotation has excess data".into()));
                }
                Value::Annotated {
                    annots,
                    value: Box::new(value),
                }
            }
            15 => return Err(IonError::Malformed("reserved type".into())),
            _ => unreachable!("signature {signature} handled by caller"),
        })
    }

    fn sequence(&self, body: &[u8]) -> Result<Vec<Value>, IonError> {
        let mut c = Cursor::new(body);
        let mut out = Vec::new();
        while c.remaining() > 0 {
            if let Some(v) = self.value(&mut c)? {
                out.push(v);
            }
        }
        Ok(out)
    }

    fn struct_body(&self, body: &[u8]) -> Result<Struct, IonError> {
        let mut c = Cursor::new(body);
        let mut s = Struct::new();
        while c.remaining() > 0 {
            let key = self.symbol(var_uint(&mut c)?)?;
            if let Some(v) = self.value(&mut c)? {
                s.0.push((key, v));
            }
        }
        Ok(s)
    }

    fn timestamp(&self, body: &[u8]) -> Result<Timestamp, IonError> {
        let mut c = Cursor::new(body);
        let offset_minutes = var_int(&mut c, true)?.map(|v| v as i32);
        let year = var_uint(&mut c)? as u32;
        let next = |c: &mut Cursor| -> Result<Option<u32>, IonError> {
            if c.remaining() > 0 {
                Ok(Some(var_uint(c)? as u32))
            } else {
                Ok(None)
            }
        };
        let month = next(&mut c)?;
        let day = next(&mut c)?;
        let hour = next(&mut c)?;
        let minute = next(&mut c)?;
        let second = next(&mut c)?;
        let fraction = if c.remaining() > 0 {
            let exponent = var_int(&mut c, false)?.unwrap_or(0) as i32;
            let coefficient = signed_int(c.rest())?;
            if coefficient == 0 && exponent > -1 {
                None
            } else {
                Some((exponent, coefficient))
            }
        } else {
            None
        };
        // Offsets are only meaningful at minute precision or finer.
        let offset_minutes = if hour.is_some() { offset_minutes } else { None };
        Ok(Timestamp {
            offset_minutes,
            year,
            month,
            day,
            hour,
            minute,
            second,
            fraction,
        })
    }
}

/// Decodes exactly one value from a BVM-prefixed stream.
pub fn decode_single(data: &[u8], symtab: &SymbolTable) -> Result<Value, IonError> {
    let mut values = Decoder::new(symtab).values(data)?;
    if values.len() != 1 {
        return Err(IonError::NotSingle(values.len()));
    }
    Ok(values.pop().unwrap())
}

/// Decodes one annotated value, optionally checking its single annotation.
pub fn decode_annotated(
    data: &[u8],
    symtab: &SymbolTable,
    expect: Option<&str>,
) -> Result<(Vec<Sym>, Value), IonError> {
    match decode_single(data, symtab)? {
        Value::Annotated { annots, value } => {
            if let Some(e) = expect
                && !(annots.len() == 1 && annots[0].0 == e)
            {
                return Err(IonError::WrongAnnotation {
                    expected: e.to_owned(),
                    found: annots.iter().map(|a| a.0.clone()).collect(),
                });
            }
            Ok((annots, *value))
        }
        _ => Err(IonError::NotAnnotated),
    }
}
