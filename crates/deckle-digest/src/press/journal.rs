// SPDX-License-Identifier: GPL-3.0-or-later
//! Private local snapshots. Keep this directory outside all synced exports.
use super::queue::{JournalData, validate_data};
use super::{PressError, PressErrorCode};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{
    fs::{self, File, OpenOptions},
    io::Write,
    path::{Path, PathBuf},
};

pub(crate) fn storage() -> PressError {
    PressError::new(PressErrorCode::StorageUnavailable)
}
#[derive(Serialize, Deserialize)]
struct Snapshot {
    version: u32,
    sequence: u64,
    payload: String,
    sha256: String,
}
/// Owns the local supervisor lock until dropped. Shared files are never locks.
/// One instance covers all configured papers, ensuring globally exclusive admission.
pub struct PressJournal {
    root: PathBuf,
    _lock: File,
    pub(crate) data: JournalData,
    sequence: u64,
    healthy: bool,
}
impl PressJournal {
    pub fn open(root: &Path) -> Result<Self, PressError> {
        fs::create_dir_all(root).map_err(|_| storage())?;
        let lock = OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .truncate(false)
            .open(root.join("supervisor.lock"))
            .map_err(|_| storage())?;
        lock.try_lock().map_err(|e| match e {
            std::fs::TryLockError::WouldBlock => PressError::new(PressErrorCode::Busy),
            _ => storage(),
        })?;
        let mut found = false;
        let mut snapshots = Vec::new();
        for name in ["journal.json", "journal.backup.json"] {
            let path = root.join(name);
            match fs::read(path) {
                Ok(bytes) => {
                    found = true;
                    if let Ok(snapshot) = serde_json::from_slice::<Snapshot>(&bytes)
                        && snapshot.version == 1
                        && snapshot.sequence > 0
                        && snapshot.sha256 == checksum(snapshot.payload.as_bytes())
                        && let Ok(data) = serde_json::from_str::<JournalData>(&snapshot.payload)
                        && validate_data(&data)
                    {
                        snapshots.push((snapshot.sequence, data));
                    }
                }
                Err(e) if e.kind() == std::io::ErrorKind::NotFound => (),
                Err(_) => return Err(storage()),
            }
        }
        snapshots.sort_by_key(|s| s.0);
        let (sequence, data) = match snapshots.pop() {
            Some(v) => v,
            None if !found => (0, JournalData::default()),
            None => return Err(storage()),
        };
        Ok(Self {
            root: root.to_owned(),
            _lock: lock,
            data,
            sequence,
            healthy: true,
        })
    }
    pub(crate) fn local_root(&self) -> &Path {
        &self.root
    }
    pub(crate) fn commit(&mut self, data: JournalData) -> Result<(), PressError> {
        self.ensure_healthy()?;
        let payload = serde_json::to_string(&data).map_err(|_| storage())?;
        let sequence = self.sequence.checked_add(1).ok_or_else(storage)?;
        let snapshot = Snapshot {
            version: 1,
            sequence,
            sha256: checksum(payload.as_bytes()),
            payload,
        };
        let bytes = serde_json::to_vec(&snapshot).map_err(|_| storage())?;
        // The backup is the same new revision, not an older state that can resurrect a lease.
        // A crash between the two renames is recovered by choosing the highest valid sequence.
        self.healthy = false;
        self.atomic("journal.backup.json", &bytes)?;
        self.data = data;
        self.sequence = sequence;
        self.atomic("journal.json", &bytes)?;
        self.healthy = true;
        Ok(())
    }
    pub(crate) fn ensure_healthy(&self) -> Result<(), PressError> {
        if self.healthy { Ok(()) } else { Err(storage()) }
    }
    fn atomic(&self, name: &str, bytes: &[u8]) -> Result<(), PressError> {
        let temporary = self.root.join(format!("{name}.tmp"));
        let mut file = OpenOptions::new()
            .write(true)
            .create(true)
            .truncate(true)
            .open(&temporary)
            .map_err(|_| storage())?;
        file.write_all(bytes)
            .and_then(|_| file.sync_all())
            .map_err(|_| storage())?;
        fs::rename(temporary, self.root.join(name)).map_err(|_| storage())?;
        #[cfg(unix)]
        File::open(&self.root)
            .and_then(|dir| dir.sync_all())
            .map_err(|_| storage())?;
        Ok(())
    }
}

fn checksum(bytes: &[u8]) -> String {
    Sha256::digest(bytes)
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect()
}
