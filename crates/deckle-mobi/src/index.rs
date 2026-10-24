// SPDX-License-Identifier: GPL-3.0-or-later
//! INDX records (MobileRead wiki, KindleUnpack's `mobi_index`): a header record with its TAGX
//! tag table, then index records of entries, each a name and a map of tag → values, and CTOC
//! records holding the strings entries point into.

use std::collections::BTreeMap;

use crate::MobiError;
use crate::palmdb::{PalmDb, be16, be32};

/// One index entry: its name and its tags' values.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Entry {
    pub name: Vec<u8>,
    pub tags: BTreeMap<u8, Vec<u64>>,
}

impl Entry {
    pub fn tag(&self, t: u8, i: usize) -> Option<u64> {
        self.tags.get(&t).and_then(|v| v.get(i)).copied()
    }
}

/// An index: its entries in order, and its CTOC strings by offset.
#[derive(Clone, Debug, Default)]
pub struct Index {
    pub entries: Vec<Entry>,
    pub ctoc: BTreeMap<u64, Vec<u8>>,
}

/// The INDX header's fields after the magic, in order (all big-endian `u32`).
struct Header {
    len: usize,
    start: usize,
    count: usize,
    nctoc: usize,
}

fn header(rec: &[u8]) -> Result<Header, MobiError> {
    if rec.get(..4) != Some(b"INDX") {
        return Err(MobiError::Corrupt("an index record without INDX"));
    }
    let field = |i: usize| {
        be32(rec, 4 + i * 4)
            .map(|v| v as usize)
            .ok_or(MobiError::Corrupt("an INDX header cut short"))
    };
    Ok(Header {
        len: field(0)?,
        start: field(4)?,
        count: field(5)?,
        nctoc: field(12)?,
    })
}

/// A forward variable-width integer: 7 bits a byte, the last byte has its high bit set.
fn vwi(data: &[u8], at: usize) -> Option<(usize, u64)> {
    let mut value = 0u64;
    let mut n = 0;
    loop {
        let b = *data.get(at + n)?;
        n += 1;
        value = (value << 7) | u64::from(b & 0x7F);
        if b & 0x80 != 0 || n > 9 {
            return Some((n, value));
        }
    }
}

/// `(tag, values per entry, mask, end flag)` rows of a TAGX block, and its control byte count.
fn tagx(rec: &[u8], start: usize) -> (usize, Vec<[u8; 4]>) {
    if rec.get(start..start + 4) != Some(b"TAGX") {
        return (0, Vec::new());
    }
    let first = be32(rec, start + 4).unwrap_or(12) as usize;
    let control = be32(rec, start + 8).unwrap_or(0) as usize;
    let mut rows = Vec::new();
    let mut i = 12;
    while i + 4 <= first {
        if let Some(r) = rec.get(start + i..start + i + 4) {
            rows.push([r[0], r[1], r[2], r[3]]);
        }
        i += 4;
    }
    (control, rows)
}

fn tag_map(
    control: usize,
    table: &[[u8; 4]],
    data: &[u8],
    start: usize,
    end: usize,
) -> BTreeMap<u8, Vec<u64>> {
    struct Want {
        tag: u8,
        count: Option<u64>,
        bytes: Option<u64>,
        per: u8,
    }
    let mut wants = Vec::new();
    let mut ci = 0;
    let mut pos = start + control;
    for &[tag, per, mask, end_flag] in table {
        if end_flag == 1 {
            ci += 1;
            continue;
        }
        let Some(&cbyte) = data.get(start + ci) else {
            break;
        };
        let mut value = cbyte & mask;
        if value == 0 {
            continue;
        }
        if value == mask {
            if mask.count_ones() > 1 {
                // A length in bytes of the values follows the control bytes.
                let Some((n, bytes)) = vwi(data, pos) else {
                    break;
                };
                pos += n;
                wants.push(Want {
                    tag,
                    count: None,
                    bytes: Some(bytes),
                    per,
                });
            } else {
                wants.push(Want {
                    tag,
                    count: Some(1),
                    bytes: None,
                    per,
                });
            }
        } else {
            let mut m = mask;
            while m & 1 == 0 {
                m >>= 1;
                value >>= 1;
            }
            wants.push(Want {
                tag,
                count: Some(u64::from(value)),
                bytes: None,
                per,
            });
        }
    }
    let mut out = BTreeMap::new();
    for w in wants {
        let mut values = Vec::new();
        if let Some(count) = w.count {
            for _ in 0..count * u64::from(w.per) {
                if pos >= end {
                    break;
                }
                let Some((n, v)) = vwi(data, pos) else { break };
                pos += n;
                values.push(v);
            }
        } else if let Some(bytes) = w.bytes {
            let mut used = 0u64;
            while used < bytes && pos < end {
                let Some((n, v)) = vwi(data, pos) else { break };
                pos += n;
                used += n as u64;
                values.push(v);
            }
        }
        out.insert(w.tag, values);
    }
    out
}

/// The strings of one CTOC record, by offset (plus `base`, 0x10000 a record).
fn ctoc(rec: &[u8], base: u64, out: &mut BTreeMap<u64, Vec<u8>>) {
    let mut at = 0;
    while at < rec.len() && rec[at] != 0 {
        let Some((n, len)) = vwi(rec, at) else { break };
        let s = at + n;
        let e = (s + len as usize).min(rec.len());
        out.insert(base + at as u64, rec[s..e].to_vec());
        at = e;
    }
}

/// Reads the index whose header record is `first` (relative to `base`, the book's first
/// record).
pub fn read(db: &PalmDb<'_>, base: usize, first: usize) -> Result<Index, MobiError> {
    let head = db.record(base + first);
    let h = header(head)?;
    let (control, table) = tagx(head, h.len);
    let mut index = Index::default();
    for j in 0..h.nctoc {
        ctoc(
            db.record(base + first + h.count + 1 + j),
            (j as u64) << 16,
            &mut index.ctoc,
        );
    }
    for i in 1..=h.count {
        let rec = db.record(base + first + i);
        let rh = header(rec)?;
        let idxt = rh.start;
        let mut starts: Vec<usize> = (0..rh.count)
            .filter_map(|j| be16(rec, idxt + 4 + 2 * j).map(usize::from))
            .collect();
        starts.push(idxt);
        for w in starts.windows(2) {
            let (s, e) = (w[0], w[1]);
            let Some(&len) = rec.get(s) else { continue };
            let name = rec
                .get(s + 1..s + 1 + usize::from(len))
                .unwrap_or_default()
                .to_vec();
            let tags = tag_map(control, &table, rec, s + 1 + usize::from(len), e);
            index.entries.push(Entry { name, tags });
        }
    }
    Ok(index)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn variable_width_integers_end_on_the_high_bit() {
        assert_eq!(vwi(&[0x81], 0), Some((1, 1)));
        assert_eq!(vwi(&[0x01, 0x80], 0), Some((2, 128)));
        assert_eq!(vwi(&[0x01], 0), None);
    }

    #[test]
    fn a_tag_map_reads_counts_and_byte_lengths() {
        // Tag 1: one value (mask 0x01); tag 6: two values per entry (mask 0x02).
        let table = [[1, 1, 0x01, 0], [6, 2, 0x02, 0], [0, 0, 0, 1]];
        let data = [0x03, 0x85, 0x8A, 0x94];
        let m = tag_map(1, &table, &data, 0, data.len());
        assert_eq!(m.get(&1), Some(&vec![5]));
        assert_eq!(m.get(&6), Some(&vec![10, 20]));
    }
}
