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
        while !exth_block.len().is_multiple_of(4) {
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

/// A small KF8 (AZW3) book for tests: each part one skeleton (the document without its body's
/// content) and one fragment (the body's content), uncompressed text, a CSS flow, images as
/// resources, and skeleton, fragment and NCX indexes as a Kindle book has them.
#[derive(Clone, Debug, Default)]
pub struct Kf8<'a> {
    pub title: &'a str,
    pub authors: &'a [&'a str],
    /// Whole XHTML documents. `{{pos:N:needle}}` becomes a `kindle:pos` link to the first
    /// `needle` in part N's body; `kindle:embed:0001` names image 1, `kindle:flow:0001` the CSS.
    pub parts: &'a [&'a str],
    pub css: &'a str,
    pub images: &'a [&'a [u8]],
    /// Contents entries: title, level, part, and a needle in that part's body.
    pub toc: &'a [(&'a str, u64, usize, &'a str)],
    /// The cover's image number, from 1.
    pub cover: Option<u32>,
    /// Damage for the quality gate: the text records lose their last bytes.
    pub cut_text: bool,
}

fn b32(mut v: u64, width: usize) -> String {
    const D: &[u8] = b"0123456789ABCDEFGHIJKLMNOPQRSTUV";
    let mut out = vec![b'0'; width];
    for i in (0..width).rev() {
        out[i] = D[(v % 32) as usize];
        v /= 32;
    }
    String::from_utf8(out).unwrap()
}

fn vwi(mut v: u64) -> Vec<u8> {
    let mut out = vec![(v & 0x7F) as u8 | 0x80];
    v >>= 7;
    while v > 0 {
        out.insert(0, (v & 0x7F) as u8);
        v >>= 7;
    }
    out
}

/// An index: its header record (with TAGX), one data record, and CTOC records when given.
fn indx(
    tags: &[[u8; 4]],
    control: usize,
    entries: &[(Vec<u8>, Vec<u8>)],
    ctoc: &[u8],
) -> Vec<Vec<u8>> {
    const HEADER: usize = 0xC0;
    let field = |rec: &mut Vec<u8>, i: usize, v: u32| {
        rec[4 + i * 4..8 + i * 4].copy_from_slice(&v.to_be_bytes())
    };
    let mut head = vec![0u8; HEADER];
    head[..4].copy_from_slice(b"INDX");
    field(&mut head, 0, HEADER as u32);
    field(&mut head, 5, 1);
    field(&mut head, 8, entries.len() as u32);
    field(&mut head, 12, u32::from(!ctoc.is_empty()));
    head.extend(b"TAGX");
    head.extend((12 + 4 * tags.len() as u32).to_be_bytes());
    head.extend((control as u32).to_be_bytes());
    for t in tags {
        head.extend(t);
    }
    let mut data = vec![0u8; HEADER];
    data[..4].copy_from_slice(b"INDX");
    field(&mut data, 0, HEADER as u32);
    let mut offsets = Vec::new();
    for (name, body) in entries {
        offsets.push(data.len() as u16);
        data.push(name.len() as u8);
        data.extend(name);
        data.extend(body);
    }
    let idxt = data.len() as u32;
    field(&mut data, 4, idxt);
    field(&mut data, 5, entries.len() as u32);
    data.extend(b"IDXT");
    for o in offsets {
        data.extend(o.to_be_bytes());
    }
    let mut out = vec![head, data];
    if !ctoc.is_empty() {
        out.push(ctoc.to_vec());
    }
    out
}

/// The file's bytes.
pub fn build_kf8(b: &Kf8<'_>) -> Vec<u8> {
    let link_len = "kindle:pos:fid:0000:off:0000000000".len();
    // Parts with links at their final length, split at the body.
    let mut docs: Vec<String> = b.parts.iter().map(|p| p.to_string()).collect();
    let mut links = Vec::new();
    for (i, d) in docs.iter_mut().enumerate() {
        while let Some(s) = d.find("{{pos:") {
            let e = d[s..].find("}}").unwrap() + s + 2;
            let spec = d[s + 6..e - 2].to_owned();
            let (part, needle) = spec.split_once(':').unwrap();
            d.replace_range(s..e, &"#".repeat(link_len));
            links.push((i, s, part.parse::<usize>().unwrap(), needle.to_owned()));
        }
    }
    let split = |d: &str| {
        let open = d.find("<body").unwrap();
        let start = d[open..].find('>').unwrap() + open + 1;
        let end = d.rfind("</body>").unwrap();
        (start, end)
    };
    let mut text = Vec::new();
    let mut skels = Vec::new();
    let mut frags = Vec::new();
    for (i, d) in docs.iter().enumerate() {
        let (s, e) = split(d);
        let pos = text.len();
        let skeleton = format!("{}{}", &d[..s], &d[e..]);
        text.extend(skeleton.as_bytes());
        let fstart = text.len();
        text.extend(&d.as_bytes()[s..e]);
        skels.push((pos, skeleton.len()));
        frags.push((pos + s, fstart, e - s, i));
    }
    // Links: fragment `part`, offset of the needle within its body.
    for (from, at, part, needle) in &links {
        let (s, e) = split(&docs[*part]);
        let off = docs[*part][s..e].find(needle.as_str()).unwrap();
        let link = format!(
            "kindle:pos:fid:{}:off:{}",
            b32(*part as u64, 4),
            b32(off as u64, 10)
        );
        let (_, fstart, _, _) = frags[*from];
        let (fs, _) = split(&docs[*from]);
        let p = fstart + (at - fs);
        text[p..p + link_len].copy_from_slice(link.as_bytes());
    }
    let flow0 = text.len();
    text.extend(b.css.as_bytes());

    let mut records: Vec<Vec<u8>> = vec![Vec::new()];
    let mut stream = text.clone();
    if b.cut_text {
        stream.truncate(stream.len() / 2);
    }
    for chunk in stream.chunks(4096) {
        records.push(chunk.to_vec());
    }
    let text_records = records.len() - 1;

    // Fragment index (with its CTOC of anchors), skeleton index, NCX.
    let frag_at = records.len();
    let ctoc_entry = b"P-//*[@aid='0']";
    let mut ctoc = vwi(ctoc_entry.len() as u64);
    ctoc.extend(ctoc_entry);
    let frag_entries: Vec<(Vec<u8>, Vec<u8>)> = frags
        .iter()
        .enumerate()
        .map(|(i, &(insert, _, len, _))| {
            let mut body = vec![0x0F];
            for v in [0, i as u64, i as u64, 0, len as u64] {
                body.extend(vwi(v));
            }
            (insert.to_string().into_bytes(), body)
        })
        .collect();
    records.extend(indx(
        &[
            [2, 1, 1, 0],
            [3, 1, 2, 0],
            [4, 1, 4, 0],
            [6, 2, 8, 0],
            [0, 0, 0, 1],
        ],
        1,
        &frag_entries,
        &ctoc,
    ));
    let skel_at = records.len();
    let skel_entries: Vec<(Vec<u8>, Vec<u8>)> = skels
        .iter()
        .enumerate()
        .map(|(i, &(pos, len))| {
            let mut body = vec![0x05];
            for v in [1, pos as u64, len as u64] {
                body.extend(vwi(v));
            }
            (format!("SKEL{i:010}").into_bytes(), body)
        })
        .collect();
    records.extend(indx(
        &[[1, 1, 3, 0], [6, 2, 0x0C, 0], [0, 0, 0, 1]],
        1,
        &skel_entries,
        &[],
    ));
    let ncx_at = records.len();
    let mut titles = Vec::new();
    let mut ncx_entries = Vec::new();
    for (i, (title, level, part, needle)) in b.toc.iter().enumerate() {
        let (s, e) = split(&docs[*part]);
        let off = docs[*part][s..e].find(needle).unwrap() as u64;
        let noffs = titles.len() as u64;
        titles.extend(vwi(title.len() as u64));
        titles.extend(title.as_bytes());
        let mut body = vec![0x1F];
        for v in [0, 0, noffs, *level, *part as u64, off] {
            body.extend(vwi(v));
        }
        ncx_entries.push((format!("{i:02X}").into_bytes(), body));
    }
    if !ncx_entries.is_empty() {
        records.extend(indx(
            &[
                [1, 1, 1, 0],
                [2, 1, 2, 0],
                [3, 1, 4, 0],
                [4, 1, 8, 0],
                [6, 2, 0x10, 0],
                [0, 0, 0, 1],
            ],
            1,
            &ncx_entries,
            &titles,
        ));
    }
    let first_image = records.len();
    for img in b.images {
        records.push(img.to_vec());
    }
    let fdst_at = records.len();
    let mut fdst = b"FDST".to_vec();
    fdst.extend(12u32.to_be_bytes());
    fdst.extend(2u32.to_be_bytes());
    for (s, e) in [(0, flow0), (flow0, text.len())] {
        fdst.extend((s as u32).to_be_bytes());
        fdst.extend((e as u32).to_be_bytes());
    }
    records.push(fdst);
    records.push(b"FLIS\0\0\0\x08".to_vec());

    // Record 0: PalmDOC header, a 264-byte KF8 MOBI header, EXTH, the full name.
    const HL: usize = 264;
    let mut ex: Vec<(u32, Vec<u8>)> = b
        .authors
        .iter()
        .map(|a| (100, a.as_bytes().to_vec()))
        .collect();
    if let Some(c) = b.cover {
        ex.push((201, (c - 1).to_be_bytes().to_vec()));
    }
    ex.push((503, b.title.as_bytes().to_vec()));
    ex.push((524, b"en".to_vec()));
    let body: Vec<u8> = ex.iter().flat_map(|(k, d)| exth(*k, d)).collect();
    let mut exth_block = b"EXTH".to_vec();
    exth_block.extend((12 + body.len() as u32).to_be_bytes());
    exth_block.extend((ex.len() as u32).to_be_bytes());
    exth_block.extend(body);
    while !exth_block.len().is_multiple_of(4) {
        exth_block.push(0);
    }
    let mut rec0 = vec![0u8; 16 + HL];
    let put = |r: &mut Vec<u8>, at: usize, v: u32| r[at..at + 4].copy_from_slice(&v.to_be_bytes());
    rec0[0..2].copy_from_slice(&1u16.to_be_bytes());
    put(&mut rec0, 4, text.len() as u32);
    rec0[8..10].copy_from_slice(&(text_records as u16).to_be_bytes());
    rec0[10..12].copy_from_slice(&4096u16.to_be_bytes());
    rec0[16..20].copy_from_slice(b"MOBI");
    put(&mut rec0, 20, HL as u32);
    put(&mut rec0, 24, 2);
    put(&mut rec0, 28, 65001);
    put(&mut rec0, 36, 8);
    put(&mut rec0, 0x50, first_image as u32);
    put(&mut rec0, 0x54, (16 + HL + exth_block.len()) as u32);
    put(&mut rec0, 0x58, b.title.len() as u32);
    put(
        &mut rec0,
        0x6C,
        if b.images.is_empty() {
            u32::MAX
        } else {
            first_image as u32
        },
    );
    put(&mut rec0, 0x80, 0x40);
    put(&mut rec0, 0xC0, fdst_at as u32);
    put(&mut rec0, 0xC4, 2);
    put(&mut rec0, 0xF0, 0);
    put(
        &mut rec0,
        0xF4,
        if b.toc.is_empty() {
            u32::MAX
        } else {
            ncx_at as u32
        },
    );
    put(&mut rec0, 0xF8, frag_at as u32);
    put(&mut rec0, 0xFC, skel_at as u32);
    put(&mut rec0, 0x104, u32::MAX);
    rec0.extend(exth_block);
    rec0.extend(b.title.as_bytes());
    rec0.extend([0, 0]);
    records[0] = rec0;

    let mut out = vec![0u8; 78];
    out[..b.title.len().min(31)].copy_from_slice(&b.title.as_bytes()[..b.title.len().min(31)]);
    out[60..68].copy_from_slice(b"BOOKMOBI");
    out[76..78].copy_from_slice(&(records.len() as u16).to_be_bytes());
    let mut at = 78 + records.len() * 8 + 2;
    for (i, r) in records.iter().enumerate() {
        out.extend((at as u32).to_be_bytes());
        out.extend([0, (i >> 16) as u8, (i >> 8) as u8, i as u8]);
        at += r.len();
    }
    out.extend([0, 0]);
    for r in records {
        out.extend(r);
    }
    out
}
