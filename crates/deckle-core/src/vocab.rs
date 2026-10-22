// SPDX-License-Identifier: GPL-3.0-or-later
//! The Kindle's `system/vocabulary/vocab.db` (From Your Shelf design §4.3), read over the
//! connection an action already holds.

use std::path::Path;

use crate::deliver::{Conn, DeliverError, Target};

const DIR: [&str; 2] = ["system", "vocabulary"];

/// The vocabulary builder's database and, when the Kindle holds the database open, its
/// write-ahead log: the newest lookups live in the log until it is checkpointed.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct VocabFiles {
    pub db: Vec<u8>,
    pub wal: Option<Vec<u8>>,
}

/// `system/vocabulary/vocab.db` (and `-wal`) from the connection's target; `Ok(None)` when the
/// Kindle doesn't share it. A folder export has no `system/`.
pub fn read_vocab_on(conn: &mut Conn<'_>) -> Result<Option<VocabFiles>, DeliverError> {
    match conn.target {
        Target::Folder(_) => Ok(None),
        Target::MassStorage(dev) => {
            let dir = dev.root.join(DIR[0]).join(DIR[1]);
            let Some(db) = read_file(&dir.join("vocab.db"))? else {
                return Ok(None);
            };
            let wal = read_file(&dir.join("vocab.db-wal"))?;
            Ok(Some(VocabFiles { db, wal }))
        }
        Target::Mtp(_) => {
            let mtp_err = |e: deckle_transport::session::MtpError| DeliverError::Mtp(e.to_string());
            let dev = match conn.mtp() {
                Some(Ok(dev)) => dev,
                Some(Err(e)) => return Err(mtp_err(e)),
                None => unreachable!("an MTP target always has a connection"),
            };
            let storage = *dev
                .storages()
                .map_err(mtp_err)?
                .first()
                .ok_or_else(|| DeliverError::Mtp("device reports no storage".into()))?;
            let root = dev.content_root(storage).map_err(mtp_err)?;
            let Some(dir) = dev.find_path(storage, root, &DIR).map_err(mtp_err)? else {
                return Ok(None);
            };
            let mut get = |name: &str| match dev.find_child(storage, dir, name).map_err(mtp_err)? {
                Some(h) => dev.get_object(h).map(Some).map_err(mtp_err),
                None => Ok(None),
            };
            let Some(db) = get("vocab.db")? else {
                return Ok(None);
            };
            let wal = get("vocab.db-wal")?;
            Ok(Some(VocabFiles { db, wal }))
        }
    }
}

fn read_file(path: &Path) -> Result<Option<Vec<u8>>, DeliverError> {
    match std::fs::read(path) {
        Ok(bytes) => Ok(Some(bytes)),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(e) => Err(DeliverError::Refused(format!(
            "can't read {}: {e}",
            path.display()
        ))),
    }
}

#[cfg(test)]
mod tests {
    use deckle_transport::usb::MtpInfo;

    use super::*;
    use crate::deliver::{Conn, Device, Target};
    use crate::fake_mtp as fake;

    fn kindle(root: &std::path::Path) -> Target {
        Target::MassStorage(Device {
            root: root.to_path_buf(),
            model_hint: "Kindle Paperwhite".into(),
            thumbnails_writable: true,
            firmware: None,
        })
    }

    #[test]
    fn vocab_from_mass_storage_with_wal() {
        let tmp = tempfile::tempdir().unwrap();
        let dir = tmp.path().join("system/vocabulary");
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("vocab.db"), b"DB").unwrap();
        std::fs::write(dir.join("vocab.db-wal"), b"WAL").unwrap();
        let target = kindle(tmp.path());
        let got = read_vocab_on(&mut Conn::new(&target)).unwrap().unwrap();
        assert_eq!(got.db, b"DB");
        assert_eq!(got.wal.as_deref(), Some(&b"WAL"[..]));
    }

    #[test]
    fn vocab_absent_is_none() {
        let tmp = tempfile::tempdir().unwrap();
        let target = kindle(tmp.path());
        assert!(read_vocab_on(&mut Conn::new(&target)).unwrap().is_none());
        let folder = Target::Folder(tmp.path().to_owned());
        assert!(read_vocab_on(&mut Conn::new(&folder)).unwrap().is_none());
        fake::reset(&[]);
        let mtp = Target::Mtp(MtpInfo {
            vendor_id: deckle_transport::usb::AMAZON_VENDOR_ID,
            product_id: 1,
            serial: Some("G0001".into()),
            product: Some("Kindle".into()),
            bus: 1,
            address: 2,
        });
        let mut conn = Conn::with_opener(&mtp, fake::open);
        assert!(read_vocab_on(&mut conn).unwrap().is_none());
    }
}
