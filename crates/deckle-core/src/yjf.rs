// SPDX-License-Identifier: GPL-3.0-or-later
//! Each Deckle Daily edition's `.yjf` reading-position sidecar (readership design §3), read
//! over the connection an action already holds: `documents/<stem>.sdr/<stem><suffix>.yjf`.

use std::path::Path;

use crate::deliver::{Conn, DeliverError, Target};

/// An edition as found on the Kindle.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum OnDevice {
    /// No `documents/<stem>.kfx`.
    Absent,
    /// The edition is there but was never opened: no `.sdr`, or no `.yjf` in it.
    Unopened,
    /// The `.yjf`'s bytes.
    Opened(Vec<u8>),
}

fn is_yjf(name: &str) -> bool {
    name.to_ascii_lowercase().ends_with(".yjf")
}

/// What the Kindle has for each edition stem.
pub fn read_on(
    conn: &mut Conn<'_>,
    stems: &[String],
) -> Result<Vec<(String, OnDevice)>, DeliverError> {
    if stems.is_empty() {
        return Ok(Vec::new());
    }
    match conn.target {
        // A folder export stands in for `documents/`, as in the inventory.
        Target::Folder(dir) => Ok(read_dir(dir, stems)),
        Target::MassStorage(dev) => Ok(read_dir(&dev.root.join("documents"), stems)),
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
            let listing = dev.list(storage, docs).map_err(mtp_err)?;
            let mut out = Vec::new();
            for stem in stems {
                let kfx = format!("{stem}.kfx");
                let sdr = format!("{stem}.sdr");
                let found = if !listing
                    .iter()
                    .any(|(_, i)| !i.association && i.filename == kfx)
                {
                    OnDevice::Absent
                } else if let Some((h, _)) = listing
                    .iter()
                    .find(|(_, i)| i.association && i.filename == sdr)
                {
                    let inside = dev.list(storage, *h).map_err(mtp_err)?;
                    match inside
                        .iter()
                        .find(|(_, i)| !i.association && is_yjf(&i.filename))
                    {
                        Some((fh, _)) => OnDevice::Opened(dev.get_object(*fh).map_err(mtp_err)?),
                        None => OnDevice::Unopened,
                    }
                } else {
                    OnDevice::Unopened
                };
                out.push((stem.clone(), found));
            }
            Ok(out)
        }
    }
}

fn read_dir(docs: &Path, stems: &[String]) -> Vec<(String, OnDevice)> {
    stems
        .iter()
        .map(|stem| {
            let found = if !docs.join(format!("{stem}.kfx")).is_file() {
                OnDevice::Absent
            } else {
                std::fs::read_dir(docs.join(format!("{stem}.sdr")))
                    .ok()
                    .and_then(|entries| {
                        entries.flatten().find(|e| {
                            e.path().is_file() && is_yjf(&e.file_name().to_string_lossy())
                        })
                    })
                    .and_then(|e| std::fs::read(e.path()).ok())
                    .map_or(OnDevice::Unopened, OnDevice::Opened)
            };
            (stem.clone(), found)
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use deckle_transport::usb::MtpInfo;

    use super::*;
    use crate::deliver::{Device, Target};
    use crate::fake_mtp as fake;

    const E: &str = "deckle-daily-2026-10-21";

    fn stems() -> Vec<String> {
        vec![E.to_owned()]
    }

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

    fn edition_in(docs: &Path, with_sdr: bool, with_yjf: bool) {
        std::fs::create_dir_all(docs).unwrap();
        std::fs::write(docs.join(format!("{E}.kfx")), b"KFX").unwrap();
        if with_sdr {
            let sdr = docs.join(format!("{E}.sdr"));
            std::fs::create_dir_all(&sdr).unwrap();
            std::fs::write(sdr.join(format!("{E}9d6e5f44.yjr")), b"YJR").unwrap();
            std::fs::write(sdr.join(format!("{E}9d6e5f44.yjr.bad_file")), b"BAD").unwrap();
            if with_yjf {
                std::fs::write(sdr.join(format!("{E}9d6e5f44.yjf")), b"YJF").unwrap();
            }
        }
    }

    #[test]
    fn reads_yjf_from_a_folder_and_mass_storage() {
        let tmp = tempfile::tempdir().unwrap();
        edition_in(tmp.path(), true, true);
        let folder = Target::Folder(tmp.path().to_owned());
        assert_eq!(
            read_on(&mut Conn::new(&folder), &stems()).unwrap(),
            [(E.to_owned(), OnDevice::Opened(b"YJF".to_vec()))]
        );
        let root = tmp.path().join("Kindle");
        edition_in(&root.join("documents"), true, true);
        let ms = Target::MassStorage(Device {
            root,
            model_hint: "Kindle".into(),
            thumbnails_writable: true,
            firmware: None,
        });
        assert_eq!(
            read_on(&mut Conn::new(&ms), &stems()).unwrap()[0].1,
            OnDevice::Opened(b"YJF".to_vec()),
            "only the .yjf, not the .yjr or .bad_file"
        );
    }

    #[test]
    fn an_edition_not_on_the_kindle_is_absent_not_unopened() {
        let tmp = tempfile::tempdir().unwrap();
        edition_in(tmp.path(), true, true);
        std::fs::remove_file(tmp.path().join(format!("{E}.kfx"))).unwrap();
        let folder = Target::Folder(tmp.path().to_owned());
        assert_eq!(
            read_on(&mut Conn::new(&folder), &stems()).unwrap()[0].1,
            OnDevice::Absent
        );
    }

    #[test]
    fn a_kfx_without_a_sidecar_is_unopened() {
        let tmp = tempfile::tempdir().unwrap();
        edition_in(tmp.path(), false, false);
        let folder = Target::Folder(tmp.path().to_owned());
        assert_eq!(
            read_on(&mut Conn::new(&folder), &stems()).unwrap()[0].1,
            OnDevice::Unopened
        );
        let tmp = tempfile::tempdir().unwrap();
        edition_in(tmp.path(), true, false);
        let folder = Target::Folder(tmp.path().to_owned());
        assert_eq!(
            read_on(&mut Conn::new(&folder), &stems()).unwrap()[0].1,
            OnDevice::Unopened,
            ".sdr without a .yjf"
        );
    }

    #[test]
    fn reads_over_mtp_in_the_connection_it_is_given() {
        fake::reset(&[&format!("{E}.kfx"), "deckle-daily-2026-10-20.kfx"]);
        fake::put_bytes(&format!("{E}.sdr/{E}9d6e5f44.yjr"), b"YJR");
        fake::put_bytes(&format!("{E}.sdr/{E}9d6e5f44.yjf"), b"YJF");
        let target = mtp_target();
        let mut conn = Conn::with_opener(&target, fake::open);
        let wanted = vec![
            E.to_owned(),
            "deckle-daily-2026-10-20".to_owned(),
            "deckle-daily-2026-10-19".to_owned(),
        ];
        let got = read_on(&mut conn, &wanted).unwrap();
        assert_eq!(got[0].1, OnDevice::Opened(b"YJF".to_vec()));
        assert_eq!(got[1].1, OnDevice::Unopened);
        assert_eq!(got[2].1, OnDevice::Absent);
        drop(conn);
        assert_eq!(fake::opens(), 1);
    }

    #[test]
    fn no_stems_opens_nothing() {
        fake::reset(&[]);
        let target = mtp_target();
        assert!(
            read_on(&mut Conn::with_opener(&target, fake::open), &[])
                .unwrap()
                .is_empty()
        );
        assert_eq!(fake::opens(), 0);
        let _ = PathBuf::new();
    }
}
