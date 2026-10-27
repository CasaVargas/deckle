// SPDX-License-Identifier: GPL-3.0-or-later
use deckle_kfx::ion::{Sym, Value, decode_annotated, decode_single, encode_single};
use deckle_kfx::symtab::SymbolTable;

#[test]
fn default_table_layout() {
    let t = SymbolTable::yj_default();
    assert_eq!(t.symbol_of(1), Some("$ion"));
    assert_eq!(t.symbol_of(10), Some("$10"));
    assert_eq!(t.symbol_of(859), Some("$859"));
    assert_eq!(t.symbol_of(860), None);
    assert_eq!(t.local_min_id(), 860);
    assert_eq!(t.len(), 859);
    assert_eq!(t.id_of(&Sym::new("$348")), Some(348));
    // kfxlib resolves any `$N` symbol to id N even when it is outside the table
    assert_eq!(t.id_of(&Sym::new("$900")), Some(900));
    assert_eq!(t.id_of(&Sym::new("content_1")), None);
}

#[test]
fn local_symbols_are_numbered_from_860_and_idempotent() {
    let mut t = SymbolTable::yj_default();
    let a = t.create_local("content_1");
    let b = t.create_local("content_1");
    assert_eq!(a, b);
    assert_eq!(t.id_of(&a), Some(860));
    assert_eq!(t.symbol_of(860), Some("content_1"));
    assert_eq!(t.local_symbols(), vec!["content_1".to_string()]);
    t.replace_locals(vec!["x".into(), "y".into()]);
    assert_eq!(t.id_of(&Sym::new("content_1")), None);
    assert_eq!(t.id_of(&Sym::new("y")), Some(861));
    assert_eq!(t.len(), 861);
}

#[test]
fn unknown_ids_decode_to_dollar_placeholders_like_kfxlib() {
    // symbol id 900 is outside the table: kfxlib yields `$900` rather than failing
    let t = SymbolTable::yj_default();
    let bytes = [0xE0, 0x01, 0x00, 0xEA, 0x72, 0x03, 0x84];
    assert_eq!(decode_single(&bytes, &t).unwrap(), Value::sym("$900"));
    assert_eq!(
        encode_single(&Value::sym("$900"), &t).unwrap(),
        bytes.to_vec()
    );
}

fn golden_table() -> (Vec<Sym>, Value) {
    let kfx = std::fs::read(deckle_kfx::fixtures::path(
        "deckle-mini-pdoc-nopages.golden.kfx",
    ))
    .unwrap();
    let base = SymbolTable::yj_default();
    let cinfo_off = u32::from_le_bytes(kfx[10..14].try_into().unwrap()) as usize;
    let cinfo_len = u32::from_le_bytes(kfx[14..18].try_into().unwrap()) as usize;
    let cinfo = decode_single(&kfx[cinfo_off..cinfo_off + cinfo_len], &base).unwrap();
    let c = cinfo.as_struct().unwrap();
    let soff = c.get("$415").unwrap().as_int().unwrap() as usize;
    let slen = c.get("$416").unwrap().as_int().unwrap() as usize;
    decode_annotated(&kfx[soff..soff + slen], &base, Some("$ion_symbol_table")).unwrap()
}

#[test]
fn golden_symbol_table_reloads_and_reproduces() {
    let (_, table) = golden_table();
    // On disk the import max_id is inflated by the 9 system symbols; from_container undoes it.
    let t = SymbolTable::from_container_table(&table).unwrap();
    assert_eq!(t.local_min_id(), 860);
    assert_eq!(t.local_symbols().len(), 30);
    assert_eq!(t.symbol_of(860), Some("c0"));
    assert_eq!(t.imports()[0].max_id, 850);
    assert_eq!(t.len(), 889);
    // import_fragment() is the raw form (import max_id 850); the container writer adds 9.
    let frag = t.import_fragment();
    let mut expected = table.clone();
    let imports = expected
        .as_struct_mut()
        .unwrap()
        .get_mut("imports")
        .unwrap()
        .as_list_mut()
        .unwrap();
    imports[0]
        .as_struct_mut()
        .unwrap()
        .insert("max_id", Value::Int(850));
    assert_eq!(frag, expected);
}

#[test]
fn from_kdf_table_keeps_raw_max_id() {
    let (_, mut table) = golden_table();
    let imports = table
        .as_struct_mut()
        .unwrap()
        .get_mut("imports")
        .unwrap()
        .as_list_mut()
        .unwrap();
    imports[0]
        .as_struct_mut()
        .unwrap()
        .insert("max_id", Value::Int(853));
    let t = SymbolTable::from_kdf_table(&table).unwrap();
    assert_eq!(t.imports()[0].max_id, 853);
    assert_eq!(t.local_min_id(), 863);
    assert_eq!(t.symbol_of(861), None); // placeholder for an unknown YJ symbol
    assert_eq!(t.symbol_of(863), Some("c0"));
}
