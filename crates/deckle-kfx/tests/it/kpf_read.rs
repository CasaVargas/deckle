// SPDX-License-Identifier: GPL-3.0-or-later
use std::io::{Read, Write};

use deckle_kfx::ion::Sym;
use deckle_kfx::kpf::{KpfError, read_kpf, strip_sqlite_fingerprints};

fn mini_kdf() -> Vec<u8> {
    let f = std::fs::File::open(deckle_kfx::fixtures::path("deckle-mini.kpf")).unwrap();
    let mut z = zip::ZipArchive::new(f).unwrap();
    let mut kdf = Vec::new();
    z.by_name("resources/book.kdf")
        .unwrap()
        .read_to_end(&mut kdf)
        .unwrap();
    kdf
}

#[test]
fn fingerprint_strip_removes_the_record_from_mini_kdf() {
    let kdf = mini_kdf();
    let (out, removed) = strip_sqlite_fingerprints(&kdf);
    // The mini KDF is 148 480 bytes: one record at offset 1024, none at the next MiB.
    assert_eq!(removed, 1);
    assert_eq!(out.len(), kdf.len() - 1024);
    assert!(out.starts_with(b"SQLite format 3\0"));
    // idempotent on an already clean file
    let (again, removed2) = strip_sqlite_fingerprints(&out);
    assert_eq!(removed2, 0);
    assert_eq!(again, out);
}

#[test]
fn mini_kpf_fragment_inventory_matches_kfxlib() {
    let k = read_kpf(&deckle_kfx::fixtures::path("deckle-mini.kpf")).unwrap();
    assert_eq!(k.fragments.len(), 82);
    let count = |t: &str| k.fragments.all(&Sym::new(t)).count();
    for (t, n) in [
        ("$157", 4),
        ("$164", 1),
        ("$258", 1),
        ("$259", 4),
        ("$260", 4),
        ("$270", 1),
        ("$389", 1),
        ("$417", 1),
        ("$490", 1),
        ("$538", 1),
        ("$550", 1),
        ("$585", 1),
        ("$593", 1),
        ("$597", 5),
        ("$608", 22),
        ("$609", 4),
        ("$610", 27),
        ("$611", 1),
        ("$ion_symbol_table", 1),
    ] {
        assert_eq!(count(t), n, "{t}");
    }
    assert_eq!(k.symtab.local_min_id(), 860);
    assert_eq!(k.symtab.len(), 944);
    assert_eq!(k.kcb["metadata"]["tool_version"], "4.0.3");
    assert!(k.is_prepub);
    assert!(k.source_epub.is_some());
    // the one $417 is the cover resource, loaded from resources/res via a `path` payload
    let raw = Sym::new("$417");
    let res = k.fragments.all(&raw).next().unwrap();
    assert!(res.fid.as_ref().unwrap().as_str().starts_with("rsrc"));
    assert!(
        res.value.as_blob().unwrap().starts_with(&[0xFF, 0xD8]),
        "cover is JPEG in the KPF"
    );
    // singleton/root fragments carry no fid
    assert!(
        k.fragments
            .get_single(&Sym::new("$490"))
            .unwrap()
            .fid
            .is_none()
    );
    // the synthesized $270 says KPF
    let c = k.fragments.get_single(&Sym::new("$270")).unwrap();
    assert_eq!(
        c.value.as_struct().unwrap().get("$161").unwrap().as_str(),
        Some("KPF")
    );
}

#[test]
fn drm_kdf_is_refused() {
    // Build a tiny KPF whose single fragment blob carries the DRMION signature.
    let dir = tempfile::tempdir().unwrap();
    let kdf_path = dir.path().join("book.kdf");
    {
        let conn = rusqlite::Connection::open(&kdf_path).unwrap();
        conn.execute_batch(
            "CREATE TABLE fragments(id char(40), payload_type char(10), payload_value blob, primary key (id));
             CREATE TABLE capabilities(key char(20), version smallint, primary key (key, version)) without rowid;",
        )
        .unwrap();
        conn.execute(
            "INSERT INTO fragments VALUES ('x', 'blob', ?1)",
            [b"\xeaDRMION\xee\x00\x00".to_vec()],
        )
        .unwrap();
    }
    let kpf_path = dir.path().join("drm.kpf");
    let f = std::fs::File::create(&kpf_path).unwrap();
    let mut z = zip::ZipWriter::new(f);
    let opts = zip::write::SimpleFileOptions::default();
    z.start_file("resources/book.kdf", opts).unwrap();
    z.write_all(&std::fs::read(&kdf_path).unwrap()).unwrap();
    z.finish().unwrap();
    match read_kpf(&kpf_path) {
        Err(KpfError::Drm) => {}
        other => panic!("expected Drm, got {other:?}"),
    }
}
