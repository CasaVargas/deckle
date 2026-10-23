// SPDX-License-Identifier: GPL-3.0-or-later
use deckle_core::cover_rule::{DeviceClass, Override};
use deckle_core::devices::{DeviceProfile, DeviceRegistry, Seen, TransportKind};

fn seen_mass(root: &str, hint: &str, thumbs: bool) -> Seen {
    Seen {
        key: format!("ms:{hint}"),
        root: Some(root.into()),
        model_hint: hint.into(),
        transport: TransportKind::MassStorage,
        thumbnails_writable: thumbs,
        serial: None,
        firmware: None,
    }
}

#[test]
fn registry_remembers_devices_and_keeps_user_overrides() {
    let dir = tempfile::tempdir().unwrap();
    let file = dir.path().join("devices.json");
    let mut reg = DeviceRegistry::load(&file);
    assert!(reg.profiles.is_empty());
    let p = reg.remember(&seen_mass("/Volumes/Kindle", "Kindle 5.16.2.1.1", true));
    assert_eq!(p.key, "ms:Kindle 5.16.2.1.1");
    assert_eq!(p.transport, TransportKind::MassStorage);
    assert!(p.thumbnails_writable);
    assert!(p.first_seen.len() >= 10);
    reg.set_override(&p.key, Some("pdoc"));
    reg.set_verified_real_asin(&p.key, true);
    reg.save(&file).unwrap();
    let mut again = DeviceRegistry::load(&file);
    let p2 = again.remember(&seen_mass("/Volumes/Kindle", "Kindle 5.16.2.1.1", false));
    assert_eq!(
        p2.cde_override.as_deref(),
        Some("pdoc"),
        "override survives"
    );
    assert!(p2.verified_real_asin);
    assert!(
        !p2.thumbnails_writable,
        "live facts refresh on every sighting"
    );
    assert_eq!(again.profiles.len(), 1);
    assert_eq!(p2.first_seen, p.first_seen);
    assert_ne!(p2.last_seen, "");
}

#[test]
fn profile_maps_to_the_cover_rule_inputs() {
    let p = DeviceProfile {
        transport: TransportKind::Mtp,
        thumbnails_writable: false,
        cde_override: Some("ebok".into()),
        ..DeviceProfile::new("mtp:G0X1", "Kindle Scribe")
    };
    assert_eq!(p.device_class(), DeviceClass::Mtp);
    assert_eq!(p.user_override(), Override::PreferFeatures);
    let m = DeviceProfile {
        transport: TransportKind::MassStorage,
        thumbnails_writable: true,
        ..DeviceProfile::new("ms:PW", "Paperwhite")
    };
    assert_eq!(
        m.device_class(),
        DeviceClass::MassStorage {
            thumbnails_writable: true
        }
    );
    assert_eq!(m.user_override(), Override::None);
}

#[test]
fn firmware_round_trips_and_an_mtp_sighting_keeps_the_recorded_one() {
    let dir = tempfile::tempdir().unwrap();
    let file = dir.path().join("devices.json");
    let mut reg = DeviceRegistry::default();
    let ms = Seen {
        firmware: Some("5.16.2.1.1".into()),
        ..seen_mass("/Volumes/Kindle", "Kindle 5.16.2.1.1", true)
    };
    assert_eq!(reg.remember(&ms).firmware.as_deref(), Some("5.16.2.1.1"));
    let mtp = Seen {
        key: "mtp:G0X1".into(),
        root: None,
        model_hint: "Kindle Scribe".into(),
        transport: TransportKind::Mtp,
        thumbnails_writable: false,
        serial: Some("G0X1".into()),
        firmware: None,
    };
    reg.remember(&mtp);
    reg.set_firmware("mtp:G0X1", "5.18.1.0.1");
    reg.save(&file).unwrap();
    let mut again = DeviceRegistry::load(&file);
    assert_eq!(
        again
            .get("ms:Kindle 5.16.2.1.1")
            .unwrap()
            .firmware
            .as_deref(),
        Some("5.16.2.1.1")
    );
    let p = again.remember(&mtp);
    assert_eq!(
        p.firmware.as_deref(),
        Some("5.18.1.0.1"),
        "a sighting without firmware keeps the one a send recorded"
    );
}

#[test]
fn a_devices_json_from_before_firmware_still_loads() {
    let dir = tempfile::tempdir().unwrap();
    let file = dir.path().join("devices.json");
    std::fs::write(
        &file,
        r#"{"profiles":[{"key":"mtp:G0X1","model_hint":"Kindle Scribe","transport":"mtp",
        "thumbnails_writable":false,"serial":"G0X1","last_root":null,
        "first_seen":"2026-10-19T00:00:00Z","last_seen":"2026-10-19T00:00:00Z",
        "cde_override":"ebok","verified_real_asin":true}]}"#,
    )
    .unwrap();
    let reg = DeviceRegistry::load(&file);
    let p = reg.get("mtp:G0X1").expect("old profile loads");
    assert_eq!(p.firmware, None);
    assert!(p.verified_real_asin);
}

/// A push finds the connected Kindle's profile by the key the app records it under.
#[test]
fn a_target_finds_the_profile_the_app_recorded() {
    use deckle_core::deliver::{Device, Target};
    use deckle_core::devices::{profile_for, seen_mass_storage, seen_mtp};
    use deckle_transport::usb::MtpInfo;
    let device = Device {
        root: "/Volumes/Kindle".into(),
        model_hint: "Kindle 5.16.2.1.1 (4184940002)".into(),
        thumbnails_writable: true,
        firmware: Some("5.16.2.1.1".into()),
        serial: None,
    };
    let mtp = MtpInfo {
        vendor_id: deckle_transport::usb::AMAZON_VENDOR_ID,
        product_id: 1,
        serial: Some("G0X1".into()),
        product: Some("Kindle Scribe".into()),
        bus: 1,
        address: 2,
    };
    let mut reg = DeviceRegistry::default();
    reg.remember(&seen_mass_storage(&device));
    reg.remember(&seen_mtp(&mtp));
    reg.set_override("mtp:G0X1", Some("ebok"));
    let p = profile_for(&Target::Mtp(mtp.clone()), &reg).expect("MTP profile");
    assert_eq!(p.cde_override.as_deref(), Some("ebok"));
    assert_eq!(seen_mtp(&mtp).transport, TransportKind::Mtp);
    assert_eq!(
        profile_for(&Target::MassStorage(device), &reg).map(|p| p.key),
        Some("ms:Kindle 5.16.2.1.1 (4184940002)".into())
    );
    assert!(profile_for(&Target::Folder("/tmp".into()), &reg).is_none());
    let unknown = MtpInfo {
        serial: None,
        ..mtp
    };
    assert!(profile_for(&Target::Mtp(unknown), &reg).is_none());
}
