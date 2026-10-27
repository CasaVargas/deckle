// SPDX-License-Identifier: GPL-3.0-or-later
//! Explicit, fail-closed ownership of a configured bundle export. Shared files advertise
//! configuration, not liveness; concurrent setup by different producers is unsupported.
use super::{
    MAX_CONTROL_BYTES, PressBinding, PressError, PressErrorCode, PressOperation, ProducerManifest,
    Uuid, decode_manifest,
};
use crate::config::{self, Config, ExportFormat};
use chrono::Utc;
use serde::{Deserialize, Serialize};
use std::fs::{self, OpenOptions};
use std::io::{Read, Write};
use std::path::{Path, PathBuf};

fn storage() -> PressError {
    PressError::new(PressErrorCode::StorageUnavailable)
}
fn conflict() -> PressError {
    PressError::new(PressErrorCode::ProtocolConflict)
}
fn unsupported() -> PressError {
    PressError::new(PressErrorCode::Unsupported)
}

/// Get or create this installation's local identity. Corrupt existing identities fail closed;
/// never put this file in the shared folder or regenerate it silently after a reinstall.
pub fn producer_identity(path: &Path) -> Result<Uuid, PressError> {
    #[derive(Serialize, Deserialize)]
    struct Identity {
        producer_id: Uuid,
    }
    match OpenOptions::new().write(true).create_new(true).open(path) {
        Ok(mut f) => {
            let id = Uuid::new_v4();
            f.write_all(&serde_json::to_vec(&Identity { producer_id: id }).map_err(|_| storage())?)
                .map_err(|_| storage())?;
            f.sync_all().map_err(|_| storage())?;
            Ok(id)
        }
        Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => {
            let bytes = bounded_regular(path)?;
            let identity: Identity = serde_json::from_slice(&bytes).map_err(|_| unsupported())?;
            if identity.producer_id.is_nil() {
                return Err(unsupported());
            }
            Ok(identity.producer_id)
        }
        Err(_) => Err(storage()),
    }
}

fn root_path(dir: &str) -> PathBuf {
    crate::inbox::expand_home(dir.trim())
}

/// Resolve filesystem aliases and validate a single local mapping. The root must already
/// exist: an unavailable synced mount must never be replaced with a new local directory.
pub fn prepare_binding(
    configs: &[Config],
    paper: &Config,
    export_index: usize,
    producer_id: Uuid,
) -> Result<PressBinding, PressError> {
    let binding = resolve_binding(configs, paper, export_index, producer_id)?;
    check_write_access(&binding.root)?;
    Ok(binding)
}

fn resolve_binding(
    configs: &[Config],
    paper: &Config,
    export_index: usize,
    producer_id: Uuid,
) -> Result<PressBinding, PressError> {
    let export = paper.export.get(export_index).ok_or_else(unsupported)?;
    if export.format != ExportFormat::Bundle || producer_id.is_nil() {
        return Err(unsupported());
    }
    let paper_id = paper
        .paper_id
        .filter(|id| !id.is_nil())
        .ok_or_else(unsupported)?;
    let root = fs::canonicalize(root_path(&export.dir)).map_err(|_| storage())?;
    if !root.is_dir() {
        return Err(storage());
    }
    let mut skipped_self = false;
    for config in configs {
        for (i, candidate) in config.export.iter().enumerate() {
            if !skipped_self && config.slug() == paper.slug() && i == export_index {
                skipped_self = true;
                continue;
            }
            if fs::canonicalize(root_path(&candidate.dir)).is_ok_and(|p| p == root) {
                return Err(conflict());
            }
        }
    }
    // Also validate siblings when the caller omitted the selected paper from configs.
    for (i, candidate) in paper.export.iter().enumerate() {
        if i != export_index && fs::canonicalize(root_path(&candidate.dir)).is_ok_and(|p| p == root)
        {
            return Err(conflict());
        }
    }
    Ok(PressBinding {
        root,
        producer_id,
        paper_id,
    })
}

fn check_write_access(root: &Path) -> Result<(), PressError> {
    let probe = root.join(format!(".deckle-write-check-{}", Uuid::new_v4()));
    let mut file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&probe)
        .map_err(|_| storage())?;
    let checked = file
        .write_all(b"write check")
        .and_then(|()| file.sync_all());
    drop(file);
    let removed = fs::remove_file(probe);
    checked.and(removed).map_err(|_| storage())?;
    Ok(())
}

fn control_dir(root: &Path, create: bool) -> Result<PathBuf, PressError> {
    if !root.is_dir() {
        return Err(storage());
    }
    let path = root.join(".deckle-press");
    if create {
        match fs::create_dir(&path) {
            Ok(()) => {}
            Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => {}
            Err(_) => return Err(storage()),
        }
    }
    match fs::symlink_metadata(&path) {
        Ok(meta) if meta.file_type().is_dir() => Ok(path),
        Ok(_) => Err(conflict()),
        Err(_) => Err(storage()),
    }
}

fn bounded_regular(path: &Path) -> Result<Vec<u8>, PressError> {
    let meta = fs::symlink_metadata(path).map_err(|_| storage())?;
    if !meta.file_type().is_file() {
        return Err(conflict());
    }
    if meta.len() > MAX_CONTROL_BYTES as u64 {
        return Err(unsupported());
    }
    let mut bytes = Vec::new();
    fs::File::open(path)
        .map_err(|_| storage())?
        .take((MAX_CONTROL_BYTES + 1) as u64)
        .read_to_end(&mut bytes)
        .map_err(|_| storage())?;
    if bytes.len() > MAX_CONTROL_BYTES {
        return Err(unsupported());
    }
    Ok(bytes)
}

fn owned_manifest(binding: &PressBinding) -> Result<Option<ProducerManifest>, PressError> {
    // The prepared canonical path may have been replaced while OS setup was in progress.
    if fs::canonicalize(&binding.root).map_err(|_| storage())? != binding.root {
        return Err(conflict());
    }
    let dir = binding.root.join(".deckle-press");
    match fs::symlink_metadata(&dir) {
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(_) => return Err(storage()),
        Ok(_) => {
            control_dir(&binding.root, false)?;
        }
    }
    let path = dir.join("producer.json");
    match fs::symlink_metadata(&path) {
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(_) => Err(storage()),
        Ok(_) => decode_manifest(&bounded_regular(&path)?, binding).map(Some),
    }
}

/// Publish an owned manifest atomically. A foreign, malformed, or unsupported manifest is
/// preserved for explicit disconnection; failed setup never silently claims it. The folder
/// must support safe atomic file publication; unsupported storage fails setup closed.
pub fn write_manifest(
    binding: &PressBinding,
    manifest: &ProducerManifest,
) -> Result<(), PressError> {
    write_manifest_using(binding, manifest, |from, to| fs::hard_link(from, to))
}

fn write_manifest_using(
    binding: &PressBinding,
    manifest: &ProducerManifest,
    publish_new: impl FnOnce(&Path, &Path) -> std::io::Result<()>,
) -> Result<(), PressError> {
    let bytes = serde_json::to_vec_pretty(manifest).map_err(|_| unsupported())?;
    decode_manifest(&bytes, binding)?;
    let existing = owned_manifest(binding)?;
    let dir = control_dir(&binding.root, true)?;
    let path = dir.join("producer.json");
    let temp = dir.join(format!(".producer-{}.tmp", Uuid::new_v4()));
    let result = (|| {
        let mut f = OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&temp)
            .map_err(|_| storage())?;
        f.write_all(&bytes)
            .and_then(|()| f.sync_all())
            .map_err(|_| storage())?;
        // Check ownership again after preparing bytes. Sync is not a distributed lock.
        if owned_manifest(binding)? != existing {
            return Err(conflict());
        }
        if existing.is_none() {
            publish_new(&temp, &path).map_err(|error| {
                if error.kind() == std::io::ErrorKind::AlreadyExists {
                    conflict()
                } else {
                    storage()
                }
            })?;
        } else {
            fs::rename(&temp, &path).map_err(|_| storage())?;
        }
        Ok(())
    })();
    // A provider can change the folder while publication is in flight. Leave our staging
    // behind rather than follow a replaced root/control directory during cleanup.
    if fs::canonicalize(&binding.root).is_ok_and(|root| root == binding.root)
        && control_dir(&binding.root, false).is_ok()
    {
        let _ = fs::remove_file(&temp);
    }
    result
}

/// Deliberate disconnect before reconnecting this folder to another producer/paper. The
/// exact previously displayed manifest must still match; no requests or editions are removed.
pub fn disconnect_manifest(root: &Path, expected: &ProducerManifest) -> Result<(), PressError> {
    let root = fs::canonicalize(root).map_err(|_| storage())?;
    let dir = control_dir(&root, false)?;
    let path = dir.join("producer.json");
    let binding = PressBinding {
        root,
        producer_id: expected.producer_id,
        paper_id: expected.paper_id,
    };
    let current = decode_manifest(&bounded_regular(&path)?, &binding)?;
    if &current != expected {
        return Err(conflict());
    }
    fs::remove_file(path).map_err(|_| storage())
}

fn advertisement(paper: &Config, binding: &PressBinding, enabled: bool) -> ProducerManifest {
    ProducerManifest {
        format: "deckle-press".into(),
        version: 1,
        producer_id: binding.producer_id,
        paper_id: binding.paper_id,
        paper_name: paper.title.clone(),
        enabled,
        operations: vec![PressOperation::FreshFull],
        updated_at: Utc::now(),
    }
}
fn save(path: &Path, paper: &Config) -> Result<(), PressError> {
    config::save(path, paper).map_err(|_| storage())
}

/// Activation transaction. Only the OS service operation is injected; all local configuration,
/// ownership checks and shared writes are real. Call with `schedule::ensure_watcher` in the UI.
pub fn enable_with_watcher(
    configs: &[Config],
    paper: &mut Config,
    export_index: usize,
    producer_id: Uuid,
    config_path: &Path,
    ensure_watcher: impl FnOnce() -> Result<(), String>,
) -> Result<PressBinding, PressError> {
    paper
        .export
        .get_mut(export_index)
        .ok_or_else(unsupported)?
        .press_enabled = false;
    paper.paper_id.get_or_insert_with(Uuid::new_v4);
    save(config_path, paper)?;
    let binding = prepare_binding(configs, paper, export_index, producer_id)?;
    let existing = owned_manifest(&binding)?;
    // An interrupted earlier activation may have advertised enabled while locally disabled.
    if existing.is_some_and(|m| m.enabled) {
        write_manifest(&binding, &advertisement(paper, &binding, false))?;
    }
    ensure_watcher().map_err(|_| storage())?;
    write_manifest(&binding, &advertisement(paper, &binding, true))?;
    paper.export[export_index].press_enabled = true;
    if let Err(e) = save(config_path, paper) {
        paper.export[export_index].press_enabled = false;
        let _ = write_manifest(&binding, &advertisement(paper, &binding, false));
        return Err(e);
    }
    Ok(binding)
}

/// Disable locally before touching the possibly unavailable shared folder.
pub fn disable(
    configs: &[Config],
    paper: &mut Config,
    export_index: usize,
    producer_id: Uuid,
    config_path: &Path,
) -> Result<(), PressError> {
    paper
        .export
        .get_mut(export_index)
        .ok_or_else(unsupported)?
        .press_enabled = false;
    save(config_path, paper)?;
    let binding = prepare_binding(configs, paper, export_index, producer_id)?;
    write_manifest(&binding, &advertisement(paper, &binding, false))
}

/// Scanner admission requires both the local opt-in and a matching enabled advertisement.
/// Failures, conflicts and offline folders all reject admission.
pub fn binding_is_enabled(
    configs: &[Config],
    paper: &Config,
    export_index: usize,
    producer_id: Uuid,
) -> bool {
    paper
        .export
        .get(export_index)
        .is_some_and(|e| e.press_enabled)
        && resolve_binding(configs, paper, export_index, producer_id)
            .and_then(|b| owned_manifest(&b))
            .is_ok_and(|m| m.is_some_and(|m| m.enabled))
}

/// Recover setup on startup. Never infer intent from a shared manifest alone. Owned incomplete
/// activation is disabled; already opted-in setup can repair its watcher/advertisement.
pub fn reconcile_with_watcher(
    configs: &[Config],
    paper: &mut Config,
    export_index: usize,
    producer_id: Uuid,
    config_path: &Path,
    ensure_watcher: impl FnOnce() -> Result<(), String>,
) -> Result<(), PressError> {
    let enabled = paper
        .export
        .get(export_index)
        .ok_or_else(unsupported)?
        .press_enabled;
    if !enabled && paper.paper_id.is_none() {
        return Ok(());
    }
    if enabled {
        match enable_with_watcher(
            configs,
            paper,
            export_index,
            producer_id,
            config_path,
            ensure_watcher,
        ) {
            Ok(_) => Ok(()),
            Err(error) => {
                let _ = disable(configs, paper, export_index, producer_id, config_path);
                Err(error)
            }
        }
    } else {
        let binding = prepare_binding(configs, paper, export_index, producer_id)?;
        if owned_manifest(&binding)?.is_some_and(|m| m.enabled) {
            write_manifest(&binding, &advertisement(paper, &binding, false))?;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn setup(root: &Path) -> (PressBinding, ProducerManifest) {
        let binding = PressBinding {
            root: fs::canonicalize(root).unwrap(),
            producer_id: Uuid::new_v4(),
            paper_id: Uuid::new_v4(),
        };
        let manifest = advertisement(&Config::default(), &binding, true);
        (binding, manifest)
    }

    #[test]
    fn unsupported_atomic_publication_returns_storage_error_and_cleans_own_staging() {
        let dir = tempfile::tempdir().unwrap();
        let (binding, manifest) = setup(dir.path());
        let error = write_manifest_using(&binding, &manifest, |_, _| {
            Err(std::io::Error::from(std::io::ErrorKind::Unsupported))
        })
        .unwrap_err();
        assert_eq!(error.code, PressErrorCode::StorageUnavailable);
        assert!(!dir.path().join(".deckle-press/producer.json").exists());
        assert_eq!(
            fs::read_dir(dir.path().join(".deckle-press"))
                .unwrap()
                .count(),
            0
        );
    }

    #[test]
    fn atomic_no_clobber_preserves_foreign_manifest_arriving_during_publication() {
        let dir = tempfile::tempdir().unwrap();
        let (binding, manifest) = setup(dir.path());
        let mut foreign = manifest.clone();
        foreign.producer_id = Uuid::new_v4();
        let foreign_bytes = serde_json::to_vec(&foreign).unwrap();
        let error = write_manifest_using(&binding, &manifest, |_, to| {
            fs::write(to, &foreign_bytes)?;
            Err(std::io::Error::from(std::io::ErrorKind::AlreadyExists))
        })
        .unwrap_err();
        assert_eq!(error.code, PressErrorCode::ProtocolConflict);
        assert_eq!(
            fs::read(dir.path().join(".deckle-press/producer.json")).unwrap(),
            foreign_bytes
        );
        assert_eq!(
            fs::read_dir(dir.path().join(".deckle-press"))
                .unwrap()
                .count(),
            1
        );
    }
    #[cfg(unix)]
    #[test]
    fn failed_publication_cleanup_cannot_follow_a_replaced_control_directory() {
        let dir = tempfile::tempdir().unwrap();
        let outside = tempfile::tempdir().unwrap();
        let (binding, manifest) = setup(dir.path());
        let mut foreign_path = None;
        let error = write_manifest_using(&binding, &manifest, |from, _| {
            fs::rename(from.parent().unwrap(), dir.path().join("moved-control"))?;
            std::os::unix::fs::symlink(outside.path(), from.parent().unwrap())?;
            let foreign = outside.path().join(from.file_name().unwrap());
            fs::write(&foreign, b"foreign staging")?;
            foreign_path = Some(foreign);
            Err(std::io::Error::from(std::io::ErrorKind::Unsupported))
        })
        .unwrap_err();
        assert_eq!(error.code, PressErrorCode::StorageUnavailable);
        assert_eq!(fs::read(foreign_path.unwrap()).unwrap(), b"foreign staging");
    }
}
