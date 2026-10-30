// SPDX-License-Identifier: GPL-3.0-or-later
//! The Kindle's `documents/My Clippings.txt` (highlights export design §3), read over the
//! connection an action already holds.

use std::path::Path;

use crate::deliver::{Conn, DeliverError, Target};

pub const CLIPPINGS: &str = "My Clippings.txt";

/// `documents/My Clippings.txt` from the connection's target, with KOReader's highlights after
/// it on a folder or a Kindle on USB storage (KOReader reads design §6); `Ok(None)` when there
/// are none.
pub fn read_on(conn: &mut Conn<'_>) -> Result<Option<Vec<u8>>, DeliverError> {
    match conn.target {
        // A folder export stands in for `documents/`, as in the inventory.
        Target::Folder(dir) => with_koreader(read_file(&dir.join(CLIPPINGS))?, dir),
        Target::MassStorage(dev) => with_koreader(
            read_file(&dev.root.join("documents").join(CLIPPINGS))?,
            &dev.root,
        ),
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
            let docs = dev.find_documents(storage).map_err(mtp_err)?;
            match dev.find_child(storage, docs, CLIPPINGS).map_err(mtp_err)? {
                Some(h) => dev.get_object(h).map(Some).map_err(mtp_err),
                None => Ok(None),
            }
        }
    }
}

/// `clippings`, then the KOReader highlights under `root`. A sidecar that can't be read is a
/// warning on stderr.
fn with_koreader(clippings: Option<Vec<u8>>, root: &Path) -> Result<Option<Vec<u8>>, DeliverError> {
    let ko = crate::koreader::read(root);
    for w in &ko.warnings {
        eprintln!("KOReader: {w}");
    }
    Ok(match (clippings, ko.clippings) {
        (Some(mut a), Some(b)) => {
            if !a.ends_with(b"\n") {
                a.push(b'\n');
            }
            a.extend(b);
            Some(a)
        }
        (a, b) => a.or(b),
    })
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

/// A Kindle that keeps its notes where USB can't reach them: the Scribe 2024 over MTP has no
/// `My Clippings.txt` there (#99), so a missing file says nothing about its owner's notes.
pub fn notes_unreachable(target: &Target) -> bool {
    matches!(target, Target::Mtp(i)
        if i.product.as_deref().is_some_and(|p| p.to_lowercase().contains("scribe")))
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use deckle_transport::usb::MtpInfo;

    use super::*;
    use crate::deliver::{Device, Target};
    use crate::fake_mtp as fake;

    fn mtp_target() -> Target {
        Target::Mtp(MtpInfo {
            vendor_id: deckle_transport::usb::AMAZON_VENDOR_ID,
            product_id: 1,
            serial: Some("G0001".into()),
            product: Some("Kindle".into()),
            bus: 1,
            address: 2,
        })
    }

    #[test]
    fn reads_from_a_folder_and_mass_storage() {
        let tmp = tempfile::tempdir().unwrap();
        std::fs::write(tmp.path().join(CLIPPINGS), b"folder").unwrap();
        let folder = Target::Folder(tmp.path().to_owned());
        assert_eq!(
            read_on(&mut Conn::new(&folder)).unwrap().as_deref(),
            Some(&b"folder"[..])
        );

        let root = tmp.path().join("Kindle");
        std::fs::create_dir_all(root.join("documents")).unwrap();
        std::fs::write(root.join("documents").join(CLIPPINGS), b"usb").unwrap();
        let ms = Target::MassStorage(Device {
            root: root.clone(),
            model_hint: "Kindle".into(),
            thumbnails_writable: true,
            firmware: None,
            serial: None,
        });
        assert_eq!(
            read_on(&mut Conn::new(&ms)).unwrap().as_deref(),
            Some(&b"usb"[..])
        );
    }

    #[test]
    fn reads_over_mtp_in_the_connection_it_is_given() {
        fake::reset(&["a.kfx"]);
        // Bigger than one USB read, so it arrives in pieces.
        let big: Vec<u8> = (0..300_000u32).map(|i| (i % 251) as u8).collect();
        fake::put_bytes(CLIPPINGS, &big);
        let target = mtp_target();
        let mut conn = Conn::with_opener(&target, fake::open);
        assert_eq!(read_on(&mut conn).unwrap(), Some(big));
        drop(conn);
        assert_eq!(fake::opens(), 1);
    }

    #[test]
    fn no_file_is_none_not_an_error() {
        let tmp = tempfile::tempdir().unwrap();
        let folder = Target::Folder(tmp.path().to_owned());
        assert_eq!(read_on(&mut Conn::new(&folder)).unwrap(), None);
        let ms = Target::MassStorage(Device {
            root: PathBuf::from("/nonexistent/kindle"),
            model_hint: "Kindle".into(),
            thumbnails_writable: false,
            firmware: None,
            serial: None,
        });
        assert_eq!(read_on(&mut Conn::new(&ms)).unwrap(), None);
        fake::reset(&["a.kfx"]);
        let target = mtp_target();
        assert_eq!(
            read_on(&mut Conn::with_opener(&target, fake::open)).unwrap(),
            None
        );
    }

    #[test]
    fn only_a_scribe_over_mtp_hides_its_notes() {
        let mtp = |product: &str| {
            Target::Mtp(deckle_transport::usb::MtpInfo {
                vendor_id: 0x1949,
                product_id: 0x9981,
                serial: None,
                product: Some(product.into()),
                bus: 1,
                address: 2,
            })
        };
        assert!(notes_unreachable(&mtp("Kindle Scribe")));
        assert!(!notes_unreachable(&mtp("Kindle Paperwhite")));
        assert!(!notes_unreachable(&Target::Folder("/tmp/k".into())));
    }
}
