// SPDX-License-Identifier: GPL-3.0-or-later
use deckle_digest::config::{self, Config, ExportConfig, ExportFormat};
use deckle_digest::press::setup::{
    binding_is_enabled, disable, disconnect_manifest, enable_with_watcher, prepare_binding,
    producer_identity, reconcile_with_watcher, write_manifest,
};
use deckle_digest::press::{PressErrorCode, PressOperation, ProducerManifest, Uuid};
use deckle_digest::schedule::{WatcherService, ensure_watcher_with};
use std::path::Path;

fn paper(root: &Path) -> Config {
    Config {
        paper_id: Some(Uuid::parse_str("ea249ae5-771c-4d2b-9f11-5119fbe3feda").unwrap()),
        export: vec![ExportConfig {
            name: "Shared".into(),
            dir: root.to_string_lossy().into(),
            format: ExportFormat::Bundle,
            ..Default::default()
        }],
        ..Default::default()
    }
}
fn producer() -> Uuid {
    Uuid::parse_str("ba9e81c1-d5e3-43cb-bca1-acaf771caf79").unwrap()
}
fn manifest(c: &Config, enabled: bool) -> ProducerManifest {
    ProducerManifest {
        format: "deckle-press".into(),
        version: 1,
        producer_id: producer(),
        paper_id: c.paper_id.unwrap(),
        paper_name: c.title.clone(),
        enabled,
        operations: vec![PressOperation::FreshFull],
        updated_at: "2026-10-27T16:00:00Z".parse().unwrap(),
    }
}
fn read(root: &Path) -> ProducerManifest {
    serde_json::from_slice(&std::fs::read(root.join(".deckle-press/producer.json")).unwrap())
        .unwrap()
}

#[test]
fn press_setup_old_configuration_is_opted_out_and_round_trips_identity() {
    let old = Config::parse("# preserved\ntitle = \"Morning\"\n[[export]]\nname = \"Shared\"\ndir = \"/shared\"\nformat = \"bundle\"\n").unwrap();
    assert!(!old.export[0].press_enabled);
    assert!(old.paper_id.is_none());
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("digest.toml");
    std::fs::write(&path, "# preserved\ntitle = \"Morning\"\n").unwrap();
    let mut c = old;
    c.paper_id = Some(producer());
    c.export[0].press_enabled = true;
    config::save(&path, &c).unwrap();
    let saved = Config::load(&path).unwrap();
    assert_eq!(saved.paper_id, Some(producer()));
    assert!(saved.export[0].press_enabled);
    assert!(
        std::fs::read_to_string(path)
            .unwrap()
            .starts_with("# preserved")
    );
}

#[test]
fn press_setup_duplicate_canonical_mappings_are_rejected() {
    let dir = tempfile::tempdir().unwrap();
    let c = paper(dir.path());
    let mut other = c.clone();
    other.paper = Some(config::PaperRef {
        slug: "other".into(),
        state_dir: dir.path().join("other"),
    });
    other.paper_id = Some(Uuid::new_v4());
    other.export[0].dir = dir.path().join(".").to_string_lossy().into();
    assert_eq!(
        prepare_binding(&[c.clone(), other], &c, 0, producer())
            .unwrap_err()
            .code,
        PressErrorCode::ProtocolConflict
    );
    let mut duplicate = c.clone();
    duplicate.export.push(ExportConfig {
        name: "Alias".into(),
        ..c.export[0].clone()
    });
    assert!(prepare_binding(&[duplicate.clone()], &duplicate, 0, producer()).is_err());
}

#[cfg(unix)]
#[test]
fn press_setup_symlink_root_alias_cannot_claim_a_second_paper() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path().join("root");
    std::fs::create_dir(&root).unwrap();
    let alias = dir.path().join("alias");
    std::os::unix::fs::symlink(&root, &alias).unwrap();
    let c = paper(&root);
    let mut other = paper(&alias);
    other.paper = Some(config::PaperRef {
        slug: "other".into(),
        state_dir: dir.path().join("other"),
    });
    assert!(prepare_binding(&[c.clone(), other], &c, 0, producer()).is_err());
}

#[cfg(windows)]
#[test]
fn press_setup_windows_case_and_verbatim_aliases_cannot_claim_a_second_paper() {
    let dir = tempfile::tempdir().unwrap();
    let c = paper(dir.path());
    let mut other = c.clone();
    other.paper = Some(config::PaperRef {
        slug: "other".into(),
        state_dir: dir.path().join("other"),
    });
    other.export[0].dir = std::fs::canonicalize(dir.path())
        .unwrap()
        .to_string_lossy()
        .to_uppercase();
    assert!(prepare_binding(&[c.clone(), other], &c, 0, producer()).is_err());
}

#[test]
fn press_setup_rejects_non_bundle_and_missing_root_without_creating_it() {
    let dir = tempfile::tempdir().unwrap();
    let mut c = paper(dir.path());
    c.export[0].format = ExportFormat::Epub;
    assert_eq!(
        prepare_binding(&[], &c, 0, producer()).unwrap_err().code,
        PressErrorCode::Unsupported
    );
    let root = dir.path().join("absent");
    c = paper(&root);
    assert_eq!(
        prepare_binding(&[], &c, 0, producer()).unwrap_err().code,
        PressErrorCode::StorageUnavailable
    );
    assert!(!root.exists());
}

#[cfg(unix)]
#[test]
fn press_setup_rejects_read_only_root() {
    use std::os::unix::fs::PermissionsExt;
    let dir = tempfile::tempdir().unwrap();
    let c = paper(dir.path());
    std::fs::set_permissions(dir.path(), std::fs::Permissions::from_mode(0o500)).unwrap();
    let result = prepare_binding(&[], &c, 0, producer());
    std::fs::set_permissions(dir.path(), std::fs::Permissions::from_mode(0o700)).unwrap();
    assert_eq!(result.unwrap_err().code, PressErrorCode::StorageUnavailable);
}

#[test]
fn press_setup_foreign_manifest_survives_failed_activation_and_explicit_reconnect() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path().join("shared");
    std::fs::create_dir(&root).unwrap();
    let mut c = paper(&root);
    let path = dir.path().join("digest.toml");
    let binding = prepare_binding(&[], &c, 0, producer()).unwrap();
    let mut foreign = manifest(&c, true);
    foreign.producer_id = Uuid::new_v4();
    std::fs::create_dir(root.join(".deckle-press")).unwrap();
    std::fs::write(
        root.join(".deckle-press/producer.json"),
        serde_json::to_vec(&foreign).unwrap(),
    )
    .unwrap();
    assert!(
        enable_with_watcher(&[], &mut c, 0, producer(), &path, || panic!(
            "foreign ownership must precede watcher setup"
        ))
        .is_err()
    );
    assert_eq!(read(&root), foreign);
    assert!(!Config::load(&path).unwrap().export[0].press_enabled);
    assert!(write_manifest(&binding, &manifest(&c, true)).is_err());
    let mut stale = foreign.clone();
    stale.enabled = false;
    assert!(disconnect_manifest(&root, &stale).is_err());
    disconnect_manifest(&root, &foreign).unwrap();
    enable_with_watcher(&[], &mut c, 0, producer(), &path, || Ok(())).unwrap();
    assert_eq!(read(&root).producer_id, producer());
}

#[test]
fn press_setup_watcher_failure_and_interrupted_activation_fail_closed() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path().join("shared");
    std::fs::create_dir(&root).unwrap();
    let mut c = paper(&root);
    c.paper_id = None;
    let path = dir.path().join("digest.toml");
    assert!(
        enable_with_watcher(&[], &mut c, 0, producer(), &path, || Err(
            "install failed".into()
        ))
        .is_err()
    );
    assert!(c.paper_id.is_some());
    assert!(!Config::load(&path).unwrap().export[0].press_enabled);
    assert!(!root.join(".deckle-press/producer.json").exists());
    let binding = prepare_binding(&[], &c, 0, producer()).unwrap();
    write_manifest(&binding, &manifest(&c, true)).unwrap(); // crash before local enable
    assert!(!binding_is_enabled(&[], &c, 0, producer()));
    reconcile_with_watcher(&[], &mut c, 0, producer(), &path, || {
        panic!("disabled setup cannot install watcher")
    })
    .unwrap();
    assert!(!read(&root).enabled);
}

#[test]
fn press_setup_enable_rename_disable_and_recovery_preserve_schedule() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path().join("shared");
    std::fs::create_dir(&root).unwrap();
    let mut c = paper(&root);
    let path = dir.path().join("digest.toml");
    c.schedule.at = Some("05:45".into());
    c.schedule.days = vec![config::Day::Mon, config::Day::Fri];
    let schedule = c.schedule.clone();
    let id = c.paper_id;
    enable_with_watcher(&[], &mut c, 0, producer(), &path, || {
        assert!(!Config::load(&path).unwrap().export[0].press_enabled);
        assert!(!root.join(".deckle-press/producer.json").exists());
        Ok(())
    })
    .unwrap();
    assert!(binding_is_enabled(&[], &c, 0, producer()));
    c.title = "Renamed".into();
    reconcile_with_watcher(&[], &mut c, 0, producer(), &path, || Ok(())).unwrap();
    assert_eq!(read(&root).paper_name, "Renamed");
    assert_eq!(c.paper_id, id);
    assert!(
        reconcile_with_watcher(&[], &mut c, 0, producer(), &path, || Err(
            "start failed".into()
        ))
        .is_err()
    );
    assert!(!read(&root).enabled);
    assert!(!c.export[0].press_enabled);
    enable_with_watcher(&[], &mut c, 0, producer(), &path, || Ok(())).unwrap();
    disable(&[], &mut c, 0, producer(), &path).unwrap();
    assert!(!read(&root).enabled);
    assert!(!binding_is_enabled(&[], &c, 0, producer()));
    assert_eq!(Config::load(&path).unwrap().schedule, schedule);
}

#[test]
fn press_setup_disabled_manifest_blocks_locally_enabled_config() {
    let dir = tempfile::tempdir().unwrap();
    let mut c = paper(dir.path());
    c.export[0].press_enabled = true;
    let binding = prepare_binding(&[], &c, 0, producer()).unwrap();
    write_manifest(&binding, &manifest(&c, false)).unwrap();
    assert!(!binding_is_enabled(&[], &c, 0, producer()));
}

#[test]
fn press_setup_producer_identity_is_stable_and_corruption_is_not_replaced() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("press-identity.json");
    let id = producer_identity(&path).unwrap();
    assert_eq!(producer_identity(&path).unwrap(), id);
    std::fs::write(&path, "invalid").unwrap();
    assert!(producer_identity(&path).is_err());
    assert_eq!(std::fs::read_to_string(path).unwrap(), "invalid");
}

#[cfg(unix)]
#[test]
fn press_setup_rejects_symlink_controls_without_touching_target() {
    let dir = tempfile::tempdir().unwrap();
    let outside = tempfile::tempdir().unwrap();
    let c = paper(dir.path());
    let binding = prepare_binding(&[], &c, 0, producer()).unwrap();
    std::os::unix::fs::symlink(outside.path(), dir.path().join(".deckle-press")).unwrap();
    assert!(write_manifest(&binding, &manifest(&c, true)).is_err());
    assert!(!outside.path().join("producer.json").exists());
}

#[derive(Default)]
struct Service {
    installed: bool,
    fail: bool,
    calls: Vec<String>,
}
impl WatcherService for Service {
    fn matches(&mut self, exe: &Path, config: &Path) -> Result<bool, String> {
        assert_eq!(exe, Path::new("/deckle"));
        assert_eq!(config, Path::new("/config.toml"));
        Ok(self.installed)
    }
    fn install(&mut self, _: &Path, _: &Path) -> Result<(), String> {
        self.calls.push("install watcher".into());
        if self.fail {
            Err("failed".into())
        } else {
            self.installed = true;
            Ok(())
        }
    }
    fn start(&mut self) -> Result<(), String> {
        self.calls.push("start watcher".into());
        Ok(())
    }
}
#[test]
fn press_setup_watcher_is_reused_and_install_failure_is_returned() {
    let mut s = Service::default();
    ensure_watcher_with(Path::new("/deckle"), Path::new("/config.toml"), &mut s).unwrap();
    ensure_watcher_with(Path::new("/deckle"), Path::new("/config.toml"), &mut s).unwrap();
    assert_eq!(
        s.calls,
        ["install watcher", "start watcher", "start watcher"]
    );
    let mut s = Service {
        fail: true,
        ..Default::default()
    };
    assert!(ensure_watcher_with(Path::new("/deckle"), Path::new("/config.toml"), &mut s).is_err());
    assert_eq!(s.calls, ["install watcher"]);
}

#[test]
fn press_setup_legacy_opt_out_reconciles_without_creating_identity_or_folder() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path().join("absent");
    let mut c = paper(&root);
    c.paper_id = None;
    reconcile_with_watcher(
        &[],
        &mut c,
        0,
        producer(),
        &dir.path().join("digest.toml"),
        || panic!("legacy opt-out cannot install watcher"),
    )
    .unwrap();
    assert!(c.paper_id.is_none());
    assert!(!root.exists());
}

#[cfg(unix)]
#[test]
fn press_setup_replaced_root_cannot_redirect_manifest_publication() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path().join("shared");
    std::fs::create_dir(&root).unwrap();
    let outside = tempfile::tempdir().unwrap();
    let c = paper(&root);
    let binding = prepare_binding(&[], &c, 0, producer()).unwrap();
    std::fs::rename(&root, dir.path().join("original")).unwrap();
    std::os::unix::fs::symlink(outside.path(), &root).unwrap();
    assert!(write_manifest(&binding, &manifest(&c, true)).is_err());
    assert!(!outside.path().join(".deckle-press").exists());
}

#[test]
fn press_setup_disable_persists_opt_out_when_folder_disappears() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path().join("shared");
    std::fs::create_dir(&root).unwrap();
    let mut c = paper(&root);
    let path = dir.path().join("digest.toml");
    enable_with_watcher(&[], &mut c, 0, producer(), &path, || Ok(())).unwrap();
    std::fs::rename(&root, dir.path().join("offline")).unwrap();
    assert_eq!(
        disable(&[], &mut c, 0, producer(), &path).unwrap_err().code,
        PressErrorCode::StorageUnavailable
    );
    assert!(!c.export[0].press_enabled);
    assert!(!Config::load(&path).unwrap().export[0].press_enabled);
}

#[test]
fn press_setup_storage_failure_after_watcher_cannot_enable_local_config() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path().join("shared");
    std::fs::create_dir(&root).unwrap();
    let mut c = paper(&root);
    let path = dir.path().join("digest.toml");
    let result = enable_with_watcher(&[], &mut c, 0, producer(), &path, || {
        std::fs::rename(&root, dir.path().join("offline")).unwrap();
        Ok(())
    });
    assert!(result.is_err());
    assert!(!c.export[0].press_enabled);
    assert!(!Config::load(&path).unwrap().export[0].press_enabled);
}

#[test]
fn press_setup_failed_final_config_commit_retracts_advertisement() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path().join("shared");
    std::fs::create_dir(&root).unwrap();
    let mut c = paper(&root);
    let path = dir.path().join("digest.toml");
    let result = enable_with_watcher(&[], &mut c, 0, producer(), &path, || {
        std::fs::create_dir(path.with_extension("toml.tmp")).unwrap();
        Ok(())
    });
    assert!(result.is_err());
    assert!(!c.export[0].press_enabled);
    assert!(!Config::load(&path).unwrap().export[0].press_enabled);
    assert!(!read(&root).enabled);
}

#[test]
fn press_setup_foreign_ownership_arriving_during_watcher_setup_is_preserved() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path().join("shared");
    std::fs::create_dir(&root).unwrap();
    let mut c = paper(&root);
    let path = dir.path().join("digest.toml");
    let mut foreign = manifest(&c, true);
    foreign.paper_id = Uuid::new_v4();
    assert!(
        enable_with_watcher(&[], &mut c, 0, producer(), &path, || {
            std::fs::create_dir(root.join(".deckle-press")).unwrap();
            std::fs::write(
                root.join(".deckle-press/producer.json"),
                serde_json::to_vec(&foreign).unwrap(),
            )
            .unwrap();
            Ok(())
        })
        .is_err()
    );
    assert_eq!(read(&root), foreign);
    assert!(!c.export[0].press_enabled);
}

#[test]
fn press_setup_malformed_and_oversized_manifests_are_never_replaced() {
    let dir = tempfile::tempdir().unwrap();
    let c = paper(dir.path());
    let binding = prepare_binding(&[], &c, 0, producer()).unwrap();
    std::fs::create_dir(dir.path().join(".deckle-press")).unwrap();
    let path = dir.path().join(".deckle-press/producer.json");
    for bytes in [b"invalid".to_vec(), vec![b' '; 16_385]] {
        std::fs::write(&path, &bytes).unwrap();
        assert!(write_manifest(&binding, &manifest(&c, true)).is_err());
        assert_eq!(std::fs::read(&path).unwrap(), bytes);
    }
}

#[test]
fn press_setup_transaction_propagates_injected_service_install_failure() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path().join("shared");
    std::fs::create_dir(&root).unwrap();
    let mut c = paper(&root);
    let path = dir.path().join("digest.toml");
    let mut service = Service {
        fail: true,
        ..Default::default()
    };
    assert!(
        enable_with_watcher(&[], &mut c, 0, producer(), &path, || {
            ensure_watcher_with(
                Path::new("/deckle"),
                Path::new("/config.toml"),
                &mut service,
            )
        })
        .is_err()
    );
    assert!(!Config::load(&path).unwrap().export[0].press_enabled);
    assert!(!root.join(".deckle-press/producer.json").exists());
    service.fail = false;
    enable_with_watcher(&[], &mut c, 0, producer(), &path, || {
        ensure_watcher_with(
            Path::new("/deckle"),
            Path::new("/config.toml"),
            &mut service,
        )
    })
    .unwrap();
    assert!(binding_is_enabled(&[], &c, 0, producer()));
    assert!(read(&root).enabled);
}
