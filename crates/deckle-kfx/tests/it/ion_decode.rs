// SPDX-License-Identifier: GPL-3.0-or-later
use deckle_kfx::ion::{Decimal, Sym, Value, decode_annotated, decode_single};
use deckle_kfx::symtab::SymbolTable;

fn hex(s: &str) -> Vec<u8> {
    let s: String = s.chars().filter(|c| !c.is_whitespace()).collect();
    (0..s.len())
        .step_by(2)
        .map(|i| u8::from_str_radix(&s[i..i + 2], 16).unwrap())
        .collect()
}

fn st() -> SymbolTable {
    SymbolTable::yj_default()
}

#[test]
fn decodes_scalars_and_containers() {
    assert_eq!(
        decode_single(&hex("E00100EA 2105"), &st()).unwrap(),
        Value::Int(5)
    );
    assert_eq!(
        decode_single(&hex("E00100EA 3105"), &st()).unwrap(),
        Value::Int(-5)
    );
    assert_eq!(
        decode_single(&hex("E00100EA 20"), &st()).unwrap(),
        Value::Int(0)
    );
    assert_eq!(
        decode_single(&hex("E00100EA 83616263"), &st()).unwrap(),
        Value::String("abc".into())
    );
    assert_eq!(
        decode_single(&hex("E00100EA 710A"), &st()).unwrap(),
        Value::Symbol(Sym::new("$10"))
    );
    assert_eq!(
        decode_single(&hex("E00100EA 11"), &st()).unwrap(),
        Value::Bool(true)
    );
    assert_eq!(
        decode_single(&hex("E00100EA 10"), &st()).unwrap(),
        Value::Bool(false)
    );
    assert_eq!(
        decode_single(&hex("E00100EA 0F"), &st()).unwrap(),
        Value::Null
    );
    assert_eq!(
        decode_single(&hex("E00100EA B4 2101 2102"), &st()).unwrap(),
        Value::List(vec![Value::Int(1), Value::Int(2)])
    );
    // decimal 1.5 = coefficient 15, exponent -1 : exponent VarInt C1, coefficient Int 0F
    assert_eq!(
        decode_single(&hex("E00100EA 52C10F"), &st()).unwrap(),
        Value::Decimal(Decimal {
            coefficient: 15,
            exponent: -1
        })
    );
    assert_eq!(
        decode_single(&hex("E00100EA 50"), &st()).unwrap(),
        Value::Decimal(Decimal {
            coefficient: 0,
            exponent: 0
        })
    );
    // struct { $10: 1 } : D3 8A 2101
    let v = decode_single(&hex("E00100EA D3 8A 2101"), &st()).unwrap();
    let Value::Struct(s) = v else {
        panic!("not a struct: {v:?}")
    };
    assert_eq!(s.get("$10"), Some(&Value::Int(1)));
    // annotation $490:: { } : E3 81 (annot len 1) 0x80|490? -> sid 490 = 0x1EA -> VarUInt 03 EA -> annots len 2
    let v = decode_single(&hex("E00100EA E4 82 03EA D0"), &st()).unwrap();
    assert_eq!(
        v,
        Value::Annotated {
            annots: vec![Sym::new("$490")],
            value: Box::new(Value::Struct(Default::default()))
        }
    );
    // blob
    assert_eq!(
        decode_single(&hex("E00100EA A2 0102"), &st()).unwrap(),
        Value::Blob(vec![1, 2])
    );
    // float 1.0
    assert_eq!(
        decode_single(&hex("E00100EA 48 3FF0000000000000"), &st()).unwrap(),
        Value::Float(1.0)
    );
}

#[test]
fn decode_annotated_checks_expected_annotation() {
    let bytes = hex("E00100EA E4 82 03EA D0");
    let (annots, _) = decode_annotated(&bytes, &st(), Some("$490")).unwrap();
    assert_eq!(annots, vec![Sym::new("$490")]);
    assert!(decode_annotated(&bytes, &st(), Some("$258")).is_err());
    assert!(decode_annotated(&hex("E00100EA 2105"), &st(), None).is_err());
}

#[test]
fn decodes_every_ion_fragment_blob_in_mini_kdf() {
    use std::io::Read;
    let f = std::fs::File::open(deckle_kfx::fixtures::path("deckle-mini.kpf")).unwrap();
    let mut z = zip::ZipArchive::new(f).unwrap();
    let mut kdf = Vec::new();
    z.by_name("resources/book.kdf")
        .unwrap()
        .read_to_end(&mut kdf)
        .unwrap();
    // strip SQLite fingerprint records (FA 50 0A 5F at 1024, then every MiB)
    let mut out = Vec::with_capacity(kdf.len());
    let mut off = 0usize;
    let mut next_fp = 1024usize;
    while off < kdf.len() {
        if off == next_fp && kdf.len() >= off + 4 && kdf[off..off + 4] == [0xFA, 0x50, 0x0A, 0x5F] {
            off += 1024;
            next_fp += 1024 * 1024;
            continue;
        }
        out.push(kdf[off]);
        off += 1;
    }
    let tmp = tempfile::NamedTempFile::new().unwrap();
    std::fs::write(tmp.path(), &out).unwrap();
    let conn = rusqlite::Connection::open(tmp.path()).unwrap();
    let mut stmt = conn
        .prepare("SELECT id, payload_type, payload_value FROM fragments ORDER BY id")
        .unwrap();
    let rows = stmt
        .query_map([], |r| {
            Ok((
                r.get::<_, String>(0)?,
                r.get::<_, String>(1)?,
                r.get::<_, Vec<u8>>(2)?,
            ))
        })
        .unwrap();
    // Load the KDF's own symbol table first: imports[0].max_id and the local symbols list.
    let rows: Vec<_> = rows.map(|r| r.unwrap()).collect();
    let symtab_row = rows
        .iter()
        .find(|(id, _, _)| id == "$ion_symbol_table")
        .expect("symbol table row");
    let (_, table) = decode_annotated(&symtab_row.2, &st(), Some("$ion_symbol_table")).unwrap();
    let table = table.as_struct().unwrap();
    let imports = table.get("imports").unwrap().as_list().unwrap();
    let max_id = imports[0]
        .as_struct()
        .unwrap()
        .get("max_id")
        .unwrap()
        .as_int()
        .unwrap() as u32;
    let mut st = SymbolTable::with_yj_max_id(max_id);
    for s in table
        .get("symbols")
        .map(|v| v.as_list().unwrap().clone())
        .unwrap_or_default()
    {
        st.create_local(s.as_str().unwrap());
    }
    assert!(max_id >= 850, "max_id {max_id}");
    let mut decoded = 0;
    let mut total = 0;
    for (id, ptype, blob) in rows {
        total += 1;
        if ptype != "blob" || !blob.starts_with(&[0xE0, 0x01, 0x00, 0xEA]) || blob.len() == 4 {
            continue;
        }
        if id == "$ion_symbol_table" || id == "max_id" || id == "max_eid_in_sections" {
            continue;
        }
        let (annots, _) =
            decode_annotated(&blob, &st, None).unwrap_or_else(|e| panic!("{id}: {e}"));
        assert!(!annots.is_empty(), "{id}");
        decoded += 1;
    }
    assert_eq!(total, 81, "fragment rows in the mini KDF");
    assert!(decoded >= 75, "decoded {decoded}");
}
