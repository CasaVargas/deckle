// SPDX-License-Identifier: GPL-3.0-or-later
use deckle_xray::wiki::{summary, summary_url};

#[test]
fn summary_url_is_the_rest_endpoint_with_the_title_encoded() {
    assert_eq!(
        summary_url("en", "Mr. Darcy"),
        "https://en.wikipedia.org/api/rest_v1/page/summary/Mr._Darcy"
    );
    assert_eq!(
        summary_url("de", "Köln Altstadt"),
        "https://de.wikipedia.org/api/rest_v1/page/summary/K%C3%B6ln_Altstadt"
    );
}

#[test]
fn summary_parses_extract_and_skips_disambiguation() {
    let canned = r#"{"type":"standard","title":"Longbourn","extract":"Longbourn is the Bennet family home in Pride and Prejudice.","content_urls":{"desktop":{"page":"https://en.wikipedia.org/wiki/Longbourn"}}}"#;
    let s = summary("en", "Longbourn", &mut |_| Ok(canned.to_owned())).unwrap();
    assert_eq!(
        s,
        "Longbourn is the Bennet family home in Pride and Prejudice."
    );
    let dis = r#"{"type":"disambiguation","title":"Darcy","extract":"Darcy may refer to:"}"#;
    assert!(summary("en", "Darcy", &mut |_| Ok(dis.to_owned())).is_none());
    assert!(summary("en", "Nope", &mut |_| Err("404".into())).is_none());
}
