// SPDX-License-Identifier: GPL-3.0-or-later
//! A KF8-only AZW3 for the Kindle, made from Kindle Previewer's joint KF7+KF8 MOBI and stamped
//! with the CDE type and ASIN Previewer leaves out (fixed-layout spec §4, §8).
//!
//! The split is KindleUnpack's `mobi_split`: the KF8 header becomes record 0, and the resources
//! the two halves share (page images, RESC, thumbnail), which live in the KF7 half, go back in
//! where the KF8 header's first resource index says. Every KF8 record index at or after that
//! point moves up by the number of resources inserted. An AZW3 that's already KF8-only is only
//! restamped.

use crate::kf8::{NONE, kf8_base};
use crate::palmdb::{PalmDb, be16, be32};
use crate::{MobiError, PALMDOC_HEADER, PDB_HEADER, exth, exth_records};

/// What Deckle stamps into the KF8 header's EXTH.
#[derive(Clone, Copy, Debug)]
pub struct Stamp<'a> {
    /// `EBOK` | `PDOC` (EXTH 501).
    pub cde: &'a str,
    /// EXTH 113 and 504.
    pub asin: &'a str,
}

/// MOBI header fields (offsets in record 0) that hold record indices.
const INDEX_FIELDS: [usize; 11] = [
    0x50,  // first non-text record
    0x70,  // HUFF record
    0xC0,  // FDST
    0xC8,  // FCIS
    0xD0,  // FLIS
    0xE0,  // SRCS
    0xF4,  // NCX index
    0xF8,  // fragment (chunk) index
    0xFC,  // skeleton index
    0x100, // DATP
    0x104, // guide index
];
const FIRST_RESOURCE: usize = 0x6C;
const LAST_CONTENT: usize = 0xC2;
const TITLE_OFFSET: usize = 0x54;
const TITLE_LENGTH: usize = 0x58;
const EXTH_FLAGS: usize = 0x80;
/// EXTH 125: how many resource records there are.
const EXTH_RESOURCE_COUNT: u32 = 125;

/// The AZW3 from `bytes`: a joint KF7+KF8 MOBI split to its KF8 half, or a KF8-only book
/// restamped. Refuses DRM, and an older MOBI with no KF8 part.
pub fn kf8_only(bytes: &[u8], stamp: &Stamp<'_>) -> Result<Vec<u8>, MobiError> {
    if crate::read_meta(bytes)?.drm {
        return Err(MobiError::Drm);
    }
    let db = PalmDb::parse(bytes)?;
    let base = kf8_base(&db)?;
    let mut header = db.record(base).to_vec();
    let mut records: Vec<&[u8]> = (base + 1..db.len()).map(|i| db.record(i)).collect();

    if base > 0 {
        let shared = shared_resources(&db);
        let kf8_records = db.len() - base;
        let target = match be32(&header, FIRST_RESOURCE) {
            Some(t) if t != NONE && (1..=kf8_records).contains(&(t as usize)) => t as usize,
            Some(t) if t != NONE => {
                return Err(MobiError::Corrupt("KF8 resource index past the end"));
            }
            // No resource index: they go before the end-of-file record.
            _ => kf8_records - usize::from(records.last() == Some(&EOF.as_slice())),
        };
        let n = shared.len() as u32;
        for at in INDEX_FIELDS {
            if let Some(v) = be32(&header, at).filter(|v| *v != NONE && *v as usize >= target)
                && at + 4 <= PALMDOC_HEADER + header_len(&header)
            {
                put32(&mut header, at, v + n);
            }
        }
        put32(&mut header, FIRST_RESOURCE, target as u32);
        // `records` starts at the KF8 header's record 1.
        records.splice(target - 1..target - 1, shared.iter().map(|i| db.record(*i)));
        if n > 0 {
            header = with_exth(
                &header,
                &[(EXTH_RESOURCE_COUNT, n.to_be_bytes().to_vec())],
                &[],
            )?;
        }
    }

    let header = with_exth(
        &header,
        &[
            (exth::CDE_TYPE, stamp.cde.as_bytes().to_vec()),
            (exth::ASIN, stamp.asin.as_bytes().to_vec()),
            (exth::ASIN2, stamp.asin.as_bytes().to_vec()),
        ],
        &[exth::KF8_BOUNDARY],
    )?;
    let mut all: Vec<&[u8]> = vec![&header];
    all.extend(records);
    Ok(palm_db(&bytes[..PDB_HEADER], &all))
}

const EOF: [u8; 4] = [0xE9, 0x8E, 0x0D, 0x0A];

fn header_len(rec0: &[u8]) -> usize {
    be32(rec0, PALMDOC_HEADER + 4).unwrap_or(0) as usize
}

fn put32(b: &mut [u8], at: usize, v: u32) {
    b[at..at + 4].copy_from_slice(&v.to_be_bytes());
}

/// The KF7 half's resource records (first resource to last content record), which a joint file
/// shares with its KF8 half.
fn shared_resources(db: &PalmDb<'_>) -> Vec<usize> {
    let rec0 = db.record(0);
    let Some(first) = be32(rec0, FIRST_RESOURCE).filter(|f| *f != NONE) else {
        return Vec::new();
    };
    let mut last = u32::from(be16(rec0, LAST_CONTENT).unwrap_or(0xFFFF));
    if last == 0xFFFF {
        // No last content index: up to the first of the records that follow the resources.
        last = [0xC8, 0xD0, 0x100, 0x70]
            .iter()
            .filter_map(|at| be32(rec0, *at))
            .filter(|n| *n != NONE && *n > first)
            .min()
            .map_or(first, |n| n - 1);
    }
    (first as usize..=last as usize)
        .take_while(|i| *i < db.len() && !db.record(*i).starts_with(b"BOUNDARY"))
        .collect()
}

/// `rec0` with its EXTH records of the kinds in `set` replaced (added when missing), those in
/// `drop` removed, and the full name moved after the new EXTH.
fn with_exth(rec0: &[u8], set: &[(u32, Vec<u8>)], drop: &[u32]) -> Result<Vec<u8>, MobiError> {
    let at = PALMDOC_HEADER + header_len(rec0);
    let (title_at, title_len) = match (be32(rec0, TITLE_OFFSET), be32(rec0, TITLE_LENGTH)) {
        (Some(a), Some(l)) if (a + l) as usize <= rec0.len() && a as usize >= at => {
            (a as usize, l as usize)
        }
        _ => return Err(MobiError::Corrupt("the full name is outside record 0")),
    };
    let mut kept: Vec<(u32, Vec<u8>)> = exth_records(rec0)
        .into_iter()
        .filter(|(k, _)| !drop.contains(k) && !set.iter().any(|(s, _)| s == k))
        .collect();
    kept.extend(set.iter().cloned());

    let mut body = Vec::new();
    for (kind, data) in &kept {
        body.extend(kind.to_be_bytes());
        body.extend((8 + data.len() as u32).to_be_bytes());
        body.extend(data);
    }
    let mut block = b"EXTH".to_vec();
    block.extend((12 + body.len() as u32).to_be_bytes());
    block.extend((kept.len() as u32).to_be_bytes());
    block.extend(body);
    block.resize(block.len().next_multiple_of(4), 0);

    let mut out = rec0[..at].to_vec();
    out.extend(&block);
    let new_title_at = out.len();
    out.extend(&rec0[title_at..title_at + title_len]);
    // The padding after the name (Kindle tools leave room there) stays as it was.
    out.extend(&rec0[title_at + title_len..]);
    put32(&mut out, TITLE_OFFSET, new_title_at as u32);
    let flags = be32(&out, EXTH_FLAGS).unwrap_or(0);
    put32(&mut out, EXTH_FLAGS, flags | 0x40);
    Ok(out)
}

/// A PalmDB file: `head`'s name, attributes and type, with `records`.
fn palm_db(head: &[u8], records: &[&[u8]]) -> Vec<u8> {
    let n = records.len();
    let mut out = head[..PDB_HEADER].to_vec();
    // Unique-id seed, then the record count.
    out[68..72].copy_from_slice(&(2 * n as u32 + 1).to_be_bytes());
    out[76..78].copy_from_slice(&(n as u16).to_be_bytes());
    let mut offset = (PDB_HEADER + 8 * n + 2) as u32;
    for (i, r) in records.iter().enumerate() {
        out.extend(offset.to_be_bytes());
        // Attributes 0, then a 3-byte unique id.
        out.extend((2 * i as u32).to_be_bytes());
        offset += r.len() as u32;
    }
    out.extend([0, 0]);
    for r in records {
        out.extend(*r);
    }
    out
}
