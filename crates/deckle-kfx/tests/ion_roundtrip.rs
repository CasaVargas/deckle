// SPDX-License-Identifier: GPL-3.0-or-later
//! Every Ion payload in a kfxlib-produced KFX must re-encode to the identical bytes.
use deckle_kfx::ion::{Value, decode_annotated, decode_single, encode_single};
use deckle_kfx::symtab::SymbolTable;

fn u16le(b: &[u8]) -> u16 {
    u16::from_le_bytes(b.try_into().unwrap())
}
fn u32le(b: &[u8]) -> u32 {
    u32::from_le_bytes(b.try_into().unwrap())
}
fn u64le(b: &[u8]) -> u64 {
    u64::from_le_bytes(b.try_into().unwrap())
}

/// Minimal hand-rolled CONT parser so this test does not depend on Task 7's container module.
fn entities(kfx: &[u8]) -> (SymbolTable, Vec<(u32, Vec<u8>)>) {
    assert_eq!(&kfx[..4], b"CONT");
    assert_eq!(u16le(&kfx[4..6]), 2);
    let header_len = u32le(&kfx[6..10]) as usize;
    let cinfo_off = u32le(&kfx[10..14]) as usize;
    let cinfo_len = u32le(&kfx[14..18]) as usize;
    let base = SymbolTable::yj_default();
    let cinfo = decode_single(&kfx[cinfo_off..cinfo_off + cinfo_len], &base).unwrap();
    let cinfo = cinfo.as_struct().unwrap();
    let get = |k: &str| cinfo.get(k).unwrap().as_int().unwrap() as usize;
    let (toff, tlen, soff, slen) = (get("$413"), get("$414"), get("$415"), get("$416"));
    let (_, table) =
        decode_annotated(&kfx[soff..soff + slen], &base, Some("$ion_symbol_table")).unwrap();
    let table = table.as_struct().unwrap();
    let max_id = table.get("imports").unwrap().as_list().unwrap()[0]
        .as_struct()
        .unwrap()
        .get("max_id")
        .unwrap()
        .as_int()
        .unwrap() as u32;
    // A KFX container stores the import max_id inflated by the 9 system symbols.
    let mut st = SymbolTable::with_yj_max_id(max_id - 9);
    for s in table.get("symbols").unwrap().as_list().unwrap() {
        st.create_local(s.as_str().unwrap());
    }
    let mut out = Vec::new();
    for row in kfx[toff..toff + tlen].chunks(24) {
        let type_id = u32le(&row[4..8]);
        let off = u64le(&row[8..16]) as usize + header_len;
        let len = u64le(&row[16..24]) as usize;
        let ent = &kfx[off..off + len];
        assert_eq!(&ent[..4], b"ENTY");
        let ehl = u32le(&ent[6..10]) as usize;
        out.push((type_id, ent[ehl..].to_vec()));
    }
    (st, out)
}

#[test]
fn golden_entities_roundtrip_byte_exact() {
    let kfx = std::fs::read(deckle_kfx::fixtures::path(
        "deckle-mini-pdoc-nopages.golden.kfx",
    ))
    .unwrap();
    let (st, ents) = entities(&kfx);
    assert_eq!(ents.len(), 37); // 40 fragments minus the 3 container-level ones ($ion_symbol_table, $270, $593)
    let mut compared = 0;
    for (type_id, payload) in ents {
        let ftype = st.symbol_of(type_id).unwrap();
        if ftype == "$417" || ftype == "$418" {
            continue;
        }
        let v = decode_single(&payload, &st).unwrap_or_else(|e| panic!("{ftype}: {e}"));
        let enc = encode_single(&v, &st).unwrap();
        assert_eq!(enc, payload, "re-encoding {ftype} differs");
        compared += 1;
    }
    assert_eq!(compared, 36);
}

#[test]
fn encodes_scalars_like_kfxlib() {
    let st = SymbolTable::yj_default();
    let enc = |v: Value| encode_single(&v, &st).unwrap()[4..].to_vec();
    assert_eq!(enc(Value::Int(0)), vec![0x20]);
    assert_eq!(enc(Value::Int(5)), vec![0x21, 0x05]);
    assert_eq!(enc(Value::Int(-5)), vec![0x31, 0x05]);
    assert_eq!(enc(Value::Int(256)), vec![0x22, 0x01, 0x00]);
    assert_eq!(enc(Value::Bool(true)), vec![0x11]);
    assert_eq!(enc(Value::Null), vec![0x0F]);
    assert_eq!(enc(Value::Float(0.0)), vec![0x40]);
    assert_eq!(enc(Value::sym("$10")), vec![0x71, 0x0A]);
    assert_eq!(enc(Value::string("abc")), vec![0x83, 0x61, 0x62, 0x63]);
    assert_eq!(
        enc(Value::Decimal(deckle_kfx::ion::Decimal {
            coefficient: 15,
            exponent: -1
        })),
        vec![0x52, 0xC1, 0x0F]
    );
    assert_eq!(
        enc(Value::Decimal(deckle_kfx::ion::Decimal {
            coefficient: 0,
            exponent: 0
        })),
        vec![0x50]
    );
    assert_eq!(
        enc(Value::Decimal(deckle_kfx::ion::Decimal {
            coefficient: -15,
            exponent: -1
        })),
        vec![0x52, 0xC1, 0x8F]
    );
    // a 14-byte string must use the variable-length form: 8E 8E ...
    let s = "a".repeat(14);
    let mut want = vec![0x8E, 0x8E];
    want.extend(s.as_bytes());
    assert_eq!(enc(Value::string(&s)), want);
    assert!(encode_single(&Value::sym("not-defined"), &st).is_err());
}
