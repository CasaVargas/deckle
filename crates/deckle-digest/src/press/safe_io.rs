// SPDX-License-Identifier: GPL-3.0-or-later
//! Pinned, no-follow shared directory operations. No absolute-path mutation on Unix.
use super::{PressError, journal::storage, publish::conflict};
#[cfg(windows)]
use std::fs;
use std::{
    fs::File,
    io::{Read, Write},
    path::{Component, Path, PathBuf},
};

pub(crate) struct Dir {
    path: PathBuf,
    file: File,
    // Holding every ancestor matters on Windows: denying delete sharing prevents rename
    // or reparse replacement until all descendant operations finish.
    #[cfg(windows)]
    _ancestors: Vec<File>,
}
impl Dir {
    pub(crate) fn open(path: &Path) -> Result<Self, PressError> {
        if !path.is_absolute() {
            return Err(conflict());
        }
        #[cfg(unix)]
        {
            let mut dir = Self {
                path: PathBuf::from("/"),
                file: File::open("/").map_err(|_| storage())?,
            };
            for part in path.components().skip(1) {
                let Component::Normal(name) = part else {
                    return Err(conflict());
                };
                dir = dir.child_unchecked(Path::new(name), false)?;
            }
            Ok(dir)
        }
        #[cfg(windows)]
        {
            use std::os::windows::fs::{MetadataExt, OpenOptionsExt};
            use windows_sys::Win32::Storage::FileSystem::{
                FILE_ATTRIBUTE_REPARSE_POINT, FILE_FLAG_BACKUP_SEMANTICS,
                FILE_FLAG_OPEN_REPARSE_POINT, FILE_SHARE_READ, FILE_SHARE_WRITE,
            };
            let mut current = PathBuf::new();
            let mut held = Vec::new();
            for part in path.components() {
                current.push(part);
                if matches!(part, Component::Prefix(_)) {
                    continue;
                }
                if !matches!(part, Component::Normal(_) | Component::RootDir) {
                    return Err(conflict());
                }
                let file = fs::OpenOptions::new()
                    .read(true)
                    .share_mode(FILE_SHARE_READ | FILE_SHARE_WRITE)
                    .custom_flags(FILE_FLAG_BACKUP_SEMANTICS | FILE_FLAG_OPEN_REPARSE_POINT)
                    .open(&current)
                    .map_err(|_| storage())?;
                let meta = file.metadata().map_err(|_| storage())?;
                if !meta.is_dir() || meta.file_attributes() & FILE_ATTRIBUTE_REPARSE_POINT != 0 {
                    return Err(conflict());
                }
                held.push(file);
            }
            let file = held.pop().ok_or_else(conflict)?;
            Ok(Self {
                path: path.into(),
                file,
                _ancestors: held,
            })
        }
        #[cfg(not(any(unix, windows)))]
        {
            Err(super::PressError::new(super::PressErrorCode::Unsupported))
        }
    }
    pub(crate) fn object_id(&self) -> Result<(u64, u64), PressError> {
        identity(&self.file)
    }
    pub(crate) fn validate(&self) -> Result<(), PressError> {
        let current = Self::open(&self.path)?;
        if identity(&self.file)? != identity(&current.file)? {
            return Err(conflict());
        }
        Ok(())
    }
    pub(crate) fn child(&self, name: &Path, create: bool) -> Result<Self, PressError> {
        self.validate()?;
        let child = self.child_unchecked(name, create)?;
        child.validate()?;
        Ok(child)
    }
    fn child_unchecked(&self, name: &Path, create: bool) -> Result<Self, PressError> {
        component(name)?;
        #[cfg(unix)]
        {
            use std::os::fd::{AsRawFd, FromRawFd};
            let name_c = cstr(name)?;
            if create {
                // SAFETY: live directory descriptor and NUL-terminated single component.
                let result =
                    unsafe { libc::mkdirat(self.file.as_raw_fd(), name_c.as_ptr(), 0o700) };
                if result != 0
                    && std::io::Error::last_os_error().kind() != std::io::ErrorKind::AlreadyExists
                {
                    return Err(storage());
                }
            }
            let fd = unsafe {
                libc::openat(
                    self.file.as_raw_fd(),
                    name_c.as_ptr(),
                    libc::O_RDONLY | libc::O_CLOEXEC | libc::O_DIRECTORY | libc::O_NOFOLLOW,
                )
            };
            if fd < 0 {
                return Err(storage());
            }
            let child = Self {
                path: self.path.join(name),
                file: unsafe { File::from_raw_fd(fd) },
            };
            if create {
                self.sync()?;
            }
            Ok(child)
        }
        #[cfg(windows)]
        {
            self.validate()?;
            let path = self.path.join(name);
            if create {
                match fs::create_dir(&path) {
                    Ok(()) => (),
                    Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => (),
                    Err(_) => return Err(storage()),
                }
            }
            Self::open(&path)
        }
        #[cfg(not(any(unix, windows)))]
        {
            Err(super::PressError::new(super::PressErrorCode::Unsupported))
        }
    }
    pub(crate) fn read(&self, name: &Path, limit: usize) -> Result<Vec<u8>, PressError> {
        let mut file = self.open_file(name, false)?;
        let meta = file.metadata().map_err(|_| storage())?;
        if !meta.is_file() || meta.len() > limit as u64 {
            return Err(conflict());
        }
        let mut bytes = Vec::new();
        (&mut file)
            .take(limit as u64 + 1)
            .read_to_end(&mut bytes)
            .map_err(|_| storage())?;
        if bytes.len() > limit {
            return Err(conflict());
        }
        Ok(bytes)
    }
    pub(crate) fn write_new(&self, name: &Path, bytes: &[u8]) -> Result<(), PressError> {
        self.validate()?;
        let mut file = self.open_file(name, true)?;
        file.write_all(bytes)
            .and_then(|()| file.sync_all())
            .map_err(|_| storage())
    }
    pub(crate) fn open_file(&self, name: &Path, create: bool) -> Result<File, PressError> {
        component(name)?;
        if create {
            self.validate()?;
        }
        #[cfg(unix)]
        {
            use std::os::fd::{AsRawFd, FromRawFd};
            let name = cstr(name)?;
            let flags = libc::O_CLOEXEC
                | libc::O_NOFOLLOW
                | libc::O_NONBLOCK
                | if create {
                    libc::O_WRONLY | libc::O_CREAT | libc::O_EXCL
                } else {
                    libc::O_RDONLY
                };
            let fd = unsafe { libc::openat(self.file.as_raw_fd(), name.as_ptr(), flags, 0o600) };
            if fd < 0 {
                return Err(storage());
            }
            Ok(unsafe { File::from_raw_fd(fd) })
        }
        #[cfg(windows)]
        {
            use std::os::windows::fs::{MetadataExt, OpenOptionsExt};
            use windows_sys::Win32::Storage::FileSystem::{
                FILE_ATTRIBUTE_REPARSE_POINT, FILE_FLAG_OPEN_REPARSE_POINT, FILE_SHARE_READ,
            };
            let f = fs::OpenOptions::new()
                .read(!create)
                .write(create)
                .create_new(create)
                .share_mode(FILE_SHARE_READ)
                .custom_flags(FILE_FLAG_OPEN_REPARSE_POINT)
                .open(self.path.join(name))
                .map_err(|_| storage())?;
            if f.metadata().map_err(|_| storage())?.file_attributes() & FILE_ATTRIBUTE_REPARSE_POINT
                != 0
            {
                return Err(conflict());
            }
            Ok(f)
        }
        #[cfg(not(any(unix, windows)))]
        {
            Err(super::PressError::new(super::PressErrorCode::Unsupported))
        }
    }
    pub(crate) fn rename(&self, from: &Path, to: &Path) -> Result<(), PressError> {
        component(from)?;
        component(to)?;
        self.validate()?;
        #[cfg(unix)]
        {
            use std::os::fd::AsRawFd;
            let (from, to) = (cstr(from)?, cstr(to)?);
            if unsafe {
                libc::renameat(
                    self.file.as_raw_fd(),
                    from.as_ptr(),
                    self.file.as_raw_fd(),
                    to.as_ptr(),
                )
            } != 0
            {
                return Err(storage());
            }
        }
        #[cfg(windows)]
        fs::rename(self.path.join(from), self.path.join(to)).map_err(|_| storage())?;
        #[cfg(not(any(unix, windows)))]
        return Err(super::PressError::new(super::PressErrorCode::Unsupported));
        self.validate()?;
        self.sync()
    }
    pub(crate) fn link_new(&self, from: &Path, to: &Path) -> Result<(), PressError> {
        component(from)?;
        component(to)?;
        self.validate()?;
        #[cfg(unix)]
        {
            use std::os::fd::AsRawFd;
            let (from, to) = (cstr(from)?, cstr(to)?);
            if unsafe {
                libc::linkat(
                    self.file.as_raw_fd(),
                    from.as_ptr(),
                    self.file.as_raw_fd(),
                    to.as_ptr(),
                    0,
                )
            } != 0
            {
                return Err(
                    if std::io::Error::last_os_error().kind() == std::io::ErrorKind::AlreadyExists {
                        conflict()
                    } else {
                        storage()
                    },
                );
            }
        }
        #[cfg(windows)]
        fs::hard_link(self.path.join(from), self.path.join(to)).map_err(|e| {
            if e.kind() == std::io::ErrorKind::AlreadyExists {
                conflict()
            } else {
                storage()
            }
        })?;
        #[cfg(not(any(unix, windows)))]
        return Err(super::PressError::new(super::PressErrorCode::Unsupported));
        self.validate()?;
        self.sync()
    }
    pub(crate) fn remove(&self, name: &Path, directory: bool) -> Result<(), PressError> {
        component(name)?;
        self.validate()?;
        #[cfg(unix)]
        {
            use std::os::fd::AsRawFd;
            let name = cstr(name)?;
            if unsafe {
                libc::unlinkat(
                    self.file.as_raw_fd(),
                    name.as_ptr(),
                    if directory { libc::AT_REMOVEDIR } else { 0 },
                )
            } != 0
            {
                return Err(storage());
            }
        }
        #[cfg(windows)]
        {
            let path = self.path.join(name);
            if directory {
                fs::remove_dir(path)
            } else {
                fs::remove_file(path)
            }
            .map_err(|_| storage())?;
        }
        #[cfg(not(any(unix, windows)))]
        return Err(super::PressError::new(super::PressErrorCode::Unsupported));
        self.sync()
    }
    pub(crate) fn sync(&self) -> Result<(), PressError> {
        #[cfg(unix)]
        self.file.sync_all().map_err(|_| storage())?;
        Ok(())
    }
}
fn component(path: &Path) -> Result<(), PressError> {
    let mut parts = path.components();
    if !matches!(parts.next(), Some(Component::Normal(_))) || parts.next().is_some() {
        return Err(conflict());
    }
    Ok(())
}
#[cfg(unix)]
fn cstr(path: &Path) -> Result<std::ffi::CString, PressError> {
    use std::os::unix::ffi::OsStrExt;
    std::ffi::CString::new(path.as_os_str().as_bytes()).map_err(|_| conflict())
}
fn identity(file: &File) -> Result<(u64, u64), PressError> {
    #[cfg(unix)]
    {
        use std::os::unix::fs::MetadataExt;
        let m = file.metadata().map_err(|_| storage())?;
        Ok((m.dev(), m.ino()))
    }
    #[cfg(windows)]
    {
        use std::os::windows::io::AsRawHandle;
        use windows_sys::Win32::Storage::FileSystem::{
            BY_HANDLE_FILE_INFORMATION, GetFileInformationByHandle,
        };
        let mut info = std::mem::MaybeUninit::<BY_HANDLE_FILE_INFORMATION>::uninit();
        if unsafe { GetFileInformationByHandle(file.as_raw_handle(), info.as_mut_ptr()) } == 0 {
            return Err(storage());
        }
        let info = unsafe { info.assume_init() };
        Ok((
            u64::from(info.dwVolumeSerialNumber),
            (u64::from(info.nFileIndexHigh) << 32) | u64::from(info.nFileIndexLow),
        ))
    }
    #[cfg(not(any(unix, windows)))]
    {
        Err(super::PressError::new(super::PressErrorCode::Unsupported))
    }
}

#[cfg(all(test, unix))]
mod tests {
    use super::*;
    #[test]
    fn replacement_cannot_redirect_setup_status_publication_or_deletion() {
        for operation in ["mkdir", "write", "rename", "link", "remove"] {
            let temp = tempfile::tempdir().unwrap();
            let root = temp.path().canonicalize().unwrap();
            let current = root.join("control");
            std::fs::create_dir(&current).unwrap();
            std::fs::write(current.join("staging"), b"owned").unwrap();
            let pinned = Dir::open(&current).unwrap();
            let outside = root.join("outside");
            std::fs::rename(&current, &outside).unwrap();
            std::os::unix::fs::symlink(&outside, &current).unwrap();
            let result = match operation {
                "mkdir" => pinned.child(Path::new("editions"), true).map(|_| ()),
                "write" => pinned.write_new(Path::new("new"), b"bytes"),
                "rename" => pinned.rename(Path::new("staging"), Path::new("published")),
                "link" => pinned.link_new(Path::new("staging"), Path::new("producer.json")),
                "remove" => pinned.remove(Path::new("staging"), false),
                _ => unreachable!(),
            };
            assert!(result.is_err(), "{operation} accepted replaced ancestor");
            assert_eq!(std::fs::read(outside.join("staging")).unwrap(), b"owned");
            assert_eq!(std::fs::read_dir(outside).unwrap().count(), 1);
        }
    }
}

#[cfg(all(test, windows))]
mod windows_tests {
    use super::*;
    #[test]
    fn held_ancestors_prevent_replacement_until_publication_finishes() {
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path().join("root");
        std::fs::create_dir_all(root.join("control/status")).unwrap();
        let pinned = Dir::open(&root.join("control/status")).unwrap();
        assert!(std::fs::rename(&root, temp.path().join("moved")).is_err());
        assert!(std::fs::rename(root.join("control"), root.join("moved-control")).is_err());
        pinned.write_new(Path::new("temp"), b"status").unwrap();
        pinned
            .rename(Path::new("temp"), Path::new("final"))
            .unwrap();
        assert_eq!(pinned.read(Path::new("final"), 16).unwrap(), b"status");
        drop(pinned);
        std::fs::rename(&root, temp.path().join("moved")).unwrap();
    }
}
