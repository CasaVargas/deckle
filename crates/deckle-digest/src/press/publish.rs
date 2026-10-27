// SPDX-License-Identifier: GPL-3.0-or-later
//! Immutable publication. The caller retains the local build lock throughout.
use super::journal::storage;
use super::*;
use chrono::{DateTime, Utc};
use sha2::{Digest, Sha256};
use std::{
    fs::{self, File, OpenOptions},
    io::{Read, Write},
    path::{Component, Path, PathBuf},
};

pub(crate) fn conflict() -> PressError {
    PressError::new(PressErrorCode::ProtocolConflict)
}
pub(crate) fn directory(path: &Path, create: bool) -> Result<(), PressError> {
    if create {
        match fs::create_dir(path) {
            Ok(()) => sync_dir(path.parent().ok_or_else(conflict)?)?,
            Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => (),
            Err(_) => return Err(storage()),
        }
    }
    if !fs::symlink_metadata(path)
        .map_err(|_| storage())?
        .file_type()
        .is_dir()
    {
        return Err(conflict());
    }
    Ok(())
}
pub(crate) fn binding(lease: &JobLease) -> PressBinding {
    PressBinding {
        root: lease.root.clone(),
        producer_id: lease.producer_id,
        paper_id: lease.paper_id,
    }
}
pub(crate) fn check_binding(b: &PressBinding) -> Result<(), PressError> {
    directory(&b.root, false)?;
    if fs::canonicalize(&b.root).map_err(|_| storage())? != b.root {
        return Err(conflict());
    }
    directory(&b.root.join(".deckle-press"), false)?;
    setup::owned_manifest(b)?.ok_or_else(conflict)?;
    Ok(())
}
pub(crate) fn editions(b: &PressBinding, create: bool) -> Result<PathBuf, PressError> {
    check_binding(b)?;
    let p = b.root.join(".deckle-press/editions");
    directory(&p, create)?;
    Ok(p)
}
fn regular(path: &Path) -> Result<Vec<u8>, PressError> {
    if !fs::symlink_metadata(path)
        .map_err(|_| storage())?
        .file_type()
        .is_file()
    {
        return Err(conflict());
    }
    fs::read(path).map_err(|_| storage())
}
fn hash(bytes: &[u8]) -> String {
    Sha256::digest(bytes)
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect()
}
fn file_hash(path: &Path, size: u64, expected: &str) -> Result<(), PressError> {
    let meta = fs::symlink_metadata(path).map_err(|_| storage())?;
    if !meta.file_type().is_file() || meta.len() != size {
        return Err(conflict());
    }
    let mut file = File::open(path).map_err(|_| storage())?;
    let mut hash = Sha256::new();
    let mut buf = [0u8; 65536];
    loop {
        let n = file.read(&mut buf).map_err(|_| storage())?;
        if n == 0 {
            break;
        }
        hash.update(&buf[..n]);
    }
    let actual: String = hash.finalize().iter().map(|b| format!("{b:02x}")).collect();
    if actual != expected {
        return Err(conflict());
    }
    Ok(())
}
fn safe_file(root: &Path, relative: &str) -> Result<PathBuf, PressError> {
    let p = Path::new(relative);
    if relative.contains('\\')
        || relative.is_empty()
        || p.components().any(|c| !matches!(c, Component::Normal(_)))
        || relative
            .split('/')
            .any(|c| c.is_empty() || c == "." || c == "..")
    {
        return Err(conflict());
    }
    let mut joined = root.to_path_buf();
    let parts: Vec<_> = p.components().collect();
    for c in &parts[..parts.len() - 1] {
        joined.push(c);
        directory(&joined, false)?;
    }
    joined.push(parts.last().unwrap());
    Ok(joined)
}
/// Verify exact manifest bytes, fresh identity and every listed asset's size/hash.
pub(crate) fn verify(
    path: &Path,
    lease: &JobLease,
    expected: &str,
) -> Result<Vec<String>, PressError> {
    directory(path, false)?;
    let bytes = regular(&path.join("edition.json"))?;
    if hash(&bytes) != expected {
        return Err(conflict());
    }
    let v: serde_json::Value = serde_json::from_slice(&bytes).map_err(|_| conflict())?;
    if v["format"] != "deckle-edition"
        || v["version"] != "1.4"
        || v["edition"]["id"] != lease.edition_id.to_string()
        || v["edition"]["paper_id"] != lease.paper_id.to_string()
        || v["edition"]["kind"] != "fresh"
    {
        return Err(conflict());
    }
    let files = v["files"].as_array().ok_or_else(conflict)?;
    let mut paths = std::collections::HashSet::new();
    for f in files {
        let p = f["path"].as_str().ok_or_else(conflict)?;
        if p == "edition.json" || !paths.insert(p.to_owned()) {
            return Err(conflict());
        }
        let size = f["size"].as_u64().ok_or_else(conflict)?;
        let h = f["sha256"].as_str().ok_or_else(conflict)?;
        if h.len() != 64 {
            return Err(conflict());
        }
        file_hash(&safe_file(path, p)?, size, h)?;
    }
    if !v["edition"]["cover"]
        .as_str()
        .is_some_and(|cover| paths.contains(cover))
    {
        return Err(conflict());
    }
    Ok(paths.into_iter().collect())
}
pub(crate) fn sync_dir(p: &Path) -> Result<(), PressError> {
    #[cfg(unix)]
    File::open(p)
        .and_then(|f| f.sync_all())
        .map_err(|_| storage())?;
    Ok(())
}
fn write_new(path: &Path, bytes: &[u8]) -> Result<(), PressError> {
    let mut f = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(path)
        .map_err(|_| storage())?;
    f.write_all(bytes)
        .and_then(|()| f.sync_all())
        .map_err(|_| storage())
}
fn stage(
    j: &mut PressJournal,
    source: &Path,
    parent: &Path,
    lease: &JobLease,
    hash: &str,
    local: bool,
) -> Result<PathBuf, PressError> {
    let paths = verify(source, lease, hash)?;
    let name = format!(".{}-{}.tmp", lease.edition_id, Uuid::new_v4());
    j.record_stage(lease, name.clone(), local)?;
    let staging = parent.join(name);
    fs::create_dir(&staging).map_err(|_| storage())?;
    let mut asset_dirs = std::collections::BTreeSet::new();
    for rel in paths {
        let dest = staging.join(&rel);
        if let Some(p) = dest.parent() {
            fs::create_dir_all(p).map_err(|_| storage())?;
            let mut current = p;
            while current != staging {
                asset_dirs.insert(current.to_path_buf());
                current = current.parent().ok_or_else(conflict)?;
            }
        }
        let mut from = File::open(safe_file(source, &rel)?).map_err(|_| storage())?;
        let mut to = OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(dest)
            .map_err(|_| storage())?;
        std::io::copy(&mut from, &mut to)
            .and_then(|_| to.sync_all())
            .map_err(|_| storage())?;
    }
    for dir in asset_dirs.iter().rev() {
        sync_dir(dir)?;
    }
    write_new(
        &staging.join("edition.json"),
        &regular(&source.join("edition.json"))?,
    )?;
    verify(&staging, lease, hash)?;
    sync_dir(&staging)?;
    Ok(staging)
}
pub(crate) fn archive(j: &PressJournal, l: &JobLease, create: bool) -> Result<PathBuf, PressError> {
    let root = j.local_root().join("archive");
    directory(&root, create)?;
    let paper = root.join(l.paper_id.to_string());
    directory(&paper, create)?;
    Ok(paper.join(format!("{}.deckle", l.edition_id)))
}
/// Copy and verify before the short journal finalizer. Clock is sampled after all
/// expensive work, immediately before the final generation/deadline check and rename.
pub fn publish_result(
    j: &mut PressJournal,
    l: &JobLease,
    a: &FreshArtifact,
    mut clock: impl FnMut() -> DateTime<Utc>,
) -> Result<PressStatus, PressError> {
    j.validate_lease(l, clock())?;
    let b = binding(l);
    check_binding(&b)?;
    let result = PressResult {
        edition_id: l.edition_id,
        bundle: format!(".deckle-press/editions/{}.deckle", l.edition_id),
        manifest_sha256: a.manifest_sha256.clone(),
    };
    verify(&a.bundle, l, &a.manifest_sha256)?;
    j.record_intent(l, result.clone(), clock())?;
    let local = archive(j, l, true)?;
    if fs::symlink_metadata(&local).is_ok() {
        verify(&local, l, &a.manifest_sha256)?;
    } else {
        let tmp = stage(
            j,
            &a.bundle,
            local.parent().unwrap(),
            l,
            &a.manifest_sha256,
            true,
        )?;
        fs::rename(tmp, &local).map_err(|_| storage())?;
        sync_dir(local.parent().unwrap())?;
    }
    let parent = editions(&b, true)?;
    let final_path = parent.join(format!("{}.deckle", l.edition_id));
    if fs::symlink_metadata(&final_path).is_ok() {
        return recover_result(j, l, clock())?.ok_or_else(conflict);
    }
    let temp = stage(j, &local, &parent, l, &a.manifest_sha256, false)?;
    check_binding(&b)?;
    let now = clock();
    j.publish(l, result, now, || {
        check_binding(&b)?;
        if clock() >= l.deadline {
            return Err(PressError::new(PressErrorCode::TimedOut));
        }
        if fs::symlink_metadata(&final_path).is_ok() {
            return Err(conflict());
        }
        fs::rename(&temp, &final_path).map_err(|_| storage())?;
        sync_dir(&parent)
    })?;
    let status = j.job_status(l.job_id).ok_or_else(conflict)?;
    flush_statuses(j)?;
    Ok(status)
}
/// Reconcile only an already-existing, completely verified output using the journal's
/// pre-rename intent. Call before recovery/timeout; this never copies or renames assets.
pub fn recover_result(
    j: &mut PressJournal,
    l: &JobLease,
    now: DateTime<Utc>,
) -> Result<Option<PressStatus>, PressError> {
    j.ensure_healthy()?;
    let Some(o) = j
        .outputs()
        .into_iter()
        .find(|o| o.lease.job_id == l.job_id && o.removed_at.is_none())
    else {
        return Ok(None);
    };
    let b = binding(l);
    check_binding(&b)?;
    let parent = b.root.join(".deckle-press/editions");
    if fs::symlink_metadata(&parent).is_err_and(|e| e.kind() == std::io::ErrorKind::NotFound) {
        return Ok(None);
    }
    directory(&parent, false)?;
    let path = parent.join(format!("{}.deckle", l.edition_id));
    if fs::symlink_metadata(&path).is_err_and(|e| e.kind() == std::io::ErrorKind::NotFound) {
        return Ok(None);
    }
    j.reconcile_published(l, o.result.clone(), now, || {
        check_binding(&b)?;
        verify(&path, l, &o.result.manifest_sha256)?;
        Ok(())
    })?;
    let s = j.job_status(l.job_id).ok_or_else(conflict)?;
    flush_statuses(j)?;
    Ok(Some(s))
}
/// Retry committed snapshots without rebuilding. An unavailable/foreign root stops writes.
pub fn flush_statuses(j: &mut PressJournal) -> Result<(), PressError> {
    j.ensure_healthy()?;
    for s in j.statuses() {
        let b = j.binding_for_request(s.request_id).ok_or_else(conflict)?;
        check_binding(&b)?;
        let parent = b.root.join(".deckle-press/status");
        directory(&parent, true)?;
        let dest = parent.join(format!("{}.json", s.request_id));
        if let Ok(meta) = fs::symlink_metadata(&dest) {
            if meta.len() > MAX_CONTROL_BYTES as u64 || !meta.file_type().is_file() {
                return Err(conflict());
            }
            let existing = decode_status(
                &regular(&dest)?,
                Path::new(&format!(".deckle-press/status/{}.json", s.request_id)),
                &b,
            )?;
            if existing.revision > s.revision || (existing.revision == s.revision && existing != s)
            {
                return Err(conflict());
            }
            if existing == s {
                continue;
            }
        }
        let temp = parent.join(format!(".{}-{}.tmp", s.request_id, Uuid::new_v4()));
        let bytes = serde_json::to_vec(&s).map_err(|_| storage())?;
        write_new(&temp, &bytes)?;
        check_binding(&b)?;
        directory(&parent, false)?;
        fs::rename(temp, dest).map_err(|_| storage())?;
        sync_dir(&parent)?;
    }
    Ok(())
}

/// Return only the journal-intended and completely verified private archive. It may
/// be republished under a valid lease/deadline, avoiding a repeated source fetch.
pub fn archived_artifact(
    j: &PressJournal,
    l: &JobLease,
) -> Result<Option<FreshArtifact>, PressError> {
    j.ensure_healthy()?;
    let Some(o) = j
        .outputs()
        .into_iter()
        .find(|o| o.lease.job_id == l.job_id && o.removed_at.is_none())
    else {
        return Ok(None);
    };
    for dir in [
        j.local_root().join("archive"),
        j.local_root().join("archive").join(l.paper_id.to_string()),
    ] {
        match fs::symlink_metadata(&dir) {
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(None),
            Err(_) => return Err(storage()),
            Ok(_) => directory(&dir, false)?,
        }
    }
    let bundle = archive(j, l, false)?;
    if fs::symlink_metadata(&bundle).is_err_and(|e| e.kind() == std::io::ErrorKind::NotFound) {
        return Ok(None);
    }
    verify(&bundle, l, &o.result.manifest_sha256)?;
    Ok(Some(FreshArtifact {
        bundle,
        manifest_sha256: o.result.manifest_sha256,
    }))
}
