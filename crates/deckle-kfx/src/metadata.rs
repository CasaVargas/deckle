// SPDX-License-Identifier: GPL-3.0-or-later
//! Title metadata stamping (kfxlib `set_yj_metadata_to_book`).

use crate::fixup::Book;
use crate::ion::{Struct, Sym, Value};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum CdeType {
    #[default]
    Pdoc,
    Ebok,
}

impl CdeType {
    pub fn as_str(self) -> &'static str {
        match self {
            CdeType::Pdoc => "PDOC",
            CdeType::Ebok => "EBOK",
        }
    }
}

/// What to stamp. `None` leaves the book's value; `Some(vec![])` for authors removes them.
#[derive(Clone, Debug, Default)]
pub struct Metadata {
    pub title: Option<String>,
    /// Author names in display form; stored in sort form (`Last, First`).
    pub authors: Option<Vec<String>>,
    pub cde_type: Option<CdeType>,
    pub asin: Option<String>,
    pub language: Option<String>,
    /// Replacement cover image; must be JPEG (ideally JFIF, `FF D8 FF E0`) for the lockscreen.
    pub cover_jpeg: Option<Vec<u8>>,
}

#[derive(Debug, thiserror::Error)]
pub enum MetadataError {
    #[error("book has no metadata fragment")]
    NoMetadata,
    #[error("cover image must be JPEG")]
    CoverNotJpeg,
    #[error("book has no cover resource to replace")]
    NoCoverResource,
}

/// Deterministic, recognizable ASIN: `B0` + four `[A-Z0-9]` from SHA-1(seed) + `DECK`.
pub fn generate_asin(seed: &[u8]) -> String {
    use sha1::{Digest, Sha1};
    const ALPHABET: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789";
    let digest = Sha1::digest(seed);
    let mid: String = digest[..4]
        .iter()
        .map(|b| ALPHABET[usize::from(*b) % ALPHABET.len()] as char)
        .collect();
    format!("B0{mid}DECK")
}

/// (width, height) from a JPEG's SOF marker.
pub fn jpeg_dimensions(data: &[u8]) -> Option<(i64, i64)> {
    if data.len() < 4 || data[0] != 0xFF || data[1] != 0xD8 {
        return None;
    }
    let mut i = 2;
    while i + 9 < data.len() {
        if data[i] != 0xFF {
            i += 1;
            continue;
        }
        let marker = data[i + 1];
        if marker == 0xFF {
            i += 1;
            continue;
        }
        if (0xD0..=0xD9).contains(&marker) || marker == 0x01 {
            i += 2;
            continue;
        }
        let len = usize::from(u16::from_be_bytes([data[i + 2], data[i + 3]]));
        let is_sof = (0xC0..=0xCF).contains(&marker) && !matches!(marker, 0xC4 | 0xC8 | 0xCC);
        if is_sof {
            let height = i64::from(u16::from_be_bytes([data[i + 5], data[i + 6]]));
            let width = i64::from(u16::from_be_bytes([data[i + 7], data[i + 8]]));
            return Some((width, height));
        }
        i += 2 + len;
    }
    None
}

fn set_cover(book: &mut Book, jpeg: &[u8]) -> Result<(), MetadataError> {
    if !jpeg.starts_with(&[0xFF, 0xD8, 0xFF]) {
        return Err(MetadataError::CoverNotJpeg);
    }
    let cover_name = match book.title_metadata("cover_image") {
        Some(Value::String(s)) => s.clone(),
        Some(Value::Symbol(s)) => s.0.clone(),
        _ => return Err(MetadataError::NoCoverResource),
    };
    let (width, height) = jpeg_dimensions(jpeg).ok_or(MetadataError::CoverNotJpeg)?;
    let res_key = Sym::new(&cover_name);
    let location = {
        let res = book
            .fragments
            .get_mut(&Sym::new("$164"), Some(&res_key))
            .ok_or(MetadataError::NoCoverResource)?;
        let s = res
            .value
            .as_struct_mut()
            .ok_or(MetadataError::NoCoverResource)?;
        s.insert("$161", Value::sym("$285"));
        s.insert("$162", Value::string("image/jpg"));
        s.insert("$422", Value::Int(width));
        s.insert("$423", Value::Int(height));
        s.get("$165")
            .and_then(Value::as_str)
            .map(str::to_owned)
            .ok_or(MetadataError::NoCoverResource)?
    };
    let raw = book
        .fragments
        .get_mut(&Sym::new("$417"), Some(&Sym::new(&location)))
        .ok_or(MetadataError::NoCoverResource)?;
    raw.value = Value::Blob(jpeg.to_vec());
    Ok(())
}

/// kfxlib `author_sort_name`: "First Last" -> "Last, First"; names with a comma or a single
/// token are unchanged.
pub fn author_sort(name: &str) -> String {
    let name = name.trim();
    if name.contains(',') {
        return name.to_owned();
    }
    let parts: Vec<&str> = name.split_whitespace().collect();
    if parts.len() < 2 {
        return name.to_owned();
    }
    let (last, first) = parts.split_last().unwrap();
    format!("{last}, {}", first.join(" "))
}

const LANGUAGE_FIXUPS: [(&str, &str); 12] = [
    ("nob", "nb"),
    ("cor", "kw"),
    ("fry", "fy"),
    ("gle", "ga"),
    ("jpn", "ja"),
    ("ltz", "lb"),
    ("glv", "gv"),
    ("por", "pt"),
    ("roh", "rm"),
    ("gla", "gd"),
    ("spa", "es"),
    ("swe", "sv"),
];

/// kfxlib `fix_language_for_kfx`.
pub fn fix_language(language: &str) -> String {
    let short = language.to_lowercase();
    let short = short
        .split(' ')
        .next()
        .unwrap_or("")
        .split('-')
        .next()
        .unwrap_or("");
    let fixed = LANGUAGE_FIXUPS
        .iter()
        .find(|(k, _)| *k == short)
        .map(|(_, v)| (*v).to_owned())
        .unwrap_or_else(|| language.to_owned());
    if fixed.eq_ignore_ascii_case("zh-tw") {
        "zh-hant".to_owned()
    } else {
        fixed
    }
}

fn kv(key: &str, value: Value) -> Value {
    let mut s = Struct::new();
    s.insert("$492", Value::string(key));
    s.insert("$307", value);
    Value::Struct(s)
}

/// Applies the metadata to `$490` (kindle_title_metadata), sorting entries by key like kfxlib.
pub fn apply(book: &mut Book, md: &Metadata) -> Result<(), MetadataError> {
    let authors: Option<Vec<String>> = md
        .authors
        .as_ref()
        .map(|a| a.iter().map(|n| author_sort(n)).collect());
    let mut language = md.language.as_deref().map(fix_language);
    if let Some(lang) = &language
        && let Some(orig) = book.title_metadata("language").and_then(Value::as_str)
        && orig.starts_with(&format!("{lang}-"))
    {
        language = None;
    }
    if let Some(cover) = &md.cover_jpeg {
        set_cover(book, cover)?;
    }
    let Some(frag) = book.fragments.get_single_mut(&Sym::new("$490")) else {
        return Err(MetadataError::NoMetadata);
    };
    let Some(cats) = frag
        .value
        .as_struct_mut()
        .and_then(|s| s.get_mut("$491"))
        .and_then(Value::as_list_mut)
    else {
        return Err(MetadataError::NoMetadata);
    };
    for cat in cats.iter_mut() {
        let Some(cs) = cat.as_struct_mut() else {
            continue;
        };
        if cs.get("$495").and_then(Value::as_str) != Some("kindle_title_metadata") {
            continue;
        }
        let Some(list) = cs.get_mut("$258").and_then(Value::as_list_mut) else {
            continue;
        };
        // (key, seq, value) like kfxlib's new_ksv, then sorted by key then seq.
        let mut ksv: Vec<(String, usize, Value)> = Vec::new();
        for item in list.iter() {
            let Some(s) = item.as_struct() else { continue };
            let key = s
                .get("$492")
                .and_then(Value::as_str)
                .unwrap_or("")
                .to_owned();
            let val = s.get("$307").cloned().unwrap_or(Value::Null);
            let replaced = (key == "author" && authors.is_some())
                || (key == "title" && md.title.is_some())
                || (key == "cde_content_type" && md.cde_type.is_some())
                || ((key == "ASIN" || key == "content_id") && md.asin.is_some())
                || (key == "language" && language.is_some());
            if replaced || key.is_empty() {
                continue;
            }
            ksv.push((key, ksv.len(), val));
        }
        if let Some(a) = &authors {
            for author in a {
                ksv.push(("author".into(), ksv.len(), Value::String(author.clone())));
            }
        }
        if let Some(t) = &md.title {
            ksv.push(("title".into(), ksv.len(), Value::String(t.clone())));
        }
        if let Some(c) = md.cde_type {
            ksv.push((
                "cde_content_type".into(),
                ksv.len(),
                Value::string(c.as_str()),
            ));
        }
        if let Some(asin) = &md.asin {
            ksv.push(("ASIN".into(), ksv.len(), Value::String(asin.clone())));
            ksv.push(("content_id".into(), ksv.len(), Value::String(asin.clone())));
        }
        if let Some(l) = &language {
            ksv.push(("language".into(), ksv.len(), Value::String(l.clone())));
        }
        ksv.sort_by(|a, b| a.0.cmp(&b.0).then(a.1.cmp(&b.1)));
        *list = ksv.into_iter().map(|(k, _, v)| kv(&k, v)).collect();
    }
    // `$258` metadata fragment: only when it carries more than reading orders.
    let Some(md_frag) = book.fragments.get_single_mut(&Sym::new("$258")) else {
        return Ok(());
    };
    let Some(mdx) = md_frag.value.as_struct_mut() else {
        return Ok(());
    };
    if mdx.is_empty() || (mdx.len() == 1 && mdx.contains("$169")) {
        return Ok(());
    }
    if let Some(a) = &authors {
        mdx.insert("$222", Value::String(a.join(" & ")));
    }
    if let Some(t) = &md.title {
        mdx.insert("$153", Value::String(t.clone()));
    }
    if let Some(c) = md.cde_type {
        mdx.insert("$251", Value::string(c.as_str()));
    }
    if let Some(asin) = &md.asin {
        mdx.insert("$224", Value::String(asin.clone()));
    }
    Ok(())
}
