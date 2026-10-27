// SPDX-License-Identifier: GPL-3.0-or-later
//! Producer-recorded fresh outputs only; shared requests are never removed.
use super::*;
use super::{
    journal::storage,
    publish::{archive, binding, check_binding, directory, editions, sync_dir, verify},
};
use chrono::{DateTime, Duration, Utc};
use std::fs;

fn remove_bundle(path: &std::path::Path, o: &super::queue::OutputRecord) -> Result<(), PressError> {
    match fs::symlink_metadata(path) {
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(()),
        Err(_) => return Err(storage()),
        Ok(_) => (),
    }
    verify(path, &o.lease, &o.result.manifest_sha256)?;
    fs::remove_dir_all(path).map_err(|_| storage())?;
    sync_dir(path.parent().ok_or_else(super::publish::conflict)?)
}
/// Keep seven newest per folder, every result under 48 hours, and all unpublished
/// intents. Tombstones are committed and shared before deleting either owned copy.
pub fn prune_results(j: &mut PressJournal, now: DateTime<Utc>) -> Result<(), PressError> {
    j.ensure_healthy()?;
    for status in j.statuses() {
        check_binding(
            &j.binding_for_request(status.request_id)
                .ok_or_else(super::publish::conflict)?,
        )?;
    }
    let mut outputs = j.outputs();
    outputs.sort_by_key(|o| std::cmp::Reverse((o.published_at, o.result.edition_id)));
    let mut counts = std::collections::HashMap::new();
    for o in outputs {
        let active = j.active();
        for stage in &o.stages {
            if !j.job_terminal(o.lease.job_id)
                && active
                    .as_ref()
                    .is_none_or(|l| l.job_id == o.lease.job_id && l.generation == stage.generation)
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
                    fs::remove_dir_all(path).map_err(|_| storage())?;
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
                if let Some(artifact) = archived_artifact(j, &o.lease)? {
                    remove_bundle(&artifact.bundle, &o)?;
                }
                j.forget_intent(o.lease.job_id)?;
            }
            continue;
        };
        let count = counts.entry(o.lease.root.clone()).or_insert(0usize);
        let remove = if o.removed_at.is_some() {
            !o.deleted
        } else {
            *count += 1;
            *count > 7 && now - at >= Duration::hours(48)
        };
        if !remove {
            continue;
        }
        let b = binding(&o.lease);
        check_binding(&b)?;
        if o.removed_at.is_none() {
            j.mark_removed(o.result.edition_id, now)?;
        }
        flush_statuses(j)?;
        // Recheck after shared writes. A replaced/foreign folder cannot authorize deletion.
        let parent = editions(&b, false)?;
        remove_bundle(&parent.join(format!("{}.deckle", o.result.edition_id)), &o)?;
        let local = archive(j, &o.lease, false)?;
        remove_bundle(&local, &o)?;
        j.mark_deleted(o.result.edition_id, now)?;
    }
    flush_statuses(j)?;
    j.forget_old(now, |b, id| {
        check_binding(b)?;
        let parent = b.root.join(".deckle-press/status");
        directory(&parent, false)?;
        let path = parent.join(format!("{id}.json"));
        match fs::symlink_metadata(&path) {
            Ok(m) if m.file_type().is_file() => {
                fs::remove_file(path).map_err(|_| storage())?;
                sync_dir(&parent)
            }
            Ok(_) => Err(PressError::new(PressErrorCode::ProtocolConflict)),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
            Err(_) => Err(storage()),
        }
    })
}
