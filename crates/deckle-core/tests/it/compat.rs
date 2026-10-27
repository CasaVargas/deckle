// SPDX-License-Identifier: GPL-3.0-or-later
use deckle_core::compat::{
    Feature, FirmwareRange, RULES, Rule, SendUse, check, check_rules, compare_firmware,
    parse_firmware, send_warnings,
};
use deckle_core::devices::{DeviceProfile, TransportKind};
use std::cmp::Ordering;

const BAD_XRAY: &[Rule] = &[Rule {
    model: Some("Paperwhite"),
    transport: None,
    firmware: Some(FirmwareRange {
        min: Some("5.16.2"),
        max: Some("5.16.9.9"),
    }),
    feature: Feature::Xray,
    on_send: true,
    message: "X-Ray ignored on this firmware",
}];

#[test]
fn version_lines_yield_the_firmware_token() {
    assert_eq!(
        parse_firmware("Kindle 5.16.2.1.1 (4184940002)").as_deref(),
        Some("5.16.2.1.1")
    );
    assert_eq!(parse_firmware("5.18.1").as_deref(), Some("5.18.1"));
    assert_eq!(parse_firmware("Kindle (unknown)"), None);
    assert_eq!(parse_firmware("Kindle 7"), None);
    assert_eq!(parse_firmware(""), None);
}

#[test]
fn firmware_compares_numerically_per_component() {
    assert_eq!(
        compare_firmware("5.16.10", "5.16.9"),
        Some(Ordering::Greater)
    );
    assert_eq!(compare_firmware("5.16", "5.16.0.0"), Some(Ordering::Equal));
    assert_eq!(compare_firmware("5.15.1", "5.16"), Some(Ordering::Less));
    assert_eq!(compare_firmware("5.x", "5.16"), None);
}

#[test]
fn a_known_bad_firmware_range_warns_and_others_do_not() {
    let hit = check_rules(
        BAD_XRAY,
        "Kindle Paperwhite",
        TransportKind::MassStorage,
        Some("5.16.2.1.1"),
    );
    assert_eq!(hit.len(), 1);
    assert_eq!(hit[0].feature, Feature::Xray);
    assert_eq!(hit[0].message, "X-Ray ignored on this firmware");
    for ok in [Some("5.16.1"), Some("5.17.0"), Some("5.16.10")] {
        assert!(
            check_rules(
                BAD_XRAY,
                "Kindle Paperwhite",
                TransportKind::MassStorage,
                ok
            )
            .is_empty(),
            "{ok:?} is outside the range"
        );
    }
    assert!(
        check_rules(
            BAD_XRAY,
            "Kindle Oasis",
            TransportKind::MassStorage,
            Some("5.16.3")
        )
        .is_empty(),
        "unlisted model"
    );
}

#[test]
fn unknown_or_unreadable_firmware_never_hits_a_firmware_range() {
    for fw in [None, Some(""), Some("unknown"), Some("5.16.x")] {
        assert!(
            check_rules(BAD_XRAY, "Kindle Paperwhite", TransportKind::Mtp, fw).is_empty(),
            "{fw:?}"
        );
    }
}

#[test]
fn seed_rows_key_on_transport_and_model_without_firmware() {
    let scribe = check("Kindle Scribe", TransportKind::Mtp, None);
    let features: Vec<Feature> = scribe.iter().map(|w| w.feature).collect();
    assert_eq!(
        features,
        vec![Feature::Cover, Feature::WordWise, Feature::Kfx]
    );
    let colorsoft = check("Kindle Colorsoft", TransportKind::Mtp, Some("5.18.1"));
    assert_eq!(colorsoft.len(), 2, "the Scribe row is model-specific");
    assert!(
        check(
            "Kindle 5.16.2.1.1 (4184940002)",
            TransportKind::MassStorage,
            Some("5.16.2.1.1")
        )
        .is_empty(),
        "nothing known against mass-storage Kindles yet"
    );
    assert!(RULES.iter().all(|r| r.firmware.is_none()));
}

#[test]
fn a_profile_is_checked_with_its_firmware() {
    let p = DeviceProfile {
        transport: TransportKind::Mtp,
        firmware: Some("5.18.1.0.1".into()),
        ..DeviceProfile::new("mtp:G0X1", "Kindle Scribe")
    };
    assert_eq!(deckle_core::compat::check_profile(&p).len(), 3);
}

#[test]
fn send_warnings_keep_only_on_send_rows_for_features_the_send_used() {
    let ebok = SendUse {
        ebok_without_cover_source: true,
        xray: true,
        word_wise: true,
    };
    let w = send_warnings(RULES, "Kindle Scribe", TransportKind::Mtp, None, &ebok);
    assert_eq!(w.len(), 1, "{w:?}");
    assert!(w[0].contains("no cover"));
    let pdoc = SendUse::default();
    assert!(send_warnings(RULES, "Kindle Scribe", TransportKind::Mtp, None, &pdoc).is_empty());
    let used_xray = SendUse {
        xray: true,
        ..Default::default()
    };
    assert_eq!(
        send_warnings(
            BAD_XRAY,
            "Kindle Paperwhite",
            TransportKind::MassStorage,
            Some("5.16.5"),
            &used_xray
        ),
        vec!["X-Ray ignored on this firmware".to_owned()]
    );
    assert!(
        send_warnings(
            BAD_XRAY,
            "Kindle Paperwhite",
            TransportKind::MassStorage,
            Some("5.16.5"),
            &pdoc
        )
        .is_empty(),
        "no X-Ray shipped, nothing to warn about"
    );
}
