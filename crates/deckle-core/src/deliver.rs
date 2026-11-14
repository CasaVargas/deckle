// SPDX-License-Identifier: GPL-3.0-or-later
//! Stage 6: put the KFX on a folder or a mass-storage Kindle.

use std::io::Write;
use std::path::{Path, PathBuf};

use serde::Serialize;

use deckle_transport::session::{Mtp, MtpError, TransportError};
use deckle_transport::usb::MtpInfo;

use crate::inspect::BookInfo;

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct Device {
    pub root: PathBuf,
    /// First line of `system/version.txt`, else the volume name.
    pub model_hint: String,
    /// `system/thumbnails` exists and is writable: EBOK books can get a cover thumbnail from us.
    pub thumbnails_writable: bool,
    /// Firmware version from `system/version.txt` (`Kindle 5.16.2.1.1 (…)` → `5.16.2.1.1`).
    pub firmware: Option<String>,
    /// The USB serial number, when the OS says (see [`crate::usb_serial`]): what tells two
    /// Kindles of one model apart.
    pub serial: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub enum Target {
    Folder(PathBuf),
    MassStorage(Device),
    /// MTP-only Kindle (Scribe 2024, Colorsoft, Paperwhite 2024).
    Mtp(deckle_transport::usb::MtpInfo),
}

type MtpOpener = fn(&MtpInfo) -> Result<Mtp, TransportError>;

/// A target plus, for an MTP Kindle, one connection shared by every read and write of a book.
/// Closing a connection resets the Kindle (see `usb.rs`), which drops it to the home screen and
/// off the bus, so the Word Wise probe and the delivery must not each open their own.
pub struct Conn<'a> {
    pub target: &'a Target,
    mtp: Option<Mtp>,
    open: MtpOpener,
}

impl<'a> Conn<'a> {
    pub fn new(target: &'a Target) -> Self {
        Conn {
            target,
            mtp: None,
            open: deckle_transport::usb::open,
        }
    }

    /// A connection that opens MTP devices with `open` (tests script the device).
    #[cfg(test)]
    pub(crate) fn with_opener(target: &'a Target, open: MtpOpener) -> Self {
        Conn {
            target,
            mtp: None,
            open,
        }
    }

    /// The open MTP session, opening it on first use. `None` when the target is not MTP.
    pub fn mtp(&mut self) -> Option<Result<&mut Mtp, MtpError>> {
        let Target::Mtp(info) = self.target else {
            return None;
        };
        if self.mtp.is_none() {
            let opened = (self.open)(info)
                .map_err(MtpError::from)
                .and_then(|mut dev| dev.open_session().map(|()| dev));
            match opened {
                Ok(dev) => self.mtp = Some(dev),
                Err(e) => return Some(Err(e)),
            }
        }
        self.mtp.as_mut().map(Ok)
    }
}

impl Drop for Conn<'_> {
    fn drop(&mut self) {
        if let Some(dev) = &mut self.mtp {
            let _ = dev.close_session();
        }
    }
}

/// Extras to deliver alongside the book.
#[derive(Clone, Debug, Default)]
pub struct DeliverOptions {
    /// `(file name, JPEG bytes)` for `system/thumbnails/` on mass-storage Kindles.
    pub thumbnail: Option<(String, Vec<u8>)>,
    /// `(file name, bytes)` for `<book>.sdr/` next to the KFX (X-Ray, Word Wise).
    pub sidecars: Vec<(String, Vec<u8>)>,
    /// `(path under the device root, bytes)`, e.g. Deckle's Word Wise dictionary at
    /// `system/kll/kll.en.zh.klld`; written after the book and its sidecars.
    pub system_files: Vec<(Vec<String>, Vec<u8>)>,
}

#[derive(Debug, thiserror::Error)]
pub enum DeliverError {
    #[error("I/O error writing {path}: {source}")]
    Io {
        path: PathBuf,
        source: std::io::Error,
    },
    #[error("MTP: {0}")]
    Mtp(String),
    #[error("thumbnail: {0}")]
    Thumbnail(String),
    /// Not a book Deckle may touch, or not there (inventory remove).
    #[error("{0}")]
    Refused(String),
    /// The device is gone: the MTP session or cable failed, or the drive was unplugged. A push
    /// stops here instead of failing once per entry.
    #[error("{0}")]
    Gone(String),
}

impl DeliverError {
    /// The device itself is gone, so nothing more can go this session; any other error belongs
    /// to one file (the Kindle refused it or is full, the entry can't be read).
    pub fn stops_push(&self) -> bool {
        matches!(self, DeliverError::Gone(_))
    }

    /// [`From<MtpError>`] with the step and file it failed on.
    fn mtp_at(what: &str, e: MtpError) -> Self {
        match e {
            MtpError::Response { .. } => DeliverError::Mtp(format!("{what}: {e}")),
            _ => DeliverError::Gone(format!("MTP: {what}: {e}")),
        }
    }
}

/// An answer from the device (a refused file, a full Kindle) costs one file; anything else
/// means the session or the cable is gone, or the Kindle has no `documents` folder.
impl From<MtpError> for DeliverError {
    fn from(e: MtpError) -> Self {
        match e {
            MtpError::Response { .. } => DeliverError::Mtp(e.to_string()),
            _ => DeliverError::Gone(format!("MTP: {e}")),
        }
    }
}

/// A Kindle mounted as a drive is still there: `documents/` and `system/`, as [`detect_in`]
/// finds it. A fixed mount point leaves an empty folder after unplugging, which a write would
/// fill on the computer's own disk.
fn kindle_present(root: &Path) -> bool {
    root.join("documents").is_dir() && root.join("system").is_dir()
}

/// The drive refused a write as a whole: mounted read-only, or its folder isn't writable. Every
/// other book would fail the same way, so a push stops instead of trying each.
pub(crate) fn drive_refuses(e: &std::io::Error) -> bool {
    matches!(
        e.kind(),
        std::io::ErrorKind::PermissionDenied | std::io::ErrorKind::ReadOnlyFilesystem
    )
}

/// `Ok` while the mass-storage Kindle at `root` is still there; else the error that stops
/// whatever was writing to it. Every write to a mass-storage Kindle asks this first.
pub(crate) fn kindle_still_there(root: &Path) -> Result<(), DeliverError> {
    if kindle_present(root) {
        Ok(())
    } else {
        Err(DeliverError::Gone(format!(
            "the Kindle at {} is no longer connected",
            root.display()
        )))
    }
}

/// A delivered book: where it went, and what went wrong once it was in place (a sidecar, a
/// system file or the thumbnail that couldn't be written). The book itself is delivered.
#[derive(Clone, Debug)]
pub struct Delivered {
    pub path: PathBuf,
    pub warnings: Vec<String>,
}

const MAX_FILENAME_LEN: usize = 120;

/// Sort-form author name: `file-as` when present, else `Last, First`.
pub fn author_sort(info: &BookInfo) -> Option<String> {
    let c = info.creators.first()?;
    if let Some(fa) = &c.file_as
        && !fa.trim().is_empty()
    {
        return Some(fa.trim().to_owned());
    }
    Some(deckle_kfx::metadata::author_sort(&c.name))
}

/// ASCII-folded and safe as a file or folder name on every OS.
pub fn sanitize(s: &str) -> String {
    let ascii = deunicode::deunicode(s);
    ascii
        .chars()
        .map(|c| {
            if c.is_ascii_control()
                || matches!(c, '\\' | '/' | ':' | '*' | '?' | '"' | '<' | '>' | '|')
            {
                '_'
            } else {
                c
            }
        })
        .collect::<String>()
        .trim()
        .to_owned()
}

/// `<Author sort> - <Title>.kfx`, ASCII-folded, filesystem-safe, at most 120 characters.
pub fn device_filename(info: &BookInfo) -> String {
    let title = sanitize(&info.title);
    let stem = match author_sort(info)
        .map(|a| sanitize(&a))
        .filter(|a| !a.is_empty())
    {
        Some(author) => format!("{author} - {title}"),
        None => title,
    };
    let stem = if stem.is_empty() {
        "book".to_owned()
    } else {
        stem
    };
    let max_stem = MAX_FILENAME_LEN - ".kfx".len();
    let stem: String = stem.chars().take(max_stem).collect();
    format!("{}.kfx", stem.trim_end())
}

/// Formats a Kindle opens as they are, so `send` copies them without converting.
const DOCUMENT_EXTENSIONS: &[&str] = &["pdf", "txt", "azw3", "azw", "mobi", "prc", "kfx"];

/// The device name for a document sent as it is: its own name, made filesystem-safe like
/// [`device_filename`], with a lowercase extension. `None` for a format a Kindle can't open.
pub fn document_filename(file_name: &str) -> Option<String> {
    let (stem, ext) = file_name.rsplit_once('.')?;
    let ext = ext.to_ascii_lowercase();
    if !DOCUMENT_EXTENSIONS.contains(&ext.as_str()) {
        return None;
    }
    let stem = sanitize(stem);
    if stem.is_empty() {
        return None;
    }
    let stem: String = stem
        .chars()
        .take(MAX_FILENAME_LEN - ext.len() - 1)
        .collect();
    Some(format!("{}.{ext}", stem.trim_end()))
}

/// Kindle detection over explicit mount roots (the public `detect_mass_storage` wraps it).
pub fn detect_in(roots: &[PathBuf]) -> Vec<Device> {
    let mut out = Vec::new();
    for root in roots {
        let documents = root.join("documents");
        let system = root.join("system");
        if !(documents.is_dir() && system.is_dir()) {
            continue;
        }
        let version_line = std::fs::read_to_string(system.join("version.txt"))
            .ok()
            .and_then(|s| s.lines().next().map(|l| l.trim().to_owned()))
            .filter(|s| !s.is_empty());
        let firmware = version_line
            .as_deref()
            .and_then(crate::compat::parse_firmware);
        let model_hint = version_line.unwrap_or_else(|| {
            root.file_name()
                .map(|n| n.to_string_lossy().into_owned())
                .unwrap_or_else(|| "Kindle".into())
        });
        let thumbs = system.join("thumbnails");
        let thumbnails_writable = thumbs.is_dir() && {
            let probe = thumbs.join(".deckle-probe");
            let ok = std::fs::write(&probe, b"").is_ok();
            let _ = std::fs::remove_file(&probe);
            ok
        };
        out.push(Device {
            root: root.clone(),
            model_hint,
            thumbnails_writable,
            firmware,
            serial: None,
        });
    }
    out
}

/// Connected mass-storage Kindles: `/Volumes/*` on macOS, drive letters on Windows,
/// `/media/$USER/*` and `/run/media/$USER/*` on Linux.
pub fn detect_mass_storage() -> Vec<Device> {
    let mut devices = detect_in(&mount_roots());
    for d in &mut devices {
        d.serial = crate::usb_serial::serial_for_mount(&d.root);
    }
    devices
}

/// Where a USB drive may be mounted: `/Volumes/*` on macOS, drive letters on Windows,
/// `/media/$USER/*` and `/run/media/$USER/*` on Linux. Every reader that mounts as a drive is
/// looked for here (a Kindle, a Kobo).
///
/// `DECKLE_MOUNTS` (a path list, as `PATH` is written) replaces them: for a reader mounted
/// somewhere else, and for tests.
pub fn mount_roots() -> Vec<PathBuf> {
    if let Some(list) = std::env::var_os("DECKLE_MOUNTS").filter(|v| !v.is_empty()) {
        return std::env::split_paths(&list).collect();
    }
    let mut roots: Vec<PathBuf> = Vec::new();
    #[cfg(target_os = "macos")]
    {
        if let Ok(rd) = std::fs::read_dir("/Volumes") {
            roots.extend(rd.flatten().map(|e| e.path()));
        }
    }
    #[cfg(target_os = "windows")]
    {
        for letter in b'D'..=b'Z' {
            roots.push(PathBuf::from(format!("{}:\\", letter as char)));
        }
    }
    #[cfg(target_os = "linux")]
    {
        if let Some(user) = std::env::var_os("USER") {
            for base in ["/media", "/run/media"] {
                if let Ok(rd) = std::fs::read_dir(std::path::Path::new(base).join(&user)) {
                    roots.extend(rd.flatten().map(|e| e.path()));
                }
            }
        }
    }
    roots
}

/// Writes the KFX and returns its path. Mass storage gets `documents/<name>`, fsynced.
pub fn deliver(kfx: &[u8], info: &BookInfo, target: &Target) -> Result<PathBuf, DeliverError> {
    deliver_with(kfx, info, target, &DeliverOptions::default())
}

/// [`deliver`] plus the extras: thumbnails on mass storage (restoring older ones too).
pub fn deliver_with(
    kfx: &[u8],
    info: &BookInfo,
    target: &Target,
    opts: &DeliverOptions,
) -> Result<PathBuf, DeliverError> {
    deliver_on(kfx, info, &mut Conn::new(target), opts)
}

/// [`deliver_with`] over a connection the caller may already have used (Word Wise probe).
pub fn deliver_on(
    kfx: &[u8],
    info: &BookInfo,
    conn: &mut Conn<'_>,
    opts: &DeliverOptions,
) -> Result<PathBuf, DeliverError> {
    deliver_named_on(kfx, &device_filename(info), conn, opts)
}

/// [`deliver_on`] under a caller-chosen file name (dated editions use sortable names). What
/// went wrong after the book was in place goes to stderr; [`deliver_named_reporting`] returns it.
pub fn deliver_named_on(
    kfx: &[u8],
    name: &str,
    conn: &mut Conn<'_>,
    opts: &DeliverOptions,
) -> Result<PathBuf, DeliverError> {
    let d = deliver_named_reporting(kfx, name, conn, opts)?;
    for w in &d.warnings {
        eprintln!("{name}: {w}");
    }
    Ok(d.path)
}

/// [`deliver_named_on`], returning what went wrong once the book was in place as warnings: an
/// `Err` means the book itself didn't go, and the copy already there is as it was.
pub fn deliver_named_reporting(
    kfx: &[u8],
    name: &str,
    conn: &mut Conn<'_>,
    opts: &DeliverOptions,
) -> Result<Delivered, DeliverError> {
    let name = name.to_owned();
    let mut warnings = Vec::new();
    let mut warn = |r: Result<(), DeliverError>| {
        if let Err(e) = r {
            warnings.push(e.to_string());
        }
    };
    match conn.target {
        Target::Folder(dir) => {
            remove_leftovers(dir);
            let path = write_file(&dir.join(&name), kfx)?;
            warn(write_sidecars(&path, &opts.sidecars));
            warn(write_system_files(dir, &opts.system_files));
            Ok(Delivered { path, warnings })
        }
        Target::MassStorage(dev) => {
            kindle_still_there(&dev.root)?;
            remove_leftovers(&dev.root.join("documents"));
            let path = dev.root.join("documents").join(&name);
            // `documents/` itself refusing (mounted read-only) stops a push; one locked book is
            // that book's failure alone.
            write_beside_at(&path, kfx).map_err(|b| {
                if b.folder_refuses() {
                    DeliverError::Gone(format!(
                        "the Kindle at {} can't be written to: {}",
                        dev.root.display(),
                        b.into_inner()
                    ))
                } else if b.locked() {
                    DeliverError::Refused(format!(
                        "{name} on the Kindle is locked or read-only; unlock or delete it"
                    ))
                } else {
                    DeliverError::Io {
                        path: path.clone(),
                        source: b.into_inner(),
                    }
                }
            })?;
            warn(write_sidecars(&path, &opts.sidecars));
            warn(write_system_files(&dev.root, &opts.system_files));
            if let Some((thumb_name, jpeg)) = &opts.thumbnail {
                warn(
                    crate::thumbnail::install(&dev.root, thumb_name, jpeg)
                        .map(|_| ())
                        .map_err(|e| DeliverError::Thumbnail(e.to_string())),
                );
            }
            let _ = crate::thumbnail::sync(&dev.root);
            Ok(Delivered { path, warnings })
        }
        Target::Mtp(mtp) => {
            let serial = mtp.serial.clone();
            match conn.mtp() {
                Some(Ok(dev)) => deliver_mtp(kfx, &name, serial, dev, opts),
                Some(Err(e)) => Err(DeliverError::from(e)),
                None => unreachable!("an MTP target always has a connection"),
            }
        }
    }
}

/// Writes each file it can; the first error is returned once all were tried.
fn write_system_files(root: &Path, files: &[(Vec<String>, Vec<u8>)]) -> Result<(), DeliverError> {
    let mut first = Ok(());
    for (parts, bytes) in files {
        let path = parts.iter().fold(root.to_path_buf(), |p, s| p.join(s));
        if let Err(e) = write_file(&path, bytes)
            && first.is_ok()
        {
            first = Err(e);
        }
    }
    first
}

/// `<book>.sdr/<name>` files beside a delivered KFX (book first, sidecars after).
fn write_sidecars(book: &Path, sidecars: &[(String, Vec<u8>)]) -> Result<(), DeliverError> {
    let sdr = book.with_extension("sdr");
    // A re-run with another ASIN (or X-Ray / Word Wise off) must not leave yesterday's
    // sidecar behind.
    if let Ok(entries) = std::fs::read_dir(&sdr) {
        for e in entries.flatten() {
            if is_deckle_sidecar(&e.file_name().to_string_lossy()) {
                let _ = std::fs::remove_file(e.path());
            }
        }
    }
    if sidecars.is_empty() {
        return Ok(());
    }
    for (name, bytes) in sidecars {
        write_file(&sdr.join(name), bytes)?;
    }
    Ok(())
}

/// [`write_beside`], its error naming `path`.
pub(crate) fn write_file(path: &Path, data: &[u8]) -> Result<PathBuf, DeliverError> {
    write_beside(path, data).map_err(|source| DeliverError::Io {
        path: path.to_path_buf(),
        source,
    })?;
    Ok(path.to_path_buf())
}

/// Writes `data` to `path` so that `path` only ever holds the old file or the whole new one: the
/// bytes go to a hidden `.<name>.<pid>.part` beside it, flushed to the drive, then renamed over
/// it in one step. On any error the `.part` file is removed and the old file was never touched.
/// The process id keeps two Deckle processes off one temporary file.
pub(crate) fn write_beside(path: &Path, data: &[u8]) -> std::io::Result<()> {
    write_beside_at(path, data).map_err(Beside::into_inner)
}

/// Where [`write_beside_at`] failed.
#[derive(Debug)]
pub(crate) enum Beside {
    /// The folder refused the temporary file: nothing can be written there (mounted read-only,
    /// a folder that isn't writable).
    Folder(std::io::Error),
    /// The temporary file was made, then the write, the flush or the rename failed: this file
    /// alone (a locked book refuses the rename).
    File(std::io::Error),
}

impl Beside {
    pub(crate) fn into_inner(self) -> std::io::Error {
        match self {
            Beside::Folder(e) | Beside::File(e) => e,
        }
    }

    /// The device can't be written to at all: a push stops here.
    pub(crate) fn folder_refuses(&self) -> bool {
        matches!(self, Beside::Folder(e) if drive_refuses(e))
    }

    /// The book already there refused to be replaced: locked or read-only.
    pub(crate) fn locked(&self) -> bool {
        matches!(self, Beside::File(e) if e.kind() == std::io::ErrorKind::PermissionDenied)
    }
}

/// [`write_beside`], saying whether the folder or the file failed.
pub(crate) fn write_beside_at(path: &Path, data: &[u8]) -> Result<(), Beside> {
    let dir = path.parent().unwrap_or(Path::new(""));
    if !dir.as_os_str().is_empty() {
        std::fs::create_dir_all(dir).map_err(Beside::Folder)?;
    }
    let name = path
        .file_name()
        .ok_or_else(|| Beside::File(std::io::Error::other("no file name")))?
        .to_string_lossy();
    let mut part = dir.join(format!(".{name}.{}.part", std::process::id()));
    let mut f = match std::fs::File::create(&part) {
        Ok(f) => f,
        // A stale, read-only `.part` an earlier process with this id left: try one other name
        // (still digits, so a later cleanup finds it) before calling the folder unwritable.
        Err(e) if e.kind() == std::io::ErrorKind::PermissionDenied => {
            let nanos = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map_or(0, |d| d.as_nanos());
            part = dir.join(format!(".{name}.{nanos}.part"));
            std::fs::File::create(&part).map_err(Beside::Folder)?
        }
        Err(e) => return Err(Beside::Folder(e)),
    };
    let written = (|| {
        write_data(&mut f, data)?;
        f.sync_all()?;
        drop(f);
        #[cfg(test)]
        if FAIL_RENAME_OF.with(|f| f.borrow().as_deref() == Some(&*name)) {
            return Err(std::io::Error::from(std::io::ErrorKind::PermissionDenied));
        }
        std::fs::rename(&part, path)
    })();
    if written.is_err() {
        let _ = std::fs::remove_file(&part);
    }
    written.map_err(Beside::File)
}

#[cfg(test)]
thread_local! {
    /// Tests: the next writes stop with an error after this many bytes (a full drive, a pulled
    /// cable).
    pub(crate) static FAIL_WRITE_AFTER: std::cell::Cell<Option<usize>> =
        const { std::cell::Cell::new(None) };
    /// Tests: renaming onto this file name is refused as a locked file is (`chflags uchg` in
    /// Finder gives EPERM).
    pub(crate) static FAIL_RENAME_OF: std::cell::RefCell<Option<String>> =
        const { std::cell::RefCell::new(None) };
}

fn write_data(f: &mut std::fs::File, data: &[u8]) -> std::io::Result<()> {
    #[cfg(test)]
    if let Some(n) = FAIL_WRITE_AFTER.with(std::cell::Cell::get) {
        f.write_all(&data[..n.min(data.len())])?;
        return Err(std::io::Error::other("the write stopped half-way"));
    }
    f.write_all(data)
}

/// The extensions of what Deckle writes into `documents/` or `Deckle/`.
const BOOK_EXTENSIONS: &[&str] = &["kfx", "azw3", "azw", "mobi", "prc", "pdf", "txt", "epub"];

/// Only the shapes Deckle writes: `.<book>.<digits>.part` ([`write_beside`]) and `.<epub>.tmp`
/// (an older Deckle's Kobo and PocketBook deliveries), where `<book>` has a book's extension. A
/// folder the owner picked may hold their own `.notes.tmp`.
fn is_deckle_temp(file: &str) -> bool {
    let Some(rest) = file.strip_prefix('.') else {
        return false;
    };
    if let Some(book) = rest.strip_suffix(".tmp") {
        return book.ends_with(".epub");
    }
    let Some((book, pid)) = rest.strip_suffix(".part").and_then(|r| r.rsplit_once('.')) else {
        return false;
    };
    !pid.is_empty()
        && pid.bytes().all(|b| b.is_ascii_digit())
        && book
            .rsplit_once('.')
            .is_some_and(|(stem, ext)| !stem.is_empty() && BOOK_EXTENSIONS.contains(&ext))
}

/// A temporary file [`write_beside`] (`.<name>.<pid>.part`) or an older Deckle (`.<name>.tmp`)
/// left in `dir` when a cable was pulled, older than an hour, so another Deckle's live write is
/// spared. Run at the start of a delivery to that folder.
pub(crate) fn remove_leftovers(dir: &Path) {
    const AGE: std::time::Duration = std::time::Duration::from_secs(3600);
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for e in entries.flatten() {
        if !is_deckle_temp(&e.file_name().to_string_lossy()) {
            continue;
        }
        let old = e
            .metadata()
            .ok()
            .filter(|m| m.is_file())
            .and_then(|m| m.modified().ok())
            .and_then(|t| t.elapsed().ok())
            .is_some_and(|age| age > AGE);
        if old {
            let _ = std::fs::remove_file(e.path());
        }
    }
}

/// `XRAY.entities.<ASIN>.asc`
pub fn is_xray_sidecar(name: &str) -> bool {
    name.starts_with("XRAY.entities.") && name.ends_with(".asc")
}

/// A sidecar Deckle writes and replaces on every send: X-Ray or `LanguageLayer.<lang>.<ASIN>.kll`.
pub fn is_deckle_sidecar(name: &str) -> bool {
    is_xray_sidecar(name) || (name.starts_with("LanguageLayer.") && name.ends_with(".kll"))
}

/// Room left free on an MTP Kindle beyond what a send needs: it writes its own index files after.
const MTP_MARGIN: u64 = 1 << 20;

/// In an error when the copy already on the Kindle was removed before the send failed, which
/// happens only on a Kindle that can't rename over MTP.
pub const OLD_COPY_REMOVED: &str = "the old copy had been removed first";

/// `<name>.<n>.part`: a temporary name [`mtp_send_replacing`] gave a send.
fn is_part_of(file: &str, name: &str) -> bool {
    let lower = file.to_ascii_lowercase();
    lower
        .strip_prefix(&format!("{}.", name.to_ascii_lowercase()))
        .and_then(|rest| rest.strip_suffix(".part"))
        .is_some_and(|n| !n.is_empty() && n.bytes().all(|b| b.is_ascii_digit()))
}

/// The Kindle can rename a file: it lists SetObjectPropValue and GetObjectPropDesc, and says
/// a file's name can be set. Not known for any Kindle yet (`docs/devices.md`).
fn can_rename(dev: &mut Mtp) -> bool {
    use deckle_transport::ptp::{fmt, op, prop};
    dev.device_info().is_ok_and(|i| {
        i.supports(op::SET_OBJECT_PROP_VALUE) && i.supports(op::GET_OBJECT_PROP_DESC)
    }) && dev
        .object_prop_settable(prop::OBJECT_FILE_NAME, fmt::UNDEFINED)
        .unwrap_or(false)
}

/// Deletes `parent/<name>` if it's there; for cleaning up after a failure, so errors are moot.
fn remove_if_there(dev: &mut Mtp, storage: u32, parent: u32, name: &str) {
    if let Ok(Some(h)) = dev.find_child(storage, parent, name) {
        let _ = dev.delete(h);
    }
}

fn megabytes(bytes: u64) -> String {
    format!("{:.1} MB", bytes as f64 / 1_000_000.0)
}

/// Sends `bytes` as `parent/<name>`, replacing a file of that name so that a failed send leaves
/// the old copy whole (device delivery safety design). `extra` is what's sent with it (the
/// sidecars), for the room check. Returns warnings.
///
/// 0. Leftover `<name>.<n>.part` files are earlier failed sends when `<name>` is there, and
///    are deleted; without `<name>` one may be the only copy, so they stay until the new copy
///    is in place.
/// 1. Not enough room fails before anything is touched. (Unknown free space doesn't.)
/// 2. The book goes as `<name>.<n>.part`, a number no leftover has; 3. the old copy is
///    deleted; 4. the `.part` is renamed to `<name>`.
/// 5. A send that fails removes its `.part`: the old copy is as it was.
/// 6. A rename that fails leaves the new copy whole under its `.part` name; the error says so.
///
/// A Kindle that can't rename gets the old way, with the room check: delete, then send. A
/// failed send there has lost the old copy, and the error says [`OLD_COPY_REMOVED`]; after a
/// pulled cable a partial file may stay under the book's name until the next send.
pub(crate) fn mtp_send_replacing(
    dev: &mut Mtp,
    storage: u32,
    parent: u32,
    name: &str,
    bytes: &[u8],
    extra: u64,
) -> Result<Vec<String>, DeliverError> {
    let m = DeliverError::from;
    let mut warnings = Vec::new();
    let children = dev.list(storage, parent).map_err(m)?;
    let existing = children
        .iter()
        .find(|(_, i)| i.filename.eq_ignore_ascii_case(name))
        .map(|(h, i)| (*h, u64::from(i.size)));
    let leftovers: Vec<(u32, String)> = children
        .iter()
        .filter(|(_, i)| is_part_of(&i.filename, name))
        .map(|(h, i)| (*h, i.filename.clone()))
        .collect();
    let part = (1..)
        .map(|n| format!("{name}.{n}.part"))
        .find(|p| !leftovers.iter().any(|(_, f)| f.eq_ignore_ascii_case(p)))
        .expect("a free number");
    let rename = can_rename(dev);
    if let Ok(s) = dev.storage_info(storage) {
        let need = bytes.len() as u64 + extra + MTP_MARGIN;
        // Without a rename the old copy goes first, so its room counts.
        let free = match (rename, existing) {
            (false, Some((_, old))) => s.free_bytes.saturating_add(old),
            _ => s.free_bytes,
        };
        if free < need {
            return Err(DeliverError::Mtp(format!(
                "not enough room on the Kindle for {name}: it needs {}, {} is free; the copy \
                 already there is as it was",
                megabytes(need),
                megabytes(free)
            )));
        }
    }
    if existing.is_some() {
        for (h, _) in &leftovers {
            dev.delete(*h).map_err(m)?;
        }
    }
    let at = |what: String| move |e: MtpError| DeliverError::mtp_at(&what, e);
    if rename {
        let sent = match dev.send_file(storage, parent, &part, bytes) {
            Ok(h) => h,
            Err(e) => {
                remove_if_there(dev, storage, parent, &part);
                return Err(at(format!("sending documents/{name}"))(e));
            }
        };
        if let Some((old, _)) = existing
            && let Err(e) = dev.delete(old)
        {
            remove_if_there(dev, storage, parent, &part);
            return Err(at(format!("removing the old documents/{name}"))(e));
        }
        dev.rename(sent, name).map_err(at(format!(
            "renaming documents/{part} to {name} (the new copy stays on the Kindle as {part} \
             until this book is sent again{})",
            if existing.is_some() {
                format!("; {OLD_COPY_REMOVED}")
            } else {
                String::new()
            }
        )))?;
    } else {
        if let Some((old, _)) = existing {
            dev.delete(old).map_err(m)?;
        }
        if let Err(e) = dev.send_file(storage, parent, name, bytes) {
            // Try to take half a book off the book's name. With the session still up this works;
            // after a pulled cable it can't, and the partial file stays under the name until the
            // next send of this book replaces it.
            remove_if_there(dev, storage, parent, name);
            let what = match existing {
                Some(_) => format!(
                    "sending documents/{name} ({OLD_COPY_REMOVED}: this Kindle can't rename \
                     over MTP; send the book again)"
                ),
                None => format!("sending documents/{name}"),
            };
            return Err(at(what)(e));
        }
    }
    // The new copy is in place: a leftover kept for want of `<name>` can go.
    if existing.is_none() {
        for (h, file) in leftovers {
            if let Err(e) = dev.delete(h) {
                warnings.push(format!("documents/{file} not removed: {e}"));
            }
        }
    }
    Ok(warnings)
}

/// Sends the book into the Kindle's `documents` folder over MTP, replacing a same-named file.
/// Returns a pseudo path `mtp://<serial>/documents/<name>`.
fn deliver_mtp(
    kfx: &[u8],
    name: &str,
    serial: Option<String>,
    dev: &mut Mtp,
    opts: &DeliverOptions,
) -> Result<Delivered, DeliverError> {
    let m = DeliverError::from;
    let storages = dev.storages().map_err(m)?;
    let storage = *storages
        .first()
        .ok_or_else(|| DeliverError::Mtp("device reports no storage".into()))?;
    let docs = dev.find_documents(storage).map_err(m)?;
    let extras: usize = opts.sidecars.iter().map(|(_, b)| b.len()).sum::<usize>()
        + opts
            .system_files
            .iter()
            .map(|(_, b)| b.len())
            .sum::<usize>();
    let mut warnings = mtp_send_replacing(dev, storage, docs, name, kfx, extras as u64)?;
    // The book is in place: what fails from here is a warning.
    if let Err(e) = mtp_extras(dev, storage, docs, name, opts) {
        warnings.push(e.to_string());
    }
    Ok(Delivered {
        path: PathBuf::from(format!(
            "mtp://{}/documents/{name}",
            serial.unwrap_or_else(|| "kindle".into())
        )),
        warnings,
    })
}

/// The book's sidecars (yesterday's removed first) and the system files, after the book.
fn mtp_extras(
    dev: &mut Mtp,
    storage: u32,
    docs: u32,
    name: &str,
    opts: &DeliverOptions,
) -> Result<(), DeliverError> {
    let sidecars = &opts.sidecars;
    let m = DeliverError::from;
    // The step and file in the error: the Kindle answers a refused write with a bare code.
    let step = |what: String| move |e: MtpError| DeliverError::mtp_at(&what, e);
    let sdr_name = Path::new(name)
        .with_extension("sdr")
        .to_string_lossy()
        .into_owned();
    let existing_sdr = dev.find_child(storage, docs, &sdr_name).map_err(m)?;
    if let Some(sdr) = existing_sdr {
        for (h, info) in dev.list(storage, sdr).map_err(m)? {
            if is_deckle_sidecar(&info.filename) {
                dev.delete(h).map_err(m)?;
            }
        }
    }
    if !sidecars.is_empty() {
        let sdr = match existing_sdr {
            Some(h) => h,
            None => dev
                .make_folder(storage, docs, &sdr_name)
                .map_err(step(format!("creating documents/{sdr_name}")))?,
        };
        for (sc_name, bytes) in sidecars {
            dev.send_file(storage, sdr, sc_name, bytes)
                .map_err(step(format!("sending documents/{sdr_name}/{sc_name}")))?;
        }
    }
    if !opts.system_files.is_empty() {
        let root = dev.object_info(docs).map_err(m)?.parent;
        for (parts, bytes) in &opts.system_files {
            let Some((file, folders)) = parts.split_last() else {
                continue;
            };
            let mut at = root;
            for f in folders {
                at = dev
                    .ensure_folder(storage, at, f)
                    .map_err(step(format!("creating {}", parts.join("/"))))?;
            }
            if let Some(old) = dev.find_child(storage, at, file).map_err(m)? {
                dev.delete(old).map_err(m)?;
            }
            dev.send_file(storage, at, file, bytes)
                .map_err(step(format!("sending {}", parts.join("/"))))?;
        }
    }
    Ok(())
}

/// The names to remove so that only the `keep` greatest (by name) `<prefix>….kfx` files are
/// left. Dated editions named `<prefix>YYYY-MM-DD.kfx` sort oldest first.
/// `<prefix>YYYY-MM-DD.<ext>`, exactly: one paper's dated edition. Another paper's
/// (`deckle-daily-kid-courier-2026-10-24.kfx` for prefix `deckle-daily-`) and an undated name
/// (`deckle-daily-email-test.kfx`) aren't (many papers §4).
pub fn is_dated(name: &str, prefix: &str) -> bool {
    let Some((date, ext)) = name.strip_prefix(prefix).and_then(|r| r.split_once('.')) else {
        return false;
    };
    !ext.is_empty()
        && date.len() == 10
        && date.char_indices().all(|(i, c)| match i {
            4 | 7 => c == '-',
            _ => c.is_ascii_digit(),
        })
}

pub fn prunable<I: IntoIterator<Item = String>>(
    names: I,
    prefix: &str,
    keep: usize,
) -> Vec<String> {
    prunable_except(names, prefix, keep, &[])
}

/// A paper's file on a Kindle: the Kindle book, or the PDF a Kindle Scribe writes on.
fn is_paper_file(name: &str) -> bool {
    name.ends_with(".kfx") || name.ends_with(".pdf")
}

/// [`prunable`], leaving `exempt` names alone: they don't count toward `keep` either (an
/// edition sent again must not push a newer one out).
pub fn prunable_except<I: IntoIterator<Item = String>>(
    names: I,
    prefix: &str,
    keep: usize,
    exempt: &[String],
) -> Vec<String> {
    let mut matching: Vec<String> = names
        .into_iter()
        .filter(|n| is_dated(n, prefix) && is_paper_file(n) && !exempt.contains(n))
        .collect();
    matching.sort_unstable_by(|a, b| b.cmp(a));
    matching.into_iter().skip(keep).collect()
}

/// Keeps the newest `keep` dated editions named `<prefix>….kfx` on the connection's target and
/// removes the rest with their `.sdr` folders. Runs over the caller's connection, so an MTP
/// Kindle is pruned in the same session as the delivery (a second session resets it again).
pub fn prune_named_on(
    conn: &mut Conn<'_>,
    prefix: &str,
    keep: usize,
) -> Result<Vec<String>, DeliverError> {
    prune_named_on_except(conn, prefix, keep, &[])
}

/// [`prune_named_on`], leaving the `exempt` file names (and their `.sdr` folders) alone.
pub fn prune_named_on_except(
    conn: &mut Conn<'_>,
    prefix: &str,
    keep: usize,
    exempt: &[String],
) -> Result<Vec<String>, DeliverError> {
    let dir = match conn.target {
        Target::Folder(dir) => dir.clone(),
        Target::MassStorage(dev) => dev.root.join("documents"),
        Target::Mtp(_) => {
            return match conn.mtp() {
                Some(Ok(dev)) => prune_mtp(dev, prefix, keep, exempt),
                Some(Err(e)) => Err(DeliverError::from(e)),
                None => unreachable!("an MTP target always has a connection"),
            };
        }
    };
    let io = |path: &Path| {
        let path = path.to_path_buf();
        move |source| DeliverError::Io { path, source }
    };
    let names = std::fs::read_dir(&dir)
        .map_err(io(&dir))?
        .flatten()
        .map(|e| e.file_name().to_string_lossy().into_owned());
    let removed = prunable_except(names, prefix, keep, exempt);
    for name in &removed {
        let path = dir.join(name);
        std::fs::remove_file(&path).map_err(io(&path))?;
        let sdr = path.with_extension("sdr");
        if sdr.is_dir() {
            std::fs::remove_dir_all(&sdr).map_err(io(&sdr))?;
        }
    }
    Ok(removed)
}

fn prune_mtp(
    dev: &mut Mtp,
    prefix: &str,
    keep: usize,
    exempt: &[String],
) -> Result<Vec<String>, DeliverError> {
    let m = DeliverError::from;
    let storage = *dev
        .storages()
        .map_err(m)?
        .first()
        .ok_or_else(|| DeliverError::Mtp("device reports no storage".into()))?;
    let docs = dev.find_documents(storage).map_err(m)?;
    let children = dev.list(storage, docs).map_err(m)?;
    let removed = prunable_except(
        children.iter().map(|(_, info)| info.filename.clone()),
        prefix,
        keep,
        exempt,
    );
    for name in &removed {
        let sdr_name = Path::new(name)
            .with_extension("sdr")
            .to_string_lossy()
            .into_owned();
        for (handle, info) in &children {
            if info.filename == *name || info.filename == sdr_name {
                dev.delete(*handle).map_err(|e| {
                    DeliverError::mtp_at(&format!("deleting documents/{}", info.filename), e)
                })?;
            }
        }
    }
    Ok(removed)
}

#[cfg(test)]
mod tests {
    use std::cell::{Cell, RefCell};

    use deckle_transport::ptp::{Container, ContainerKind, op, rsp};
    use deckle_transport::session::Transport;

    use super::*;

    thread_local! {
        static OPENS: Cell<u32> = const { Cell::new(0) };
        static SENT: RefCell<Vec<u16>> = const { RefCell::new(Vec::new()) };
    }

    /// Answers every command with OK and logs its operation code.
    struct Ack(Option<Vec<u8>>);

    impl Transport for Ack {
        fn write(&mut self, bytes: &[u8]) -> Result<(), TransportError> {
            let (c, _) = Container::decode(bytes).unwrap();
            SENT.with(|s| s.borrow_mut().push(c.code));
            self.0 = Some(
                Container {
                    kind: ContainerKind::Response,
                    code: rsp::OK,
                    tx_id: c.tx_id,
                    params: Vec::new(),
                }
                .encode(&[]),
            );
            Ok(())
        }
        fn read(&mut self, buf: &mut [u8]) -> Result<usize, TransportError> {
            let r = self.0.take().ok_or(TransportError::Timeout)?;
            buf[..r.len()].copy_from_slice(&r);
            Ok(r.len())
        }
    }

    fn open_ack(_: &MtpInfo) -> Result<Mtp, TransportError> {
        OPENS.with(|n| n.set(n.get() + 1));
        Ok(Mtp::new(Box::new(Ack(None))))
    }

    #[test]
    fn an_mtp_conn_opens_once_for_every_use_and_closes_on_drop() {
        let target = Target::Mtp(MtpInfo {
            vendor_id: deckle_transport::usb::AMAZON_VENDOR_ID,
            product_id: 1,
            serial: Some("S".into()),
            product: None,
            bus: 1,
            address: 2,
        });
        {
            let mut conn = Conn {
                target: &target,
                mtp: None,
                open: open_ack,
            };
            // the Word Wise probe, then the delivery
            conn.mtp().unwrap().unwrap();
            conn.mtp().unwrap().unwrap();
        }
        assert_eq!(OPENS.with(Cell::get), 1);
        assert_eq!(
            SENT.with(|s| s.borrow().clone()),
            vec![op::OPEN_SESSION, op::CLOSE_SESSION]
        );
    }

    /// What's in `dir`, sorted.
    fn names(dir: &Path) -> Vec<String> {
        let mut n: Vec<String> = std::fs::read_dir(dir)
            .unwrap()
            .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
            .collect();
        n.sort();
        n
    }

    #[test]
    fn a_write_that_fails_half_way_leaves_the_old_file_whole() {
        let tmp = tempfile::tempdir().unwrap();
        let book = tmp.path().join("A - Book.kfx");
        std::fs::write(&book, b"the old copy").unwrap();
        FAIL_WRITE_AFTER.with(|f| f.set(Some(4)));
        let r = write_file(&book, b"the new copy, longer");
        FAIL_WRITE_AFTER.with(|f| f.set(None));
        assert!(matches!(r, Err(DeliverError::Io { .. })), "{r:?}");
        assert_eq!(std::fs::read(&book).unwrap(), b"the old copy");
        assert_eq!(names(tmp.path()), ["A - Book.kfx"], "no .part is left");
        // A write that goes through replaces it, again with nothing left beside it.
        write_file(&book, b"the new copy").unwrap();
        assert_eq!(std::fs::read(&book).unwrap(), b"the new copy");
        assert_eq!(names(tmp.path()), ["A - Book.kfx"]);
    }

    #[cfg(unix)]
    #[test]
    fn a_stale_read_only_part_of_this_process_doesnt_read_as_a_refusing_drive() {
        use std::os::unix::fs::PermissionsExt;
        let tmp = tempfile::tempdir().unwrap();
        let book = tmp.path().join("a.kfx");
        // Left by an earlier process that had this one's id, and read-only.
        let stale = tmp
            .path()
            .join(format!(".a.kfx.{}.part", std::process::id()));
        std::fs::write(&stale, b"HALF").unwrap();
        std::fs::set_permissions(&stale, std::fs::Permissions::from_mode(0o444)).unwrap();
        let r = write_beside_at(&book, b"BOOK");
        assert!(r.is_ok(), "{r:?}");
        assert_eq!(std::fs::read(&book).unwrap(), b"BOOK");
    }

    #[test]
    fn a_failed_resend_to_a_folder_keeps_the_old_copy() {
        let tmp = tempfile::tempdir().unwrap();
        let target = Target::Folder(tmp.path().to_path_buf());
        let mut conn = Conn::new(&target);
        let opts = DeliverOptions::default();
        deliver_named_on(b"edition one", "paper.kfx", &mut conn, &opts).unwrap();
        FAIL_WRITE_AFTER.with(|f| f.set(Some(3)));
        let r = deliver_named_on(b"edition two", "paper.kfx", &mut conn, &opts);
        FAIL_WRITE_AFTER.with(|f| f.set(None));
        assert!(r.is_err());
        assert_eq!(
            std::fs::read(tmp.path().join("paper.kfx")).unwrap(),
            b"edition one"
        );
        assert_eq!(names(tmp.path()), ["paper.kfx"]);
    }

    fn kindle_at(root: &Path) -> Target {
        Target::MassStorage(Device {
            root: root.to_path_buf(),
            model_hint: "Kindle".into(),
            thumbnails_writable: true,
            firmware: None,
            serial: None,
        })
    }

    #[test]
    fn a_thumbnail_that_wont_write_is_a_warning_once_the_book_is_there() {
        let tmp = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(tmp.path().join("documents")).unwrap();
        std::fs::create_dir_all(tmp.path().join("system")).unwrap();
        // A file where the thumbnails folder goes: the thumbnail can't be written.
        std::fs::write(tmp.path().join("system/thumbnails"), b"").unwrap();
        let opts = DeliverOptions {
            thumbnail: Some(("thumbnail_X_PDOC_portrait.jpg".into(), b"jpg".to_vec())),
            ..Default::default()
        };
        let target = kindle_at(tmp.path());
        let d = deliver_named_reporting(b"book", "a.kfx", &mut Conn::new(&target), &opts).unwrap();
        assert_eq!(d.path, tmp.path().join("documents/a.kfx"));
        assert_eq!(std::fs::read(&d.path).unwrap(), b"book");
        assert_eq!(d.warnings.len(), 1, "{:?}", d.warnings);
        assert!(d.warnings[0].starts_with("thumbnail"), "{:?}", d.warnings);
    }

    #[test]
    fn an_unplugged_kindle_at_a_fixed_mount_point_gets_nothing() {
        // The mount point stays as an empty folder after the Kindle is unplugged.
        let tmp = tempfile::tempdir().unwrap();
        let target = kindle_at(tmp.path());
        let e = deliver_named_on(b"x", "a.kfx", &mut Conn::new(&target), &Default::default())
            .unwrap_err();
        assert!(e.stops_push(), "{e}");
        assert_eq!(names(tmp.path()), Vec::<String>::new(), "nothing written");
    }

    #[cfg(unix)]
    #[test]
    fn a_kindle_that_cant_be_written_to_stops_a_push() {
        use std::os::unix::fs::PermissionsExt;
        let tmp = tempfile::tempdir().unwrap();
        let docs = tmp.path().join("documents");
        std::fs::create_dir_all(&docs).unwrap();
        std::fs::create_dir_all(tmp.path().join("system")).unwrap();
        // Mounted read-only (a dirty FAT volume on macOS): every book would fail alike.
        std::fs::set_permissions(&docs, std::fs::Permissions::from_mode(0o555)).unwrap();
        let target = kindle_at(tmp.path());
        let r = deliver_named_on(b"x", "a.kfx", &mut Conn::new(&target), &Default::default());
        std::fs::set_permissions(&docs, std::fs::Permissions::from_mode(0o755)).unwrap();
        let e = r.unwrap_err();
        assert!(e.stops_push(), "{e}");
    }

    #[test]
    fn a_lost_session_stops_a_push_and_a_refused_file_does_not() {
        let gone = [
            MtpError::Transport(TransportError::Timeout),
            MtpError::NoDocumentsFolder,
        ];
        for e in gone {
            let d = DeliverError::from(e);
            assert!(d.stops_push(), "{d}");
            assert!(d.to_string().starts_with("MTP: "), "{d}");
        }
        let refused = DeliverError::from(MtpError::Response {
            op: op::SEND_OBJECT,
            code: 0x200C,
        });
        assert!(!refused.stops_push());
        assert!(refused.to_string().starts_with("MTP: "), "{refused}");
        let io = DeliverError::Io {
            path: "/x".into(),
            source: std::io::Error::other("full"),
        };
        assert!(!io.stops_push());
    }

    #[test]
    fn a_folder_keeps_temporary_files_that_arent_deckles() {
        let tmp = tempfile::tempdir().unwrap();
        let hours_ago = std::time::SystemTime::now() - std::time::Duration::from_secs(7200);
        for n in [
            ".notes.tmp",
            ".download.part",
            ".x.7.part",
            ".old.kfx.42.part",
            ".old.epub.tmp",
        ] {
            let f = std::fs::File::create(tmp.path().join(n)).unwrap();
            f.set_modified(hours_ago).unwrap();
        }
        let target = Target::Folder(tmp.path().to_path_buf());
        deliver_named_on(
            b"x",
            "c.kfx",
            &mut Conn::new(&target),
            &DeliverOptions::default(),
        )
        .unwrap();
        // Only the shapes Deckle writes: `.<book>.<digits>.part` and `.<epub>.tmp`.
        assert_eq!(
            names(tmp.path()),
            [".download.part", ".notes.tmp", ".x.7.part", "c.kfx"]
        );
    }

    #[test]
    fn a_delivery_clears_temporary_files_older_than_an_hour() {
        let tmp = tempfile::tempdir().unwrap();
        let docs = tmp.path().join("documents");
        std::fs::create_dir_all(&docs).unwrap();
        std::fs::create_dir_all(tmp.path().join("system")).unwrap();
        let hours_ago = std::time::SystemTime::now() - std::time::Duration::from_secs(7200);
        for n in [
            ".a.kfx.123.part",
            ".b.kepub.epub.tmp",
            ".live.kfx.9.part",
            "keep.part",
        ] {
            let f = std::fs::File::create(docs.join(n)).unwrap();
            if n != ".live.kfx.9.part" {
                f.set_modified(hours_ago).unwrap();
            }
        }
        let target = Target::MassStorage(Device {
            root: tmp.path().to_path_buf(),
            model_hint: "Kindle".into(),
            thumbnails_writable: false,
            firmware: None,
            serial: None,
        });
        deliver_named_on(
            b"x",
            "c.kfx",
            &mut Conn::new(&target),
            &DeliverOptions::default(),
        )
        .unwrap();
        // Another Deckle's live write is spared, and so is a name that isn't Deckle's.
        assert_eq!(names(&docs), [".live.kfx.9.part", "c.kfx", "keep.part"]);
    }

    #[test]
    fn a_folder_conn_has_no_mtp_session() {
        let target = Target::Folder(PathBuf::from("/nowhere"));
        assert!(Conn::new(&target).mtp().is_none());
    }
}

#[cfg(test)]
mod mtp_send_tests {
    use deckle_transport::ptp::op;

    use super::*;
    use crate::fake_mtp as fake;

    fn kindle() -> Target {
        Target::Mtp(MtpInfo {
            vendor_id: deckle_transport::usb::AMAZON_VENDOR_ID,
            product_id: 1,
            serial: Some("G0001".into()),
            product: Some("Kindle".into()),
            bus: 1,
            address: 2,
        })
    }

    fn send(bytes: &[u8]) -> Result<Delivered, DeliverError> {
        let target = kindle();
        let mut conn = Conn::with_opener(&target, fake::open);
        deliver_named_reporting(bytes, "a.kfx", &mut conn, &DeliverOptions::default())
    }

    /// Sent since the last reset, not counting the session itself.
    fn sent(code: u16) -> bool {
        fake::ops().contains(&code)
    }

    #[test]
    fn a_resend_goes_beside_the_old_copy_and_is_renamed_over_it() {
        fake::reset(&[]);
        fake::put_bytes("a.kfx", b"OLD");
        send(b"NEW").unwrap();
        assert_eq!(fake::documents(), ["a.kfx"]);
        assert_eq!(fake::bytes("a.kfx").unwrap(), b"NEW");
        assert!(sent(op::SET_OBJECT_PROP_VALUE));
    }

    #[test]
    fn too_little_room_fails_before_anything_is_deleted() {
        fake::reset(&[]);
        fake::put_bytes("a.kfx", b"OLD");
        fake::set_free(1000);
        let e = send(&[7; 2000]).unwrap_err();
        assert!(e.to_string().contains("not enough room"), "{e}");
        assert!(!e.stops_push());
        assert!(!sent(op::DELETE_OBJECT) && !sent(op::SEND_OBJECT_INFO));
        assert_eq!(fake::bytes("a.kfx").unwrap(), b"OLD");
    }

    #[test]
    fn a_send_that_fails_leaves_the_old_copy_and_no_part() {
        fake::reset(&[]);
        fake::put_bytes("a.kfx", b"OLD");
        fake::fail_next_send();
        let e = send(b"NEW COPY").unwrap_err();
        assert!(!e.stops_push(), "{e}");
        assert!(!e.to_string().contains(OLD_COPY_REMOVED), "{e}");
        assert_eq!(fake::documents(), ["a.kfx"]);
        assert_eq!(fake::bytes("a.kfx").unwrap(), b"OLD");
    }

    #[test]
    fn a_kindle_that_cant_rename_gets_delete_then_send() {
        fake::reset(&[]);
        fake::no_rename();
        fake::put_bytes("a.kfx", b"OLD");
        send(b"NEW").unwrap();
        assert_eq!(fake::documents(), ["a.kfx"]);
        assert_eq!(fake::bytes("a.kfx").unwrap(), b"NEW");
        assert!(!sent(op::SET_OBJECT_PROP_VALUE));

        // A failed send there loses the old copy, and says so.
        fake::reset(&[]);
        fake::no_rename();
        fake::put_bytes("a.kfx", b"OLD");
        fake::fail_next_send();
        let e = send(b"NEW COPY").unwrap_err();
        assert!(e.to_string().contains(OLD_COPY_REMOVED), "{e}");
        assert_eq!(fake::documents(), Vec::<String>::new(), "no half book");
    }

    #[test]
    fn without_rename_the_old_copys_room_counts() {
        fake::reset(&[]);
        fake::no_rename();
        fake::put_bytes("a.kfx", &[1; 3 << 20]);
        fake::set_free(2 << 20);
        // 3 MB and the margin go, 2 MB are free: it fits once the old 3 MB copy is gone.
        send(&[2; 3 << 20]).unwrap();
        assert_eq!(fake::bytes("a.kfx").unwrap(), [2; 3 << 20]);
        // A Kindle that renames keeps the old copy during the send: the same is refused.
        fake::reset(&[]);
        fake::put_bytes("a.kfx", &[1; 3 << 20]);
        fake::set_free(2 << 20);
        assert!(send(&[2; 3 << 20]).is_err());
    }

    #[test]
    fn a_leftover_part_beside_the_book_doesnt_block_the_next_send() {
        fake::reset(&[]);
        fake::put_bytes("a.kfx", b"OLD");
        fake::put_bytes("a.kfx.1.part", b"HALF");
        send(b"NEW").unwrap();
        assert_eq!(fake::documents(), ["a.kfx"]);
        assert_eq!(fake::bytes("a.kfx").unwrap(), b"NEW");
    }

    #[test]
    fn a_leftover_part_without_its_book_is_kept_until_a_send_goes() {
        fake::reset(&[]);
        // Perhaps the only copy (a rename that failed), perhaps half of one.
        fake::put_bytes("a.kfx.1.part", b"MAYBE");
        fake::fail_next_send();
        send(b"NEW").unwrap_err();
        assert_eq!(fake::documents(), ["a.kfx.1.part"], "nothing under a.kfx");
        assert_eq!(fake::bytes("a.kfx.1.part").unwrap(), b"MAYBE");
        send(b"NEW").unwrap();
        assert_eq!(fake::documents(), ["a.kfx"]);
        assert_eq!(fake::bytes("a.kfx").unwrap(), b"NEW");
    }
}

#[cfg(test)]
mod dated_tests {
    use super::{is_dated, prunable};

    #[test]
    fn prunable_ignores_other_papers_and_undated_names() {
        let names: Vec<String> = [
            "deckle-daily-2026-10-20.kfx",
            "deckle-daily-2026-10-21.kfx",
            "deckle-daily-2026-10-22.kfx",
            "deckle-daily-2026-10-23.kfx",
            "deckle-daily-kid-courier-2026-10-17.kfx",
            "deckle-daily-kid-courier-2026-10-24.kfx",
            "deckle-daily-email-test.kfx",
        ]
        .map(String::from)
        .to_vec();
        assert_eq!(
            prunable(names.clone(), "deckle-daily-", 2),
            ["deckle-daily-2026-10-21.kfx", "deckle-daily-2026-10-20.kfx"]
        );
        assert_eq!(
            prunable(names, "deckle-daily-kid-courier-", 1),
            ["deckle-daily-kid-courier-2026-10-17.kfx"]
        );
        assert!(is_dated("deckle-daily-2026-10-24.epub", "deckle-daily-"));
        assert!(!is_dated("deckle-daily-2026-10-24", "deckle-daily-"));
        assert!(!is_dated("deckle-daily-2026-1-024.kfx", "deckle-daily-"));
    }
}

#[cfg(test)]
mod document_tests {
    use super::document_filename;

    #[test]
    fn a_document_keeps_its_name_with_a_lowercase_extension() {
        assert_eq!(
            document_filename("Write-on Probe C - PDF.pdf").as_deref(),
            Some("Write-on Probe C - PDF.pdf")
        );
        assert_eq!(document_filename("Notes.TXT").as_deref(), Some("Notes.txt"));
        assert_eq!(
            document_filename("Café: menu.azw3").as_deref(),
            Some("Cafe_ menu.azw3")
        );
    }

    #[test]
    fn a_long_name_is_cut_before_the_extension() {
        let name = document_filename(&format!("{}.pdf", "a".repeat(300))).unwrap();
        assert_eq!(name.len(), 120);
        assert!(name.ends_with("a.pdf"), "{name}");
    }

    #[test]
    fn only_formats_a_kindle_opens_are_documents() {
        for ok in [
            "a.pdf", "a.txt", "a.azw3", "a.azw", "a.mobi", "a.prc", "a.kfx",
        ] {
            assert!(document_filename(ok).is_some(), "{ok}");
        }
        for no in ["a.docx", "a.epub", "a.cbz", "a", ".pdf"] {
            assert!(document_filename(no).is_none(), "{no}");
        }
    }
}
