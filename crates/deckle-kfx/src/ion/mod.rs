// SPDX-License-Identifier: GPL-3.0-or-later
//! Amazon Ion data model and binary codec, as used by KFX (mirrors kfxlib's `ion.py` and
//! `ion_binary.py`).

mod decode;
mod value;

pub use decode::{decode_annotated, decode_single};
pub use value::{Decimal, Struct, Sym, Timestamp, Value};

/// Ion binary version marker that prefixes every serialized value stream.
pub const BVM: [u8; 4] = [0xE0, 0x01, 0x00, 0xEA];

#[derive(Debug, thiserror::Error)]
pub enum IonError {
    #[error("unexpected end of Ion data")]
    UnexpectedEof,
    #[error("Ion data does not start with the binary version marker")]
    BadSignature,
    #[error("unknown symbol id {0}")]
    UnknownSymbolId(u32),
    #[error("undefined symbol {0:?}")]
    UndefinedSymbol(String),
    #[error("malformed Ion: {0}")]
    Malformed(String),
    #[error("invalid UTF-8 in Ion string")]
    Utf8,
    #[error("expected a single Ion value, found {0}")]
    NotSingle(usize),
    #[error("expected an annotated value")]
    NotAnnotated,
    #[error("expected annotation {expected}, found {found:?}")]
    WrongAnnotation {
        expected: String,
        found: Vec<String>,
    },
}
