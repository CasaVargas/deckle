// SPDX-License-Identifier: GPL-3.0-or-later
//! Ion binary encoder, a port of the serialize side of kfxlib's `ion_binary.py`. Output is
//! byte-identical to kfxlib for the structures KFX uses.

use super::{BVM, Decimal, IonError, Struct, Sym, Timestamp, Value};
use crate::symtab::SymbolTable;

const VARIABLE_LEN_FLAG: u8 = 14;
const NULL_FLAG: u8 = 15;

fn descriptor(signature: u8, flag: u8) -> u8 {
    (signature << 4) | flag
}

/// Big-endian magnitude with leading zero bytes removed (empty for zero).
fn unsigned_int(value: u128) -> Vec<u8> {
    let bytes = value.to_be_bytes();
    let start = bytes.iter().position(|b| *b != 0).unwrap_or(bytes.len());
    bytes[start..].to_vec()
}

/// Sign-and-magnitude Int: keep one leading zero byte when the top bit is set.
fn signed_int(value: i128) -> Vec<u8> {
    let bytes = value.unsigned_abs().to_be_bytes();
    let mut start = 0;
    while start < bytes.len() && bytes[start] == 0 {
        if start + 1 < bytes.len() && bytes[start + 1] & 0x80 != 0 {
            break;
        }
        start += 1;
    }
    let mut data = bytes[start..].to_vec();
    if value < 0 {
        if data.is_empty() {
            data.push(0);
        }
        data[0] |= 0x80;
    }
    data
}

pub(crate) fn var_uint(mut value: u64, out: &mut Vec<u8>) {
    let mut parts = vec![(value & 0x7f) as u8 | 0x80];
    loop {
        value >>= 7;
        if value == 0 {
            break;
        }
        parts.insert(0, (value & 0x7f) as u8);
    }
    out.extend(parts);
}

/// VarInt; `None` encodes "-0" (the unknown timestamp offset).
pub(crate) fn var_int(value: Option<i64>, out: &mut Vec<u8>) {
    let Some(value) = value else {
        out.push(0xC0);
        return;
    };
    let mut data = Vec::new();
    var_uint(value.unsigned_abs(), &mut data);
    if data[0] & 0x40 != 0 {
        data.insert(0, 0);
    }
    if value < 0 {
        data[0] |= 0x40;
    }
    out.extend(data);
}

pub(crate) struct Encoder<'s> {
    symtab: &'s SymbolTable,
}

impl<'s> Encoder<'s> {
    pub(crate) fn new(symtab: &'s SymbolTable) -> Self {
        Encoder { symtab }
    }

    fn sid(&self, sym: &Sym) -> Result<u64, IonError> {
        self.symtab
            .id_of(sym)
            .map(u64::from)
            .ok_or_else(|| IonError::UndefinedSymbol(sym.0.clone()))
    }

    /// Writes a type descriptor + optional length + body.
    fn framed(&self, signature: u8, body: &[u8], out: &mut Vec<u8>) {
        if body.len() < usize::from(VARIABLE_LEN_FLAG) {
            out.push(descriptor(signature, body.len() as u8));
        } else {
            out.push(descriptor(signature, VARIABLE_LEN_FLAG));
            var_uint(body.len() as u64, out);
        }
        out.extend_from_slice(body);
    }

    pub(crate) fn value(&self, v: &Value, out: &mut Vec<u8>) -> Result<(), IonError> {
        match v {
            Value::Null => out.push(descriptor(0, NULL_FLAG)),
            Value::Bool(b) => out.push(descriptor(1, u8::from(*b))),
            Value::Int(i) => {
                let sig = if *i >= 0 { 2 } else { 3 };
                self.framed(sig, &unsigned_int(i.unsigned_abs().into()), out);
            }
            Value::Float(f) => {
                if *f == 0.0 {
                    self.framed(4, &[], out);
                } else {
                    self.framed(4, &f.to_be_bytes(), out);
                }
            }
            Value::Decimal(Decimal {
                coefficient,
                exponent,
            }) => {
                if *coefficient == 0 && *exponent == 0 {
                    self.framed(5, &[], out);
                } else {
                    let mut body = Vec::new();
                    var_int(Some(i64::from(*exponent)), &mut body);
                    body.extend(signed_int(*coefficient));
                    self.framed(5, &body, out);
                }
            }
            Value::Timestamp(t) => {
                let body = timestamp(t);
                self.framed(6, &body, out);
            }
            Value::Symbol(s) => {
                let id = self.sid(s)?;
                self.framed(7, &unsigned_int(id.into()), out);
            }
            Value::String(s) => self.framed(8, s.as_bytes(), out),
            Value::Clob(b) => self.framed(9, b, out),
            Value::Blob(b) => self.framed(10, b, out),
            Value::List(items) => {
                let body = self.sequence(items)?;
                self.framed(11, &body, out);
            }
            Value::SExp(items) => {
                let body = self.sequence(items)?;
                self.framed(12, &body, out);
            }
            Value::Struct(s) => {
                let body = self.struct_body(s)?;
                self.framed(13, &body, out);
            }
            Value::Annotated { annots, value } => {
                if annots.is_empty() {
                    return Err(IonError::Malformed("annotation without annotations".into()));
                }
                let mut annot_bytes = Vec::new();
                for a in annots {
                    var_uint(self.sid(a)?, &mut annot_bytes);
                }
                let mut body = Vec::new();
                var_uint(annot_bytes.len() as u64, &mut body);
                body.extend(annot_bytes);
                self.value(value, &mut body)?;
                self.framed(14, &body, out);
            }
        }
        Ok(())
    }

    fn sequence(&self, items: &[Value]) -> Result<Vec<u8>, IonError> {
        let mut body = Vec::new();
        for item in items {
            self.value(item, &mut body)?;
        }
        Ok(body)
    }

    fn struct_body(&self, s: &Struct) -> Result<Vec<u8>, IonError> {
        let mut body = Vec::new();
        for (k, v) in s.iter() {
            var_uint(self.sid(k)?, &mut body);
            self.value(v, &mut body)?;
        }
        Ok(body)
    }
}

fn timestamp(t: &Timestamp) -> Vec<u8> {
    let mut body = Vec::new();
    var_int(t.offset_minutes.map(i64::from), &mut body);
    var_uint(u64::from(t.year), &mut body);
    for part in [t.month, t.day] {
        match part {
            Some(p) => var_uint(u64::from(p), &mut body),
            None => return body,
        }
    }
    let (Some(h), Some(m)) = (t.hour, t.minute) else {
        return body;
    };
    var_uint(u64::from(h), &mut body);
    var_uint(u64::from(m), &mut body);
    let Some(s) = t.second else { return body };
    var_uint(u64::from(s), &mut body);
    if let Some((exp, coef)) = t.fraction {
        var_int(Some(i64::from(exp)), &mut body);
        body.extend(signed_int(coef));
    }
    body
}

/// Encodes one value with the BVM prefix (kfxlib's `serialize_single_value`).
pub fn encode_single(v: &Value, symtab: &SymbolTable) -> Result<Vec<u8>, IonError> {
    let mut out = BVM.to_vec();
    Encoder::new(symtab).value(v, &mut out)?;
    Ok(out)
}

/// Encodes one value without the BVM prefix.
pub fn encode_value(v: &Value, symtab: &SymbolTable, out: &mut Vec<u8>) -> Result<(), IonError> {
    Encoder::new(symtab).value(v, out)
}
