// SPDX-License-Identifier: GPL-3.0-or-later
//! Explicit setup and the app's config-save boundary. Shared paths always come from disk.
use deckle_digest::{
    Config,
    config::ExportFormat,
    press::{PressErrorCode, Uuid, setup},
};
use serde::{Deserialize, Serialize};
use std::path::Path;

use crate::config_write;
use crate::daily::paper_config_at;

#[derive(Clone, Debug, Serialize)]
pub struct PressSetupStatus {
    pub enabled: bool,
    pub watcher_ready: bool,
    pub error: Option<PressErrorCode>,
    pub producer_id: Option<Uuid>,
    pub paper_id: Option<Uuid>,
}

/// A stale-row precondition, never authority to choose a filesystem path.
#[derive(Clone, Debug, Deserialize)]
pub struct ExpectedExport {
    pub dir: String,
    pub format: ExportFormat,
}

fn identity(root: &Path) -> Result<Uuid, String> {
    let mut builder = std::fs::DirBuilder::new();
    builder.recursive(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::DirBuilderExt;
        builder.mode(0o700);
    }
    builder
        .create(root)
        .map_err(|_| "Private request storage is unavailable.".to_owned())?;
    setup::producer_identity(&root.join("producer.json")).map_err(|_| {
        "Private request identity is unavailable. Restore it before reconnecting.".into()
    })
}

/// `exe` is the app's own binary, which the watcher job runs.
pub fn press_setup(
    paper: Option<String>,
    export_index: usize,
    enabled: bool,
    expected: Option<ExpectedExport>,
    exe: &Path,
) -> Result<PressSetupStatus, String> {
    let main = deckle_digest::config::default_path();
    setup_at(
        &main,
        &setup::private_root(),
        paper.as_deref(),
        export_index,
        enabled,
        expected.as_ref(),
        || deckle_digest::schedule::ensure_watcher(exe, &main),
    )
}

pub fn setup_at(
    main: &Path,
    root: &Path,
    paper: Option<&str>,
    index: usize,
    enabled: bool,
    expected: Option<&ExpectedExport>,
    watcher: impl FnOnce() -> Result<(), String>,
) -> Result<PressSetupStatus, String> {
    let _lock = config_write()?;
    let mut config = paper_config_at(main, paper)
        .map_err(|_| "Could not read this paper's settings.".to_owned())?;
    let export = config
        .export
        .get(index)
        .ok_or("Folder changed. Reopen Delivery and try again.")?;
    if expected.is_some_and(|e| e.dir != export.dir || e.format != export.format) {
        return Err("Folder changed. Reopen Delivery and try again.".into());
    }
    if export.format != ExportFormat::Bundle {
        return Err("Fresh-paper requests require a bundle folder.".into());
    }
    let configs = deckle_digest::papers::load_all(main)
        .map_err(|_| "Could not read paper settings.".to_owned())?
        .papers;
    // Do not create private state inside a configured shared root (or the reverse).
    let private = root.canonicalize().unwrap_or_else(|_| root.to_path_buf());
    for export in configs.iter().flat_map(|c| &c.export) {
        if let Ok(shared) = deckle_digest::inbox::expand_home(&export.dir).canonicalize()
            && (shared.starts_with(&private) || private.starts_with(&shared))
        {
            return Err("Shared folders must be separate from private request storage.".into());
        }
    }
    let producer = identity(root)?;
    let path = deckle_digest::papers::file_of(main, &config);
    let result = if enabled {
        setup::enable_with_watcher(&configs, &mut config, index, producer, &path, watcher)
            .map(|_| ())
    } else {
        setup::disable(&configs, &mut config, index, producer, &path)
    };
    let error = result.err().map(|e| e.code);
    let saved_enabled = paper_config_at(main, paper)
        .ok()
        .and_then(|c| c.export.get(index).map(|e| e.press_enabled))
        .unwrap_or(false);
    Ok(PressSetupStatus {
        enabled: saved_enabled,
        watcher_ready: error.is_none() && enabled,
        error,
        producer_id: Some(producer),
        paper_id: config.paper_id,
    })
}

/// Called while holding CONFIG_WRITE, before any replacement config is committed.
/// Saving settings can retain an owned activation, but only setup may create one.
pub fn guard_save(main: &Path, root: &Path, old: &Config, next: &mut Config) -> Result<(), String> {
    next.paper_id = old.paper_id;
    let same = |a: &deckle_digest::config::ExportConfig,
                b: &deckle_digest::config::ExportConfig| {
        a.dir == b.dir && a.format == b.format
    };
    for export in next.export.iter().filter(|e| e.press_enabled) {
        if !old
            .export
            .iter()
            .any(|e| e.press_enabled && same(e, export))
        {
            return Err("Use Fresh-paper requests setup to enable this folder.".into());
        }
    }
    let changed_bundle = old.paper_id.is_some()
        && old
            .export
            .iter()
            .any(|e| e.format == ExportFormat::Bundle && !next.export.iter().any(|n| same(e, n)));
    if !old.export.iter().any(|e| e.press_enabled) && !changed_bundle {
        return Ok(());
    }
    if !root.join("producer.json").is_file() {
        return Err("Request identity is unavailable. Restore it before changing folders.".into());
    }
    let producer = setup::producer_identity(&root.join("producer.json")).map_err(|_| {
        "Request identity is unavailable. Restore it before changing folders.".to_owned()
    })?;
    let configs = deckle_digest::papers::load_all(main)?.papers;
    let mut proposed = configs.clone();
    proposed.retain(|c| c.slug() != old.slug());
    proposed.push(next.clone());
    for (index, export) in next
        .export
        .iter()
        .enumerate()
        .filter(|(_, e)| e.press_enabled)
    {
        if !setup::binding_is_enabled(&proposed, next, index, producer) {
            return Err(format!(
                "Fresh-paper setup for {} needs attention. Turn requests off or retry setup.",
                export.name
            ));
        }
    }
    let path = deckle_digest::papers::file_of(main, old);
    let mut current = old.clone();
    for (index, export) in old.export.iter().enumerate() {
        if next
            .export
            .iter()
            .any(|e| e.press_enabled && same(e, export))
        {
            continue;
        }
        let changed = !next.export.iter().any(|e| same(e, export));
        // A failed previous disconnect has already opted out locally. Keep the old path
        // until its shared advertisement can be checked and revoked on a later retry.
        let mut advertised = old.clone();
        advertised.export[index].press_enabled = true;
        let maybe_owned = export.format == ExportFormat::Bundle && changed;
        if maybe_owned && !deckle_digest::inbox::expand_home(&export.dir).is_dir() {
            if export.press_enabled {
                let _ = setup::disable(&configs, &mut current, index, producer, &path);
            }
            return Err("Old request folder is unavailable. Reconnect the folder and retry before removing or changing it.".into());
        }
        if export.press_enabled
            || (maybe_owned && setup::binding_is_enabled(&configs, &advertised, index, producer))
        {
            setup::disable(&configs, &mut current, index, producer, &path).map_err(|_| "Could not disconnect the old request folder. Check local storage, reconnect the folder, and retry.".to_owned())?;
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use deckle_digest::{
        Config,
        config::{ExportConfig, ExportFormat},
        press::Uuid,
    };
    use std::path::PathBuf;
    fn fixture() -> (tempfile::TempDir, PathBuf, PathBuf) {
        let dir = tempfile::tempdir().unwrap();
        let main = dir.path().join("digest.toml");
        let root = dir.path().join("private");
        let folder = dir.path().join("shared");
        std::fs::create_dir(&folder).unwrap();
        let cfg = Config {
            export: vec![ExportConfig {
                name: "Paper".into(),
                dir: folder.display().to_string(),
                format: ExportFormat::Bundle,
                ..Default::default()
            }],
            ..Default::default()
        };
        deckle_digest::config::save(&main, &cfg).unwrap();
        (dir, main, root)
    }
    #[test]
    fn press_setup_serializes_concurrent_operations() {
        let (_dir, main, root) = fixture();
        let (entered_tx, entered_rx) = std::sync::mpsc::channel();
        let (release_tx, release_rx) = std::sync::mpsc::channel();
        let first_main = main.clone();
        let first_root = root.clone();
        let first = std::thread::spawn(move || {
            setup_at(&first_main, &first_root, None, 0, true, None, || {
                entered_tx.send(()).unwrap();
                release_rx.recv().unwrap();
                Ok(())
            })
            .unwrap()
        });
        entered_rx
            .recv_timeout(std::time::Duration::from_secs(5))
            .unwrap();
        assert!(crate::CONFIG_WRITE.try_lock().is_err());
        let (done_tx, done_rx) = std::sync::mpsc::channel();
        let second = std::thread::spawn(move || {
            let status = setup_at(&main, &root, None, 0, false, None, || {
                panic!("disable never installs")
            })
            .unwrap();
            done_tx.send(status).unwrap();
        });
        assert!(
            done_rx
                .recv_timeout(std::time::Duration::from_millis(30))
                .is_err()
        );
        release_tx.send(()).unwrap();
        assert!(first.join().unwrap().enabled);
        let disabled = done_rx
            .recv_timeout(std::time::Duration::from_secs(5))
            .unwrap();
        assert!(!disabled.enabled && !disabled.watcher_ready && disabled.error.is_none());
        second.join().unwrap();
    }

    #[test]
    fn press_setup_format_change_disables_old_manifest_without_enabling_replacement() {
        let (_dir, main, root) = fixture();
        setup_at(&main, &root, None, 0, true, None, || Ok(())).unwrap();
        let old = Config::load(&main).unwrap();
        let mut next = old.clone();
        next.export[0].format = ExportFormat::Pdf;
        next.export[0].press_enabled = false;
        guard_save(&main, &root, &old, &mut next).unwrap();
        deckle_digest::papers::save(&main, &next).unwrap();
        let manifest: serde_json::Value = serde_json::from_slice(
            &std::fs::read(
                std::path::Path::new(&old.export[0].dir).join(".deckle-press/producer.json"),
            )
            .unwrap(),
        )
        .unwrap();
        assert_eq!(manifest["enabled"], false);
        assert!(!Config::load(&main).unwrap().export[0].press_enabled);
    }

    #[test]
    fn press_setup_opted_out_preferences_do_not_require_private_state() {
        let (_dir, main, root) = fixture();
        let mut old = Config::load(&main).unwrap();
        old.paper_id = Some(Uuid::new_v4());
        let mut next = old.clone();
        next.title = "Updated title".into();
        guard_save(&main, &root, &old, &mut next).unwrap();
        assert!(!root.exists());
    }

    #[test]
    fn press_setup_lost_identity_is_not_recreated_by_autosave() {
        let (_dir, main, root) = fixture();
        setup_at(&main, &root, None, 0, true, None, || Ok(())).unwrap();
        std::fs::remove_file(root.join("producer.json")).unwrap();
        let old = Config::load(&main).unwrap();
        let mut next = old.clone();
        assert!(guard_save(&main, &root, &old, &mut next).is_err());
        assert!(!root.join("producer.json").exists());
    }

    #[test]
    fn press_setup_failure_is_safe_and_never_ready() {
        let (_dir, main, root) = fixture();
        let status = setup_at(&main, &root, None, 0, true, None, || {
            Err("secret diagnostic".into())
        })
        .unwrap();
        assert!(!status.enabled && !status.watcher_ready);
        assert!(status.error.is_some());
        assert!(
            !serde_json::to_string(&status)
                .unwrap()
                .contains("secret diagnostic")
        );
        assert!(!Config::load(&main).unwrap().export[0].press_enabled);
    }
    #[test]
    fn press_setup_raw_autosave_cannot_activate() {
        let (_dir, main, root) = fixture();
        let old = Config::load(&main).unwrap();
        let mut next = old.clone();
        next.export[0].press_enabled = true;
        assert!(guard_save(&main, &root, &old, &mut next).is_err());
        assert!(!root.exists());
        assert!(!Config::load(&main).unwrap().export[0].press_enabled);
    }
    #[test]
    fn press_setup_owned_binding_survives_save_and_old_folder_is_disabled_first() {
        let (_dir, main, root) = fixture();
        let status = setup_at(&main, &root, None, 0, true, None, || Ok(())).unwrap();
        assert!(status.enabled && status.watcher_ready);
        let old = Config::load(&main).unwrap();
        let mut next = old.clone();
        next.paper_id = Some(Uuid::new_v4());
        next.title = "Renamed".into();
        guard_save(&main, &root, &old, &mut next).unwrap();
        assert_eq!(next.paper_id, old.paper_id);
        next.export.clear();
        guard_save(&main, &root, &old, &mut next).unwrap();
        let bytes = std::fs::read(
            std::path::Path::new(&old.export[0].dir).join(".deckle-press/producer.json"),
        )
        .unwrap();
        let manifest: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
        assert_eq!(manifest["enabled"], false);
    }
    #[test]
    fn press_setup_stale_index_and_foreign_producer_fail_closed() {
        let (_dir, main, root) = fixture();
        let expected = ExpectedExport {
            dir: "/wrong".into(),
            format: ExportFormat::Bundle,
        };
        assert!(
            setup_at(&main, &root, None, 0, true, Some(&expected), || panic!(
                "must not install"
            ))
            .is_err()
        );
        setup_at(&main, &root, None, 0, true, None, || Ok(())).unwrap();
        let foreign_root = root.with_file_name("other-private");
        let status = setup_at(&main, &foreign_root, None, 0, true, None, || {
            panic!("foreign must not install")
        })
        .unwrap();
        assert!(!status.enabled && !status.watcher_ready);
        assert_eq!(
            status.error,
            Some(deckle_digest::press::PressErrorCode::ProtocolConflict)
        );
    }
    #[test]
    fn press_setup_offline_old_folder_cannot_be_forgotten_by_retrying_save() {
        let (_dir, main, root) = fixture();
        setup_at(&main, &root, None, 0, true, None, || Ok(())).unwrap();
        let old = Config::load(&main).unwrap();
        std::fs::rename(&old.export[0].dir, format!("{}-offline", old.export[0].dir)).unwrap();
        let mut next = old.clone();
        next.export.clear();
        assert!(guard_save(&main, &root, &old, &mut next).is_err());
        let old = Config::load(&main).unwrap();
        assert!(!old.export[0].press_enabled);
        assert!(guard_save(&main, &root, &old, &mut next).is_err());
        assert_eq!(Config::load(&main).unwrap().export.len(), 1);
    }
}
