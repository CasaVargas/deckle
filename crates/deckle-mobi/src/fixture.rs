// SPDX-License-Identifier: GPL-3.0-or-later
//! Small, valid PalmDB books for tests in this and other crates: record 0 with a MOBI header
//! and EXTH records, one uncompressed text record, and an optional cover record.

/// What a fixture book says about itself.
#[derive(Clone, Debug, Default)]
pub struct Book<'a> {
    pub title: &'a str,
    pub authors: &'a [&'a str],
    pub asin: Option<&'a str>,
    pub cde: Option<&'a str>,
    pub isbn: Option<&'a str>,
    pub calibre_id: Option<&'a str>,
    pub drm: bool,
    pub fixed_layout: bool,
    pub kf8: bool,
    /// The cover record's bytes.
    pub cover: Option<&'a [u8]>,
    /// Strings in Windows-1252 rather than UTF-8.
    pub cp1252: bool,
    /// No EXTH block: the title is only the full name.
    pub no_exth: bool,
}

const MOBI_HEADER_LEN: usize = 232;
/// The text record's size, and so the PalmDOC text length.
pub const TEXT_LEN: usize = 4096;

fn exth(kind: u32, data: &[u8]) -> Vec<u8> {
    let mut r = kind.to_be_bytes().to_vec();
    r.extend((8 + data.len() as u32).to_be_bytes());
    r.extend(data);
    r
}

/// The file's bytes.
pub fn build(b: &Book<'_>) -> Vec<u8> {
    let enc = |s: &str| -> Vec<u8> {
        if b.cp1252 {
            encoding_rs::WINDOWS_1252.encode(s).0.into_owned()
        } else {
            s.as_bytes().to_vec()
        }
    };
    let mut records: Vec<(u32, Vec<u8>)> = Vec::new();
    if !b.no_exth {
        for a in b.authors {
            records.push((100, enc(a)));
        }
        if let Some(v) = b.isbn {
            records.push((104, enc(v)));
        }
        if let Some(v) = b.calibre_id {
            records.push((112, enc(&format!("calibre:{v}"))));
        }
        if let Some(v) = b.asin {
            records.push((113, enc(v)));
        }
        if b.fixed_layout {
            records.push((122, b"true".to_vec()));
        }
        if b.cover.is_some() {
            records.push((201, 0u32.to_be_bytes().to_vec()));
        }
        if let Some(v) = b.cde {
            records.push((501, enc(v)));
        }
        records.push((503, enc(b.title)));
    }
    let mut exth_block = Vec::new();
    if !b.no_exth {
        let body: Vec<u8> = records.iter().flat_map(|(k, d)| exth(*k, d)).collect();
        exth_block.extend(b"EXTH");
        exth_block.extend((12 + body.len() as u32).to_be_bytes());
        exth_block.extend((records.len() as u32).to_be_bytes());
        exth_block.extend(body);
        while exth_block.len() % 4 != 0 {
            exth_block.push(0);
        }
    }
    let name = enc(b.title);
    let name_at = 16 + MOBI_HEADER_LEN + exth_block.len();

    let mut rec0 = vec![0u8; 16 + MOBI_HEADER_LEN];
    rec0[0..2].copy_from_slice(&1u16.to_be_bytes()); // no compression
    rec0[4..8].copy_from_slice(&(TEXT_LEN as u32).to_be_bytes());
    rec0[8..10].copy_from_slice(&1u16.to_be_bytes()); // one text record
    rec0[10..12].copy_from_slice(&4096u16.to_be_bytes());
    rec0[12..14].copy_from_slice(&u16::from(b.drm).wrapping_mul(2).to_be_bytes());
    rec0[16..20].copy_from_slice(b"MOBI");
    rec0[20..24].copy_from_slice(&(MOBI_HEADER_LEN as u32).to_be_bytes());
    rec0[24..28].copy_from_slice(&2u32.to_be_bytes());
    let encoding: u32 = if b.cp1252 { 1252 } else { 65001 };
    rec0[28..32].copy_from_slice(&encoding.to_be_bytes());
    rec0[36..40].copy_from_slice(&(if b.kf8 { 8u32 } else { 6 }).to_be_bytes());
    rec0[0x50..0x54].copy_from_slice(&2u32.to_be_bytes()); // first non-book record
    rec0[0x54..0x58].copy_from_slice(&(name_at as u32).to_be_bytes());
    rec0[0x58..0x5C].copy_from_slice(&(name.len() as u32).to_be_bytes());
    let first_image: u32 = if b.cover.is_some() { 2 } else { u32::MAX };
    rec0[0x6C..0x70].copy_from_slice(&first_image.to_be_bytes());
    let flags: u32 = if b.no_exth { 0 } else { 0x40 };
    rec0[0x80..0x84].copy_from_slice(&flags.to_be_bytes());
    rec0.extend(exth_block);
    rec0.extend(&name);
    rec0.extend([0, 0]);

    let mut recs = vec![rec0, vec![b'a'; TEXT_LEN]];
    if let Some(c) = b.cover {
        recs.push(c.to_vec());
    }
    let mut out = vec![0u8; 78];
    let short = b.title.as_bytes();
    out[..short.len().min(31)].copy_from_slice(&short[..short.len().min(31)]);
    out[60..68].copy_from_slice(b"BOOKMOBI");
    out[76..78].copy_from_slice(&(recs.len() as u16).to_be_bytes());
    let mut at = 78 + recs.len() * 8 + 2;
    for (i, r) in recs.iter().enumerate() {
        out.extend((at as u32).to_be_bytes());
        out.extend([0, 0, 0, i as u8]);
        at += r.len();
    }
    out.extend([0, 0]);
    for r in recs {
        out.extend(r);
    }
    out
}
