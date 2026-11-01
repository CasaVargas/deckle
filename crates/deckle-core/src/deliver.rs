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

/// [`deliver_on`] under a caller-chosen file name (dated editions use sortable names).
pub fn deliver_named_on(
    kfx: &[u8],
    name: &str,
    conn: &mut Conn<'_>,
    opts: &DeliverOptions,
) -> Result<PathBuf, DeliverError> {
    let name = name.to_owned();
    match conn.target {
        Target::Folder(dir) => {
            let path = write_file(&dir.join(&name), kfx)?;
            write_sidecars(&path, &opts.sidecars)?;
            write_system_files(dir, &opts.system_files)?;
            Ok(path)
        }
        Target::MassStorage(dev) => {
            let path = write_file(&dev.root.join("documents").join(&name), kfx)?;
            write_sidecars(&path, &opts.sidecars)?;
            write_system_files(&dev.root, &opts.system_files)?;
            if let Some((thumb_name, jpeg)) = &opts.thumbnail {
                crate::thumbnail::install(&dev.root, thumb_name, jpeg)
                    .map_err(|e| DeliverError::Thumbnail(e.to_string()))?;
            }
            let _ = crate::thumbnail::sync(&dev.root);
            Ok(path)
        }
        Target::Mtp(mtp) => {
            let serial = mtp.serial.clone();
            match conn.mtp() {
                Some(Ok(dev)) => deliver_mtp(kfx, &name, serial, dev, opts),
                Some(Err(e)) => Err(DeliverError::Mtp(e.to_string())),
                None => unreachable!("an MTP target always has a connection"),
            }
        }
    }
}

fn write_system_files(root: &Path, files: &[(Vec<String>, Vec<u8>)]) -> Result<(), DeliverError> {
    for (parts, bytes) in files {
        let path = parts.iter().fold(root.to_path_buf(), |p, s| p.join(s));
        write_file(&path, bytes)?;
    }
    Ok(())
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

fn write_file(path: &Path, data: &[u8]) -> Result<PathBuf, DeliverError> {
    let io = |source: std::io::Error| DeliverError::Io {
        path: path.to_path_buf(),
        source,
    };
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(io)?;
    }
    let mut f = std::fs::File::create(path).map_err(io)?;
    f.write_all(data).map_err(io)?;
    f.sync_all().map_err(io)?;
    Ok(path.to_path_buf())
}

/// `XRAY.entities.<ASIN>.asc`
pub fn is_xray_sidecar(name: &str) -> bool {
    name.starts_with("XRAY.entities.") && name.ends_with(".asc")
}

/// A sidecar Deckle writes and replaces on every send: X-Ray or `LanguageLayer.<lang>.<ASIN>.kll`.
pub fn is_deckle_sidecar(name: &str) -> bool {
    is_xray_sidecar(name) || (name.starts_with("LanguageLayer.") && name.ends_with(".kll"))
}

/// Sends the book into the Kindle's `documents` folder over MTP, replacing a same-named file.
/// Returns a pseudo path `mtp://<serial>/documents/<name>`.
fn deliver_mtp(
    kfx: &[u8],
    name: &str,
    serial: Option<String>,
    dev: &mut Mtp,
    opts: &DeliverOptions,
) -> Result<PathBuf, DeliverError> {
    let sidecars = &opts.sidecars;
    let m = |e: &dyn std::fmt::Display| DeliverError::Mtp(e.to_string());
    // The step and file in the error: the Kindle answers a refused write with a bare code.
    let step = |what: String| move |e: MtpError| DeliverError::Mtp(format!("{what}: {e}"));
    let storages = dev.storages().map_err(|e| m(&e))?;
    let storage = *storages
        .first()
        .ok_or_else(|| DeliverError::Mtp("device reports no storage".into()))?;
    let docs = dev.find_documents(storage).map_err(|e| m(&e))?;
    if let Some(existing) = dev.find_child(storage, docs, name).map_err(|e| m(&e))? {
        dev.delete(existing).map_err(|e| m(&e))?;
    }
    dev.send_file(storage, docs, name, kfx)
        .map_err(step(format!("sending documents/{name}")))?;
    let sdr_name = Path::new(name)
        .with_extension("sdr")
        .to_string_lossy()
        .into_owned();
    let existing_sdr = dev
        .find_child(storage, docs, &sdr_name)
        .map_err(|e| m(&e))?;
    if let Some(sdr) = existing_sdr {
        for (h, info) in dev.list(storage, sdr).map_err(|e| m(&e))? {
            if is_deckle_sidecar(&info.filename) {
                dev.delete(h).map_err(|e| m(&e))?;
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
        let root = dev.object_info(docs).map_err(|e| m(&e))?.parent;
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
            if let Some(old) = dev.find_child(storage, at, file).map_err(|e| m(&e))? {
                dev.delete(old).map_err(|e| m(&e))?;
            }
            dev.send_file(storage, at, file, bytes)
                .map_err(step(format!("sending {}", parts.join("/"))))?;
        }
    }
    Ok(PathBuf::from(format!(
        "mtp://{}/documents/{name}",
        serial.unwrap_or_else(|| "kindle".into())
    )))
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
        .filter(|n| is_dated(n, prefix) && n.ends_with(".kfx") && !exempt.contains(n))
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
                Some(Err(e)) => Err(DeliverError::Mtp(e.to_string())),
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
    let m = |e: MtpError| DeliverError::Mtp(e.to_string());
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
                    DeliverError::Mtp(format!("deleting documents/{}: {e}", info.filename))
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

    #[test]
    fn a_folder_conn_has_no_mtp_session() {
        let target = Target::Folder(PathBuf::from("/nowhere"));
        assert!(Conn::new(&target).mtp().is_none());
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
