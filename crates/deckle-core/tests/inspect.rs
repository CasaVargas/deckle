// SPDX-License-Identifier: GPL-3.0-or-later
use std::io::Write;

use deckle_core::inspect::{InspectError, inspect};

fn mini() -> std::path::PathBuf {
    deckle_kfx::fixtures::path("deckle-mini.epub")
}

#[test]
fn inspects_the_mini_fixture() {
    let info = inspect(&mini()).unwrap();
    assert_eq!(info.title, "Deckle Mini Fixture");
    assert_eq!(info.creators.len(), 1);
    assert_eq!(info.creators[0].name, "Jane Testwright");
    assert_eq!(
        info.creators[0].file_as.as_deref(),
        Some("Testwright, Jane")
    );
    assert_eq!(info.language, "en");
    let series = info.series.as_ref().expect("series");
    assert_eq!(series.name, "Fixture Series");
    assert_eq!(series.index, Some(2.0));
    assert_eq!(info.cover_href.as_deref(), Some("cover.png"));
    assert_eq!(info.spine.len(), 3);
    let titles: Vec<&str> = info.toc.iter().map(|t| t.title.as_str()).collect();
    assert_eq!(titles, ["Chapter One", "Chapter Two", "Chapter Three"]);
    assert_eq!(info.epub_version, "3.0");
    assert!(info.identifiers.iter().any(|i| i.contains("deckle000001")));
    assert_eq!(info.isbn, None);
    assert!(info.warnings.is_empty(), "{:?}", info.warnings);
}

/// Copies the fixture and adds/replaces entries.
fn variant(edits: &[(&str, &[u8])]) -> tempfile::NamedTempFile {
    let src = std::fs::File::open(mini()).unwrap();
    let mut zin = zip::ZipArchive::new(src).unwrap();
    let out = tempfile::Builder::new().suffix(".epub").tempfile().unwrap();
    let mut zout = zip::ZipWriter::new(std::fs::File::create(out.path()).unwrap());
    for i in 0..zin.len() {
        let mut f = zin.by_index(i).unwrap();
        let name = f.name().unwrap().into_owned();
        if edits.iter().any(|(n, _)| *n == name) {
            continue;
        }
        let opts =
            zip::write::SimpleFileOptions::default().compression_method(if name == "mimetype" {
                zip::CompressionMethod::Stored
            } else {
                zip::CompressionMethod::Deflated
            });
        zout.start_file(&name, opts).unwrap();
        std::io::copy(&mut f, &mut zout).unwrap();
    }
    for (name, data) in edits {
        zout.start_file(*name, zip::write::SimpleFileOptions::default())
            .unwrap();
        zout.write_all(data).unwrap();
    }
    zout.finish().unwrap();
    out
}

#[test]
fn drm_is_refused_before_anything_else() {
    let f = variant(&[(
        "META-INF/encryption.xml",
        br#"<encryption xmlns="urn:oasis:names:tc:opendocument:xmlns:container" xmlns:enc="http://www.w3.org/2001/04/xmlenc#"><enc:EncryptedData><enc:EncryptionMethod Algorithm="http://www.w3.org/2001/04/xmlenc#aes128-cbc"/><enc:CipherData><enc:CipherReference URI="OEBPS/c0.xhtml"/></enc:CipherData></enc:EncryptedData></encryption>"#,
    )]);
    match inspect(f.path()) {
        Err(InspectError::Drm) => {}
        other => panic!("expected Drm, got {other:?}"),
    }
}

#[test]
fn calibre_series_meta_is_understood() {
    let src = std::fs::File::open(mini()).unwrap();
    let mut zin = zip::ZipArchive::new(src).unwrap();
    let mut opf = String::new();
    std::io::Read::read_to_string(&mut zin.by_name("OEBPS/content.opf").unwrap(), &mut opf)
        .unwrap();
    // strip the EPUB 3 collection meta and add Calibre's legacy pair + an ISBN identifier
    let opf = opf
        .replace(r##"<meta property="belongs-to-collection" id="s1">Fixture Series</meta><meta refines="#s1" property="collection-type">series</meta><meta refines="#s1" property="group-position">2</meta>"##,
                 r##"<meta name="calibre:series" content="Legacy Series"/><meta name="calibre:series_index" content="3.5"/><dc:identifier opf:scheme="ISBN" xmlns:opf="http://www.idpf.org/2007/opf">urn:isbn:9780141439518</dc:identifier>"##);
    assert!(opf.contains("calibre:series"), "replace failed");
    let f = variant(&[("OEBPS/content.opf", opf.as_bytes())]);
    let info = inspect(f.path()).unwrap();
    let s = info.series.unwrap();
    assert_eq!(s.name, "Legacy Series");
    assert_eq!(s.index, Some(3.5));
    assert_eq!(info.isbn.as_deref(), Some("9780141439518"));
}

#[test]
fn not_a_zip_and_missing_container_are_distinct_errors() {
    let f = tempfile::NamedTempFile::new().unwrap();
    std::fs::write(f.path(), b"hello").unwrap();
    assert!(matches!(inspect(f.path()), Err(InspectError::NotZip)));
    let out = tempfile::NamedTempFile::new().unwrap();
    let mut z = zip::ZipWriter::new(std::fs::File::create(out.path()).unwrap());
    z.start_file("mimetype", zip::write::SimpleFileOptions::default())
        .unwrap();
    z.write_all(b"application/epub+zip").unwrap();
    z.finish().unwrap();
    assert!(matches!(
        inspect(out.path()),
        Err(InspectError::NoContainerXml)
    ));
}

#[test]
fn font_obfuscation_in_encryption_xml_is_not_drm() {
    let enc = br#"<?xml version="1.0"?><encryption xmlns="urn:oasis:names:tc:opendocument:xmlns:container" xmlns:enc="http://www.w3.org/2001/04/xmlenc#"><enc:EncryptedData><enc:EncryptionMethod Algorithm="http://www.idpf.org/2008/embedding"/><enc:CipherData><enc:CipherReference URI="OEBPS/fonts/Font.otf"/></enc:CipherData></enc:EncryptedData><enc:EncryptedData><enc:EncryptionMethod Algorithm="http://ns.adobe.com/pdf/enc#RC"/><enc:CipherData><enc:CipherReference URI="OEBPS/fonts/Other.ttf"/></enc:CipherData></enc:EncryptedData></encryption>"#;
    let f = variant(&[("META-INF/encryption.xml", enc)]);
    let info = inspect(f.path()).expect("font obfuscation is not DRM");
    assert_eq!(info.title, "Deckle Mini Fixture");
}

#[test]
fn adept_encryption_of_content_is_drm() {
    let enc = br#"<?xml version="1.0"?><encryption xmlns="urn:oasis:names:tc:opendocument:xmlns:container" xmlns:enc="http://www.w3.org/2001/04/xmlenc#"><enc:EncryptedData><enc:EncryptionMethod Algorithm="http://www.w3.org/2001/04/xmlenc#aes128-cbc"/><enc:CipherData><enc:CipherReference URI="OEBPS/c0.xhtml"/></enc:CipherData></enc:EncryptedData></encryption>"#;
    let f = variant(&[("META-INF/encryption.xml", enc)]);
    assert!(matches!(inspect(f.path()), Err(InspectError::Drm)));
}

#[test]
fn a_finished_kfx_reports_title_type_and_cover() {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../fixtures/mini");
    for (file, cde) in [
        ("deckle-mini-pdoc.golden.kfx", "PDOC"),
        ("deckle-mini-ebok.golden.kfx", "EBOK"),
    ] {
        let k =
            deckle_core::inspect::inspect_kfx(&std::fs::read(root.join(file)).unwrap()).unwrap();
        assert!(!k.info.title.is_empty(), "{file}");
        assert_eq!(k.cde_type.as_deref(), Some(cde), "{file}");
        let cover = k.cover.unwrap_or_default();
        assert!(cover.starts_with(&[0xFF, 0xD8]), "{file}: cover is a JPEG");
    }
    assert!(deckle_core::inspect::inspect_kfx(b"not a kfx").is_err());
}
