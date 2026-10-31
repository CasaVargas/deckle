// SPDX-License-Identifier: GPL-3.0-or-later
//! One connected reader, driven from Deckle's side as calibre drives it (calibre bridge design
//! §1, §4.2): Deckle asks, the reader answers.

use std::io::{self, Read, Write};

use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use sha1::{Digest, Sha1};

use super::frame::{self, Op};

/// The calibre Deckle presents itself as: KOReader sends `ERROR` replies (not just a message on
/// its screen) to a calibre of 4.18 or later.
pub const CALIBRE_VERSION: [u32; 3] = [8, 0, 0];
/// calibre's `PROTOCOL_VERSION`.
pub const PROTOCOL_VERSION: u32 = 1;
/// The formats offered, as calibre's `ALL_FORMATS` begins; the reader answers with the ones
/// it takes.
pub const FORMATS: &[&str] = &[
    "epub", "kepub", "pdf", "azw3", "mobi", "fb2", "cbz", "txt", "html", "djvu",
];

/// Who Deckle says it is.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Library {
    pub name: String,
    pub uuid: String,
}

/// A reader, as its handshake describes it.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Device {
    /// `appName`, e.g. "KOReader".
    pub app: String,
    /// `deviceKind`, e.g. "Kobo_frost".
    pub kind: String,
    /// `deviceName`, e.g. "KOReader (Kobo_frost)".
    pub name: String,
    /// The reader's own id for its storage (`device_store_uuid`), which names it in the queue.
    pub store_uuid: String,
    pub extensions: Vec<String>,
    /// How many bytes of a book go in each write.
    pub packet: usize,
    pub cover_height: u32,
    /// It answers `SEND_BOOK` before the bytes (OK, or ERROR when it won't take the book).
    pub send_ok: bool,
    pub library_info: bool,
}

/// A book the reader says it has.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct OnReader {
    pub lpath: String,
    pub uuid: Option<String>,
    pub title: Option<String>,
    #[serde(default)]
    pub authors: Vec<String>,
    pub last_modified: Option<String>,
    pub size: Option<u64>,
}

/// What went wrong with a reader.
#[derive(Debug)]
pub enum Error {
    Io(io::Error),
    /// The reader's password didn't match; it has been told.
    Password,
    /// The reader answered, but not as calibre's protocol says.
    Protocol(String),
    /// The reader said goodbye (a `NOOP`) where an answer was due.
    HungUp,
}

impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Error::Io(e) => write!(f, "{e}"),
            Error::Password => write!(f, "the reader's password doesn't match"),
            Error::Protocol(why) => write!(f, "{why}"),
            Error::HungUp => write!(f, "the reader hung up"),
        }
    }
}

impl From<io::Error> for Error {
    fn from(e: io::Error) -> Self {
        Error::Io(e)
    }
}

fn protocol(why: impl Into<String>) -> Error {
    Error::Protocol(why.into())
}

/// `sha1(password + challenge)`, hex: what the reader answers a password challenge with.
pub fn password_hash(password: &str, challenge: &str) -> String {
    let mut h = Sha1::new();
    h.update(password.as_bytes());
    h.update(challenge.as_bytes());
    h.finalize().iter().map(|b| format!("{b:02x}")).collect()
}

pub struct Session<S> {
    stream: S,
    pub device: Device,
}

impl<S: Read + Write> Session<S> {
    pub fn new(stream: S) -> Self {
        Session {
            stream,
            device: Device::default(),
        }
    }

    pub fn stream(&mut self) -> &mut S {
        &mut self.stream
    }

    fn call(&mut self, op: Op, arg: Value) -> Result<Value, Error> {
        frame::send(&mut self.stream, op, &arg)?;
        self.answer()
    }

    /// The reader's next answer, which must be OK.
    fn answer(&mut self) -> Result<Value, Error> {
        let (code, arg) = frame::read(&mut self.stream)?;
        match Op::from_code(code) {
            Some(Op::Ok) => Ok(arg),
            Some(Op::Noop) => Err(Error::HungUp),
            Some(Op::Error) => Err(protocol(
                arg["message"].as_str().unwrap_or("the reader said no"),
            )),
            _ => Err(protocol(format!("expected OK, got opcode {code}"))),
        }
    }

    /// The handshake: who the reader is, the password when there is one, the drive info a
    /// calibre writes, and the library's name. `challenge` is an ISO time; `now` is too.
    pub fn handshake(
        &mut self,
        library: &Library,
        password: Option<&str>,
        challenge: &str,
        now: &str,
    ) -> Result<&Device, Error> {
        let password = password.filter(|p| !p.is_empty());
        let r = self.call(
            Op::GetInitializationInfo,
            json!({
                "serverProtocolVersion": PROTOCOL_VERSION,
                "validExtensions": FORMATS,
                "passwordChallenge": if password.is_some() { challenge } else { "" },
                "currentLibraryName": library.name,
                "currentLibraryUUID": library.uuid,
                "pubdateFormat": "MMM yyyy",
                "timestampFormat": "dd MMM yyyy",
                "lastModifiedFormat": "dd MMM yyyy",
                "calibre_version": CALIBRE_VERSION,
                "canSupportUpdateBooks": true,
                "canSupportLpathChanges": true,
            }),
        )?;
        if !r["versionOK"].as_bool().unwrap_or(false) {
            return Err(protocol("the reader doesn't speak this protocol version"));
        }
        let packet = r["maxBookContentPacketLen"].as_u64().unwrap_or(0) as usize;
        if packet == 0 {
            return Err(protocol("the reader gave no packet length"));
        }
        let can = |k: &str| r[k].as_bool().unwrap_or(false);
        if !(can("canStreamBooks")
            && can("canStreamMetadata")
            && can("canReceiveBookBinary")
            && can("canDeleteMultipleBooks"))
        {
            return Err(protocol(
                "the app on the reader is too old for calibre's wireless connection",
            ));
        }
        let extensions: Vec<String> = r["acceptedExtensions"]
            .as_array()
            .map(|a| {
                a.iter()
                    .filter_map(|e| e.as_str().map(str::to_ascii_lowercase))
                    .collect()
            })
            .unwrap_or_default();
        if extensions.is_empty() {
            return Err(protocol("the reader accepts no formats"));
        }
        if let Some(p) = password
            && r["passwordHash"].as_str() != Some(password_hash(p, challenge).as_str())
        {
            // Not waited on: the reader shows it, then the connection closes.
            let _ = frame::send(
                &mut self.stream,
                Op::DisplayMessage,
                &json!({
                    "messageKind": 1,
                    "currentLibraryName": library.name,
                    "currentLibraryUUID": library.uuid,
                }),
            );
            return Err(Error::Password);
        }
        let kind = r["deviceKind"].as_str().unwrap_or("").to_owned();
        self.device = Device {
            app: r["appName"].as_str().unwrap_or("").to_owned(),
            name: r["deviceName"].as_str().unwrap_or(&kind).to_owned(),
            kind,
            store_uuid: String::new(),
            extensions,
            packet: packet.clamp(512, 1024 * 1024),
            cover_height: r["coverHeight"].as_u64().unwrap_or(160) as u32,
            send_ok: can("canSendOkToSendbook"),
            library_info: can("canAcceptLibraryInfo"),
        };

        let info = self.call(Op::GetDeviceInformation, json!({}))?;
        let d = &info["device_info"];
        self.device.store_uuid = d["device_store_uuid"].as_str().unwrap_or("").to_owned();
        if let Some(n) = d["device_name"].as_str().filter(|n| !n.is_empty()) {
            self.device.name = n.to_owned();
        }
        if self.device.store_uuid.is_empty() {
            // A reader that never met a calibre has none yet: it keeps the one sent below.
            self.device.store_uuid = uuid::Uuid::new_v4().to_string();
        }
        let v = CALIBRE_VERSION.map(|n| n.to_string()).join(".");
        self.call(
            Op::SetCalibreDeviceInfo,
            json!({
                "device_store_uuid": self.device.store_uuid,
                "device_name": self.device.name,
                "location_code": "main",
                "prefix": "",
                "calibre_version": v,
                "last_library_uuid": library.uuid,
                "date_last_connected": now,
            }),
        )?;
        if self.device.library_info {
            self.call(
                Op::SetLibraryInfo,
                json!({
                    "libraryName": library.name,
                    "libraryUuid": library.uuid,
                    "fieldMetadata": {},
                    "otherInfo": { "id_link_rules": {} },
                }),
            )?;
        }
        Ok(&self.device)
    }

    /// The bytes free on the reader.
    pub fn free_space(&mut self) -> Result<u64, Error> {
        let r = self.call(Op::FreeSpace, json!({}))?;
        Ok(bytes(&r["free_space_on_device"]).unwrap_or(0))
    }

    /// Every book the reader has. Titles and authors are asked for only for books `known`
    /// doesn't recognize (by `lpath`), as calibre does for books missing from its cache.
    pub fn books(&mut self, known: impl Fn(&OnReader) -> bool) -> Result<Vec<OnReader>, Error> {
        let r = self.call(
            Op::GetBookCount,
            json!({
                "canStream": true,
                "canScan": true,
                "willUseCachedMetadata": true,
                "supportsSync": false,
                "canSupportBookFormatSync": true,
            }),
        )?;
        let count = r["count"].as_u64().unwrap_or(0).min(1_000_000) as usize;
        let mut ids = Vec::with_capacity(count.min(10_000));
        for _ in 0..count {
            ids.push(self.answer()?);
        }
        let mut books: Vec<OnReader> = ids.iter().map(on_reader).collect();
        let wanted: Vec<(usize, Value)> = ids
            .iter()
            .enumerate()
            .filter(|(i, _)| !known(&books[*i]))
            .map(|(i, id)| (i, id["priKey"].clone()))
            .collect();
        if !wanted.is_empty() {
            frame::send(
                &mut self.stream,
                Op::Noop,
                &json!({ "count": wanted.len() }),
            )?;
            for (_, key) in &wanted {
                frame::send(&mut self.stream, Op::Noop, &json!({ "priKey": key }))?;
            }
            for (i, _) in &wanted {
                let full = on_reader(&self.answer()?);
                books[*i] = OnReader {
                    lpath: books[*i].lpath.clone(),
                    ..full
                };
            }
        }
        Ok(books)
    }

    /// Sends one book: `metadata` is calibre's record of it (see [`super::book::metadata`]).
    /// Returns where the reader put it.
    pub fn send_book(
        &mut self,
        lpath: &str,
        metadata: Value,
        bytes: &[u8],
        this_book: usize,
        total_books: usize,
    ) -> Result<String, Error> {
        let args = json!({
            "lpath": lpath,
            "length": bytes.len(),
            "metadata": metadata,
            "thisBook": this_book,
            "totalBooks": total_books,
            "willStreamBooks": true,
            "willStreamBinary": true,
            "wantsSendOkToSendbook": self.device.send_ok,
            "canSupportLpathChanges": true,
        });
        let mut at = lpath.to_owned();
        if self.device.send_ok {
            let r = self.call(Op::SendBook, args)?;
            if let Some(l) = r["lpath"].as_str().filter(|l| !l.is_empty()) {
                at = l.to_owned();
            }
        } else {
            frame::send(&mut self.stream, Op::SendBook, &args)?;
        }
        for chunk in bytes.chunks(self.device.packet.max(1)) {
            self.stream.write_all(chunk)?;
        }
        self.stream.flush()?;
        Ok(at)
    }

    /// Takes books off the reader. Returns the uuids it reports removing.
    pub fn delete(&mut self, lpaths: &[String]) -> Result<Vec<String>, Error> {
        self.call(Op::DeleteBook, json!({ "lpaths": lpaths }))?;
        let mut gone = Vec::new();
        for _ in lpaths {
            let r = self.answer()?;
            if let Some(u) = r["uuid"].as_str() {
                gone.push(u.to_owned());
            }
        }
        Ok(gone)
    }

    /// The reader's collections, `name → absolute paths`. A KOReader too old to know the
    /// request never answers, so the caller sets a short read timeout first.
    pub fn collections(&mut self) -> Result<Value, Error> {
        let r = self.call(Op::GetCollections, json!({}))?;
        Ok(r["collections"].clone())
    }

    /// Changes KOReader's collections: `create` and `delete` whole ones, `add` and `remove`
    /// books (absolute paths on the reader) in them.
    pub fn update_collections(
        &mut self,
        create: &[String],
        delete: &[String],
        add: &std::collections::BTreeMap<String, Vec<String>>,
        remove: &std::collections::BTreeMap<String, Vec<String>>,
    ) -> Result<(), Error> {
        self.call(
            Op::UpdateCollections,
            json!({
                "add_collections": create,
                "remove_collections": delete,
                "add": add,
                "remove": remove,
            }),
        )
        .map(|_| ())
    }

    /// Keeps the connection alive.
    pub fn noop(&mut self) -> Result<(), Error> {
        self.call(Op::Noop, json!({})).map(|_| ())
    }

    /// Ends the session, as calibre's eject does.
    pub fn eject(&mut self) -> Result<(), Error> {
        self.call(Op::Noop, json!({ "ejecting": true })).map(|_| ())
    }
}

fn on_reader(v: &Value) -> OnReader {
    let s = |k: &str| v[k].as_str().filter(|s| !s.is_empty()).map(str::to_owned);
    OnReader {
        lpath: s("lpath").unwrap_or_default(),
        uuid: s("uuid"),
        title: s("title"),
        authors: v["authors"]
            .as_array()
            .map(|a| {
                a.iter()
                    .filter_map(|x| x.as_str().map(str::to_owned))
                    .collect()
            })
            .unwrap_or_default(),
        last_modified: s("last_modified"),
        size: bytes(&v["size"]),
    }
}

/// A count of bytes, whole or as a float: KOReader's numbers are Lua numbers, and a large one
/// (its free space) can arrive as `2.55717e10`.
fn bytes(v: &Value) -> Option<u64> {
    v.as_u64().or_else(|| {
        v.as_f64()
            .filter(|f| f.is_finite() && *f >= 0.0)
            .map(|f| f.round() as u64)
    })
}

/// Tells a reader that connected while another is: calibre's `CALIBRE_BUSY`.
pub fn busy(w: &mut impl Write, other: &str) -> io::Result<()> {
    frame::send(w, Op::CalibreBusy, &json!({ "otherDevice": other }))
}

#[cfg(test)]
mod tests {
    #[test]
    fn byte_counts_read_whole_or_as_luas_floats() {
        use serde_json::json;
        // KOReader's free space is a Lua number, which can reach us as a float.
        assert_eq!(
            super::bytes(&json!(25_571_700_000u64)),
            Some(25_571_700_000)
        );
        assert_eq!(super::bytes(&json!(2.55717e10)), Some(25_571_700_000));
        assert_eq!(super::bytes(&json!(-1.0)), None);
        assert_eq!(super::bytes(&json!("12")), None);
    }

    #[test]
    fn the_password_hash_is_sha1_of_password_then_challenge() {
        // `printf 'secret2026-10-31T12:00:00+00:00' | shasum`
        assert_eq!(
            super::password_hash("secret", "2026-10-31T12:00:00+00:00"),
            "0e08bffc8e2f492e1adbafdd4eb5496a55d67032"
        );
    }
}
