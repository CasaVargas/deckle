// SPDX-License-Identifier: GPL-3.0-or-later
//! Incremental, bounded scans of untrusted shared request controls.
use super::*;
use sha2::{Digest, Sha256};
use std::{
    collections::HashMap,
    fs,
    io::Read,
    path::{Path, PathBuf},
};

#[derive(Debug)]
pub enum ScanEvent {
    Request {
        request: PressRequest,
        hash: String,
        rejection: Option<PressErrorCode>,
    },
    Diagnostic {
        path: PathBuf,
        code: PressErrorCode,
    },
}
#[derive(Default)]
pub struct ScanCursor {
    root: Option<PathBuf>,
    readers: Option<fs::ReadDir>,
    files: Option<fs::ReadDir>,
    reader: Option<PathBuf>,
    malformed: HashMap<PathBuf, (String, DateTime<Utc>, bool)>,
    examined: usize,
}
impl ScanCursor {
    pub fn examined(&self) -> usize {
        self.examined
    }
}
fn safe_dir(root: &Path, relative: &Path) -> bool {
    if root.canonicalize().ok().as_deref() != Some(root) {
        return false;
    }
    let mut p = root.to_owned();
    for c in relative.components() {
        if !matches!(c, std::path::Component::Normal(_)) {
            return false;
        }
        p.push(c);
        if !fs::symlink_metadata(&p).is_ok_and(|m| m.is_dir() && !m.file_type().is_symlink()) {
            return false;
        }
        #[cfg(windows)]
        {
            use std::os::windows::fs::MetadataExt;
            if fs::symlink_metadata(&p).is_ok_and(|m| m.file_attributes() & 0x400 != 0) {
                return false;
            }
        }
    }
    true
}
/// Each directory entry, including invalid client folders, counts toward the tick budget.
/// Iterators survive ticks so old controls cannot starve newly arriving requests.
pub fn scan_requests(
    binding: &PressBinding,
    cursor: &mut ScanCursor,
    now: DateTime<Utc>,
) -> Vec<ScanEvent> {
    let base = Path::new(".deckle-press/requests");
    if cursor.root.as_ref() != Some(&binding.root) {
        *cursor = ScanCursor {
            root: Some(binding.root.clone()),
            ..Default::default()
        };
    }
    cursor.examined = 0;
    if !safe_dir(&binding.root, base) {
        cursor.readers = None;
        cursor.files = None;
        return vec![];
    }
    if cursor.readers.is_none() {
        cursor.readers = fs::read_dir(binding.root.join(base)).ok();
    }
    let mut events = Vec::new();
    while cursor.examined < 100 {
        let candidate = if let Some(files) = cursor.files.as_mut() {
            match files.next() {
                Some(e) => Some(e),
                None => {
                    cursor.files = None;
                    cursor.reader = None;
                    continue;
                }
            }
        } else {
            let Some(readers) = cursor.readers.as_mut() else {
                break;
            };
            let Some(entry) = readers.next() else {
                cursor.readers = None;
                break;
            };
            cursor.examined += 1;
            if let Ok(entry) = entry {
                let name = entry.file_name();
                if name
                    .to_str()
                    .and_then(|s| Uuid::parse_str(s).ok())
                    .is_some()
                {
                    let relative = base.join(&name);
                    if safe_dir(&binding.root, &relative) {
                        cursor.files = fs::read_dir(binding.root.join(&relative)).ok();
                        cursor.reader = Some(relative);
                    }
                }
            }
            continue;
        };
        cursor.examined += 1;
        let Some(Ok(entry)) = candidate else { continue };
        let relative = cursor.reader.as_ref().unwrap().join(entry.file_name());
        let Some(name) = relative.file_name().and_then(|v| v.to_str()) else {
            continue;
        };
        if name
            .strip_suffix(".json")
            .and_then(|s| Uuid::parse_str(s).ok())
            .is_none()
        {
            continue;
        }
        let Ok(bytes) = read_control(&binding.root, &relative) else {
            continue;
        };
        let hash = Sha256::digest(&bytes)
            .iter()
            .map(|b| format!("{b:02x}"))
            .collect::<String>();
        let decoded = decode_request(&bytes, &relative, binding, now);
        let valid = match decoded {
            Ok(request) => Some((request, None)),
            Err(ref e)
                if matches!(
                    e.code,
                    PressErrorCode::Expired | PressErrorCode::ClockMismatch
                ) =>
            {
                serde_json::from_slice::<PressRequest>(&bytes)
                    .ok()
                    .map(|r| (r, Some(e.code)))
            }
            _ => None,
        };
        if let Some((request, rejection)) = valid {
            cursor.malformed.remove(&relative);
            events.push(ScanEvent::Request {
                request,
                hash,
                rejection,
            });
        } else {
            // Bound diagnostic bookkeeping as well as work. A large hostile directory may
            // evict old diagnostics, but never affects valid request discovery/admission.
            if cursor.malformed.len() >= 4096
                && !cursor.malformed.contains_key(&relative)
                && let Some(old) = cursor
                    .malformed
                    .iter()
                    .min_by_key(|(_, v)| v.1)
                    .map(|(p, _)| p.clone())
            {
                cursor.malformed.remove(&old);
            }
            let record =
                cursor
                    .malformed
                    .entry(relative.clone())
                    .or_insert((hash.clone(), now, false));
            if record.0 != hash {
                *record = (hash, now, false);
            }
            if !record.2 && now - record.1 >= chrono::Duration::seconds(60) {
                record.2 = true;
                events.push(ScanEvent::Diagnostic {
                    path: relative,
                    code: PressErrorCode::Unsupported,
                });
            }
        }
    }
    events
}
/// Open each component without following links; read only regular bounded controls.
/// On Unix openat pins ancestors; on Windows handles deny deletion while descendants open.
pub fn read_control(root: &Path, relative: &Path) -> Result<Vec<u8>, PressError> {
    read_bounded(root, relative, MAX_CONTROL_BYTES)
}
pub fn read_bounded(root: &Path, relative: &Path, limit: usize) -> Result<Vec<u8>, PressError> {
    let error = || PressError::new(PressErrorCode::StorageUnavailable);
    if !root.is_absolute()
        || relative
            .components()
            .any(|c| !matches!(c, std::path::Component::Normal(_)))
    {
        return Err(error());
    }
    let mut file = open_beneath(root, relative).map_err(|_| error())?;
    let meta = file.metadata().map_err(|_| error())?;
    if !meta.is_file() || meta.len() > limit as u64 {
        return Err(error());
    }
    let mut bytes = Vec::new();
    (&mut file)
        .take((limit + 1) as u64)
        .read_to_end(&mut bytes)
        .map_err(|_| error())?;
    if bytes.len() > limit {
        return Err(error());
    }
    Ok(bytes)
}
#[cfg(unix)]
fn open_beneath(root: &Path, relative: &Path) -> std::io::Result<fs::File> {
    use std::{
        ffi::CString,
        os::{
            fd::{AsRawFd, FromRawFd},
            unix::ffi::OsStrExt,
        },
    };
    let mut parent = fs::File::open("/")?;
    let components: Vec<_> = root
        .components()
        .skip(1)
        .chain(relative.components())
        .collect();
    for (i, c) in components.iter().enumerate() {
        let std::path::Component::Normal(name) = c else {
            return Err(std::io::ErrorKind::InvalidInput.into());
        };
        let name = CString::new(name.as_bytes()).map_err(|_| std::io::ErrorKind::InvalidInput)?;
        let flags = libc::O_RDONLY
            | libc::O_CLOEXEC
            | libc::O_NOFOLLOW
            | libc::O_NONBLOCK
            | if i + 1 < components.len() {
                libc::O_DIRECTORY
            } else {
                0
            };
        // SAFETY: parent owns a live descriptor, name is NUL terminated, and a successful
        // openat transfers exactly one new descriptor to File. No shared mutable memory.
        let fd = unsafe { libc::openat(parent.as_raw_fd(), name.as_ptr(), flags) };
        if fd < 0 {
            return Err(std::io::Error::last_os_error());
        }
        parent = unsafe { fs::File::from_raw_fd(fd) };
    }
    Ok(parent)
}
#[cfg(windows)]
fn open_beneath(root: &Path, relative: &Path) -> std::io::Result<fs::File> {
    use std::os::windows::fs::{MetadataExt, OpenOptionsExt};
    use windows_sys::Win32::Storage::FileSystem::{
        FILE_ATTRIBUTE_REPARSE_POINT, FILE_FLAG_BACKUP_SEMANTICS, FILE_FLAG_OPEN_REPARSE_POINT,
        FILE_SHARE_READ, FILE_SHARE_WRITE,
    };
    let mut held = Vec::new();
    let mut path = PathBuf::new();
    let components: Vec<_> = root.components().chain(relative.components()).collect();
    for (i, c) in components.iter().enumerate() {
        path.push(c);
        if !matches!(c, std::path::Component::Normal(_)) {
            continue;
        }
        let file = fs::OpenOptions::new()
            .read(true)
            .share_mode(FILE_SHARE_READ | FILE_SHARE_WRITE)
            .custom_flags(FILE_FLAG_OPEN_REPARSE_POINT | FILE_FLAG_BACKUP_SEMANTICS)
            .open(&path)?;
        let m = file.metadata()?;
        if m.file_attributes() & FILE_ATTRIBUTE_REPARSE_POINT != 0
            || (i + 1 < components.len() && !m.is_dir())
        {
            return Err(std::io::ErrorKind::InvalidInput.into());
        }
        held.push(file);
    }
    held.pop()
        .ok_or_else(|| std::io::ErrorKind::InvalidInput.into())
}
#[cfg(not(any(unix, windows)))]
fn open_beneath(_: &Path, _: &Path) -> std::io::Result<fs::File> {
    Err(std::io::ErrorKind::Unsupported.into())
}
