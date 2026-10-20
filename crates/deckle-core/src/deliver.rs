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
}

const MAX_FILENAME_LEN: usize = 120;

/// Sort-form author name: `file-as` when present, else `Last, First`.
fn author_sort(info: &BookInfo) -> Option<String> {
    let c = info.creators.first()?;
    if let Some(fa) = &c.file_as
        && !fa.trim().is_empty()
    {
        return Some(fa.trim().to_owned());
    }
    Some(deckle_kfx::metadata::author_sort(&c.name))
}

fn sanitize(s: &str) -> String {
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

/// Kindle detection over explicit mount roots (the public `detect_mass_storage` wraps it).
pub fn detect_in(roots: &[PathBuf]) -> Vec<Device> {
    let mut out = Vec::new();
    for root in roots {
        let documents = root.join("documents");
        let system = root.join("system");
        if !(documents.is_dir() && system.is_dir()) {
            continue;
        }
        let model_hint = std::fs::read_to_string(system.join("version.txt"))
            .ok()
            .and_then(|s| s.lines().next().map(|l| l.trim().to_owned()))
            .filter(|s| !s.is_empty())
            .unwrap_or_else(|| {
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
        });
    }
    out
}

/// Connected mass-storage Kindles: `/Volumes/*` on macOS, drive letters on Windows,
/// `/media/$USER/*` and `/run/media/$USER/*` on Linux.
pub fn detect_mass_storage() -> Vec<Device> {
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
    detect_in(&roots)
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
    let name = device_filename(info);
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

/// Keeps the newest `keep` books (by modification time) whose device filename starts with
/// `prefix` and removes the rest, with their `.sdr` folders. Used for dated editions such as
/// the daily digest. Returns the removed paths; `None` on an MTP target, which is not pruned
/// yet (reconnecting would reset the Kindle a second time).
pub fn prune_editions(
    target: &Target,
    prefix: &str,
    keep: usize,
) -> Result<Option<Vec<PathBuf>>, DeliverError> {
    let dir = match target {
        Target::Folder(dir) => dir.clone(),
        Target::MassStorage(dev) => dev.root.join("documents"),
        Target::Mtp(_) => return Ok(None),
    };
    let io = |path: &Path| {
        let path = path.to_path_buf();
        move |source| DeliverError::Io { path, source }
    };
    let mut editions: Vec<(std::time::SystemTime, PathBuf)> = Vec::new();
    for entry in std::fs::read_dir(&dir).map_err(io(&dir))?.flatten() {
        let name = entry.file_name().to_string_lossy().into_owned();
        if name.starts_with(prefix) && name.ends_with(".kfx") {
            let modified = entry
                .metadata()
                .and_then(|m| m.modified())
                .map_err(io(&entry.path()))?;
            editions.push((modified, entry.path()));
        }
    }
    editions.sort_by_key(|e| std::cmp::Reverse(e.0));
    let mut removed = Vec::new();
    for (_, path) in editions.into_iter().skip(keep) {
        std::fs::remove_file(&path).map_err(io(&path))?;
        let sdr = path.with_extension("sdr");
        if sdr.is_dir() {
            std::fs::remove_dir_all(&sdr).map_err(io(&sdr))?;
        }
        removed.push(path);
    }
    Ok(Some(removed))
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
