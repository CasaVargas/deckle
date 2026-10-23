// SPDX-License-Identifier: GPL-3.0-or-later
//! Kindle's PalmDB formats, MOBI, AZW and AZW3 (KF8), as far as the Kindle tab needs them
//! (Books and Kindle design §3): the metadata in record 0's MOBI header and EXTH records, the
//! DRM flag, and the cover. Nothing past record 0 and the cover is read, and a DRM-protected
//! file is never decrypted.

pub mod fixture;

use std::io::{Read, Seek, SeekFrom};

/// A PalmDB file's header: 78 bytes, then 8 bytes per record.
const PDB_HEADER: usize = 78;
/// Record 0 starts with the PalmDOC header; the MOBI header follows it.
const PALMDOC_HEADER: usize = 16;

/// EXTH record types (MobileRead's MOBI wiki).
mod exth {
    pub const AUTHOR: u32 = 100;
    pub const ISBN: u32 = 104;
    pub const SOURCE: u32 = 112;
    pub const ASIN: u32 = 113;
    pub const KF8_BOUNDARY: u32 = 121;
    pub const FIXED_LAYOUT: u32 = 122;
    pub const COVER_OFFSET: u32 = 201;
    pub const CDE_TYPE: u32 = 501;
    pub const UPDATED_TITLE: u32 = 503;
    pub const ASIN2: u32 = 504;
}

#[derive(Debug, thiserror::Error)]
pub enum MobiError {
    #[error("not a Kindle MOBI, AZW or AZW3 file: {0}")]
    NotMobi(&'static str),
    #[error("reading the file: {0}")]
    Io(#[from] std::io::Error),
}

/// What record 0 says about a book.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Meta {
    pub title: Option<String>,
    pub authors: Vec<String>,
    pub asin: Option<String>,
    /// `EBOK` | `PDOC`, from EXTH 501.
    pub cde: Option<String>,
    pub isbn: Option<String>,
    /// calibre's book id, from a `calibre:<uuid>` source record (EXTH 112).
    pub calibre_id: Option<String>,
    /// Record 0's encryption type isn't 0: the text is DRM-protected.
    pub drm: bool,
    /// EXTH 122 says `true`: a comic, a children's book, a fixed-layout AZW3.
    pub fixed_layout: bool,
    /// A KF8 book: an AZW3 (MOBI version 8) or a joint file with a KF8 part (EXTH 121).
    pub kf8: bool,
    /// The uncompressed text's length in bytes (PalmDOC header): the denominator of a byte
    /// position, and the size a rebuild is checked against.
    pub text_length: u32,
    /// The cover image's bytes, as stored (JPEG, GIF or PNG).
    pub cover: Option<Vec<u8>>,
}

fn be16(b: &[u8], at: usize) -> Option<u16> {
    Some(u16::from_be_bytes(b.get(at..at + 2)?.try_into().ok()?))
}

fn be32(b: &[u8], at: usize) -> Option<u32> {
    Some(u32::from_be_bytes(b.get(at..at + 4)?.try_into().ok()?))
}

/// Whether `head` (a file's first bytes, 68 are enough) is a PalmDB book Deckle reads.
pub fn is_mobi(head: &[u8]) -> bool {
    matches!(head.get(60..68), Some(b"BOOKMOBI") | Some(b"TEXtREAd"))
}

/// Reads [`Meta`] from a whole file in memory.
pub fn read_meta(bytes: &[u8]) -> Result<Meta, MobiError> {
    read_meta_from(&mut std::io::Cursor::new(bytes))
}

/// Reads [`Meta`] from `r`, reading only the header, the record list, record 0 and the cover
/// record.
pub fn read_meta_from<R: Read + Seek>(r: &mut R) -> Result<Meta, MobiError> {
    let mut head = [0u8; PDB_HEADER];
    r.read_exact(&mut head)
        .map_err(|_| MobiError::NotMobi("shorter than a PalmDB header"))?;
    if !is_mobi(&head) {
        return Err(MobiError::NotMobi("no BOOKMOBI type and creator"));
    }
    let count = usize::from(be16(&head, 76).unwrap_or(0));
    if count == 0 {
        return Err(MobiError::NotMobi("no records"));
    }
    let mut list = vec![0u8; count * 8];
    r.read_exact(&mut list)
        .map_err(|_| MobiError::NotMobi("record list cut short"))?;
    let offset = |i: usize| be32(&list, i * 8).map(u64::from);
    let end = r.seek(SeekFrom::End(0))?;
    let record = |r: &mut R, i: usize| -> Result<Vec<u8>, MobiError> {
        let start = offset(i).ok_or(MobiError::NotMobi("record index out of range"))?;
        let stop = if i + 1 < count {
            offset(i + 1).unwrap_or(end)
        } else {
            end
        };
        if stop < start || stop > end {
            return Err(MobiError::NotMobi("record offsets out of order"));
        }
        r.seek(SeekFrom::Start(start))?;
        let mut buf = vec![0u8; (stop - start) as usize];
        r.read_exact(&mut buf)?;
        Ok(buf)
    };
    let rec0 = record(r, 0)?;
    let mut meta = parse_record0(&rec0)?;
    if let Some(i) = cover_index(&rec0, count) {
        meta.cover = record(r, i).ok().filter(|b| is_image(b));
    }
    Ok(meta)
}

fn is_image(b: &[u8]) -> bool {
    b.starts_with(&[0xFF, 0xD8])
        || b.starts_with(b"GIF8")
        || b.starts_with(&[0x89, b'P', b'N', b'G'])
}

/// The cover's record: the first image record plus EXTH 201's offset.
fn cover_index(rec0: &[u8], count: usize) -> Option<usize> {
    let first_image = be32(rec0, 0x6C)?;
    let offset = exth_records(rec0)
        .into_iter()
        .find(|(t, _)| *t == exth::COVER_OFFSET)
        .and_then(|(_, d)| be32(&d, 0))?;
    let i = usize::try_from(first_image.checked_add(offset)?).ok()?;
    // 0xFFFFFFFF marks "none" in either field.
    (first_image != u32::MAX && offset != u32::MAX && i < count).then_some(i)
}

/// The EXTH records of record 0, in order; empty when there are none.
fn exth_records(rec0: &[u8]) -> Vec<(u32, Vec<u8>)> {
    let Some(header_len) = be32(rec0, PALMDOC_HEADER + 4) else {
        return Vec::new();
    };
    let has_exth = be32(rec0, 0x80).is_some_and(|f| f & 0x40 != 0);
    let at = PALMDOC_HEADER + header_len as usize;
    if !has_exth || rec0.get(at..at + 4) != Some(b"EXTH") {
        return Vec::new();
    }
    let n = be32(rec0, at + 8).unwrap_or(0);
    let mut out = Vec::new();
    let mut p = at + 12;
    for _ in 0..n {
        let (Some(kind), Some(len)) = (be32(rec0, p), be32(rec0, p + 4)) else {
            break;
        };
        let len = len as usize;
        let Some(data) = rec0.get(p + 8..p + len.max(8)) else {
            break;
        };
        out.push((kind, data.to_vec()));
        p += len.max(8);
    }
    out
}

fn parse_record0(rec0: &[u8]) -> Result<Meta, MobiError> {
    let encryption = be16(rec0, 12).ok_or(MobiError::NotMobi("record 0 too short"))?;
    let text_length = be32(rec0, 4).unwrap_or(0);
    let mut meta = Meta {
        drm: encryption != 0,
        text_length,
        ..Default::default()
    };
    // A plain PalmDOC (TEXtREAd) file has no MOBI header.
    if rec0.get(PALMDOC_HEADER..PALMDOC_HEADER + 4) != Some(b"MOBI") {
        return Ok(meta);
    }
    let utf8 = be32(rec0, 0x1C) == Some(65001);
    let text = |b: &[u8]| -> String {
        let s = if utf8 {
            String::from_utf8_lossy(b).into_owned()
        } else {
            encoding_rs::WINDOWS_1252.decode(b).0.into_owned()
        };
        s.trim_matches(char::from(0)).trim().to_owned()
    };
    meta.kf8 = be32(rec0, 0x24) == Some(8);
    let full_name = match (be32(rec0, 0x54), be32(rec0, 0x58)) {
        (Some(at), Some(len)) => rec0.get(at as usize..(at + len) as usize).map(text),
        _ => None,
    };
    let mut title = None;
    for (kind, data) in exth_records(rec0) {
        match kind {
            exth::AUTHOR => {
                let a = text(&data);
                if !a.is_empty() {
                    meta.authors.push(a);
                }
            }
            exth::ISBN => meta.isbn = Some(text(&data)).filter(|s| !s.is_empty()),
            exth::SOURCE => {
                let s = text(&data);
                if let Some(id) = s.strip_prefix("calibre:") {
                    meta.calibre_id = Some(id.to_owned());
                }
            }
            exth::ASIN | exth::ASIN2 => {
                if meta.asin.is_none() {
                    meta.asin = Some(text(&data)).filter(|s| !s.is_empty());
                }
            }
            exth::KF8_BOUNDARY => meta.kf8 |= be32(&data, 0).is_some_and(|b| b != u32::MAX),
            exth::FIXED_LAYOUT => meta.fixed_layout = text(&data).eq_ignore_ascii_case("true"),
            exth::CDE_TYPE => meta.cde = Some(text(&data)).filter(|s| !s.is_empty()),
            exth::UPDATED_TITLE => title = Some(text(&data)).filter(|s| !s.is_empty()),
            _ => {}
        }
    }
    meta.title = title.or(full_name).filter(|t| !t.is_empty());
    Ok(meta)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fixture::{Book, build};

    #[test]
    fn metadata_comes_from_the_exth_records() {
        let bytes = build(&Book {
            title: "Middlemarch",
            authors: &["George Eliot"],
            asin: Some("B000JQUSTM"),
            cde: Some("PDOC"),
            isbn: Some("9780141439549"),
            calibre_id: Some("5c1f-uuid"),
            kf8: true,
            cover: Some(b"\xFF\xD8cover"),
            ..Default::default()
        });
        let m = read_meta(&bytes).unwrap();
        assert_eq!(m.title.as_deref(), Some("Middlemarch"));
        assert_eq!(m.authors, ["George Eliot"]);
        assert_eq!(m.asin.as_deref(), Some("B000JQUSTM"));
        assert_eq!(m.cde.as_deref(), Some("PDOC"));
        assert_eq!(m.isbn.as_deref(), Some("9780141439549"));
        assert_eq!(m.calibre_id.as_deref(), Some("5c1f-uuid"));
        assert!(m.kf8 && !m.drm && !m.fixed_layout);
        assert_eq!(m.cover.as_deref(), Some(&b"\xFF\xD8cover"[..]));
        assert_eq!(m.text_length, 4096);
    }

    #[test]
    fn drm_and_fixed_layout_are_flagged() {
        let m = read_meta(&build(&Book {
            title: "A Store Book",
            drm: true,
            cde: Some("EBOK"),
            ..Default::default()
        }))
        .unwrap();
        assert!(m.drm);
        assert_eq!(m.cde.as_deref(), Some("EBOK"));
        let m = read_meta(&build(&Book {
            title: "Picture Book",
            fixed_layout: true,
            ..Default::default()
        }))
        .unwrap();
        assert!(m.fixed_layout && !m.drm);
    }

    #[test]
    fn a_cp1252_title_and_no_exth_fall_back_to_the_full_name() {
        let m = read_meta(&build(&Book {
            title: "Caf\u{e9} Society",
            cp1252: true,
            no_exth: true,
            ..Default::default()
        }))
        .unwrap();
        assert_eq!(m.title.as_deref(), Some("Caf\u{e9} Society"));
        assert!(m.authors.is_empty() && m.cover.is_none());
    }

    #[test]
    fn other_files_are_refused() {
        assert!(read_meta(b"PK\x03\x04 not a palm database").is_err());
        let mut bytes = build(&Book::default());
        bytes.truncate(90);
        assert!(read_meta(&bytes).is_err());
        assert!(!is_mobi(b"short"));
    }
}
