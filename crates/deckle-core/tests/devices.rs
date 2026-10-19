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
