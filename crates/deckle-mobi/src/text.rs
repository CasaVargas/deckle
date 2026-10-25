// SPDX-License-Identifier: GPL-3.0-or-later
//! A MOBI book's text records: their trailing entries stripped and PalmDOC (LZ77) or HUFF/CDIC
//! decompressed.

use crate::MobiError;
use crate::palmdb::{be16, be32};

/// Strips the trailing entries `extra_flags` (MOBI header 0xF2) says each text record has: one
/// backward-encoded size per flag bit above bit 0, then the multibyte overlap when bit 0 is set.
pub fn trim_trailing(mut rec: &[u8], extra_flags: u16) -> &[u8] {
    let trailers = (extra_flags >> 1).count_ones();
    for _ in 0..trailers {
        // The size sits in the last bytes, 7 bits each, most significant first; the high bit
        // marks the first byte of the number.
        let tail = &rec[rec.len().saturating_sub(4)..];
        let mut size = 0usize;
        for &b in tail {
            if b & 0x80 != 0 {
                size = 0;
            }
            size = (size << 7) | usize::from(b & 0x7F);
        }
        rec = &rec[..rec.len().saturating_sub(size)];
    }
    if extra_flags & 1 != 0
        && let Some(&last) = rec.last()
    {
        let n = usize::from(last & 3) + 1;
        rec = &rec[..rec.len().saturating_sub(n)];
    }
    rec
}

/// PalmDOC LZ77 decompression.
pub fn palmdoc(data: &[u8], out: &mut Vec<u8>) -> Result<(), MobiError> {
    let mut i = 0;
    while i < data.len() {
        let c = data[i];
        i += 1;
        match c {
            1..=8 => {
                let end = (i + usize::from(c)).min(data.len());
                out.extend_from_slice(&data[i..end]);
                i = end;
            }
            0 | 9..=0x7F => out.push(c),
            0xC0..=0xFF => {
                out.push(b' ');
                out.push(c ^ 0x80);
            }
            _ => {
                let Some(&c2) = data.get(i) else {
                    return Err(MobiError::Corrupt("a back-reference cut short"));
                };
                i += 1;
                let m = ((usize::from(c) << 8) | usize::from(c2)) & 0x3FFF;
                let distance = m >> 3;
                let length = (m & 7) + 3;
                if distance == 0 || distance > out.len() {
                    return Err(MobiError::Corrupt("a back-reference before the start"));
                }
                for _ in 0..length {
                    out.push(out[out.len() - distance]);
                }
            }
        }
    }
    Ok(())
}

/// How deep a dictionary phrase may refer to other phrases before the data is taken as damaged.
const HUFF_DEPTH: usize = 32;

/// A HUFF/CDIC decoder, after KindleUnpack's `HuffcdicReader` (GPL-3.0): the HUFF record's
/// code tables and the CDIC records' phrase dictionary.
pub struct Huff {
    /// By a code's top byte: (code length, terminal, max code).
    dict1: Vec<(u32, bool, u64)>,
    mincode: Vec<u64>,
    maxcode: Vec<u64>,
    /// Phrases, and whether each is already decompressed.
    dictionary: Vec<(Vec<u8>, bool)>,
}

impl Huff {
    /// From the HUFF record and its CDIC records, in order.
    pub fn new(huff: &[u8], cdics: &[&[u8]]) -> Result<Self, MobiError> {
        let bad = || MobiError::Corrupt("a damaged HUFF record");
        if huff.get(..8) != Some(b"HUFF\0\0\0\x18") {
            return Err(bad());
        }
        let off1 = be32(huff, 8).ok_or_else(bad)? as usize;
        let off2 = be32(huff, 12).ok_or_else(bad)? as usize;
        let mut dict1 = Vec::with_capacity(256);
        for i in 0..256 {
            let v = be32(huff, off1 + i * 4).ok_or_else(bad)?;
            let codelen = v & 0x1F;
            let term = v & 0x80 != 0;
            if codelen == 0 || (codelen <= 8 && !term) {
                return Err(bad());
            }
            let maxcode = ((u64::from(v >> 8) + 1) << (32 - codelen)) - 1;
            dict1.push((codelen, term, maxcode));
        }
        let mut mincode = vec![0u64];
        let mut maxcode = vec![0u64];
        for len in 1..=32u32 {
            let i = (len as usize - 1) * 2;
            let lo = u64::from(be32(huff, off2 + i * 4).ok_or_else(bad)?);
            let hi = u64::from(be32(huff, off2 + (i + 1) * 4).ok_or_else(bad)?);
            mincode.push(lo << (32 - len));
            maxcode.push(((hi + 1) << (32 - len)) - 1);
        }
        let mut dictionary = Vec::new();
        for cdic in cdics {
            if cdic.get(..8) != Some(b"CDIC\0\0\0\x10") {
                return Err(MobiError::Corrupt("a damaged CDIC record"));
            }
            let phrases = be32(cdic, 8).unwrap_or(0) as usize;
            let bits = be32(cdic, 12).unwrap_or(0).min(16);
            let n = (1usize << bits).min(phrases.saturating_sub(dictionary.len()));
            for k in 0..n {
                let off = usize::from(be16(cdic, 16 + k * 2).ok_or_else(bad)?);
                let blen = be16(cdic, 16 + off).ok_or_else(bad)?;
                let len = usize::from(blen & 0x7FFF);
                let slice = cdic.get(18 + off..18 + off + len).ok_or_else(bad)?;
                dictionary.push((slice.to_vec(), blen & 0x8000 != 0));
            }
        }
        Ok(Huff {
            dict1,
            mincode,
            maxcode,
            dictionary,
        })
    }

    /// One text record, decompressed.
    pub fn unpack(&mut self, data: &[u8]) -> Result<Vec<u8>, MobiError> {
        self.unpack_at(data, 0)
    }

    fn unpack_at(&mut self, data: &[u8], depth: usize) -> Result<Vec<u8>, MobiError> {
        if depth > HUFF_DEPTH {
            return Err(MobiError::Corrupt(
                "HUFF phrases refer to each other too deeply",
            ));
        }
        let bad = || MobiError::Corrupt("damaged HUFF/CDIC text");
        let mut padded = data.to_vec();
        padded.extend([0u8; 8]);
        let word = |pos: usize| u64::from_be_bytes(padded[pos..pos + 8].try_into().unwrap());
        let mut bitsleft = data.len() as i64 * 8;
        let mut pos = 0;
        let mut x = word(pos);
        let mut n: i64 = 32;
        let mut out = Vec::new();
        loop {
            if n <= 0 {
                pos += 4;
                if pos + 8 > padded.len() {
                    break;
                }
                x = word(pos);
                n += 32;
            }
            let code = (x >> n) & 0xFFFF_FFFF;
            let (mut codelen, term, mut maxcode) = self.dict1[(code >> 24) as usize];
            if !term {
                while codelen < 32 && code < self.mincode[codelen as usize] {
                    codelen += 1;
                }
                maxcode = self.maxcode[codelen as usize];
            }
            n -= i64::from(codelen);
            bitsleft -= i64::from(codelen);
            if bitsleft < 0 {
                break;
            }
            let r = (maxcode.checked_sub(code).ok_or_else(bad)? >> (32 - codelen)) as usize;
            let (slice, done) = self.dictionary.get(r).cloned().ok_or_else(bad)?;
            let slice = if done {
                slice
            } else {
                let unpacked = self.unpack_at(&slice, depth + 1)?;
                self.dictionary[r] = (unpacked.clone(), true);
                unpacked
            };
            out.extend_from_slice(&slice);
        }
        Ok(out)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn huff_cdic_decodes_literal_and_nested_phrases() {
        let (huff, cdic) = crate::fixture::huff_tables(&[(0x01, b"\x41\x42", false)]);
        let mut h = Huff::new(&huff, &[&cdic]).unwrap();
        // Byte b decodes to itself, except 0x01, a phrase that is itself compressed ("AB").
        assert_eq!(h.unpack(b"xy\x01z").unwrap(), b"xyABz");
        assert!(Huff::new(b"nope", &[]).is_err());
    }

    #[test]
    fn literals_runs_spaces_and_back_references() {
        // "abc" literal, then copy 3 from distance 3, then " x" as 0xF8, then a 2-byte run.
        let data = [b'a', b'b', b'c', 0x80, (3 << 3) as u8, 0xF8, 2, 0x01, 0x02];
        let mut out = Vec::new();
        palmdoc(&data, &mut out).unwrap();
        assert_eq!(out, b"abcabc x\x01\x02");
        assert!(palmdoc(&[0x80, 0x08], &mut Vec::new()).is_err());
    }

    #[test]
    fn trailing_entries_come_off() {
        // Text "hello", its multibyte byte (0: just itself), then a 3-byte trailer whose last
        // byte gives its size.
        let rec = [b'h', b'e', b'l', b'l', b'o', 0x00, 0xAA, 0xBB, 0x83];
        assert_eq!(trim_trailing(&rec, 0b11), b"hello");
        assert_eq!(trim_trailing(b"plain", 0), b"plain");
    }
}
