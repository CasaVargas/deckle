// SPDX-License-Identifier: GPL-3.0-or-later
//! Monolithic KFX container ("CONT") reader and writer. Port of kfxlib's `kfx_container.py`;
//! the writer reproduces kfxlib's output byte for byte.

use sha1::{Digest, Sha1};

use crate::fragment::{CONTAINER_FRAGMENT_TYPES, Fragment, FragmentList, is_raw_type};
use crate::ion::{IonError, Struct, Sym, Value, decode_annotated, decode_single, encode_single};
use crate::symtab::{SYSTEM_SYMBOL_COUNT, SymbolTable};

const SIGNATURE: &[u8; 4] = b"CONT";
const VERSION: u16 = 2;
const MIN_LENGTH: usize = 18;
const ENTITY_SIGNATURE: &[u8; 4] = b"ENTY";
const ENTITY_VERSION: u16 = 1;
const DEFAULT_CHUNK_SIZE: i64 = 4096;
const DEFAULT_COMPRESSION_TYPE: i64 = 0;
const DEFAULT_DRM_SCHEME: i64 = 0;
/// kfxlib warns above this size; some devices lock up.
pub const MAX_KFX_CONTAINER_SIZE: usize = 16 * 1024 * 1024;
/// Entity id used for fragments that have no fid.
const SINGLETON_ID: &str = "$348";

#[derive(Debug, thiserror::Error)]
pub enum ContainerError {
    #[error("not a KFX container (bad signature)")]
    BadSignature,
    #[error("KFX container is truncated")]
    Truncated,
    #[error("unsupported KFX container version {0}")]
    Version(u16),
    #[error("Ion error: {0}")]
    Ion(#[from] IonError),
    #[error("container needs exactly one $270, one $ion_symbol_table and one $419 (found {0})")]
    MissingContainerFragments(String),
    #[error("fragment {0} has no symbol id")]
    UndefinedSymbol(String),
    #[error("raw fragment {0} must hold a blob")]
    RawNotBlob(String),
    #[error("malformed container: {0}")]
    Malformed(String),
}

/// A decoded monolithic KFX: its symbol table and fragments, container-level ones first.
#[derive(Debug, Clone)]
pub struct KfxDoc {
    pub symtab: SymbolTable,
    pub fragments: FragmentList,
    pub container_id: String,
    /// `$587`, e.g. `kfxlib-20260827` or `deckle-0.1.0`.
    pub creator: String,
    /// `$588`.
    pub package_version: String,
}

#[derive(Debug, Clone)]
pub struct WriteOptions {
    pub container_id: String,
    pub creator: String,
    pub package_version: String,
}

impl WriteOptions {
    /// Reuse the identifiers a document was read with.
    pub fn from_doc(doc: &KfxDoc) -> Self {
        WriteOptions {
            container_id: doc.container_id.clone(),
            creator: doc.creator.clone(),
            package_version: doc.package_version.clone(),
        }
    }
}

fn u16le(b: &[u8]) -> u16 {
    u16::from_le_bytes([b[0], b[1]])
}
fn u32le(b: &[u8]) -> u32 {
    u32::from_le_bytes([b[0], b[1], b[2], b[3]])
}
fn u64le(b: &[u8]) -> u64 {
    u64::from_le_bytes(b[..8].try_into().unwrap())
}

fn slice(data: &[u8], off: usize, len: usize) -> Result<&[u8], ContainerError> {
    data.get(off..off.checked_add(len).ok_or(ContainerError::Truncated)?)
        .ok_or(ContainerError::Truncated)
}

/// Parses a monolithic KFX file.
pub fn read_kfx(data: &[u8]) -> Result<KfxDoc, ContainerError> {
    if data.len() < MIN_LENGTH {
        return Err(if data.starts_with(SIGNATURE) {
            ContainerError::Truncated
        } else {
            ContainerError::BadSignature
        });
    }
    if &data[..4] != SIGNATURE {
        return Err(ContainerError::BadSignature);
    }
    let version = u16le(&data[4..6]);
    if !(1..=2).contains(&version) {
        return Err(ContainerError::Version(version));
    }
    let header_len = u32le(&data[6..10]) as usize;
    let cinfo_off = u32le(&data[10..14]) as usize;
    let cinfo_len = u32le(&data[14..18]) as usize;

    let base = SymbolTable::yj_default();
    let cinfo = decode_single(slice(data, cinfo_off, cinfo_len)?, &base)?;
    let mut cinfo = cinfo
        .as_struct()
        .cloned()
        .ok_or_else(|| ContainerError::Malformed("container info is not a struct".into()))?;
    let take_int = |c: &mut Struct, k: &str| c.remove(k).and_then(|v| v.as_int());
    let container_id = cinfo
        .remove("$409")
        .and_then(|v| v.as_str().map(str::to_owned))
        .unwrap_or_default();
    let compression = take_int(&mut cinfo, "$410").unwrap_or(DEFAULT_COMPRESSION_TYPE);
    let drm = take_int(&mut cinfo, "$411").unwrap_or(DEFAULT_DRM_SCHEME);
    let sym_off = take_int(&mut cinfo, "$415");
    let sym_len = take_int(&mut cinfo, "$416").unwrap_or(0);
    let chunk = take_int(&mut cinfo, "$412").unwrap_or(DEFAULT_CHUNK_SIZE);
    let fc_off = take_int(&mut cinfo, "$594");
    let fc_len = take_int(&mut cinfo, "$595").unwrap_or(0);
    let tbl_off = take_int(&mut cinfo, "$413");
    let tbl_len = take_int(&mut cinfo, "$414").unwrap_or(0);

    let mut fragments = FragmentList::new();
    let mut symtab = base;
    if sym_len > 0 {
        let off = sym_off.ok_or(ContainerError::Truncated)? as usize;
        let (_, table) = decode_annotated(
            slice(data, off, sym_len as usize)?,
            &symtab,
            Some("$ion_symbol_table"),
        )?;
        symtab = SymbolTable::from_container_table(&table)?;
        // store the raw (KDF-style) form, as kfxlib does after subtracting the system count
        fragments.push(Fragment::root(
            "$ion_symbol_table",
            symtab.import_fragment(),
        ));
    }

    // kfxgen_info trailer: between the container info and the end of the header
    let trailer = String::from_utf8_lossy(slice(
        data,
        cinfo_off + cinfo_len,
        header_len.saturating_sub(cinfo_off + cinfo_len),
    )?)
    .replace('\x1b', "");
    let (mut creator, mut package_version) = (String::new(), String::new());
    for (key, value) in parse_kfxgen_info(&trailer) {
        match key.as_str() {
            "appVersion" | "kfxgen_application_version" => creator = value,
            "buildVersion" | "kfxgen_package_version" => package_version = value,
            _ => {}
        }
    }

    let mut entity_refs: Vec<Value> = Vec::new();
    let mut entities: Vec<Fragment> = Vec::new();
    if tbl_len > 0 {
        let table = slice(
            data,
            tbl_off.ok_or(ContainerError::Truncated)? as usize,
            tbl_len as usize,
        )?;
        for row in table.as_chunks::<24>().0 {
            let id_idnum = u32le(&row[0..4]);
            let type_idnum = u32le(&row[4..8]);
            let ent_off = header_len + u64le(&row[8..16]) as usize;
            let ent_len = u64le(&row[16..24]) as usize;
            let ent = slice(data, ent_off, ent_len)?;
            entities.push(read_entity(ent, id_idnum, type_idnum, &symtab)?);
            entity_refs.push(Value::List(vec![
                Value::Int(i64::from(type_idnum)),
                Value::Int(i64::from(id_idnum)),
            ]));
        }
    }

    let mut c = Struct::new();
    c.insert("$409", Value::String(container_id.clone()));
    c.insert("$412", Value::Int(chunk));
    c.insert("$410", Value::Int(compression));
    c.insert("$411", Value::Int(drm));
    c.insert("$587", Value::String(creator.clone()));
    c.insert("$588", Value::String(package_version.clone()));
    c.insert("$161", Value::string("KFX main"));
    c.insert("version", Value::Int(i64::from(version)));
    c.insert("$181", Value::List(entity_refs));
    fragments.push(Fragment::root("$270", Value::Struct(c)));

    if fc_len > 0 {
        let off = fc_off.ok_or(ContainerError::Truncated)? as usize;
        let (_, caps) =
            decode_annotated(slice(data, off, fc_len as usize)?, &symtab, Some("$593"))?;
        fragments.push(Fragment::root("$593", caps));
    }
    for e in entities {
        fragments.push(e);
    }
    Ok(KfxDoc {
        symtab,
        fragments,
        container_id,
        creator,
        package_version,
    })
}

fn read_entity(
    ent: &[u8],
    id_idnum: u32,
    type_idnum: u32,
    symtab: &SymbolTable,
) -> Result<Fragment, ContainerError> {
    if ent.len() < 10 || &ent[..4] != ENTITY_SIGNATURE {
        return Err(ContainerError::Malformed("bad entity signature".into()));
    }
    let header_len = u32le(&ent[6..10]) as usize;
    if header_len < 10 || header_len > ent.len() {
        return Err(ContainerError::Malformed("bad entity header length".into()));
    }
    let payload = &ent[header_len..];
    let ftype = symtab
        .symbol_of(type_idnum)
        .map(Sym::new)
        .unwrap_or_else(|| Sym(format!("${type_idnum}")));
    let fid_text = symtab
        .symbol_of(id_idnum)
        .map(str::to_owned)
        .unwrap_or_else(|| format!("${id_idnum}"));
    let value = if is_raw_type(ftype.as_str()) {
        Value::Blob(payload.to_vec())
    } else {
        decode_single(payload, symtab)?
    };
    // An annotated singleton unwraps to its value (kfxlib KfxContainerEntity.deserialize).
    let (fid, value) = match value {
        Value::Annotated { annots, value }
            if fid_text == SINGLETON_ID && annots.len() == 1 && annots[0] == ftype =>
        {
            (None, *value)
        }
        v => (
            if fid_text == SINGLETON_ID {
                None
            } else {
                Some(Sym::new(&fid_text))
            },
            v,
        ),
    };
    Ok(Fragment { ftype, fid, value })
}

/// Parses kfxlib's `[{key:"k",value:"v"},...]` trailer (keys are unquoted).
fn parse_kfxgen_info(text: &str) -> Vec<(String, String)> {
    let json = text
        .replace("key :", "\"key\":")
        .replace("key:", "\"key\":")
        .replace("value:", "\"value\":");
    let parsed: serde_json::Value = match serde_json::from_str(json.trim()) {
        Ok(v) => v,
        Err(_) => return Vec::new(),
    };
    parsed
        .as_array()
        .map(|items| {
            items
                .iter()
                .filter_map(|i| {
                    Some((
                        i.get("key")?.as_str()?.to_owned(),
                        i.get("value")?
                            .as_str()
                            .map(str::to_owned)
                            .unwrap_or_default(),
                    ))
                })
                .collect()
        })
        .unwrap_or_default()
}

fn kfxgen_info_trailer(
    package_version: &str,
    creator: &str,
    sha1_hex: &str,
    container_id: &str,
) -> Vec<u8> {
    let items = serde_json::json!([
        {"key": "kfxgen_package_version", "value": package_version},
        {"key": "kfxgen_application_version", "value": creator},
        {"key": "kfxgen_payload_sha1", "value": sha1_hex},
        {"key": "kfxgen_acr", "value": container_id},
    ]);
    // serde_json's compact form matches Python's json.dumps(separators=(",", ":")) for ASCII.
    let compact = serde_json::to_string(&items).unwrap_or_default();
    let compact = ensure_ascii(&compact);
    compact
        .replace("\"key\":", "key:")
        .replace("\"value\":", "value:")
        .into_bytes()
}

/// Python's json.dumps escapes non-ASCII as \uXXXX; mirror that so the trailer stays ASCII.
fn ensure_ascii(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for ch in s.chars() {
        if ch.is_ascii() {
            out.push(ch);
        } else {
            let mut buf = [0u16; 2];
            for unit in ch.encode_utf16(&mut buf) {
                out.push_str(&format!("\\u{unit:04x}"));
            }
        }
    }
    out
}

/// Serializes a document as a monolithic KFX, byte-identical to kfxlib for the same input.
pub fn write_kfx(doc: &KfxDoc, opts: &WriteOptions) -> Result<Vec<u8>, ContainerError> {
    let symtab = &doc.symtab;
    let mut n270 = 0;
    let mut n593 = 0;
    let mut nsym = 0;
    let mut n419 = 0;
    let mut doc_symbols: Option<Value> = None;
    let mut format_caps: Option<&Fragment> = None;
    for f in doc.fragments.iter() {
        match f.ftype.as_str() {
            "$270" => n270 += 1,
            "$593" => {
                n593 += 1;
                format_caps = Some(f);
            }
            "$ion_symbol_table" => {
                nsym += 1;
                // inflate the import max_id by the system symbol count, like kfxlib
                let mut v = f.value.clone();
                if let Some(imports) = v
                    .as_struct_mut()
                    .and_then(|s| s.get_mut("imports"))
                    .and_then(Value::as_list_mut)
                {
                    for imp in imports {
                        if let Some(Value::Int(i)) =
                            imp.as_struct_mut().and_then(|s| s.get_mut("max_id"))
                        {
                            *i += i64::from(SYSTEM_SYMBOL_COUNT);
                        }
                    }
                }
                doc_symbols = Some(v);
            }
            "$419" => n419 += 1,
            _ => {}
        }
    }
    if n270 != 1 || n593 > 1 || nsym != 1 || n419 != 1 {
        return Err(ContainerError::MissingContainerFragments(format!(
            "container={n270} format_capabilities={n593} ion_symbol_table={nsym} container_entity_map={n419}"
        )));
    }

    // entities
    let mut entity_table = Vec::new();
    let mut entity_data = Vec::new();
    for f in doc.fragments.iter() {
        let is_container_level = CONTAINER_FRAGMENT_TYPES.contains(&f.ftype.as_str());
        if is_container_level && f.ftype.as_str() != "$419" {
            continue;
        }
        let id_sym = match &f.fid {
            Some(fid) => fid.clone(),
            None => Sym::new(SINGLETON_ID),
        };
        let id_idnum = symtab
            .id_of(&id_sym)
            .ok_or_else(|| ContainerError::UndefinedSymbol(id_sym.0.clone()))?;
        let type_idnum = symtab
            .id_of(&f.ftype)
            .ok_or_else(|| ContainerError::UndefinedSymbol(f.ftype.0.clone()))?;
        let ent = write_entity(f, symtab)?;
        entity_table.extend_from_slice(&id_idnum.to_le_bytes());
        entity_table.extend_from_slice(&type_idnum.to_le_bytes());
        entity_table.extend_from_slice(&(entity_data.len() as u64).to_le_bytes());
        entity_table.extend_from_slice(&(ent.len() as u64).to_le_bytes());
        entity_data.extend_from_slice(&ent);
    }

    let mut out = Vec::new();
    out.extend_from_slice(SIGNATURE);
    out.extend_from_slice(&VERSION.to_le_bytes());
    let header_len_pos = out.len();
    out.extend_from_slice(&0u32.to_le_bytes());
    let cinfo_off_pos = out.len();
    out.extend_from_slice(&0u32.to_le_bytes());
    let cinfo_len_pos = out.len();
    out.extend_from_slice(&0u32.to_le_bytes());

    let mut cinfo = Struct::new();
    cinfo.insert("$409", Value::String(opts.container_id.clone()));
    cinfo.insert("$410", Value::Int(DEFAULT_COMPRESSION_TYPE));
    cinfo.insert("$411", Value::Int(DEFAULT_DRM_SCHEME));
    cinfo.insert("$413", Value::Int(out.len() as i64));
    cinfo.insert("$414", Value::Int(entity_table.len() as i64));
    out.extend_from_slice(&entity_table);

    let doc_symbol_data = match &doc_symbols {
        Some(v) => encode_single(
            &Value::Annotated {
                annots: vec![Sym::new("$ion_symbol_table")],
                value: Box::new(v.clone()),
            },
            symtab,
        )?,
        None => Vec::new(),
    };
    cinfo.insert("$415", Value::Int(out.len() as i64));
    cinfo.insert("$416", Value::Int(doc_symbol_data.len() as i64));
    out.extend_from_slice(&doc_symbol_data);
    cinfo.insert("$412", Value::Int(DEFAULT_CHUNK_SIZE));

    let fc_data = match format_caps {
        Some(f) => encode_single(
            &Value::Annotated {
                annots: vec![Sym::new("$593")],
                value: Box::new(f.value.clone()),
            },
            symtab,
        )?,
        None => Vec::new(),
    };
    if symtab.local_min_id() > 595 {
        cinfo.insert("$594", Value::Int(out.len() as i64));
        cinfo.insert("$595", Value::Int(fc_data.len() as i64));
        out.extend_from_slice(&fc_data);
    }

    let cinfo_data = encode_single(&Value::Struct(cinfo), symtab)?;
    out[cinfo_len_pos..cinfo_len_pos + 4].copy_from_slice(&(cinfo_data.len() as u32).to_le_bytes());
    let cinfo_off = out.len() as u32;
    out[cinfo_off_pos..cinfo_off_pos + 4].copy_from_slice(&cinfo_off.to_le_bytes());
    out.extend_from_slice(&cinfo_data);

    let sha = Sha1::digest(&entity_data);
    let sha_hex: String = sha.iter().map(|b| format!("{b:02x}")).collect();
    out.extend_from_slice(&kfxgen_info_trailer(
        &opts.package_version,
        &opts.creator,
        &sha_hex,
        &opts.container_id,
    ));
    let header_len = out.len() as u32;
    out[header_len_pos..header_len_pos + 4].copy_from_slice(&header_len.to_le_bytes());
    out.extend_from_slice(&entity_data);
    Ok(out)
}

fn write_entity(f: &Fragment, symtab: &SymbolTable) -> Result<Vec<u8>, ContainerError> {
    let mut ent = Vec::new();
    ent.extend_from_slice(ENTITY_SIGNATURE);
    ent.extend_from_slice(&ENTITY_VERSION.to_le_bytes());
    let header_len_pos = ent.len();
    ent.extend_from_slice(&0u32.to_le_bytes());
    let mut info = Struct::new();
    info.insert("$410", Value::Int(DEFAULT_COMPRESSION_TYPE));
    info.insert("$411", Value::Int(DEFAULT_DRM_SCHEME));
    ent.extend_from_slice(&encode_single(&Value::Struct(info), symtab)?);
    let header_len = ent.len() as u32;
    ent[header_len_pos..header_len_pos + 4].copy_from_slice(&header_len.to_le_bytes());
    if is_raw_type(f.ftype.as_str()) {
        match &f.value {
            Value::Blob(b) => ent.extend_from_slice(b),
            _ => return Err(ContainerError::RawNotBlob(f.ftype.0.clone())),
        }
    } else {
        ent.extend_from_slice(&encode_single(&f.value, symtab)?);
    }
    Ok(ent)
}
