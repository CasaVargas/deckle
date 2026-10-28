// SPDX-License-Identifier: GPL-3.0-or-later
//! Producer-recorded fresh outputs only; shared requests are never removed.
use super::safe_io::Dir;
use super::*;
use super::{
    journal::storage,
    publish::{archive, binding, check_binding, directory, editions, sync_dir, verify},
};
use chrono::{DateTime, Duration, Utc};
use std::fs;

// Native identities are local journal data, never shared protocol fields.
fn object_identity(path: &std::path::Path) -> Result<super::queue::ObjectIdentity, PressError> {
    use super::queue::ObjectIdentity;
    let metadata = fs::symlink_metadata(path).map_err(|_| storage())?;
    if metadata.file_type().is_symlink() || (!metadata.is_dir() && !metadata.is_file()) {
        return Err(super::publish::conflict());
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::MetadataExt;
        let created = metadata
            .created()
            .ok()
            .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
            .map(|t| (t.as_secs(), t.subsec_nanos()));
        Ok(ObjectIdentity {
            volume: metadata.dev(),
            file: metadata.ino(),
            created,
        })
    }
    #[cfg(windows)]
    {
        use std::os::windows::{
            fs::{MetadataExt, OpenOptionsExt},
            io::AsRawHandle,
        };
        use windows_sys::Win32::Storage::FileSystem::{
            BY_HANDLE_FILE_INFORMATION, FILE_ATTRIBUTE_REPARSE_POINT, FILE_FLAG_BACKUP_SEMANTICS,
            FILE_FLAG_OPEN_REPARSE_POINT, GetFileInformationByHandle,
        };
        if metadata.file_attributes() & FILE_ATTRIBUTE_REPARSE_POINT != 0 {
            return Err(super::publish::conflict());
        }
        let file = fs::OpenOptions::new()
            .read(true)
            .custom_flags(FILE_FLAG_BACKUP_SEMANTICS | FILE_FLAG_OPEN_REPARSE_POINT)
            .open(path)
            .map_err(|_| storage())?;
        let mut info = std::mem::MaybeUninit::<BY_HANDLE_FILE_INFORMATION>::uninit();
        // SAFETY: file owns a live handle; the output points to the API's exact structure.
        if unsafe { GetFileInformationByHandle(file.as_raw_handle(), info.as_mut_ptr()) } == 0 {
            return Err(storage());
        }
        // SAFETY: successful GetFileInformationByHandle initialized the complete structure.
        let info = unsafe { info.assume_init() };
        if info.dwFileAttributes & FILE_ATTRIBUTE_REPARSE_POINT != 0 {
            return Err(super::publish::conflict());
        }
        Ok(ObjectIdentity {
            volume: u64::from(info.dwVolumeSerialNumber),
            file: (u64::from(info.nFileIndexHigh) << 32) | u64::from(info.nFileIndexLow),
            created: Some((
                (u64::from(info.ftCreationTime.dwHighDateTime) << 32)
                    | u64::from(info.ftCreationTime.dwLowDateTime),
                0,
            )),
        })
    }
    #[cfg(not(any(unix, windows)))]
    {
        Err(PressError::new(PressErrorCode::Unsupported))
    }
}
fn authorize_bundle(
    j: &mut PressJournal,
    path: &std::path::Path,
    o: &super::queue::OutputRecord,
    local: bool,
) -> Result<super::queue::DeletionAuthorization, PressError> {
    use super::queue::{DeletionAuthorization, DeletionMember};
    let current = j
        .outputs()
        .into_iter()
        .find(|current| current.result.edition_id == o.result.edition_id)
        .ok_or_else(super::publish::conflict)?;
    if let Some(saved) = if local {
        current.archive_deletion
    } else {
        current.shared_deletion
    } {
        return Ok(saved);
    }
    let assets = verify(path, &o.lease, &o.result.manifest_sha256)?;
    let mut paths =
        std::collections::BTreeMap::from([(String::new(), true), ("edition.json".into(), false)]);
    for asset in assets {
        let mut parent = std::path::Path::new(&asset).parent();
        while let Some(dir) = parent.filter(|p| !p.as_os_str().is_empty()) {
            paths.insert(
                dir.to_str()
                    .ok_or_else(super::publish::conflict)?
                    .replace('\\', "/"),
                true,
            );
            parent = dir.parent();
        }
        paths.insert(asset, false);
    }
    let authorization = DeletionAuthorization {
        members: paths
            .into_iter()
            .map(|(relative, directory)| {
                Ok(DeletionMember {
                    identity: object_identity(&path.join(&relative))?,
                    relative,
                    directory,
                })
            })
            .collect::<Result<_, PressError>>()?,
    };
    check_members(path, &authorization)?;
    j.authorize_deletion(o.result.edition_id, local, authorization.clone())?;
    Ok(authorization)
}
fn check_member(
    root: &std::path::Path,
    member: &super::queue::DeletionMember,
) -> Result<bool, PressError> {
    let path = root.join(&member.relative);
    match fs::symlink_metadata(&path) {
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(false),
        Err(_) => return Err(storage()),
        Ok(m) if m.is_dir() != member.directory => return Err(super::publish::conflict()),
        Ok(_) => (),
    }
    if object_identity(&path)? != member.identity {
        return Err(super::publish::conflict());
    }
    Ok(true)
}
fn check_members(
    root: &std::path::Path,
    authorization: &super::queue::DeletionAuthorization,
) -> Result<(), PressError> {
    let names: std::collections::HashSet<_> = authorization
        .members
        .iter()
        .map(|m| std::path::Path::new(&m.relative))
        .collect();
    // Validate parents before children even when loading a persisted authorization.
    let mut members: Vec<_> = authorization.members.iter().collect();
    members.sort_by(|a, b| a.relative.cmp(&b.relative));
    for member in members {
        if check_member(root, member)? && member.directory {
            for entry in fs::read_dir(root.join(&member.relative)).map_err(|_| storage())? {
                let relative = std::path::Path::new(&member.relative)
                    .join(entry.map_err(|_| storage())?.file_name());
                if !names.contains(relative.as_path()) {
                    return Err(super::publish::conflict());
                }
            }
        }
    }
    Ok(())
}
fn check_parents(
    root: &std::path::Path,
    relative: &str,
    authorization: &super::queue::DeletionAuthorization,
) -> Result<(), PressError> {
    let mut parents: Vec<_> = std::path::Path::new(relative).ancestors().skip(1).collect();
    parents.reverse();
    for parent in parents {
        let saved = authorization
            .members
            .iter()
            .find(|m| m.directory && std::path::Path::new(&m.relative) == parent)
            .ok_or_else(super::publish::conflict)?;
        check_member(root, saved)?;
    }
    Ok(())
}
fn remove_bundle(
    j: &mut PressJournal,
    path: &std::path::Path,
    o: &super::queue::OutputRecord,
    local: bool,
) -> Result<(), PressError> {
    match fs::symlink_metadata(path) {
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(()),
        Err(_) => return Err(storage()),
        Ok(_) => (),
    }
    let authorization = authorize_bundle(j, path, o, local)?;
    check_members(path, &authorization)?;
    for member in authorization.members.iter().filter(|m| !m.directory) {
        check_parents(path, &member.relative, &authorization)?;
        if check_member(path, member)? {
            let target = path.join(&member.relative);
            let pinned = Dir::open(target.parent().ok_or_else(super::publish::conflict)?)?;
            check_parents(path, &member.relative, &authorization)?;
            check_member(path, member)?;
            pinned.remove(
                std::path::Path::new(target.file_name().ok_or_else(super::publish::conflict)?),
                false,
            )?;
        }
    }
    let mut dirs: Vec<_> = authorization
        .members
        .iter()
        .filter(|m| m.directory)
        .collect();
    dirs.sort_by_key(|m| std::cmp::Reverse(std::path::Path::new(&m.relative).components().count()));
    for member in dirs {
        check_parents(path, &member.relative, &authorization)?;
        if check_member(path, member)? {
            let target = if member.relative.is_empty() {
                path.to_owned()
            } else {
                path.join(&member.relative)
            };
            let pinned = Dir::open(target.parent().ok_or_else(super::publish::conflict)?)?;
            check_parents(path, &member.relative, &authorization)?;
            check_member(path, member)?;
            pinned.remove(
                std::path::Path::new(target.file_name().ok_or_else(super::publish::conflict)?),
                true,
            )?;
        }
    }
    sync_dir(path.parent().ok_or_else(super::publish::conflict)?)
}
/// Keep seven newest per folder, every result under 48 hours, and all unpublished
/// intents. Tombstones are committed and shared before deleting either owned copy.
pub fn prune_results(j: &mut PressJournal, now: DateTime<Utc>) -> Result<(), PressError> {
    j.ensure_healthy()?;
    let mut error = None;
    let mut outputs = j.outputs();
    outputs.sort_by_key(|o| std::cmp::Reverse((o.published_at, o.result.edition_id)));
    let mut counts = std::collections::HashMap::new();
    for o in outputs {
        let outcome = (|| -> Result<(), PressError> {
            let active = j.active();
            for stage in &o.stages {
                if !j.job_terminal(o.lease.job_id)
                    && active.as_ref().is_none_or(|l| {
                        l.job_id == o.lease.job_id && l.generation == stage.generation
                    })
                {
                    continue;
                }
                let parent = if stage.local {
                    archive(j, &o.lease, false)?.parent().unwrap().to_owned()
                } else {
                    editions(&binding(&o.lease), false)?
                };
                let path = parent.join(&stage.name);
                match fs::symlink_metadata(&path) {
                    Ok(m) if m.file_type().is_dir() => {
                        remove_stage(&path)?;
                        sync_dir(&parent)?;
                    }
                    Ok(_) => return Err(super::publish::conflict()),
                    Err(e) if e.kind() == std::io::ErrorKind::NotFound => (),
                    Err(_) => return Err(storage()),
                }
            }

            let Some(at) = o.published_at else {
                if j.job_terminal(o.lease.job_id) {
                    let b = binding(&o.lease);
                    check_binding(&b)?;
                    let final_path = b.root.join(&o.result.bundle);
                    match fs::symlink_metadata(&final_path) {
                        Err(e) if e.kind() == std::io::ErrorKind::NotFound => (),
                        Err(_) => return Err(storage()),
                        Ok(_) => return Err(super::publish::conflict()),
                    }
                    if o.archive_deletion.is_some() {
                        let local = archive(j, &o.lease, false)?;
                        remove_bundle(j, &local, &o, true)?;
                    } else if let Some(artifact) = archived_artifact(j, &o.lease)? {
                        remove_bundle(j, &artifact.bundle, &o, true)?;
                    }
                    j.forget_intent(o.lease.job_id)?;
                }
                return Ok(());
            };
            let count = counts.entry(o.lease.root.clone()).or_insert(0usize);
            let remove = if o.removed_at.is_some() {
                !o.deleted
            } else {
                *count += 1;
                *count > 7 && now - at >= Duration::hours(48)
            };
            if !remove {
                return Ok(());
            }
            let b = binding(&o.lease);
            check_binding(&b)?;
            if o.removed_at.is_none() {
                j.mark_removed(o.result.edition_id, now)?;
            }
            for status in j
                .statuses()
                .into_iter()
                .filter(|s| s.job_id == Some(o.lease.job_id))
            {
                super::publish::flush_status(j, status)?;
            }
            // Recheck after shared writes. A replaced/foreign folder cannot authorize deletion.
            let parent = editions(&b, false)?;
            remove_bundle(
                j,
                &parent.join(format!("{}.deckle", o.result.edition_id)),
                &o,
                false,
            )?;
            let local = archive(j, &o.lease, false)?;
            remove_bundle(j, &local, &o, true)?;
            j.mark_deleted(o.result.edition_id, now)?;
            Ok(())
        })();
        if let Err(e) = outcome {
            error.get_or_insert(e);
        }
    }
    let expired = j.forget_old(now, |b, id| {
        check_binding(b)?;
        let parent = b.root.join(".deckle-press/status");
        directory(&parent, false)?;
        let path = parent.join(format!("{id}.json"));
        match fs::symlink_metadata(&path) {
            Ok(m) if m.file_type().is_file() => {
                let pinned = Dir::open(&parent)?;
                pinned.remove(
                    std::path::Path::new(path.file_name().ok_or_else(super::publish::conflict)?),
                    false,
                )
            }
            Ok(_) => Err(PressError::new(PressErrorCode::ProtocolConflict)),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
            Err(_) => Err(storage()),
        }
    });
    if let Err(e) = expired {
        error.get_or_insert(e);
    }
    error.map_or(Ok(()), Err)
}

// Staging is producer-recorded. Pin every directory before descending or unlinking;
// never recurse through a replacement link or call ambient remove_dir_all.
fn remove_stage(path: &std::path::Path) -> Result<(), PressError> {
    let parent = Dir::open(path.parent().ok_or_else(super::publish::conflict)?)?;
    let dir = Dir::open(path)?;
    for entry in fs::read_dir(path).map_err(|_| storage())? {
        let entry = entry.map_err(|_| storage())?;
        let kind = entry.file_type().map_err(|_| storage())?;
        if kind.is_dir() {
            remove_stage(&entry.path())?;
        } else if kind.is_file() {
            dir.validate()?;
            dir.remove(std::path::Path::new(&entry.file_name()), false)?;
        } else {
            return Err(super::publish::conflict());
        }
    }
    dir.validate()?;
    drop(dir);
    parent.remove(
        std::path::Path::new(path.file_name().ok_or_else(super::publish::conflict)?),
        true,
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::TimeZone;
    use sha2::{Digest, Sha256};
    use std::path::Path;
    fn now() -> DateTime<Utc> {
        Utc.with_ymd_and_hms(2026, 10, 27, 12, 0, 0).unwrap()
    }
    fn fixture(
        root: &Path,
    ) -> (
        PressJournal,
        PressBinding,
        super::super::queue::OutputRecord,
        Uuid,
    ) {
        let export = root.join("export");
        fs::create_dir(&export).unwrap();
        let b = PressBinding {
            root: fs::canonicalize(export).unwrap(),
            paper_id: Uuid::new_v4(),
            producer_id: Uuid::new_v4(),
        };
        setup::write_manifest(
            &b,
            &ProducerManifest {
                format: "deckle-press".into(),
                version: 1,
                paper_id: b.paper_id,
                producer_id: b.producer_id,
                paper_name: "Test".into(),
                enabled: true,
                operations: vec![PressOperation::FreshFull],
                updated_at: now(),
            },
        )
        .unwrap();
        let mut j = PressJournal::open(&root.join("journal")).unwrap();
        j.register_binding(b.clone(), true, false).unwrap();
        let mut first = None;
        for i in 0..8 {
            let at = now() + Duration::minutes(i * 5);
            let id = Uuid::new_v4();
            j.accept(
                PressRequest {
                    format: "deckle-press-request".into(),
                    version: 1,
                    request_id: id,
                    reader_id: Uuid::new_v4(),
                    producer_id: b.producer_id,
                    paper_id: b.paper_id,
                    operation: PressOperation::FreshFull,
                    created_at: at,
                    expires_at: at + Duration::hours(24),
                },
                "hash",
                at,
            )
            .unwrap();
            let l = j.admit(at).unwrap().unwrap();
            let source = root.join(format!("source-{i}"));
            fs::create_dir(&source).unwrap();
            fs::write(source.join("cover.jpg"), b"image").unwrap();
            let mut v: serde_json::Value =
                serde_json::from_str(include_str!("../../../../fixtures/press/bundle-fresh.json"))
                    .unwrap();
            v["edition"]["id"] = l.edition_id.to_string().into();
            v["edition"]["paper_id"] = b.paper_id.to_string().into();
            let hash = |bytes: &[u8]| -> String {
                Sha256::digest(bytes)
                    .iter()
                    .map(|b| format!("{b:02x}"))
                    .collect()
            };
            v["files"] = serde_json::json!([{"path":"cover.jpg","size":5,"sha256":hash(b"image")}]);
            let bytes = serde_json::to_vec(&v).unwrap();
            fs::write(source.join("edition.json"), &bytes).unwrap();
            publish_result(
                &mut j,
                &l,
                &FreshArtifact {
                    bundle: source,
                    manifest_sha256: hash(&bytes),
                },
                || at,
            )
            .unwrap();
            if i == 0 {
                first = Some((l.edition_id, id));
            }
        }
        let (edition, id) = first.unwrap();
        let o = j
            .outputs()
            .into_iter()
            .find(|o| o.result.edition_id == edition)
            .unwrap();
        j.mark_removed(edition, now() + Duration::days(3)).unwrap();
        flush_statuses(&mut j).unwrap();
        (j, b, o, id)
    }
    #[test]
    fn unavailable_history_does_not_block_healthy_result_tombstone_and_deletion() {
        let d = tempfile::tempdir().unwrap();
        let (mut j, b, o, id) = fixture(d.path());
        let unavailable = d.path().join("unavailable");
        fs::create_dir(&unavailable).unwrap();
        let other = PressBinding {
            root: unavailable.canonicalize().unwrap(),
            paper_id: Uuid::new_v4(),
            producer_id: b.producer_id,
        };
        j.register_binding(other.clone(), true, false).unwrap();
        let request = PressRequest {
            format: "deckle-press-request".into(),
            version: 1,
            request_id: Uuid::new_v4(),
            reader_id: Uuid::new_v4(),
            producer_id: other.producer_id,
            paper_id: other.paper_id,
            operation: PressOperation::FreshFull,
            created_at: now(),
            expires_at: now() + Duration::hours(24),
        };
        j.reject_in(
            &other,
            request.clone(),
            "offline",
            PressErrorCode::Busy,
            now(),
        )
        .unwrap();
        fs::remove_dir(&other.root).unwrap();
        assert!(prune_results(&mut j, now() + Duration::days(31)).is_err());
        assert!(!b.root.join(&o.result.bundle).exists());
        assert!(j.status(request.request_id).is_some());
        let bytes = fs::read(b.root.join(format!(".deckle-press/status/{id}.json"))).unwrap();
        let status: PressStatus = serde_json::from_slice(&bytes).unwrap();
        assert_eq!(status.state, PressState::ResultRemoved);
        assert!(j.status(id).is_some());
    }
    #[test]
    fn partial_recursive_removal_resumes_after_restart_and_retains_tombstone() {
        let d = tempfile::tempdir().unwrap();
        let (mut j, b, o, id) = fixture(d.path());
        let path = b.root.join(&o.result.bundle);
        authorize_bundle(&mut j, &path, &o, false).unwrap();
        fs::remove_file(path.join("cover.jpg")).unwrap();
        drop(j);
        let mut j = PressJournal::open(&d.path().join("journal")).unwrap();
        prune_results(&mut j, now() + Duration::days(10)).unwrap();
        assert!(!path.exists());
        assert!(!archive(&j, &o.lease, false).unwrap().exists());
        prune_results(&mut j, now() + Duration::days(34)).unwrap();
        assert_eq!(j.status(id).unwrap().state, PressState::ResultRemoved);
        prune_results(&mut j, now() + Duration::days(41)).unwrap();
        assert!(j.status(id).is_none());
    }
    #[test]
    fn deletion_authorization_refuses_replacement_and_unrecorded_files() {
        for replace in [false, true] {
            let d = tempfile::tempdir().unwrap();
            let (mut j, b, o, _) = fixture(d.path());
            let path = b.root.join(&o.result.bundle);
            authorize_bundle(&mut j, &path, &o, false).unwrap();
            if replace {
                fs::rename(&path, d.path().join("original")).unwrap();
                fs::create_dir(&path).unwrap();
                fs::write(path.join("foreign"), b"keep").unwrap();
            } else {
                fs::write(path.join("foreign"), b"keep").unwrap();
            }
            drop(j);
            let mut j = PressJournal::open(&d.path().join("journal")).unwrap();
            assert!(prune_results(&mut j, now() + Duration::days(10)).is_err());
            assert_eq!(fs::read(path.join("foreign")).unwrap(), b"keep");
        }
    }
    #[cfg(unix)]
    #[test]
    fn deletion_authorization_refuses_symlink_replacement() {
        let d = tempfile::tempdir().unwrap();
        let (mut j, b, o, _) = fixture(d.path());
        let path = b.root.join(&o.result.bundle);
        authorize_bundle(&mut j, &path, &o, false).unwrap();
        let outside = d.path().join("outside");
        fs::write(&outside, b"image").unwrap();
        fs::remove_file(path.join("cover.jpg")).unwrap();
        std::os::unix::fs::symlink(&outside, path.join("cover.jpg")).unwrap();
        drop(j);
        let mut j = PressJournal::open(&d.path().join("journal")).unwrap();
        assert!(prune_results(&mut j, now() + Duration::days(10)).is_err());
        assert_eq!(fs::read(outside).unwrap(), b"image");
        assert!(path.join("edition.json").exists());
    }
}
