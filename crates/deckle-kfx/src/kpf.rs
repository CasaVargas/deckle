// SPDX-License-Identifier: GPL-3.0-or-later
//! Reads a Kindle Previewer KPF (zip of a KDF SQLite database plus resources) into fragments.
//! Port of kfxlib's `kpf_container.py`.

use std::collections::HashMap;
use std::io::Read;
use std::path::Path;

use crate::fragment::{Fragment, FragmentList, is_root_type};
use crate::ion::{BVM, IonError, Struct, Sym, Value, decode_annotated, decode_single};
use crate::symtab::SymbolTable;

/// Signature of a DRM-protected Ion payload.
pub const DRMION_SIGNATURE: &[u8] = b"\xeaDRMION\xee";
const SQLITE_SIGNATURE: &[u8] = b"SQLite format 3\0";
const FINGERPRINT_SIGNATURE: [u8; 4] = [0xFA, 0x50, 0x0A, 0x5F];
const FINGERPRINT_OFFSET: usize = 1024;
const FINGERPRINT_RECORD_LEN: usize = 1024;
const FINGERPRINT_STRIDE: usize = 1024 * 1024;

#[derive(Debug, thiserror::Error)]
pub enum KpfError {
    #[error("not a zip file: {0}")]
    Zip(#[from] zip::result::ZipError),
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),
    #[error("no book.kdf inside the KPF")]
    NoKdf,
    #[error("KDF is not an SQLite database")]
    NotSqlite,
    #[error("SQLite error: {0}")]
    Sqlite(#[from] rusqlite::Error),
    #[error("book has DRM and cannot be converted")]
    Drm,
    #[error("Ion error in fragment {id}: {source}")]
    Ion { id: String, source: IonError },
    #[error("unexpected KDF schema: {0}")]
    UnexpectedSchema(String),
    #[error("missing resource in KPF: {0}")]
    MissingResource(String),
    #[error("invalid book.kcb: {0}")]
    Kcb(#[from] serde_json::Error),
}

/// A KPF read into memory, before any fix-ups.
#[derive(Debug)]
pub struct Kpf {
    pub fragments: FragmentList,
    pub symtab: SymbolTable,
    /// Parsed `book.kcb` (empty object when absent).
    pub kcb: serde_json::Value,
    /// Previewer's cleaned copy of the source EPUB, when the KCB points at one.
    pub source_epub: Option<Vec<u8>>,
    /// True unless `$490` already carries ASIN/asset_id/cde_content_type/content_id.
    pub is_prepub: bool,
}

/// Removes the 1024-byte fingerprint records Amazon injects into the KDF SQLite file (one at
/// offset 1024, then every MiB). Returns the clean bytes and the number of records removed.
pub fn strip_sqlite_fingerprints(kdf: &[u8]) -> (Vec<u8>, usize) {
    if kdf.len() < FINGERPRINT_OFFSET + FINGERPRINT_RECORD_LEN
        || kdf[FINGERPRINT_OFFSET..FINGERPRINT_OFFSET + 4] != FINGERPRINT_SIGNATURE
    {
        return (kdf.to_vec(), 0);
    }
    let mut out = Vec::with_capacity(kdf.len());
    let mut removed = 0;
    let mut pos = 0;
    let mut next = FINGERPRINT_OFFSET;
    while pos < kdf.len() {
        if pos == next {
            if kdf.len() >= pos + FINGERPRINT_RECORD_LEN
                && kdf[pos..pos + 4] == FINGERPRINT_SIGNATURE
            {
                pos += FINGERPRINT_RECORD_LEN;
                removed += 1;
                // kfxlib advances 1 MiB in the *stripped* stream, i.e. 1 MiB + one record here.
                next += FINGERPRINT_STRIDE + FINGERPRINT_RECORD_LEN;
                continue;
            }
            break; // kfxlib: unexpected fingerprint signature, keep the rest as is
        }
        let end = next.min(kdf.len());
        out.extend_from_slice(&kdf[pos..end]);
        pos = end;
    }
    out.extend_from_slice(&kdf[pos..]);
    (out, removed)
}

struct Row {
    id: String,
    payload_type: String,
    payload: Vec<u8>,
}

/// Reads a `.kpf` file into fragments.
pub fn read_kpf(path: &Path) -> Result<Kpf, KpfError> {
    let file = std::fs::File::open(path)?;
    let mut zip = zip::ZipArchive::new(file)?;
    let mut kdf = None;
    let mut kcb: serde_json::Value = serde_json::Value::Object(Default::default());
    let mut kcb_dir = String::new();
    let mut resources: HashMap<String, Vec<u8>> = HashMap::new();
    let mut files: HashMap<String, Vec<u8>> = HashMap::new();
    for i in 0..zip.len() {
        let mut entry = zip.by_index(i)?;
        if entry.is_dir() {
            continue;
        }
        let name = entry.name()?.into_owned();
        let mut data = Vec::with_capacity(entry.size() as usize);
        entry.read_to_end(&mut data)?;
        if name.ends_with(".kdf") {
            kdf = Some((name.clone(), data.clone()));
        } else if name.ends_with(".kcb") {
            kcb = serde_json::from_slice(&data)?;
            kcb_dir = parent_dir(&name);
        }
        if let Some(rel) = name.strip_prefix("resources/") {
            resources.insert(rel.to_owned(), data.clone());
        }
        files.insert(name, data);
    }
    let (kdf_name, kdf_bytes) = kdf.ok_or(KpfError::NoKdf)?;
    let (clean, _removed) = strip_sqlite_fingerprints(&kdf_bytes);
    if !clean.starts_with(SQLITE_SIGNATURE) {
        return Err(KpfError::NotSqlite);
    }
    let kdf_dir = parent_dir(&kdf_name);

    let tmp = tempfile::NamedTempFile::new()?;
    std::fs::write(tmp.path(), &clean)?;
    let conn = rusqlite::Connection::open_with_flags(
        tmp.path(),
        rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY | rusqlite::OpenFlags::SQLITE_OPEN_NO_MUTEX,
    )?;

    let tables: Vec<String> = conn
        .prepare("SELECT name FROM sqlite_master WHERE type='table'")?
        .query_map([], |r| r.get::<_, String>(0))?
        .collect::<Result<_, _>>()?;
    if !tables.iter().any(|t| t == "fragments") {
        return Err(KpfError::UnexpectedSchema("missing fragments table".into()));
    }
    for t in &tables {
        if t.starts_with("index_") || t == "index_info" {
            return Err(KpfError::UnexpectedSchema(
                "dictionary KPF is not supported".into(),
            ));
        }
        if t == "local_action_fragments" || t == "local_delta_fragments" {
            return Err(KpfError::UnexpectedSchema(
                "Scribe notebook is not supported".into(),
            ));
        }
    }

    let mut element_type: HashMap<String, String> = HashMap::new();
    if tables.iter().any(|t| t == "fragment_properties") {
        let mut stmt = conn.prepare("SELECT id, key, value FROM fragment_properties")?;
        for row in stmt.query_map([], |r| {
            Ok((
                r.get::<_, String>(0)?,
                r.get::<_, String>(1)?,
                r.get::<_, String>(2)?,
            ))
        })? {
            let (id, key, value) = row?;
            if key == "element_type" {
                element_type.insert(id, value);
            }
        }
    }

    let mut eid_symbol: HashMap<i64, Sym> = HashMap::new();
    let mut symtab = SymbolTable::yj_default();
    let mut fragments = FragmentList::new();

    if tables.iter().any(|t| t == "kfxid_translation") {
        let mut stmt = conn.prepare("SELECT eid, kfxid FROM kfxid_translation")?;
        for row in stmt.query_map([], |r| Ok((r.get::<_, i64>(0)?, r.get::<_, String>(1)?)))? {
            let (eid, kfxid) = row?;
            eid_symbol.insert(eid, symtab.create_local(&kfxid));
        }
    }

    // Symbol table rows first (kfxlib does three passes), then everything else.
    let mut rows: Vec<Row> = Vec::new();
    let mut stmt = conn.prepare("SELECT id, payload_type, payload_value FROM fragments")?;
    for row in stmt.query_map([], |r| {
        Ok(Row {
            id: r.get::<_, String>(0)?,
            payload_type: r.get::<_, String>(1)?,
            payload: r.get::<_, Vec<u8>>(2)?,
        })
    })? {
        rows.push(row?);
    }
    rows.sort_by_key(|r| match r.id.as_str() {
        "$ion_symbol_table" => 0,
        "max_id" => 1,
        _ => 2,
    });

    let mut have_symtab = false;
    for row in &rows {
        process_row(
            row,
            &element_type,
            &eid_symbol,
            &mut symtab,
            &mut fragments,
            &mut have_symtab,
            &resources,
            &kdf_dir,
        )?;
    }

    if tables.iter().any(|t| t == "capabilities") {
        let mut stmt = conn.prepare("SELECT key, version FROM capabilities")?;
        let caps: Vec<Value> = stmt
            .query_map([], |r| Ok((r.get::<_, String>(0)?, r.get::<_, i64>(1)?)))?
            .map(|r| {
                r.map(|(key, version)| {
                    let mut s = Struct::new();
                    s.insert("$492", Value::String(key));
                    s.insert("version", Value::Int(version));
                    Value::Struct(s)
                })
            })
            .collect::<Result<_, _>>()?;
        if !caps.is_empty() {
            fragments.push(Fragment::root("$593", Value::List(caps)));
        }
    } else {
        return Err(KpfError::UnexpectedSchema(
            "missing capabilities table".into(),
        ));
    }

    let is_prepub = !has_publication_metadata(&fragments);

    let mut c = Struct::new();
    c.insert("$587", Value::string(""));
    c.insert("$588", Value::string(""));
    c.insert("$161", Value::string("KPF"));
    fragments.push(Fragment::root("$270", Value::Struct(c)));

    let source_epub = kcb
        .get("metadata")
        .and_then(|m| m.get("source_path"))
        .and_then(|p| p.as_str())
        .filter(|p| p.ends_with(".epub") || p.ends_with(".zip"))
        .and_then(|p| files.get(&join_dir(&kcb_dir, p)).cloned());

    Ok(Kpf {
        fragments,
        symtab,
        kcb,
        source_epub,
        is_prepub,
    })
}

fn parent_dir(name: &str) -> String {
    match name.rfind('/') {
        Some(i) => name[..i].to_owned(),
        None => String::new(),
    }
}

fn join_dir(dir: &str, rel: &str) -> String {
    if dir.is_empty() {
        rel.to_owned()
    } else {
        format!("{dir}/{rel}")
    }
}

#[allow(clippy::too_many_arguments)]
fn process_row(
    row: &Row,
    element_type: &HashMap<String, String>,
    eid_symbol: &HashMap<i64, Sym>,
    symtab: &mut SymbolTable,
    fragments: &mut FragmentList,
    have_symtab: &mut bool,
    resources: &HashMap<String, Vec<u8>>,
    kdf_dir: &str,
) -> Result<(), KpfError> {
    let id = &row.id;
    let ion_err = |e: IonError| KpfError::Ion {
        id: id.clone(),
        source: e,
    };
    match row.payload_type.as_str() {
        "blob" => {
            let data = &row.payload;
            if data.starts_with(DRMION_SIGNATURE) {
                return Err(KpfError::Drm);
            }
            if id == "$ion_symbol_table" {
                if !*have_symtab {
                    let (_, table) = decode_annotated(data, symtab, Some("$ion_symbol_table"))
                        .map_err(ion_err)?;
                    *symtab = SymbolTable::from_kdf_table(&table).map_err(ion_err)?;
                    fragments.push(Fragment::root("$ion_symbol_table", table));
                    *have_symtab = true;
                }
            } else if id == "max_id" {
                if !*have_symtab {
                    let max_id = decode_single(data, symtab).map_err(ion_err)?;
                    let max_id = max_id.as_int().unwrap_or(0) as u32;
                    *symtab = SymbolTable::with_yj_max_id(
                        max_id.saturating_sub(crate::symtab::SYSTEM_SYMBOL_COUNT),
                    );
                    fragments.push(Fragment::root(
                        "$ion_symbol_table",
                        symtab.import_fragment(),
                    ));
                    *have_symtab = true;
                }
            } else if id == "max_eid_in_sections" {
                // dictionary-only; ignored for books
            } else if !data.starts_with(&BVM) {
                let fid = symtab.create_local(id);
                fragments.push(Fragment {
                    ftype: Sym::new("$417"),
                    fid: Some(fid),
                    value: Value::Blob(data.clone()),
                });
            } else if data.len() == BVM.len() {
                if element_type.get(id).map(String::as_str) == Some("auxiliary_data") {
                    let fid = symtab.create_local(id);
                    let mut s = Struct::new();
                    s.insert("$598", Value::Symbol(fid.clone()));
                    s.insert("$258", Value::List(Vec::new()));
                    fragments.push(Fragment {
                        ftype: Sym::new("$597"),
                        fid: Some(fid),
                        value: Value::Struct(s),
                    });
                }
                // empty book_navigation or other empty fragments are ignored
            } else {
                let (annots, value) = decode_annotated(data, symtab, None).map_err(ion_err)?;
                let ftype = annots[0].clone();
                let fid = if is_root_type(ftype.as_str()) {
                    None
                } else {
                    Some(symtab.create_local(id))
                };
                let value = deref_kfx_ids(value, symtab, eid_symbol);
                fragments.push(Fragment { ftype, fid, value });
            }
        }
        "path" => {
            let rel = String::from_utf8_lossy(&row.payload).to_string();
            let key = rel.strip_prefix("resources/").unwrap_or(&rel).to_owned();
            let data = resources
                .get(&key)
                .or_else(|| resources.get(&join_dir(kdf_dir, &rel)))
                .ok_or_else(|| KpfError::MissingResource(rel.clone()))?;
            let fid = symtab.create_local(id);
            fragments.push(Fragment {
                ftype: Sym::new("$417"),
                fid: Some(fid),
                value: Value::Blob(data.clone()),
            });
        }
        other => {
            return Err(KpfError::UnexpectedSchema(format!(
                "payload_type {other} for {id}"
            )));
        }
    }
    Ok(())
}

/// Replaces `$598::"name"` annotations with local symbols and `$598::eid` with the translated
/// symbol (kfxlib `deref_kfx_ids`).
fn deref_kfx_ids(value: Value, symtab: &mut SymbolTable, eid_symbol: &HashMap<i64, Sym>) -> Value {
    match value {
        Value::Annotated { annots, value } => {
            if annots.len() == 1 && annots[0].as_str() == "$598" {
                return match *value {
                    Value::String(s) => Value::Symbol(symtab.create_local(&s)),
                    Value::Int(eid) => match eid_symbol.get(&eid) {
                        Some(sym) => Value::Symbol(sym.clone()),
                        None => Value::Int(eid),
                    },
                    other => other,
                };
            }
            Value::Annotated {
                annots,
                value: Box::new(deref_kfx_ids(*value, symtab, eid_symbol)),
            }
        }
        Value::List(items) => Value::List(
            items
                .into_iter()
                .map(|v| deref_kfx_ids(v, symtab, eid_symbol))
                .collect(),
        ),
        Value::SExp(items) => Value::SExp(
            items
                .into_iter()
                .map(|v| deref_kfx_ids(v, symtab, eid_symbol))
                .collect(),
        ),
        Value::Struct(s) => Value::Struct(Struct(
            s.0.into_iter()
                .map(|(k, v)| (k, deref_kfx_ids(v, symtab, eid_symbol)))
                .collect(),
        )),
        other => other,
    }
}

fn has_publication_metadata(fragments: &FragmentList) -> bool {
    let Some(md) = fragments.get_single(&Sym::new("$490")) else {
        return false;
    };
    let Some(cats) = md
        .value
        .as_struct()
        .and_then(|s| s.get("$491"))
        .and_then(Value::as_list)
    else {
        return false;
    };
    for cat in cats {
        let Some(cat) = cat.as_struct() else { continue };
        if cat.get("$495").and_then(Value::as_str) != Some("kindle_title_metadata") {
            continue;
        }
        if let Some(kvs) = cat.get("$258").and_then(Value::as_list) {
            for kv in kvs {
                let key = kv
                    .as_struct()
                    .and_then(|s| s.get("$492"))
                    .and_then(Value::as_str)
                    .unwrap_or("");
                if ["ASIN", "asset_id", "cde_content_type", "content_id"].contains(&key) {
                    return true;
                }
            }
        }
        break;
    }
    false
}
