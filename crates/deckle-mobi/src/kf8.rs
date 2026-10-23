// SPDX-License-Identifier: GPL-3.0-or-later
//! A DRM-free KF8 book (AZW3, or the KF8 half of a joint MOBI) back into an EPUB (Books and
//! Kindle design §5). The steps follow KindleUnpack's KF8 path (GPL-3.0): the text records
//! decompressed into one stream, split into flows by the FDST, the XHTML parts put back
//! together from the skeleton and fragment indexes, `kindle:` references turned into files and
//! anchors, the NCX index into a table of contents, and EXTH into the package metadata.
//!
//! A quality gate runs before anything is written: a book that doesn't come back whole is
//! refused, so an upgrade never replaces a good copy with a broken one.

use std::collections::{BTreeMap, BTreeSet};
use std::io::Read;
use std::path::Path;
use std::sync::LazyLock;

use regex::bytes::Regex;

use crate::MobiError;
use crate::epub::{Book, Resource, TocEntry};
use crate::index::Index;
use crate::palmdb::{PalmDb, be16, be32};

/// What a rebuild made.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Rebuilt {
    pub title: String,
    pub parts: usize,
    pub images: usize,
    pub warnings: Vec<String>,
}

const NONE: u32 = u32::MAX;

/// Kindle's base-32 digits.
fn base32(s: &[u8]) -> Option<u64> {
    let mut v = 0u64;
    for &c in s {
        let d = match c {
            b'0'..=b'9' => c - b'0',
            b'A'..=b'V' => c - b'A' + 10,
            b'a'..=b'v' => c - b'a' + 10,
            _ => return None,
        };
        v = v * 32 + u64::from(d);
    }
    Some(v)
}

/// One `(start, end)` piece of the text stream.
#[derive(Clone, Debug)]
struct Skeleton {
    filenum: u64,
    pos: usize,
    len: usize,
    frags: usize,
}

#[derive(Clone, Debug)]
struct Fragment {
    insert: usize,
    aid: Vec<u8>,
    filenum: u64,
    len: usize,
    start: usize,
}

/// An XHTML part as rebuilt: its file name, its text, and where it sits in the stream.
struct Part {
    name: String,
    text: Vec<u8>,
    start: usize,
    end: usize,
}

fn exth(rec0: &[u8]) -> Vec<(u32, Vec<u8>)> {
    let Some(hl) = be32(rec0, 20) else {
        return Vec::new();
    };
    let at = 16 + hl as usize;
    if be32(rec0, 0x80).is_none_or(|f| f & 0x40 == 0) || rec0.get(at..at + 4) != Some(b"EXTH") {
        return Vec::new();
    }
    let n = be32(rec0, at + 8).unwrap_or(0);
    let mut out = Vec::new();
    let mut p = at + 12;
    for _ in 0..n {
        let (Some(kind), Some(len)) = (be32(rec0, p), be32(rec0, p + 4)) else {
            break;
        };
        let len = (len as usize).max(8);
        let Some(data) = rec0.get(p + 8..p + len) else {
            break;
        };
        out.push((kind, data.to_vec()));
        p += len;
    }
    out
}

/// The record that holds the KF8 header: record 0 of an AZW3, else the one after a joint
/// file's boundary (EXTH 121).
fn kf8_base(db: &PalmDb<'_>) -> Result<usize, MobiError> {
    let rec0 = db.record(0);
    if be32(rec0, 0x24) == Some(8) {
        return Ok(0);
    }
    let boundary = exth(rec0)
        .into_iter()
        .find(|(k, _)| *k == 121)
        .and_then(|(_, d)| be32(&d, 0))
        .filter(|b| *b != NONE)
        .ok_or_else(|| MobiError::Unsupported("an older MOBI with no KF8 part".into()))?
        as usize;
    let base = if db.record(boundary).starts_with(b"BOUNDARY") {
        boundary + 1
    } else {
        boundary
    };
    if db.record(base).get(16..20) != Some(b"MOBI") {
        return Err(MobiError::Corrupt("no KF8 header where EXTH 121 points"));
    }
    Ok(base)
}

/// The book's text stream: every text record, trailing entries off, decompressed.
fn text_stream(db: &PalmDb<'_>, base: usize, rec0: &[u8]) -> Result<Vec<u8>, MobiError> {
    let compression = be16(rec0, 0).unwrap_or(0);
    if be16(rec0, 12).unwrap_or(0) != 0 {
        return Err(MobiError::Unsupported("it is DRM-protected".into()));
    }
    let length = be32(rec0, 4).unwrap_or(0) as usize;
    let count = usize::from(be16(rec0, 8).unwrap_or(0));
    let header_len = be32(rec0, 20).unwrap_or(0);
    let flags = if header_len >= 0xE4 {
        be16(rec0, 0xF2).unwrap_or(0)
    } else {
        0
    };
    let mut out = Vec::with_capacity(length);
    for i in 1..=count {
        let rec = crate::text::trim_trailing(db.record(base + i), flags);
        match compression {
            1 => out.extend_from_slice(rec),
            2 => crate::text::palmdoc(rec, &mut out)?,
            17480 => {
                return Err(MobiError::Unsupported(
                    "HUFF/CDIC compression isn't read yet".into(),
                ));
            }
            _ => {
                return Err(MobiError::Unsupported(format!(
                    "compression type {compression}"
                )));
            }
        }
    }
    if out.len() * 100 < length * 98 {
        return Err(MobiError::Corrupt("the text came back short"));
    }
    out.truncate(length);
    Ok(out)
}

fn index_at(
    db: &PalmDb<'_>,
    base: usize,
    rec0: &[u8],
    at: usize,
) -> Result<Option<Index>, MobiError> {
    match be32(rec0, at) {
        Some(i) if i != NONE => crate::index::read(db, base, i as usize).map(Some),
        _ => Ok(None),
    }
}

/// The tag that holds `aid`, as (start, end) in `skeleton`.
fn tag_with_aid(skeleton: &[u8], aid: &[u8]) -> Option<(usize, usize)> {
    for quote in *b"'\"" {
        let needle: Vec<u8> = [b"aid=".as_slice(), &[quote], aid, &[quote]].concat();
        if let Some(at) = find(skeleton, &needle, 0) {
            let start = skeleton[..at].iter().rposition(|&b| b == b'<')?;
            let end = find(skeleton, b">", at)?;
            return Some((start, end));
        }
    }
    None
}

fn find(hay: &[u8], needle: &[u8], from: usize) -> Option<usize> {
    if from > hay.len() || needle.is_empty() {
        return None;
    }
    hay[from..]
        .windows(needle.len())
        .position(|w| w == needle)
        .map(|p| p + from)
}

fn rfind(hay: &[u8], b: u8) -> Option<usize> {
    hay.iter().rposition(|&c| c == b)
}

/// The parts, from the skeleton and fragment tables over the main flow.
fn parts(
    text: &[u8],
    skels: &[Skeleton],
    frags: &[Fragment],
    warnings: &mut Vec<String>,
) -> Result<Vec<Part>, MobiError> {
    let mut out = Vec::new();
    let mut fp = 0;
    for (k, s) in skels.iter().enumerate() {
        let mut base = s.pos + s.len;
        let mut skeleton = text
            .get(s.pos..base)
            .ok_or(MobiError::Corrupt("a skeleton past the end of the text"))?
            .to_vec();
        let mut name = format!("part{:04}.xhtml", s.filenum);
        for i in 0..s.frags {
            let f = frags
                .get(fp)
                .ok_or(MobiError::Corrupt("fewer fragments than the skeletons say"))?;
            if i == 0 {
                name = format!("part{:04}.xhtml", f.filenum);
            }
            let slice = text
                .get(base..base + f.len)
                .ok_or(MobiError::Corrupt("a fragment past the end of the text"))?;
            let mut at = f
                .insert
                .checked_sub(s.pos)
                .ok_or(MobiError::Corrupt("a fragment before its skeleton"))?;
            at = at.min(skeleton.len());
            let (head, tail) = skeleton.split_at(at);
            let broken = match (find(tail, b">", 0), find(tail, b"<", 0)) {
                (Some(g), Some(l)) => g < l,
                (Some(_), None) => true,
                _ => false,
            } || match (rfind(head, b'>'), rfind(head, b'<')) {
                (Some(g), Some(l)) => g < l,
                (None, Some(_)) => true,
                _ => false,
            };
            if broken && let Some((_, end)) = tag_with_aid(&skeleton, &f.aid) {
                warnings.push(format!(
                    "{name}: a fragment's place was off; found it by its anchor"
                ));
                at = (end + 1 + f.start).min(skeleton.len());
            }
            skeleton.splice(at..at, slice.iter().copied());
            base += f.len;
            fp += 1;
        }
        let _ = k;
        out.push(Part {
            name,
            text: skeleton,
            start: s.pos,
            end: base,
        });
    }
    Ok(out)
}

/// The anchor nearest before stream position `pos` in its part: an `id` or `name`, else an
/// `aid` (made `aid-…`); empty for the top of the file.
fn anchor_at(part: &Part, pos: usize, linked: &mut BTreeSet<Vec<u8>>) -> Vec<u8> {
    static ID: LazyLock<Regex> =
        LazyLock::new(|| Regex::new(r#"(?i)^<[^>]*\s(?:id|name)\s*=\s*['"]([^'"]*)['"]"#).unwrap());
    static AID: LazyLock<Regex> =
        LazyLock::new(|| Regex::new(r#"^<[^>]+\s(?:aid|AID)\s*=\s*['"]([^'"]+)['"]"#).unwrap());
    let text = &part.text;
    let mut n = pos.saturating_sub(part.start).min(text.len());
    // Inside a tag: search from its end.
    let gt = find(text, b">", n);
    let lt = find(text, b"<", n);
    if let Some(g) = gt
        && (lt == Some(n) || lt.is_none_or(|l| g < l))
    {
        n = g + 1;
    }
    let before = &text[..n.min(text.len())];
    let mut end = before.len();
    while let Some(start) = rfind(&before[..end], b'<') {
        let tag = &before[start..end];
        end = start;
        if tag.starts_with(b"<body") {
            return Vec::new();
        }
        if tag.starts_with(b"<meta") || tag.starts_with(b"</") {
            continue;
        }
        if let Some(c) = ID.captures(tag) {
            return c[1].to_vec();
        }
        if let Some(c) = AID.captures(tag) {
            linked.insert(c[1].to_vec());
            return [b"aid-".as_slice(), &c[1]].concat();
        }
    }
    Vec::new()
}

struct Resources {
    /// Resource number (1-based, as `kindle:embed` counts) → file name and media type.
    files: BTreeMap<u64, (String, &'static str)>,
    list: Vec<Resource>,
}

fn image_type(b: &[u8]) -> Option<(&'static str, &'static str)> {
    if b.starts_with(&[0xFF, 0xD8]) {
        Some(("jpg", "image/jpeg"))
    } else if b.starts_with(&[0x89, b'P', b'N', b'G']) {
        Some(("png", "image/png"))
    } else if b.starts_with(b"GIF8") {
        Some(("gif", "image/gif"))
    } else if b.starts_with(b"BM") {
        Some(("bmp", "image/bmp"))
    } else {
        None
    }
}

/// A `FONT` record's font: its XOR obfuscation undone, then inflated.
fn font(rec: &[u8]) -> Option<(Vec<u8>, &'static str, &'static str)> {
    let flags = be32(rec, 8)?;
    let start = be32(rec, 12)? as usize;
    let xor_len = be32(rec, 16)? as usize;
    let xor_at = be32(rec, 20)? as usize;
    let mut data = rec.get(start..)?.to_vec();
    if flags & 2 != 0 && xor_len > 0 {
        let key = rec.get(xor_at..xor_at + xor_len)?;
        for (i, b) in data.iter_mut().take(1040).enumerate() {
            *b ^= key[i % xor_len];
        }
    }
    if flags & 1 != 0 {
        let mut out = Vec::new();
        flate2::read::ZlibDecoder::new(&data[..])
            .read_to_end(&mut out)
            .ok()?;
        data = out;
    }
    let (ext, mime) = match data.get(..4)? {
        b"OTTO" => ("otf", "font/otf"),
        b"wOFF" => ("woff", "font/woff"),
        _ => ("ttf", "font/ttf"),
    };
    Some((data, ext, mime))
}

fn resources(db: &PalmDb<'_>, base: usize, rec0: &[u8]) -> Resources {
    let mut out = Resources {
        files: BTreeMap::new(),
        list: Vec::new(),
    };
    let first = match be32(rec0, 0x6C) {
        Some(f) if f != NONE => f as usize,
        _ => return out,
    };
    // A joint file's resources are shared, counted from the file's start.
    let start = if image_type(db.record(base + first)).is_some()
        || db.record(base + first).starts_with(b"FONT")
    {
        base + first
    } else {
        first
    };
    let mut n = 1u64;
    let mut i = start;
    while i < db.len() {
        let rec = db.record(i);
        if rec.starts_with(b"FLIS")
            || rec.starts_with(b"FCIS")
            || rec.starts_with(b"FDST")
            || rec.starts_with(b"DATP")
            || rec.starts_with(b"SRCS")
            || rec.starts_with(b"BOUNDARY")
            || rec == b"\xe9\x8e\r\n"
        {
            break;
        }
        if let Some((ext, mime)) = image_type(rec) {
            let name = format!("image{n:05}.{ext}");
            out.files.insert(n, (format!("../Images/{name}"), mime));
            out.list.push(Resource {
                path: format!("Images/{name}"),
                media_type: mime.to_owned(),
                data: rec.to_vec(),
            });
        } else if rec.starts_with(b"FONT")
            && let Some((data, ext, mime)) = font(rec)
        {
            let name = format!("font{n:05}.{ext}");
            out.files.insert(n, (format!("../Fonts/{name}"), mime));
            out.list.push(Resource {
                path: format!("Fonts/{name}"),
                media_type: mime.to_owned(),
                data,
            });
        }
        n += 1;
        i += 1;
    }
    out
}

fn utf8(b: &[u8]) -> String {
    String::from_utf8_lossy(b).into_owned()
}

/// Rebuilds the DRM-free KF8 book in `bytes` as an EPUB at `out`.
pub fn rebuild(bytes: &[u8], out: &Path) -> Result<Rebuilt, MobiError> {
    let db = PalmDb::parse(bytes)?;
    if crate::read_meta(bytes)?.drm {
        return Err(MobiError::Unsupported("it is DRM-protected".into()));
    }
    let base = kf8_base(&db)?;
    let rec0 = db.record(base);
    let ex = exth(rec0);
    if ex
        .iter()
        .any(|(k, d)| *k == 122 && d.eq_ignore_ascii_case(b"true"))
    {
        return Err(MobiError::Unsupported("a fixed-layout book".into()));
    }
    let raw = text_stream(&db, base, rec0)?;
    let mut warnings = Vec::new();

    // Flows: the FDST's starts, else the whole stream as one.
    let mut bounds = vec![0, raw.len()];
    if let Some(f) = be32(rec0, 0xC0).filter(|f| *f != NONE) {
        let rec = db.record(base + f as usize);
        if rec.starts_with(b"FDST") {
            let n = be32(rec, 8).unwrap_or(0) as usize;
            let mut starts: Vec<usize> = (0..n)
                .filter_map(|i| be32(rec, 12 + i * 8).map(|v| v as usize))
                .collect();
            starts.push(raw.len());
            bounds = starts;
        }
    }
    let flows: Vec<&[u8]> = bounds
        .windows(2)
        .map(|w| {
            raw.get(w[0].min(raw.len())..w[1].min(raw.len()))
                .unwrap_or_default()
        })
        .collect();
    let main = *flows.first().ok_or(MobiError::Corrupt("no text"))?;

    let skel = index_at(&db, base, rec0, 0xFC)?
        .ok_or_else(|| MobiError::Unsupported("no skeleton index".into()))?;
    let frag = index_at(&db, base, rec0, 0xF8)?
        .ok_or_else(|| MobiError::Unsupported("no fragment index".into()))?;
    let skels: Vec<Skeleton> = skel
        .entries
        .iter()
        .enumerate()
        .map(|(i, e)| Skeleton {
            filenum: i as u64,
            frags: e.tag(1, 0).unwrap_or(0) as usize,
            pos: e.tag(6, 0).unwrap_or(0) as usize,
            len: e.tag(6, 1).unwrap_or(0) as usize,
        })
        .collect();
    let frags: Vec<Fragment> = frag
        .entries
        .iter()
        .map(|e| {
            let idtext = e
                .tag(2, 0)
                .and_then(|o| frag.ctoc.get(&o))
                .cloned()
                .unwrap_or_default();
            Fragment {
                insert: std::str::from_utf8(&e.name)
                    .ok()
                    .and_then(|s| s.trim().parse().ok())
                    .unwrap_or(0),
                // `P-//*[@aid='0']`: the anchor between the quotes.
                aid: idtext
                    .get(12..idtext.len().saturating_sub(2))
                    .unwrap_or_default()
                    .to_vec(),
                filenum: e.tag(3, 0).unwrap_or(0),
                start: e.tag(6, 0).unwrap_or(0) as usize,
                len: e.tag(6, 1).unwrap_or(0) as usize,
            }
        })
        .collect();
    let mut parts = parts(main, &skels, &frags, &mut warnings)?;
    let covered = parts.last().map_or(0, |p| p.end);
    if covered * 100 < main.len() * 98 {
        return Err(MobiError::Corrupt("the parts don't cover the text"));
    }

    // Links: `kindle:pos:fid:XXXX:off:YYYYYYYYYY` → `partNNNN.xhtml#anchor`.
    static POS: LazyLock<Regex> = LazyLock::new(|| {
        Regex::new(r"kindle:pos:fid:([0-9A-Va-v]{4}):off:([0-9A-Va-v]{10})").unwrap()
    });
    let mut linked: BTreeSet<Vec<u8>> = BTreeSet::new();
    let resolve = |fid: &[u8],
                   off: &[u8],
                   linked: &mut BTreeSet<Vec<u8>>,
                   parts: &[Part]|
     -> Option<String> {
        let f = frags.get(base32(fid)? as usize)?;
        let pos = f.insert + base32(off)? as usize;
        let part = parts
            .iter()
            .find(|p| p.start <= pos && pos < p.end)
            .or_else(|| parts.last())?;
        let anchor = anchor_at(part, pos, linked);
        Some(if anchor.is_empty() {
            part.name.clone()
        } else {
            format!("{}#{}", part.name, utf8(&anchor))
        })
    };
    let mut targets: Vec<Vec<(Vec<u8>, String)>> = Vec::new();
    for p in &parts {
        let mut t = Vec::new();
        for c in POS.captures_iter(&p.text) {
            if let Some(href) = resolve(&c[1], &c[2], &mut linked, &parts) {
                t.push((c[0].to_vec(), href));
            }
        }
        targets.push(t);
    }
    for (p, t) in parts.iter_mut().zip(targets) {
        for (from, to) in t {
            p.text = replace(&p.text, &from, to.as_bytes());
        }
    }

    // Resources and flows.
    let res = resources(&db, base, rec0);
    static EMBED: LazyLock<Regex> =
        LazyLock::new(|| Regex::new(r"kindle:embed:([0-9A-Va-v]{4})(\?mime=[^'\x22)]*)?").unwrap());
    static FLOW: LazyLock<Regex> = LazyLock::new(|| {
        Regex::new(r"kindle:flow:([0-9A-Va-v]{4})(\?mime=([^'\x22)]*))?").unwrap()
    });
    let mut missing = BTreeSet::new();
    let mut used_images = BTreeSet::new();
    let mut rewrite = |text: &[u8], missing: &mut BTreeSet<u64>| -> Vec<u8> {
        let t = EMBED.replace_all(text, |c: &regex::bytes::Captures| {
            match base32(&c[1]).and_then(|n| res.files.get(&n).map(|f| (n, f))) {
                Some((n, (path, _))) => {
                    used_images.insert(n);
                    path.as_bytes().to_vec()
                }
                None => {
                    if let Some(n) = base32(&c[1]) {
                        missing.insert(n);
                    }
                    c[0].to_vec()
                }
            }
        });
        FLOW.replace_all(&t, |c: &regex::bytes::Captures| {
            let n = base32(&c[1]).unwrap_or(0);
            let ext = flow_ext(
                c.get(3).map(|m| m.as_bytes()),
                flows.get(n as usize).copied().unwrap_or_default(),
            );
            format!("../Styles/flow{n:04}.{ext}").into_bytes()
        })
        .into_owned()
    };
    for p in &mut parts {
        p.text = rewrite(&p.text, &mut missing);
    }
    let mut extra = Vec::new();
    for (n, flow) in flows.iter().enumerate().skip(1) {
        if flow.is_empty() {
            continue;
        }
        let ext = flow_ext(None, flow);
        let data = rewrite(flow, &mut missing);
        extra.push(Resource {
            path: format!("Styles/flow{n:04}.{ext}"),
            media_type: match ext {
                "css" => "text/css",
                "svg" => "image/svg+xml",
                _ => "text/plain",
            }
            .to_owned(),
            data,
        });
    }
    if !missing.is_empty() {
        return Err(MobiError::Corrupt("a picture the text uses is missing"));
    }

    // Anchors: an `aid` a link needs becomes an `id`; the rest go, with the Kindle's `cid`s.
    static AID_ATTR: LazyLock<Regex> =
        LazyLock::new(|| Regex::new(r#"\s(?:aid|AID)\s*=\s*['"]([^'"]*)['"]"#).unwrap());
    static CID_ATTR: LazyLock<Regex> =
        LazyLock::new(|| Regex::new(r#"\scid\s*=\s*['"][^'"]*['"]"#).unwrap());
    for p in &mut parts {
        p.text = AID_ATTR
            .replace_all(&p.text, |c: &regex::bytes::Captures| {
                if linked.contains(&c[1]) {
                    [b" id=\"aid-".as_slice(), &c[1], b"\""].concat()
                } else {
                    Vec::new()
                }
            })
            .into_owned();
        p.text = CID_ATTR.replace_all(&p.text, &b""[..]).into_owned();
    }

    // Every part must be well-formed XML, or the book isn't whole.
    let mut xhtml = Vec::new();
    for p in &parts {
        let text = utf8(&p.text);
        if let Err(e) = roxmltree::Document::parse_with_options(
            &text,
            roxmltree::ParsingOptions {
                allow_dtd: true,
                ..Default::default()
            },
        ) {
            return Err(MobiError::BadPart(p.name.clone(), e.to_string()));
        }
        xhtml.push((format!("Text/{}", p.name), text));
    }

    // The table of contents.
    let mut toc = Vec::new();
    if let Some(ncx) = index_at(&db, base, rec0, 0xF4)? {
        for e in &ncx.entries {
            let title = e
                .tag(3, 0)
                .and_then(|o| ncx.ctoc.get(&o))
                .map(|t| utf8(t))
                .unwrap_or_default();
            let (Some(fid), Some(off)) = (e.tag(6, 0), e.tag(6, 1)) else {
                continue;
            };
            let Some(f) = frags.get(fid as usize) else {
                continue;
            };
            let pos = f.insert + off as usize;
            let Some(part) = parts
                .iter()
                .find(|p| p.start <= pos && pos < p.end)
                .or_else(|| parts.last())
            else {
                continue;
            };
            let anchor = anchor_at(part, pos, &mut BTreeSet::new());
            toc.push(TocEntry {
                level: e.tag(4, 0).unwrap_or(0) as usize,
                title: title.trim().to_owned(),
                href: if anchor.is_empty() {
                    format!("Text/{}", part.name)
                } else {
                    format!("Text/{}#{}", part.name, utf8(&anchor))
                },
            });
        }
    }

    // Metadata from EXTH.
    let get = |k: u32| {
        ex.iter()
            .filter(move |(kind, _)| *kind == k)
            .map(|(_, d)| utf8(d).trim().to_owned())
    };
    let meta = crate::read_meta(bytes)?;
    let title = get(503)
        .next()
        .or(meta.title.clone())
        .unwrap_or_else(|| "Untitled".into());
    let cover = ex
        .iter()
        .find(|(k, _)| *k == 201)
        .and_then(|(_, d)| be32(d, 0))
        .filter(|o| *o != NONE)
        .and_then(|o| res.files.get(&(u64::from(o) + 1)))
        .map(|(p, _)| p.trim_start_matches("../").to_owned());
    let images = res
        .list
        .iter()
        .filter(|r| r.media_type.starts_with("image/"))
        .count();
    let book = Book {
        title: title.clone(),
        authors: get(100).collect(),
        language: get(524).next().unwrap_or_else(|| "en".into()),
        identifier: meta
            .asin
            .clone()
            .map(|a| format!("urn:asin:{a}"))
            .unwrap_or_else(|| format!("urn:deckle:kf8:{:016x}", fnv(bytes))),
        publisher: get(101).next(),
        description: get(103).next(),
        isbn: get(104).next(),
        subjects: get(105).collect(),
        date: get(106).next(),
        cover,
        parts: xhtml,
        resources: res.list.into_iter().chain(extra).collect(),
        toc,
    };
    crate::epub::write(out, &book)?;
    Ok(Rebuilt {
        title,
        parts: book.parts.len(),
        images,
        warnings,
    })
}

fn flow_ext(mime: Option<&[u8]>, data: &[u8]) -> &'static str {
    match mime {
        Some(m) if m.ends_with(b"css") => "css",
        Some(m) if m.ends_with(b"svg+xml") => "svg",
        _ if data.starts_with(b"<?xml") || find(data, b"<svg", 0).is_some() => "svg",
        _ => "css",
    }
}

fn replace(hay: &[u8], from: &[u8], to: &[u8]) -> Vec<u8> {
    let mut out = Vec::with_capacity(hay.len());
    let mut at = 0;
    while let Some(i) = find(hay, from, at) {
        out.extend_from_slice(&hay[at..i]);
        out.extend_from_slice(to);
        at = i + from.len();
    }
    out.extend_from_slice(&hay[at..]);
    out
}

fn fnv(b: &[u8]) -> u64 {
    let mut h: u64 = 0xcbf2_9ce4_8422_2325;
    for &x in b {
        h ^= u64::from(x);
        h = h.wrapping_mul(0x0100_0000_01b3);
    }
    h
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fixture::{Kf8, build_kf8};

    fn doc(title: &str, body: &str) -> String {
        format!(
            "<?xml version=\"1.0\" encoding=\"utf-8\"?><html xmlns=\"http://www.w3.org/1999/xhtml\"><head><title>{title}</title><link rel=\"stylesheet\" type=\"text/css\" href=\"kindle:flow:0001?mime=text/css\"/></head><body aid=\"0\">{body}</body></html>"
        )
    }

    fn read_zip(path: &Path) -> BTreeMap<String, Vec<u8>> {
        let mut z = zip::ZipArchive::new(std::fs::File::open(path).unwrap()).unwrap();
        (0..z.len())
            .map(|i| {
                let mut f = z.by_index(i).unwrap();
                let mut b = Vec::new();
                f.read_to_end(&mut b).unwrap();
                (f.name().unwrap().to_string(), b)
            })
            .collect()
    }

    fn book() -> Vec<u8> {
        let jpeg: &[u8] = b"\xFF\xD8\xFF\xE0 a picture";
        let p0 = doc(
            "Cover",
            "<p><img src=\"kindle:embed:0001?mime=image/jpeg\" alt=\"\"/></p>",
        );
        let p1 = doc(
            "One",
            "<h2 id=\"c1\">Chapter One</h2><p aid=\"5\">It was a <a href=\"{{pos:2:<h2}}\">dark</a> night.</p>",
        );
        let p2 = doc(
            "Two",
            &format!(
                "<h2 id=\"c2\">Chapter Two</h2>{}",
                "<p>Then the storm came over the hill and stayed until morning.</p>".repeat(120)
            ),
        );
        let parts = [p0.as_str(), p1.as_str(), p2.as_str()];
        build_kf8(&Kf8 {
            title: "A Fixture Book",
            authors: &["Jane Testwright"],
            parts: &parts,
            css: "p { margin: 0 }",
            images: &[jpeg],
            toc: &[("Chapter One", 0, 1, "<h2"), ("Chapter Two", 0, 2, "<h2")],
            cover: Some(1),
            ..Default::default()
        })
    }

    #[test]
    fn a_kf8_book_comes_back_as_an_epub_with_its_links_pictures_and_contents() {
        let tmp = tempfile::tempdir().unwrap();
        let out = tmp.path().join("book.epub");
        let r = rebuild(&book(), &out).unwrap();
        assert_eq!(
            (r.title.as_str(), r.parts, r.images),
            ("A Fixture Book", 3, 1)
        );
        let z = read_zip(&out);
        assert_eq!(z["mimetype"], b"application/epub+zip");
        let one = String::from_utf8(z["OEBPS/Text/part0001.xhtml"].clone()).unwrap();
        assert!(
            one.contains("<a href=\"part0002.xhtml#c2\">dark</a>"),
            "{one}"
        );
        assert!(one.contains("href=\"../Styles/flow0001.css\""), "{one}");
        assert!(!one.contains("aid="), "unlinked aids go: {one}");
        assert!(!one.contains("kindle:"));
        let cover = String::from_utf8(z["OEBPS/Text/part0000.xhtml"].clone()).unwrap();
        assert!(
            cover.contains("src=\"../Images/image00001.jpg\""),
            "{cover}"
        );
        assert_eq!(
            z["OEBPS/Images/image00001.jpg"],
            b"\xFF\xD8\xFF\xE0 a picture"
        );
        assert_eq!(z["OEBPS/Styles/flow0001.css"], b"p { margin: 0 }");
        let two = String::from_utf8(z["OEBPS/Text/part0002.xhtml"].clone()).unwrap();
        assert_eq!(
            two.matches("Then the storm came over the hill").count(),
            120
        );
        let nav = String::from_utf8(z["OEBPS/nav.xhtml"].clone()).unwrap();
        assert!(
            nav.contains("<a href=\"Text/part0001.xhtml#c1\">Chapter One</a>"),
            "{nav}"
        );
        assert!(
            nav.contains("<a href=\"Text/part0002.xhtml#c2\">Chapter Two</a>"),
            "{nav}"
        );
        let opf = String::from_utf8(z["OEBPS/content.opf"].clone()).unwrap();
        assert!(opf.contains("<dc:title>A Fixture Book</dc:title>"));
        assert!(opf.contains("<dc:creator>Jane Testwright</dc:creator>"));
        assert!(opf.contains("href=\"Images/image00001.jpg\" media-type=\"image/jpeg\" properties=\"cover-image\""), "{opf}");
        for (name, bytes) in &z {
            if name.ends_with(".xhtml") || name.ends_with(".opf") || name.ends_with(".ncx") {
                roxmltree::Document::parse(std::str::from_utf8(bytes).unwrap()).unwrap();
            }
        }
    }

    #[test]
    fn the_gate_refuses_damaged_drm_fixed_layout_and_older_books() {
        let tmp = tempfile::tempdir().unwrap();
        let out = tmp.path().join("x.epub");
        let p = doc("One", "<p>Text.</p>");
        let cut = build_kf8(&Kf8 {
            title: "Cut",
            parts: &[p.as_str()],
            cut_text: true,
            ..Default::default()
        });
        assert!(matches!(rebuild(&cut, &out), Err(MobiError::Corrupt(_))));
        let drm = crate::fixture::build(&crate::fixture::Book {
            title: "Store",
            drm: true,
            kf8: true,
            ..Default::default()
        });
        assert!(matches!(rebuild(&drm, &out), Err(MobiError::Unsupported(w)) if w.contains("DRM")));
        let fixed = crate::fixture::build(&crate::fixture::Book {
            title: "Comic",
            fixed_layout: true,
            kf8: true,
            ..Default::default()
        });
        assert!(
            matches!(rebuild(&fixed, &out), Err(MobiError::Unsupported(w)) if w.contains("fixed"))
        );
        let old = crate::fixture::build(&crate::fixture::Book {
            title: "Old",
            ..Default::default()
        });
        assert!(
            matches!(rebuild(&old, &out), Err(MobiError::Unsupported(w)) if w.contains("older MOBI"))
        );
        assert!(!out.exists(), "nothing is written for a refused book");
    }

    #[test]
    fn base32_is_kindles() {
        assert_eq!(base32(b"0001"), Some(1));
        assert_eq!(base32(b"000V"), Some(31));
        assert_eq!(base32(b"0010"), Some(32));
        assert_eq!(base32(b"00W0"), None);
    }

    #[test]
    fn an_anchor_is_the_nearest_id_before_the_position() {
        let part = Part {
            name: "part0000.xhtml".into(),
            text: b"<html><body><h2 id=\"c1\">One</h2><p aid=\"7\">x</p><p>y</p></body></html>"
                .to_vec(),
            start: 100,
            end: 200,
        };
        let mut linked = BTreeSet::new();
        let at_p2 = 100 + find(&part.text, b"<p>y", 0).unwrap();
        assert_eq!(anchor_at(&part, at_p2, &mut linked), b"aid-7");
        assert!(linked.contains(b"7".as_slice()));
        let at_h2 = 100 + find(&part.text, b"One", 0).unwrap();
        assert_eq!(anchor_at(&part, at_h2, &mut linked), b"c1");
        assert_eq!(anchor_at(&part, 100, &mut linked), b"");
    }
}
