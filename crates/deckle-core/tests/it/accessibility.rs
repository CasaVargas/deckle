// SPDX-License-Identifier: GPL-3.0-or-later
use deckle_core::accessibility::{language_tag, missing_alt, normalize_opf, normalize_xhtml};

const XML: &str = "http://www.w3.org/XML/1998/namespace";
fn html(attrs: &str, body: &str) -> String {
    format!(
        "<?xml version=\"1.0\"?><html xmlns=\"http://www.w3.org/1999/xhtml\" {attrs}><head><title>Book</title></head><body>{body}</body></html>"
    )
}
fn opf(metadata: &str) -> String {
    format!(
        "<package xmlns=\"http://www.idpf.org/2007/opf\" xmlns:d=\"http://purl.org/dc/elements/1.1/\"><metadata>{metadata}</metadata></package>"
    )
}

#[test]
fn language_spelling_keeps_scripts_regions_variants_extensions_and_private_use() {
    for (input, output) in [
        (" en_US ", "en-US"),
        ("ZH_hant_tw", "zh-Hant-TW"),
        ("es_419", "es-419"),
        ("de-CH-1901", "de-CH-1901"),
        ("en-US-u-ca-gregory", "en-US-u-ca-gregory"),
        ("x-OWNER", "x-owner"),
        ("i-klingon", "i-klingon"),
        ("und", "und"),
    ] {
        assert_eq!(language_tag(input).as_deref(), Some(output));
    }
    for input in ["", "en--US", "en US", "en@US", "123"] {
        assert!(language_tag(input).is_none());
    }
}

#[test]
fn opf_changes_only_language_elements_and_handles_other_dc_prefixes() {
    let source = opf(
        "<!-- retain --><d:title>en_US</d:title><d:language id='lang'> en_US </d:language><d:language>fr_ca</d:language>",
    );
    let (fixed, primary, count) = normalize_opf(&source, &mut vec![]).unwrap();
    assert_eq!(primary, "en-US");
    assert_eq!(count, 2);
    assert_eq!(
        fixed,
        source
            .replace(" id='lang'> en_US ", " id='lang'>en-US")
            .replace(">fr_ca<", ">fr-CA<")
    );
    assert_eq!(
        normalize_opf(&fixed, &mut vec![]).unwrap(),
        (fixed, primary, 0)
    );
}

#[test]
fn missing_empty_and_malformed_package_languages_are_explicit() {
    for metadata in [
        "<d:title>Book</d:title>",
        "<d:language/>",
        "<d:language></d:language>",
    ] {
        let mut warnings = vec![];
        let (fixed, lang, count) = normalize_opf(&opf(metadata), &mut warnings).unwrap();
        assert_eq!(lang, "en");
        assert_eq!(count, 1);
        assert_eq!(warnings, ["no dc:language; assuming en"]);
        assert!(fixed.contains(">en</"));
        roxmltree::Document::parse(&fixed).unwrap();
    }
    let mut warnings = vec![];
    let (fixed, lang, _) =
        normalize_opf(&opf("<d:language>en US</d:language>"), &mut warnings).unwrap();
    assert_eq!(lang, "und");
    assert!(fixed.contains(">und</"));
    assert!(warnings[0].contains("invalid dc:language"));
    let (fixed, lang, _) = normalize_opf(
        &opf("<d:language/><d:language>fr</d:language>"),
        &mut vec![],
    )
    .unwrap();
    assert_eq!(lang, "fr");
    assert_eq!(fixed.matches(">fr</").count(), 2);
    let (fixed, _, _) = normalize_opf("<package><metadata /></package>", &mut vec![]).unwrap();
    roxmltree::Document::parse(&fixed).unwrap();
    assert!(fixed.contains(">en</"));
}

#[test]
fn root_languages_are_filled_without_touching_inline_passages_or_similar_attributes() {
    let body = "<p xml:lang='es-MX'>Hola <span lang='fr'>bonjour</span></p>";
    let source = html("data-lang='de' title='a > b'", body);
    let (fixed, count) = normalize_xhtml(&source, "en-US", &mut vec![]).unwrap();
    assert_eq!(count, 2);
    assert!(fixed.contains(body));
    assert!(fixed.contains("data-lang='de'"));
    let doc = roxmltree::Document::parse(&fixed).unwrap();
    assert_eq!(doc.root_element().attribute("lang"), Some("en-US"));
    assert_eq!(doc.root_element().attribute((XML, "lang")), Some("en-US"));
    assert_eq!(
        normalize_xhtml(&fixed, "en", &mut vec![]).unwrap(),
        (fixed, 0)
    );
}

#[test]
fn explicit_chapter_languages_and_intentional_unknown_values_win_over_book_language() {
    for (attrs, expected) in [
        ("xml:lang='fr_ca'", "fr-CA"),
        ("lang='es'", "es"),
        ("xml:lang=''", ""),
        ("lang=''", ""),
    ] {
        let (fixed, _) = normalize_xhtml(&html(attrs, "<p>Text</p>"), "en", &mut vec![]).unwrap();
        let doc = roxmltree::Document::parse(&fixed).unwrap();
        assert_eq!(doc.root_element().attribute("lang"), Some(expected));
        assert_eq!(doc.root_element().attribute((XML, "lang")), Some(expected));
    }
    let valid = html("lang='fr-CA' xml:lang='fr-CA'", "<!-- same bytes -->");
    assert_eq!(
        normalize_xhtml(&valid, "en", &mut vec![]).unwrap(),
        (valid, 0)
    );
}

#[test]
fn invalid_and_conflicting_root_languages_warn_without_guessing() {
    for attrs in ["lang='en US'", "lang='en' xml:lang='fr'"] {
        let source = html(attrs, "");
        let mut warnings = vec![];
        let (fixed, count) = normalize_xhtml(&source, "de", &mut warnings).unwrap();
        assert_eq!(fixed, source);
        assert_eq!(count, 0);
        assert_eq!(warnings.len(), 1);
    }
}

#[test]
fn alt_audit_counts_only_missing_attributes_and_never_infers_decorative_images() {
    let source = html(
        "",
        "<img src='a'/><img src='b' alt=''/><img src='c' alt='A diagram'/><img src='d' data-alt='not alt'/><svg xmlns='http://www.w3.org/2000/svg'><image href='e'/></svg>",
    );
    assert_eq!(missing_alt(&source).unwrap(), 2);
    let (fixed, _) = normalize_xhtml(&source, "en", &mut vec![]).unwrap();
    assert!(fixed.contains("<img src='b' alt=''/><img src='c' alt='A diagram'/>"));
    assert_eq!(missing_alt(&fixed).unwrap(), 2);
    assert!(missing_alt("<html><img></html>").is_err());
}
