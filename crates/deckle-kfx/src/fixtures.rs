// SPDX-License-Identifier: GPL-3.0-or-later
//! Locates files under `<workspace>/fixtures/mini/` for tests and examples.

use std::path::PathBuf;

/// Absolute path of a file in the `fixtures/mini` directory of the repository.
pub fn path(name: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../fixtures/mini")
        .join(name)
}

/// For tests: a zip of stored entries `(name, data, declared unpacked size)`, built byte by
/// byte so an entry can declare a size its data doesn't have, as a hostile file does
/// (size-limits design, "Tests"). Every entry carries zip64 sizes, so a size past 4 GB can be
/// declared.
#[cfg(any(test, feature = "testing"))]
pub fn stored_zip(entries: &[(&str, &[u8], u64)]) -> Vec<u8> {
    fn crc32(data: &[u8]) -> u32 {
        let mut c = !0u32;
        for &b in data {
            c ^= u32::from(b);
            for _ in 0..8 {
                c = if c & 1 != 0 {
                    (c >> 1) ^ 0xEDB8_8320
                } else {
                    c >> 1
                };
            }
        }
        !c
    }
    let mut out = Vec::new();
    let mut central = Vec::new();
    for &(name, data, declared) in entries {
        let offset = out.len() as u32;
        let mut zip64 = Vec::new();
        zip64.extend(1u16.to_le_bytes());
        zip64.extend(16u16.to_le_bytes());
        zip64.extend(declared.to_le_bytes());
        zip64.extend((data.len() as u64).to_le_bytes());
        // Version needed, flags, method (stored), time, date, CRC, sizes in zip64.
        let mut common = Vec::new();
        common.extend(45u16.to_le_bytes());
        common.extend(0u16.to_le_bytes());
        common.extend(0u16.to_le_bytes());
        common.extend(0u32.to_le_bytes());
        common.extend(crc32(data).to_le_bytes());
        common.extend(u32::MAX.to_le_bytes());
        common.extend(u32::MAX.to_le_bytes());
        common.extend((name.len() as u16).to_le_bytes());
        common.extend((zip64.len() as u16).to_le_bytes());

        out.extend(0x0403_4b50u32.to_le_bytes());
        out.extend(&common);
        out.extend(name.as_bytes());
        out.extend(&zip64);
        out.extend(data);

        central.extend(0x0201_4b50u32.to_le_bytes());
        central.extend(45u16.to_le_bytes()); // made by
        central.extend(&common);
        central.extend(0u16.to_le_bytes()); // comment
        central.extend(0u16.to_le_bytes()); // disk
        central.extend(0u16.to_le_bytes()); // internal attributes
        central.extend(0u32.to_le_bytes()); // external attributes
        central.extend(offset.to_le_bytes());
        central.extend(name.as_bytes());
        central.extend(&zip64);
    }
    let cd_offset = out.len() as u32;
    let n = entries.len() as u16;
    out.extend(&central);
    out.extend(0x0605_4b50u32.to_le_bytes());
    out.extend(0u16.to_le_bytes());
    out.extend(0u16.to_le_bytes());
    out.extend(n.to_le_bytes());
    out.extend(n.to_le_bytes());
    out.extend((central.len() as u32).to_le_bytes());
    out.extend(cd_offset.to_le_bytes());
    out.extend(0u16.to_le_bytes());
    out
}

/// For tests: a minimal one-chapter EPUB as a [`stored_zip`] (OPF `content.opf`, chapter
/// `c.xhtml`), its declared sizes true, followed by the entries `extra`.
#[cfg(any(test, feature = "testing"))]
pub fn stored_epub(extra: &[(&str, &[u8], u64)]) -> Vec<u8> {
    const CONTAINER: &[u8] = br#"<?xml version="1.0"?>
<container version="1.0" xmlns="urn:oasis:names:tc:opendocument:xmlns:container">
<rootfiles><rootfile full-path="content.opf" media-type="application/oebps-package+xml"/></rootfiles>
</container>"#;
    const OPF: &[u8] = br#"<?xml version="1.0"?>
<package xmlns="http://www.idpf.org/2007/opf" version="3.0" unique-identifier="id">
<metadata xmlns:dc="http://purl.org/dc/elements/1.1/"><dc:identifier id="id">x</dc:identifier>
<dc:title>T</dc:title><dc:creator>A. Writer</dc:creator><dc:language>en</dc:language></metadata>
<manifest><item id="c" href="c.xhtml" media-type="application/xhtml+xml"/></manifest>
<spine><itemref idref="c"/></spine></package>"#;
    const XHTML: &[u8] = br#"<html xmlns="http://www.w3.org/1999/xhtml"><head><title>T</title></head><body><p>Hi.</p></body></html>"#;
    let mut entries: Vec<(&str, &[u8], u64)> = vec![
        ("mimetype", b"application/epub+zip", 20),
        ("META-INF/container.xml", CONTAINER, CONTAINER.len() as u64),
        ("content.opf", OPF, OPF.len() as u64),
        ("c.xhtml", XHTML, XHTML.len() as u64),
    ];
    entries.extend_from_slice(extra);
    stored_zip(&entries)
}
