// SPDX-License-Identifier: GPL-3.0-or-later
//! A MOBI book's text records: their trailing entries stripped and PalmDOC (LZ77) decompressed.

use crate::MobiError;

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

#[cfg(test)]
mod tests {
    use super::*;

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
