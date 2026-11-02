// SPDX-License-Identifier: GPL-3.0-or-later
//! A reMarkable over SSH (fixed-layout spec §12.5): the paper and books written straight into
//! xochitl's document folder, over Wi-Fi or the USB link, with no account and no cloud.
//!
//! Deckle only ever connects to an SSH the tablet already answers on: it never enables
//! reMarkable's Developer Mode (a factory reset, the owner's decision) and never runs
//! `rm-ssh-over-wlan`. On the tablet it runs only what [`Shell`]'s callers below spell out:
//! `cat`, `cat >`, `mkdir -p`, `date +%s`, and one `systemctl restart xochitl`.
//!
//! The document layout (`<uuid>.pdf|epub`, `<uuid>.metadata`, `<uuid>.content`) is
//! reMarkable's own, read off community tools and the developer docs; xochitl sees new files
//! only when restarted, which costs the owner a few seconds, so Deckle restarts it only when
//! the tablet looks idle (§12.5, ruling b).

use std::path::Path;

use serde::{Deserialize, Serialize};

use crate::queue::Queue;
use crate::sync::RemarkablePush;

/// Where xochitl keeps documents.
pub const DIR: &str = "/home/root/.local/share/remarkable/xochitl";

/// The tablet's SSH login.
pub const USER: &str = "root";

/// How long since the tablet's newest document change before a restart is fair (seconds).
pub const QUIET_SECS: u64 = 10 * 60;

/// The outbox marker: files went up but xochitl hasn't been restarted to show them.
pub const RESTART_OWED: &str = ".restart-owed";

/// What one command on the tablet returned.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Output {
    pub status: u32,
    pub stdout: Vec<u8>,
}

impl Output {
    fn ok(self, what: &str) -> Result<Vec<u8>, String> {
        if self.status == 0 {
            Ok(self.stdout)
        } else {
            Err(format!(
                "{what}: exit {} ({})",
                self.status,
                String::from_utf8_lossy(&self.stdout).trim()
            ))
        }
    }
}

/// A command channel to the tablet: the SSH exec channel, or a stand-in in tests.
pub trait Shell {
    /// Runs `cmd` with `stdin` on its standard input; the exit status and standard output.
    fn run(&mut self, cmd: &str, stdin: &[u8]) -> Result<Output, String>;
}

/// One entry of the tablet's document list, from its `.metadata`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Doc {
    pub uuid: String,
    pub name: String,
    /// `DocumentType` or `CollectionType`.
    pub kind: String,
    /// The folder's uuid; empty at the top.
    pub parent: String,
    pub deleted: bool,
    /// Epoch milliseconds.
    pub last_modified: u64,
}

impl Doc {
    pub fn is_folder(&self) -> bool {
        self.kind == "CollectionType"
    }
}

#[derive(Deserialize)]
struct MetadataFile {
    #[serde(default, rename = "visibleName")]
    visible_name: String,
    #[serde(default, rename = "type")]
    kind: String,
    #[serde(default)]
    parent: String,
    #[serde(default)]
    deleted: bool,
    #[serde(default, rename = "lastModified")]
    last_modified: serde_json::Value,
}

/// A `.metadata` file as Deckle writes it: what xochitl needs, nothing it fills in itself.
#[derive(Serialize)]
struct NewMetadata<'a> {
    deleted: bool,
    #[serde(rename = "lastModified")]
    last_modified: String,
    #[serde(rename = "metadatamodified")]
    metadata_modified: bool,
    modified: bool,
    parent: &'a str,
    pinned: bool,
    synced: bool,
    #[serde(rename = "type")]
    kind: &'a str,
    version: u32,
    #[serde(rename = "visibleName")]
    visible_name: &'a str,
}

fn metadata_json(name: &str, kind: &str, parent: &str, now_ms: u64) -> String {
    serde_json::to_string_pretty(&NewMetadata {
        deleted: false,
        last_modified: now_ms.to_string(),
        metadata_modified: false,
        modified: false,
        parent,
        pinned: false,
        synced: false,
        kind,
        version: 1,
        visible_name: name,
    })
    .expect("a plain struct")
}

fn ms(v: &serde_json::Value) -> u64 {
    match v {
        serde_json::Value::String(s) => s.trim().parse().unwrap_or(0),
        serde_json::Value::Number(n) => n.as_u64().unwrap_or(0),
        _ => 0,
    }
}

/// The tablet's clock, in seconds.
pub fn now(sh: &mut dyn Shell) -> Result<u64, String> {
    let out = sh.run("date +%s", b"")?.ok("date")?;
    String::from_utf8_lossy(&out)
        .trim()
        .parse()
        .map_err(|_| "the tablet's clock didn't read".to_owned())
}

/// Every document and folder on the tablet, from its `.metadata` files.
pub fn list(sh: &mut dyn Shell) -> Result<Vec<Doc>, String> {
    let cmd = format!("cd {DIR} && for f in *.metadata; do echo \"== $f\"; cat \"$f\"; done");
    let out = sh.run(&cmd, b"")?.ok("listing the tablet")?;
    let text = String::from_utf8_lossy(&out);
    let mut docs = Vec::new();
    for block in text.split("== ").skip(1) {
        let (file, json) = block.split_once('\n').unwrap_or((block, ""));
        let Some(uuid) = file.trim().strip_suffix(".metadata") else {
            continue;
        };
        let Ok(m) = serde_json::from_str::<MetadataFile>(json) else {
            continue;
        };
        docs.push(Doc {
            uuid: uuid.to_owned(),
            name: m.visible_name,
            kind: m.kind,
            parent: m.parent,
            deleted: m.deleted,
            last_modified: ms(&m.last_modified),
        });
    }
    Ok(docs)
}

fn write(sh: &mut dyn Shell, file: &str, bytes: &[u8]) -> Result<(), String> {
    sh.run(&format!("cat > {DIR}/{file}"), bytes)?
        .ok(&format!("writing {file}"))
        .map(|_| ())
}

/// The uuid of the top-level folder called `name`, made when there isn't one.
pub fn folder(sh: &mut dyn Shell, docs: &[Doc], name: &str) -> Result<String, String> {
    if let Some(d) = docs
        .iter()
        .find(|d| d.is_folder() && !d.deleted && d.parent.is_empty() && d.name == name)
    {
        return Ok(d.uuid.clone());
    }
    let uuid = uuid::Uuid::new_v4().to_string();
    let now_ms = now(sh)? * 1000;
    write(
        sh,
        &format!("{uuid}.metadata"),
        metadata_json(name, "CollectionType", "", now_ms).as_bytes(),
    )?;
    write(sh, &format!("{uuid}.content"), b"[]")?;
    Ok(uuid)
}

/// `bytes` as a document called `name` (no extension) in the folder `parent`, as a `.pdf` or
/// `.epub` by `ext`.
pub fn upload(
    sh: &mut dyn Shell,
    name: &str,
    ext: &str,
    bytes: &[u8],
    parent: &str,
) -> Result<String, String> {
    let uuid = uuid::Uuid::new_v4().to_string();
    let now_ms = now(sh)? * 1000;
    sh.run(
        &format!("mkdir -p {DIR}/{uuid} && cat > {DIR}/{uuid}.{ext}"),
        bytes,
    )?
    .ok(&format!("writing {name}.{ext}"))?;
    write(
        sh,
        &format!("{uuid}.content"),
        format!("{{\"fileType\": \"{ext}\"}}").as_bytes(),
    )?;
    write(sh, &format!("{uuid}.pagedata"), b"")?;
    // The metadata last: a document without one is never shown, so a cut-off upload leaves
    // nothing half-visible.
    write(
        sh,
        &format!("{uuid}.metadata"),
        metadata_json(name, "DocumentType", parent, now_ms).as_bytes(),
    )?;
    Ok(uuid)
}

/// Marks the document `uuid` deleted, as the tablet's own trash does.
pub fn mark_deleted(sh: &mut dyn Shell, uuid: &str) -> Result<(), String> {
    let file = format!("{uuid}.metadata");
    let out = sh
        .run(&format!("cat {DIR}/{file}"), b"")?
        .ok(&format!("reading {file}"))?;
    let mut v: serde_json::Value = serde_json::from_slice(&out)
        .map_err(|e| format!("{file}: the metadata didn't parse: {e}"))?;
    let Some(obj) = v.as_object_mut() else {
        return Err(format!("{file}: not a JSON object"));
    };
    obj.insert("deleted".into(), serde_json::Value::Bool(true));
    obj.insert("metadatamodified".into(), serde_json::Value::Bool(true));
    obj.insert("synced".into(), serde_json::Value::Bool(false));
    let version = obj.get("version").and_then(|n| n.as_u64()).unwrap_or(0) + 1;
    obj.insert("version".into(), serde_json::Value::from(version));
    write(
        sh,
        &file,
        serde_json::to_string_pretty(&v).expect("json").as_bytes(),
    )
}

/// Whether the tablet looks idle: nothing changed for `quiet_secs` by its own clock.
pub fn idle(sh: &mut dyn Shell, docs: &[Doc], quiet_secs: u64) -> Result<bool, String> {
    let newest = docs
        .iter()
        .map(|d| d.last_modified / 1000)
        .max()
        .unwrap_or(0);
    Ok(now(sh)?.saturating_sub(newest) >= quiet_secs)
}

/// Restarts xochitl so it reads the folder again. `reset-failed` first: repeated restarts
/// otherwise trip systemd's start limit, which reboots the tablet.
pub fn restart(sh: &mut dyn Shell) -> Result<(), String> {
    sh.run(
        "systemctl reset-failed xochitl.service; systemctl restart xochitl.service",
        b"",
    )?
    .ok("restarting xochitl")
    .map(|_| ())
}

/// Names Deckle's own papers: `<prefix>YYYY-MM-DD`.
fn is_dated(name: &str, prefix: &str) -> bool {
    name.strip_prefix(prefix).is_some_and(|rest| {
        rest.len() == 10
            && rest.chars().enumerate().all(|(i, c)| {
                if i == 4 || i == 7 {
                    c == '-'
                } else {
                    c.is_ascii_digit()
                }
            })
    })
}

/// Uploads what waits in `queue` into the tablet's `folder`, each once, then prunes Deckle's
/// papers there to the newest `keep` when `prune` is given (prefix, keep, exempt names), and
/// restarts xochitl when something changed and the tablet is idle (§12.5).
pub fn push(
    queue: &Queue,
    sh: &mut dyn Shell,
    folder_name: &str,
    prune: Option<(&str, usize, &[String])>,
) -> RemarkablePush {
    let mut out = RemarkablePush::default();
    let pending: Vec<std::path::PathBuf> = queue
        .pending()
        .into_iter()
        .filter(|p| p.extension().is_some_and(|x| x == "pdf" || x == "epub"))
        .collect();
    let owed = queue.dir.join(RESTART_OWED);
    if pending.is_empty() && prune.is_none() && !owed.exists() {
        return out;
    }
    let docs = match list(sh) {
        Ok(d) => d,
        Err(e) => {
            out.errors.push(e);
            return out;
        }
    };
    let parent = match folder(sh, &docs, folder_name) {
        Ok(p) => p,
        Err(e) => {
            out.errors.push(format!("the {folder_name} folder: {e}"));
            return out;
        }
    };
    let shown: Vec<&Doc> = docs
        .iter()
        .filter(|d| !d.is_folder() && !d.deleted && d.parent == parent)
        .collect();
    let mut changed = false;
    for entry in pending {
        let file = entry
            .file_name()
            .unwrap_or_default()
            .to_string_lossy()
            .into_owned();
        let name = crate::remarkable::shown_as(&file);
        let ext = Path::new(&file)
            .extension()
            .map(|e| e.to_string_lossy().into_owned())
            .unwrap_or_default();
        if shown.iter().any(|d| d.name == name) {
            let _ = std::fs::remove_file(&entry);
            out.already.push(file);
            continue;
        }
        let sent = std::fs::read(&entry)
            .map_err(|e| e.to_string())
            .and_then(|bytes| upload(sh, name, &ext, &bytes, &parent));
        match sent {
            Ok(_) => {
                let _ = std::fs::remove_file(&entry);
                out.uploaded.push(file);
                changed = true;
            }
            Err(e) => out.errors.push(format!("{file}: {e}")),
        }
    }
    if let Some((prefix, keep, exempt)) = prune {
        let mut dated: Vec<&Doc> = shown
            .iter()
            .copied()
            .filter(|d| is_dated(&d.name, prefix) && !exempt.iter().any(|x| x == &d.name))
            .collect();
        dated.sort_by(|a, b| b.name.cmp(&a.name));
        for old in dated.into_iter().skip(keep.max(1)) {
            match mark_deleted(sh, &old.uuid) {
                Ok(()) => {
                    out.pruned.push(old.name.clone());
                    changed = true;
                }
                Err(e) => out.errors.push(format!("{}: {e}", old.name)),
            }
        }
    }
    if changed {
        let _ = std::fs::write(&owed, b"");
    }
    if owed.exists() {
        match idle(sh, &docs, QUIET_SECS).and_then(|idle| {
            if idle {
                restart(sh).map(|()| true)
            } else {
                Ok(false)
            }
        }) {
            Ok(true) => {
                let _ = std::fs::remove_file(&owed);
                out.notes
                    .push("xochitl restarted: the new documents show now".into());
            }
            Ok(false) => out.notes.push(
                "the tablet was in use just now, so it wasn't restarted; the new documents \
                 show after its next restart (Deckle will try again when it's been idle for \
                 10 minutes)"
                    .into(),
            ),
            Err(e) => out.errors.push(e),
        }
    }
    out
}

// ---- the SSH transport ---------------------------------------------------------------------

/// A reMarkable reached over SSH, with its host key's fingerprint as seen.
pub struct Connected {
    pub shell: SshShell,
    /// `SHA256:…`, to pin.
    pub fingerprint: String,
}

impl std::fmt::Debug for Connected {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "Connected({})", self.fingerprint)
    }
}

/// Why a connection didn't happen, told apart so the caller can say what to do.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ConnectError {
    /// The tablet's host key isn't the pinned one.
    HostKeyChanged { seen: String, pinned: String },
    /// The tablet refused the password.
    BadPassword,
    /// Not reachable, or the handshake failed.
    Other(String),
}

impl std::fmt::Display for ConnectError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ConnectError::HostKeyChanged { seen, pinned } => write!(
                f,
                "the tablet's SSH host key changed ({seen}, pinned {pinned}); if the tablet \
                 was reset or replaced, delete remarkable-hostkey.json beside digest.toml"
            ),
            ConnectError::BadPassword => write!(
                f,
                "the tablet refused the password; it's under Settings → Help → About → \
                 Copyrights and licenses, and `deckle digest remarkable password` saves it"
            ),
            ConnectError::Other(e) => write!(f, "{e}"),
        }
    }
}

/// Checks the tablet's host key against the pinned fingerprint, and keeps what it saw.
struct Check {
    pinned: Option<String>,
    seen: std::sync::Arc<std::sync::Mutex<Option<String>>>,
}

fn fingerprint_of(key: &russh::keys::PublicKeyOrCertificate) -> String {
    use russh::keys::HashAlg;
    match key {
        russh::keys::PublicKeyOrCertificate::PublicKey { key, .. } => {
            key.fingerprint(HashAlg::Sha256).to_string()
        }
        russh::keys::PublicKeyOrCertificate::Certificate(c) => {
            c.public_key().fingerprint(HashAlg::Sha256).to_string()
        }
    }
}

impl russh::client::Handler for Check {
    type Error = russh::Error;

    async fn check_server_key(
        &mut self,
        key: &russh::keys::PublicKeyOrCertificate,
    ) -> Result<bool, Self::Error> {
        let seen = fingerprint_of(key);
        let ok = self.pinned.as_deref().is_none_or(|p| p == seen);
        *self.seen.lock().unwrap() = Some(seen);
        Ok(ok)
    }
}

/// The exec channel to a connected tablet.
pub struct SshShell {
    rt: tokio::runtime::Runtime,
    handle: russh::client::Handle<Check>,
}

/// Connects to `host:port` as root with `password`, trusting the host key when `pinned` is
/// `None` (first use) and otherwise only the pinned one.
pub fn connect(
    host: &str,
    port: u16,
    password: &str,
    pinned: Option<&str>,
    timeout: std::time::Duration,
) -> Result<Connected, ConnectError> {
    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .map_err(|e| ConnectError::Other(e.to_string()))?;
    let seen = std::sync::Arc::new(std::sync::Mutex::new(None));
    let check = Check {
        pinned: pinned.map(str::to_owned),
        seen: seen.clone(),
    };
    let config = std::sync::Arc::new(russh::client::Config {
        inactivity_timeout: Some(std::time::Duration::from_secs(120)),
        ..Default::default()
    });
    let handle = rt.block_on(async {
        let connect = russh::client::connect(config, (host, port), check);
        let mut handle = tokio::time::timeout(timeout, connect)
            .await
            .map_err(|_| ConnectError::Other(format!("reMarkable at {host}: no answer")))?
            .map_err(|e| match e {
                russh::Error::UnknownKey => ConnectError::HostKeyChanged {
                    seen: seen.lock().unwrap().clone().unwrap_or_default(),
                    pinned: pinned.unwrap_or_default().to_owned(),
                },
                e => ConnectError::Other(format!("reMarkable at {host}: {e}")),
            })?;
        let auth = handle
            .authenticate_password(USER, password)
            .await
            .map_err(|e| ConnectError::Other(format!("reMarkable at {host}: {e}")))?;
        if !auth.success() {
            return Err(ConnectError::BadPassword);
        }
        Ok(handle)
    })?;
    let fingerprint = seen.lock().unwrap().clone().unwrap_or_default();
    Ok(Connected {
        shell: SshShell { rt, handle },
        fingerprint,
    })
}

impl Shell for SshShell {
    fn run(&mut self, cmd: &str, stdin: &[u8]) -> Result<Output, String> {
        let handle = &self.handle;
        self.rt.block_on(async {
            let mut ch = handle
                .channel_open_session()
                .await
                .map_err(|e| e.to_string())?;
            ch.exec(true, cmd).await.map_err(|e| e.to_string())?;
            if !stdin.is_empty() {
                ch.data(stdin).await.map_err(|e| e.to_string())?;
            }
            ch.eof().await.map_err(|e| e.to_string())?;
            let mut out = Output::default();
            while let Some(msg) = ch.wait().await {
                match msg {
                    russh::ChannelMsg::Data { data } => out.stdout.extend_from_slice(&data),
                    russh::ChannelMsg::ExtendedData { data, .. } => {
                        out.stdout.extend_from_slice(&data)
                    }
                    russh::ChannelMsg::ExitStatus { exit_status } => out.status = exit_status,
                    russh::ChannelMsg::Close => break,
                    _ => {}
                }
            }
            Ok(out)
        })
    }
}

impl Drop for SshShell {
    fn drop(&mut self) {
        let handle = &self.handle;
        let _ = self.rt.block_on(async {
            handle
                .disconnect(russh::Disconnect::ByApplication, "", "English")
                .await
        });
    }
}

#[cfg(test)]
pub mod tests {
    use super::*;
    use std::collections::BTreeMap;

    /// A stand-in tablet: a file map under [`DIR`] and the few commands Deckle runs.
    #[derive(Default)]
    pub struct FakeTablet {
        pub files: BTreeMap<String, Vec<u8>>,
        pub now: u64,
        pub restarts: usize,
        pub commands: Vec<String>,
    }

    impl FakeTablet {
        pub fn with_doc(
            &mut self,
            uuid: &str,
            name: &str,
            kind: &str,
            parent: &str,
            modified: u64,
        ) {
            self.files.insert(
                format!("{uuid}.metadata"),
                metadata_json(name, kind, parent, modified).into_bytes(),
            );
        }

        fn metadata(&self, uuid: &str) -> serde_json::Value {
            serde_json::from_slice(&self.files[&format!("{uuid}.metadata")]).unwrap()
        }
    }

    impl Shell for FakeTablet {
        fn run(&mut self, cmd: &str, stdin: &[u8]) -> Result<Output, String> {
            self.commands.push(cmd.to_owned());
            let prefix = format!("{DIR}/");
            let ok = |stdout: Vec<u8>| Ok(Output { status: 0, stdout });
            if cmd == "date +%s" {
                return ok(self.now.to_string().into_bytes());
            }
            if cmd.starts_with("cd ") && cmd.contains("*.metadata") {
                let mut out = Vec::new();
                for (f, bytes) in &self.files {
                    if f.ends_with(".metadata") {
                        out.extend_from_slice(format!("== {f}\n").as_bytes());
                        out.extend_from_slice(bytes);
                        out.push(b'\n');
                    }
                }
                return ok(out);
            }
            if let Some(rest) = cmd.strip_prefix("mkdir -p ") {
                let (_dir, write) = rest.split_once(" && ").unwrap();
                let file = write
                    .strip_prefix("cat > ")
                    .unwrap()
                    .strip_prefix(&prefix)
                    .unwrap();
                self.files.insert(file.to_owned(), stdin.to_vec());
                return ok(Vec::new());
            }
            if let Some(file) = cmd.strip_prefix("cat > ") {
                let file = file.strip_prefix(&prefix).unwrap();
                self.files.insert(file.to_owned(), stdin.to_vec());
                return ok(Vec::new());
            }
            if let Some(file) = cmd.strip_prefix("cat ") {
                let file = file.strip_prefix(&prefix).unwrap();
                return match self.files.get(file) {
                    Some(b) => ok(b.clone()),
                    None => Ok(Output {
                        status: 1,
                        stdout: b"No such file".to_vec(),
                    }),
                };
            }
            if cmd.starts_with("systemctl reset-failed xochitl.service; systemctl restart") {
                self.restarts += 1;
                return ok(Vec::new());
            }
            Err(format!("the fake tablet doesn't know: {cmd}"))
        }
    }

    fn queue_with(dir: &Path, files: &[(&str, &[u8])]) -> Queue {
        let q = Queue::new(dir.join("outbox-remarkable"));
        std::fs::create_dir_all(&q.dir).unwrap();
        for (n, b) in files {
            std::fs::write(q.dir.join(n), b).unwrap();
        }
        q
    }

    #[test]
    fn the_list_reads_every_metadata_file() {
        let mut t = FakeTablet::default();
        t.with_doc("f1", "Deckle", "CollectionType", "", 1_000);
        t.with_doc("d1", "deckle-daily-2026-10-22", "DocumentType", "f1", 2_000);
        t.files.insert("junk.metadata".into(), b"not json".to_vec());
        let docs = list(&mut t).unwrap();
        assert_eq!(docs.len(), 2);
        assert!(docs[1].is_folder() && docs[1].name == "Deckle");
        assert_eq!(docs[0].parent, "f1");
        assert_eq!(docs[0].last_modified, 2_000);
    }

    #[test]
    fn a_push_makes_the_folder_uploads_once_and_restarts_an_idle_tablet() {
        let tmp = tempfile::tempdir().unwrap();
        let q = queue_with(
            tmp.path(),
            &[
                ("deckle-daily-2026-10-23.pdf", b"%PDF-1.7 paper"),
                ("Ink.epub", b"PK book"),
            ],
        );
        let mut t = FakeTablet {
            now: 1_000_000,
            ..Default::default()
        };
        let p = push(&q, &mut t, "Deckle", None);
        assert_eq!(p.errors, Vec::<String>::new());
        assert_eq!(p.uploaded, ["Ink.epub", "deckle-daily-2026-10-23.pdf"]);
        // A folder, then two documents: metadata, content, pagedata, the file itself.
        let folder = t
            .files
            .iter()
            .find(|(f, b)| {
                f.ends_with(".metadata") && b.windows(14).any(|w| w == b"CollectionType")
            })
            .map(|(f, _)| f.trim_end_matches(".metadata").to_owned())
            .expect("the Deckle folder");
        let pdf = t.files.iter().find(|(f, _)| f.ends_with(".pdf")).unwrap();
        assert_eq!(pdf.1, b"%PDF-1.7 paper");
        let uuid = pdf.0.trim_end_matches(".pdf");
        let m = t.metadata(uuid);
        assert_eq!(m["visibleName"], "deckle-daily-2026-10-23");
        assert_eq!(m["parent"], folder);
        assert_eq!(m["type"], "DocumentType");
        assert_eq!(m["lastModified"], "1000000000");
        assert_eq!(
            t.files[&format!("{uuid}.content")],
            b"{\"fileType\": \"pdf\"}"
        );
        assert!(t.files.contains_key(&format!("{uuid}.pagedata")));
        // Idle (nothing changed for 10 minutes by its clock): restarted, once, marker gone.
        assert_eq!(t.restarts, 1);
        assert!(!q.dir.join(RESTART_OWED).exists());
        assert!(p.notes[0].contains("restarted"), "{:?}", p.notes);
        assert!(q.pending().is_empty());

        // The same paper again: already there, not sent twice, nothing to restart for.
        std::fs::write(q.dir.join("deckle-daily-2026-10-23.pdf"), b"again").unwrap();
        let before = t.files.len();
        let p = push(&q, &mut t, "Deckle", None);
        assert_eq!(p.already, ["deckle-daily-2026-10-23.pdf"]);
        assert_eq!(t.files.len(), before);
        assert_eq!(t.restarts, 1);
    }

    #[test]
    fn a_busy_tablet_isnt_restarted_and_the_restart_is_owed_to_the_next_push() {
        let tmp = tempfile::tempdir().unwrap();
        let q = queue_with(tmp.path(), &[("Ink.epub", b"PK")]);
        let mut t = FakeTablet {
            now: 1_000_000,
            ..Default::default()
        };
        // A notebook written in a minute ago.
        t.with_doc("n1", "Notes", "DocumentType", "", (1_000_000 - 60) * 1000);
        let p = push(&q, &mut t, "Deckle", None);
        assert_eq!(p.uploaded, ["Ink.epub"]);
        assert_eq!(t.restarts, 0);
        assert!(q.dir.join(RESTART_OWED).exists());
        assert!(p.notes[0].contains("in use"), "{:?}", p.notes);
        // Eleven minutes later, with nothing to send: the owed restart happens.
        t.now += 11 * 60;
        let p = push(&q, &mut t, "Deckle", None);
        assert_eq!(t.restarts, 1);
        assert!(!q.dir.join(RESTART_OWED).exists());
        assert!(p.uploaded.is_empty() && p.errors.is_empty());
        // Nothing owed, nothing queued: the tablet isn't even asked.
        let asked = t.commands.len();
        push(&q, &mut t, "Deckle", None);
        assert_eq!(t.commands.len(), asked);
    }

    #[test]
    fn prune_marks_old_papers_deleted_and_keeps_the_newest() {
        let tmp = tempfile::tempdir().unwrap();
        let q = queue_with(tmp.path(), &[]);
        let mut t = FakeTablet {
            now: 1_000_000,
            ..Default::default()
        };
        t.with_doc("f1", "Deckle", "CollectionType", "", 0);
        for d in ["20", "21", "22", "23"] {
            t.with_doc(
                &format!("d{d}"),
                &format!("deckle-daily-2026-10-{d}"),
                "DocumentType",
                "f1",
                0,
            );
        }
        t.with_doc("x", "Ink", "DocumentType", "f1", 0);
        t.with_doc("k", "deckle-daily-2026-10-19", "DocumentType", "", 0); // not in the folder
        let exempt = vec!["deckle-daily-2026-10-20".to_owned()];
        let p = push(&q, &mut t, "Deckle", Some(("deckle-daily-", 2, &exempt)));
        assert_eq!(p.pruned, ["deckle-daily-2026-10-21"]);
        assert_eq!(t.metadata("d21")["deleted"], true);
        assert_eq!(t.metadata("d21")["version"], 2);
        assert_eq!(t.metadata("d22")["deleted"], false);
        assert_eq!(t.metadata("d20")["deleted"], false, "exempt");
        assert_eq!(t.metadata("k")["deleted"], false, "outside the folder");
        assert_eq!(t.restarts, 1);
    }

    #[test]
    fn dated_names_are_deckles_papers_only() {
        assert!(is_dated("deckle-daily-2026-10-23", "deckle-daily-"));
        assert!(!is_dated("deckle-daily-2026-10-23.pdf", "deckle-daily-"));
        assert!(!is_dated("Ink", "deckle-daily-"));
        assert!(!is_dated("deckle-daily-x", "deckle-daily-"));
    }

    // ---- the transport, against an in-process SSH server standing in for dropbear --------

    use super::fake::dropbear;

    #[test]
    fn the_transport_runs_a_command_with_stdin_and_pins_the_host_key() {
        let (host, port, fingerprint) = dropbear(|cmd, stdin| {
            (
                format!("ran:{cmd}|{}", String::from_utf8_lossy(stdin)).into_bytes(),
                if cmd == "fail" { 7 } else { 0 },
            )
        });
        let timeout = std::time::Duration::from_secs(10);
        // First use: trusted, and the fingerprint comes back to pin.
        let mut c = connect(&host, port, "pw", None, timeout).unwrap();
        assert_eq!(c.fingerprint, fingerprint);
        let out = c.shell.run("cat > x", b"hello").unwrap();
        assert_eq!(out.status, 0);
        assert_eq!(String::from_utf8_lossy(&out.stdout), "ran:cat > x|hello");
        let out = c.shell.run("fail", b"").unwrap();
        assert_eq!(out.status, 7);
        drop(c);
        // Pinned and matching: fine. Pinned to something else: refused.
        connect(&host, port, "pw", Some(&fingerprint), timeout).unwrap();
        let err = connect(&host, port, "pw", Some("SHA256:nope"), timeout).unwrap_err();
        assert!(
            matches!(err, ConnectError::HostKeyChanged { ref seen, ref pinned } if *seen == fingerprint && pinned == "SHA256:nope"),
            "{err:?}"
        );
        // A wrong password is said plainly.
        let err = connect(&host, port, "wrong", None, timeout).unwrap_err();
        assert_eq!(err, ConnectError::BadPassword);
    }
}

/// A stand-in tablet for tests: an in-process SSH server (root, password `pw`) whose exec
/// channel answers through a closure. Other crates' tests get it with the `fake-tablet`
/// feature.
#[cfg(any(test, feature = "fake-tablet"))]
pub mod fake {
    use std::collections::BTreeMap;

    /// What a command does: its standard output and exit status.
    pub type Exec = std::sync::Arc<dyn Fn(&str, &[u8]) -> (Vec<u8>, u32) + Send + Sync>;
    /// Each open channel's command and the stdin gathered for it.
    type Pending = std::sync::Arc<std::sync::Mutex<BTreeMap<russh::ChannelId, (String, Vec<u8>)>>>;

    #[derive(Clone)]
    struct Dropbear {
        exec: Exec,
        cmd: Pending,
    }

    impl russh::server::Server for Dropbear {
        type Handler = Self;
        fn new_client(&mut self, _: Option<std::net::SocketAddr>) -> Self {
            self.clone()
        }
    }

    impl russh::server::Handler for Dropbear {
        type Error = russh::Error;

        async fn auth_password(
            &mut self,
            user: &str,
            password: &str,
        ) -> Result<russh::server::Auth, Self::Error> {
            Ok(if user == "root" && password == "pw" {
                russh::server::Auth::Accept
            } else {
                russh::server::Auth::reject()
            })
        }

        async fn channel_open_session(
            &mut self,
            _channel: russh::Channel<russh::server::Msg>,
            reply: russh::server::ChannelOpenHandle,
            _session: &mut russh::server::Session,
        ) -> Result<(), Self::Error> {
            reply.accept().await;
            Ok(())
        }

        async fn exec_request(
            &mut self,
            channel: russh::ChannelId,
            data: &[u8],
            session: &mut russh::server::Session,
        ) -> Result<(), Self::Error> {
            self.cmd.lock().unwrap().insert(
                channel,
                (String::from_utf8_lossy(data).into_owned(), Vec::new()),
            );
            session.channel_success(channel)?;
            Ok(())
        }

        async fn data(
            &mut self,
            channel: russh::ChannelId,
            data: &[u8],
            _session: &mut russh::server::Session,
        ) -> Result<(), Self::Error> {
            if let Some((_, stdin)) = self.cmd.lock().unwrap().get_mut(&channel) {
                stdin.extend_from_slice(data);
            }
            Ok(())
        }

        async fn channel_eof(
            &mut self,
            channel: russh::ChannelId,
            session: &mut russh::server::Session,
        ) -> Result<(), Self::Error> {
            let (cmd, stdin) = self
                .cmd
                .lock()
                .unwrap()
                .remove(&channel)
                .unwrap_or_default();
            let (stdout, status) = (self.exec)(&cmd, &stdin);
            session.data(channel, stdout)?;
            session.exit_status_request(channel, status)?;
            session.eof(channel)?;
            session.close(channel)?;
            Ok(())
        }
    }

    /// Starts the stand-in on a free port, answering each command with `exec`; its host,
    /// port and host key fingerprint.
    pub fn dropbear(
        exec: impl Fn(&str, &[u8]) -> (Vec<u8>, u32) + Send + Sync + 'static,
    ) -> (String, u16, String) {
        let exec: Exec = std::sync::Arc::new(exec);
        use russh::keys::HashAlg;
        // A fixed key: the test needs no randomness, and the fingerprint is the same each run.
        let key = russh::keys::PrivateKey::from(
            russh::keys::ssh_key::private::Ed25519Keypair::from_seed(&[7u8; 32]),
        );
        let fingerprint = key.public_key().fingerprint(HashAlg::Sha256).to_string();
        let config = std::sync::Arc::new(russh::server::Config {
            auth_rejection_time: std::time::Duration::from_millis(10),
            auth_rejection_time_initial: Some(std::time::Duration::from_millis(0)),
            keys: vec![key],
            ..Default::default()
        });
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let port = listener.local_addr().unwrap().port();
        std::thread::spawn(move || {
            let rt = tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()
                .unwrap();
            rt.block_on(async move {
                listener.set_nonblocking(true).unwrap();
                let listener = tokio::net::TcpListener::from_std(listener).unwrap();
                let mut server = Dropbear {
                    exec,
                    cmd: Default::default(),
                };
                use russh::server::Server as _;
                server.run_on_socket(config, &listener).await.unwrap();
            });
        });
        ("127.0.0.1".to_owned(), port, fingerprint)
    }
}
