// SPDX-License-Identifier: GPL-3.0-or-later
//! Reads a Kindle Previewer KPF (zip of a KDF SQLite database plus resources) into fragments.
//! Port of kfxlib's `kpf_container.py`.

use std::collections::HashMap;
use std::path::Path;

use deckle_limits::{Budget, MAX_BOOK_BYTES, MAX_ENTRY_BYTES, TooLarge};

use crate::fragment::{Fragment, FragmentList, is_root_type};
use crate::ion::{
    BVM, IonError, Struct, Sym, Value, ValueBudget, decode_annotated_in, decode_single_in,
};
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
    #[error("this book unpacks to {0}")]
    TooLarge(deckle_limits::TooLarge),
    #[error("a file in this book unpacks to {0}")]
    EntryTooLarge(deckle_limits::TooLarge),
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
    /// The KPF's unpacked-size budget after reading it: `source_epub` is read through what is
    /// left, not a new one.
    pub budget: Budget,
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
    read_kpf_with(path, MAX_ENTRY_BYTES, MAX_BOOK_BYTES)
}

fn read_kpf_with(path: &Path, entry_limit: u64, book_limit: u64) -> Result<Kpf, KpfError> {
    let too_large = |e: TooLarge| {
        if e.limit == entry_limit {
            KpfError::EntryTooLarge(e)
        } else {
            KpfError::TooLarge(e)
        }
    };
    let file = std::fs::File::open(path)?;
    let mut zip = zip::ZipArchive::new(file)?;
    // `zip` stops each entry at its declared size, so this bounds the reads below.
    let mut declared = Budget::new(book_limit);
    for i in 0..zip.len() {
        let size = zip.by_index_data(i).map(|e| e.size()).unwrap_or(0);
        if size > entry_limit {
            return Err(too_large(TooLarge { limit: entry_limit }));
        }
        declared.add(size).map_err(too_large)?;
    }
    let mut budget = Budget::new(book_limit);
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
        let data = budget
            .read(&mut entry, entry_limit)
            .map_err(|e| match TooLarge::in_io(&e) {
                Some(t) => too_large(t),
                None => KpfError::Io(e),
            })?;
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
    // One budget of decoded values for the whole book.
    let values = ValueBudget::default();
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
            &values,
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
        budget,
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
    values: &ValueBudget,
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
                    let (_, table) =
                        decode_annotated_in(data, symtab, Some("$ion_symbol_table"), values)
                            .map_err(ion_err)?;
                    *symtab = SymbolTable::from_kdf_table(&table).map_err(ion_err)?;
                    fragments.push(Fragment::root("$ion_symbol_table", table));
                    *have_symtab = true;
                }
            } else if id == "max_id" {
                if !*have_symtab {
                    let max_id = decode_single_in(data, symtab, values).map_err(ion_err)?;
                    let max_id = crate::symtab::checked_max_id(max_id.as_int().unwrap_or(0))
                        .map_err(ion_err)?;
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
                let (annots, value) =
                    decode_annotated_in(data, symtab, None, values).map_err(ion_err)?;
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fixtures::stored_zip;

    fn kpf(entries: &[(&str, &[u8], u64)]) -> tempfile::NamedTempFile {
        let f = tempfile::NamedTempFile::new().unwrap();
        std::fs::write(f.path(), stored_zip(entries)).unwrap();
        f
    }

    #[test]
    fn an_entry_declaring_2_pow_60_bytes_is_refused() {
        let f = kpf(&[("resources/book.kdf", b"tiny", 1 << 60)]);
        let e = read_kpf(f.path()).unwrap_err();
        assert_eq!(
            e.to_string(),
            "a file in this book unpacks to more than 256 MB"
        );
    }

    #[test]
    fn reading_stops_at_the_entry_and_book_limits() {
        let k = vec![0u8; 600];
        let f = kpf(&[("a", &k, 600), ("b", &k, 600)]);
        let e = read_kpf_with(f.path(), 500, 10_000).unwrap_err();
        assert!(
            matches!(e, KpfError::EntryTooLarge(t) if t.limit == 500),
            "{e}"
        );
        let e = read_kpf_with(f.path(), 1100, 1000).unwrap_err();
        assert!(matches!(e, KpfError::TooLarge(t) if t.limit == 1000), "{e}");
        // Within both, it fails later, for want of a KDF.
        assert!(matches!(
            read_kpf_with(f.path(), 1000, 10_000),
            Err(KpfError::NoKdf)
        ));
    }

    #[test]
    fn an_entry_longer_than_it_declares_is_not_read_whole() {
        let k = vec![0u8; 5000];
        let f = kpf(&[("a", &k, 10)]);
        assert!(matches!(
            read_kpf_with(f.path(), 1000, 10_000),
            Err(KpfError::Io(_))
        ));
    }

    /// A KDF row holding `value` under the annotation `ftype`, run through one book's budget.
    fn row_with(ftype: &str, value: Value, values: &ValueBudget) -> Result<(), KpfError> {
        let mut symtab = SymbolTable::yj_default();
        let annotated = Value::Annotated {
            annots: vec![Sym::new(ftype)],
            value: Box::new(value),
        };
        let row = Row {
            id: "content_1".into(),
            payload_type: "blob".into(),
            payload: crate::ion::encode_single(&annotated, &symtab).unwrap(),
        };
        let (mut fragments, mut have) = (FragmentList::new(), false);
        process_row(
            &row,
            &HashMap::new(),
            &HashMap::new(),
            &mut symtab,
            &mut fragments,
            &mut have,
            &HashMap::new(),
            "",
            values,
        )
    }

    fn too_many(r: Result<(), KpfError>) -> bool {
        matches!(
            r,
            Err(KpfError::Ion {
                source: IonError::TooManyValues { .. },
                ..
            })
        )
    }

    #[test]
    fn a_list_of_many_small_values_is_refused_at_the_value_limit() {
        // A storyline of 200,000 one-byte ints: 200 KB of Ion, a value each. Decoding stops at
        // the limit, before the rest is made.
        let ints = Value::List((0..200_000).map(|_| Value::Int(0)).collect());
        let values = ValueBudget::new(50_000);
        assert!(too_many(row_with("$259", ints, &values)));
        assert_eq!(values.used(), 50_000);
        // The same list inside the limit decodes.
        let ints = Value::List((0..20_000).map(|_| Value::Int(0)).collect());
        row_with("$259", ints, &ValueBudget::new(50_000)).unwrap();
    }

    #[test]
    fn empty_position_chunks_in_bulk_are_refused_by_the_same_budget() {
        // `$609` entries of zero length pass the position bound; their number is held by the
        // value budget, shared by every row of the book.
        let chunk = || Value::List(vec![Value::Int(1), Value::Int(1)]);
        let spim = |n: usize| {
            let mut s = Struct::new();
            s.insert("$174", Value::String("s1".into()));
            s.insert("$181", Value::List((0..n).map(|_| chunk()).collect()));
            Value::Struct(s)
        };
        let values = ValueBudget::new(10_000);
        row_with("$609", spim(2_000), &values).unwrap();
        assert!(values.used() > 6_000);
        assert!(too_many(row_with("$609", spim(2_000), &values)));
    }

    #[test]
    fn a_max_id_row_past_the_limit_is_refused() {
        // The KDF's own `max_id` row, the path without a symbol table fragment.
        let run = |max_id: i64| {
            let mut symtab = SymbolTable::yj_default();
            let row = Row {
                id: "max_id".into(),
                payload_type: "blob".into(),
                payload: crate::ion::encode_single(&Value::Int(max_id), &symtab).unwrap(),
            };
            let (mut fragments, mut have) = (FragmentList::new(), false);
            let r = process_row(
                &row,
                &HashMap::new(),
                &HashMap::new(),
                &mut symtab,
                &mut fragments,
                &mut have,
                &HashMap::new(),
                "",
                &ValueBudget::default(),
            );
            (r, symtab)
        };
        let start = std::time::Instant::now();
        for bad in [50_000_000, i64::from(u32::MAX), -1] {
            assert!(matches!(run(bad).0, Err(KpfError::Ion { .. })), "{bad}");
        }
        assert!(start.elapsed() < std::time::Duration::from_secs(1));
        let (r, symtab) = run(900);
        r.unwrap();
        assert!(symtab.len() > 800);
    }

    #[test]
    fn the_budget_left_over_comes_back_with_the_book() {
        let path = crate::fixtures::path("deckle-mini.kpf");
        let k = read_kpf(&path).unwrap();
        let unpacked: u64 = {
            let mut z = zip::ZipArchive::new(std::fs::File::open(&path).unwrap()).unwrap();
            (0..z.len()).map(|i| z.by_index(i).unwrap().size()).sum()
        };
        assert_eq!(k.budget.used(), unpacked);
        assert_eq!(k.budget.limit(), deckle_limits::MAX_BOOK_BYTES);
    }
}
