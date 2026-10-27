// SPDX-License-Identifier: GPL-3.0-or-later
//! Design §5: covers are non-negotiable; EBOK only where the cover still shows.
use deckle_core::cover_rule::{AsinChoice, DeviceClass, MatchResult, Override, decide};
use deckle_kfx::convert::CdeType;

const REAL: &str = "0141439518";

#[test]
fn mass_storage_kindle_gets_everything_with_a_generated_asin_and_our_thumbnail() {
    let d = decide(
        &DeviceClass::MassStorage {
            thumbnails_writable: true,
        },
        &MatchResult::None,
        Override::None,
    );
    assert_eq!(d.cde, CdeType::Ebok);
    assert!(d.write_thumbnail);
    assert_eq!(d.asin, AsinChoice::Generated);
    assert!(d.reason.contains("thumbnail"), "{}", d.reason);
}

#[test]
fn mtp_kindle_without_a_match_ships_pdoc_with_the_reason() {
    let d = decide(&DeviceClass::Mtp, &MatchResult::None, Override::None);
    assert_eq!(d.cde, CdeType::Pdoc);
    assert!(!d.write_thumbnail);
    assert_eq!(d.asin, AsinChoice::Generated);
    assert!(
        d.reason.contains("X-Ray") && d.reason.contains("withheld"),
        "{}",
        d.reason
    );
}

#[test]
fn mtp_kindle_with_a_verified_match_uses_the_real_asin_and_ebok() {
    let m = MatchResult::Candidate {
        asin: REAL.into(),
        verified: true,
    };
    let d = decide(&DeviceClass::Mtp, &m, Override::None);
    assert_eq!(d.cde, CdeType::Ebok);
    assert_eq!(d.asin, AsinChoice::Real(REAL.into()));
    assert!(!d.write_thumbnail);
}

#[test]
fn unverified_match_does_not_risk_the_cover() {
    let m = MatchResult::Candidate {
        asin: REAL.into(),
        verified: false,
    };
    let d = decide(&DeviceClass::Mtp, &m, Override::None);
    assert_eq!(d.cde, CdeType::Pdoc);
    assert!(
        d.reason.contains("spike") || d.reason.contains("verified"),
        "{}",
        d.reason
    );
}

#[test]
fn mass_storage_without_a_writable_thumbnail_folder_behaves_like_mtp() {
    let d = decide(
        &DeviceClass::MassStorage {
            thumbnails_writable: false,
        },
        &MatchResult::None,
        Override::None,
    );
    assert_eq!(d.cde, CdeType::Pdoc);
}

#[test]
fn folder_export_defaults_to_pdoc() {
    let d = decide(&DeviceClass::Folder, &MatchResult::None, Override::None);
    assert_eq!(d.cde, CdeType::Pdoc);
    assert!(!d.write_thumbnail);
}

#[test]
fn overrides_win_and_say_so() {
    let f = decide(
        &DeviceClass::Mtp,
        &MatchResult::None,
        Override::PreferFeatures,
    );
    assert_eq!(f.cde, CdeType::Ebok);
    assert!(
        f.reason.contains("override") && f.reason.contains("grey"),
        "{}",
        f.reason
    );
    let c = decide(
        &DeviceClass::MassStorage {
            thumbnails_writable: true,
        },
        &MatchResult::None,
        Override::PreferCover,
    );
    assert_eq!(c.cde, CdeType::Pdoc);
    assert!(!c.write_thumbnail);
    // features override on mass storage still writes our thumbnail
    let f2 = decide(
        &DeviceClass::MassStorage {
            thumbnails_writable: true,
        },
        &MatchResult::None,
        Override::PreferFeatures,
    );
    assert!(f2.write_thumbnail);
}

#[test]
fn prefer_features_uses_an_unverified_candidate_asin_so_spike_2_can_run() {
    let m = MatchResult::Candidate {
        asin: REAL.into(),
        verified: false,
    };
    let d = decide(&DeviceClass::Mtp, &m, Override::PreferFeatures);
    assert_eq!(d.cde, CdeType::Ebok);
    assert_eq!(d.asin, AsinChoice::Real(REAL.into()));
    // a real ASIN means the Kindle fetches the store cover: no grey-cover warning
    assert!(
        d.reason.contains(REAL) && d.reason.contains("store cover") && !d.reason.contains("grey"),
        "{}",
        d.reason
    );
}

#[test]
fn manual_asins_are_ten_letters_or_digits_uppercased() {
    use deckle_core::cover_rule::manual_asin;
    assert_eq!(manual_asin(" b0fxyz1234 ").as_deref(), Ok("B0FXYZ1234"));
    assert_eq!(manual_asin("1668659662").as_deref(), Ok("1668659662"));
    assert!(manual_asin("B0FXYZ123").is_err());
    assert!(manual_asin("B0FXYZ-1234").is_err());
}
