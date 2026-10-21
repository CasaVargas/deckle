// SPDX-License-Identifier: GPL-3.0-or-later
//! The outbox: books waiting for a Kindle. Built when no Kindle is connected, delivered by the
//! next `push` (or `watch`) over one connection, which also prunes old editions in that same
//! session.
//!
//! An entry is a finished `<name>.kfx`, delivered as is (the paper), or a prepared book
//! `<name>.book/` converted up to the KPF and finished at push for the Kindle that takes it
//! (device-aware build §4.2):
//!
//! ```text
//! <name>.book/
//!   book.kpf        Previewer's output
//!   prepared.json   the book's info and wishes, versioned
//!   fallback.kfx    the PDOC build, shipped if finishing fails
//! ```

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::deliver::{
    Conn, DeliverError, DeliverOptions, Target, deliver_named_on, prune_named_on,
};
use crate::devices::DeviceProfile;
use crate::inspect::BookInfo;
use crate::pipeline::{Prepared, RunOptions, Wishes, fallback_kfx, finish_with};
use crate::wordwise::WordWiseOptions;
use deckle_kfx::convert::CdeType;

/// A folder of `<name>.kfx` files and `<name>.book/` prepared books, delivered as `<name>.kfx`.
#[derive(Clone, Debug)]
pub struct Queue {
    pub dir: PathBuf,
}

/// Where a run of [`Queue::flush`] got to.
#[derive(Clone, Debug, Default, Serialize)]
pub struct FlushReport {
    /// Device paths written.
    pub delivered: Vec<PathBuf>,
    /// Old editions removed from the device.
    pub pruned: Vec<String>,
    /// `<name>: <warning>` from finishing prepared books.
    pub warnings: Vec<String>,
}

/// After a flush, editions named `<prefix>…` beyond the newest `keep` are removed.
#[derive(Clone, Debug)]
pub struct Prune {
    pub prefix: String,
    pub keep: usize,
}

/// How a flush finishes prepared books for the Kindle on the connection.
#[derive(Clone, Debug, Default)]
pub struct Finishing {
    /// The Kindle's profile from `devices.json`, when known: the owner's CDE override for it and
    /// whether spike 2 passed on it.
    pub profile: Option<DeviceProfile>,
    /// Where the Word Wise database is found (each book has its own level). A push never
    /// downloads it.
    pub word_wise: WordWiseOptions,
}

/// `(entry number from 1, entries, queue name)`, called before each entry is delivered.
pub type Progress<'a> = dyn FnMut(usize, usize, &str) + 'a;

/// The `prepared.json` this build writes; an entry with another version ships its fallback.
pub const PREPARED_VERSION: u32 = 1;
const PREPARED_EXT: &str = "book";

#[derive(Serialize, Deserialize)]
struct PreparedFile {
    version: u32,
    #[serde(default)]
    input: PathBuf,
    #[serde(default)]
    info: BookInfo,
    #[serde(default)]
    wishes: Wishes,
}

fn is_prepared(entry: &Path) -> bool {
    entry.extension().is_some_and(|x| x == PREPARED_EXT) && entry.is_dir()
}

/// The device file name of an entry: `<name>.kfx` for both kinds.
fn queue_name(entry: &Path) -> String {
    let file = entry
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_default();
    match file.strip_suffix(".book") {
        Some(stem) if is_prepared(entry) => format!("{stem}.kfx"),
        _ => file,
    }
}

fn remove_entry(entry: &Path) -> std::io::Result<()> {
    if entry.is_dir() {
        std::fs::remove_dir_all(entry)
    } else {
        std::fs::remove_file(entry)
    }
}

impl Queue {
    /// `<cache dir>/outbox`.
    pub fn default_dir() -> PathBuf {
        crate::wordwise::default_cache_dir().join("outbox")
    }

    pub fn new(dir: PathBuf) -> Self {
        Queue { dir }
    }

    /// Queued entries (`<name>.kfx` files and `<name>.book` folders), oldest name first.
    pub fn pending(&self) -> Vec<PathBuf> {
        let Ok(entries) = std::fs::read_dir(&self.dir) else {
            return Vec::new();
        };
        let mut files: Vec<PathBuf> = entries
            .flatten()
            .map(|e| e.path())
            .filter(|p| p.extension().is_some_and(|x| x == "kfx") || is_prepared(p))
            .collect();
        files.sort();
        files
    }

    /// Drops queued entries whose name starts with `prefix` (an edition supersedes yesterday's
    /// undelivered one), and any entry of either kind already called `name`.
    fn make_room(&self, name: &str, replaces: Option<&str>) -> std::io::Result<()> {
        std::fs::create_dir_all(&self.dir)?;
        for old in self.pending() {
            let file = old
                .file_name()
                .map(|n| n.to_string_lossy().into_owned())
                .unwrap_or_default();
            if replaces.is_some_and(|p| file.starts_with(p)) || queue_name(&old) == name {
                remove_entry(&old)?;
            }
        }
        Ok(())
    }

    /// Copies `kfx` into the queue as `name`, first dropping queued entries that `replaces`.
    pub fn add(&self, kfx: &Path, name: &str, replaces: Option<&str>) -> std::io::Result<PathBuf> {
        self.make_room(name, replaces)?;
        let dest = self.dir.join(name);
        std::fs::copy(kfx, &dest)?;
        Ok(dest)
    }

    /// Queues a prepared book to be delivered as `name` (`<stem>.kfx`), finished at push for
    /// the Kindle that takes it. Builds its fallback first: a book that can't even build that
    /// fails here (`InvalidData`), not at push.
    pub fn add_prepared(
        &self,
        prepared: &Prepared,
        name: &str,
        replaces: Option<&str>,
    ) -> std::io::Result<PathBuf> {
        let fallback = fallback_kfx(prepared)
            .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))?;
        self.add_prepared_with(prepared, &fallback, name, replaces)
    }

    /// [`Queue::add_prepared`] with its fallback already built ([`fallback_kfx`]).
    pub fn add_prepared_with(
        &self,
        prepared: &Prepared,
        fallback: &[u8],
        name: &str,
        replaces: Option<&str>,
    ) -> std::io::Result<PathBuf> {
        self.make_room(name, replaces)?;
        let stem = name.strip_suffix(".kfx").unwrap_or(name);
        // Written aside and renamed, so a `watch` never sees half an entry.
        let part = self.dir.join(format!("{stem}.{PREPARED_EXT}.part"));
        let _ = std::fs::remove_dir_all(&part);
        std::fs::create_dir_all(&part)?;
        std::fs::copy(&prepared.kpf, part.join("book.kpf"))?;
        std::fs::write(part.join("fallback.kfx"), fallback)?;
        let file = PreparedFile {
            version: PREPARED_VERSION,
            input: prepared.report.input.clone(),
            info: prepared.report.info.clone(),
            wishes: prepared.wishes.clone(),
        };
        std::fs::write(
            part.join("prepared.json"),
            serde_json::to_vec_pretty(&file).map_err(std::io::Error::other)?,
        )?;
        let dest = self.dir.join(format!("{stem}.{PREPARED_EXT}"));
        std::fs::rename(&part, &dest)?;
        Ok(dest)
    }

    /// Delivers every queued entry to `target` over one connection, then prunes, then removes
    /// the delivered entries from the queue. An entry stays queued if its delivery fails.
    pub fn flush(
        &self,
        target: &Target,
        prune: Option<&Prune>,
    ) -> Result<FlushReport, DeliverError> {
        self.flush_on(&mut Conn::new(target), prune)
    }

    /// [`Queue::flush`] over the caller's connection.
    pub fn flush_on(
        &self,
        conn: &mut Conn<'_>,
        prune: Option<&Prune>,
    ) -> Result<FlushReport, DeliverError> {
        self.flush_with(conn, prune, &Finishing::default(), &mut |_, _, _| {})
    }

    /// [`Queue::flush_on`], finishing prepared books with `finishing` and calling `progress`
    /// before each entry.
    pub fn flush_with(
        &self,
        conn: &mut Conn<'_>,
        prune: Option<&Prune>,
        finishing: &Finishing,
        progress: &mut Progress<'_>,
    ) -> Result<FlushReport, DeliverError> {
        let mut report = FlushReport::default();
        let pending = self.pending();
        for (i, entry) in pending.iter().enumerate() {
            let io = |source| DeliverError::Io {
                path: entry.clone(),
                source,
            };
            let name = queue_name(entry);
            progress(i + 1, pending.len(), &name);
            let path = if is_prepared(entry) {
                let (path, warnings) = deliver_prepared(entry, &name, conn, finishing)?;
                report
                    .warnings
                    .extend(warnings.into_iter().map(|w| format!("{name}: {w}")));
                path
            } else {
                let bytes = std::fs::read(entry).map_err(io)?;
                deliver_named_on(&bytes, &name, conn, &DeliverOptions::default())?
            };
            report.delivered.push(path);
            remove_entry(entry).map_err(io)?;
        }
        if let Some(p) = prune {
            report.pruned = prune_named_on(conn, &p.prefix, p.keep)?;
        }
        Ok(report)
    }
}

/// Reads a prepared entry back; `Err` names what's wrong with it.
fn load_prepared(entry: &Path, name: &str) -> Result<Prepared, String> {
    let json =
        std::fs::read(entry.join("prepared.json")).map_err(|e| format!("prepared.json: {e}"))?;
    let version = serde_json::from_slice::<serde_json::Value>(&json)
        .map_err(|e| format!("prepared.json: {e}"))?
        .get("version")
        .and_then(|v| v.as_u64());
    if version != Some(PREPARED_VERSION as u64) {
        return Err(format!(
            "prepared.json: unknown version {}",
            version
                .map(|v| v.to_string())
                .unwrap_or_else(|| "(none)".into())
        ));
    }
    let file: PreparedFile =
        serde_json::from_slice(&json).map_err(|e| format!("prepared.json: {e}"))?;
    Ok(Prepared::from_parts(
        file.input,
        file.info,
        entry.join("book.kpf"),
        file.wishes,
        Some(name.to_owned()),
    ))
}

/// What a push finishes a book with: its wishes, the device's override when the book has
/// none, and only offline work: heuristic X-Ray, no Amazon match (device-aware build §3, §5).
fn push_options(wishes: &Wishes, finishing: &Finishing, target: &Target) -> RunOptions {
    let profile = finishing.profile.as_ref();
    let device_override = match profile.and_then(|p| p.cde_override.as_deref()) {
        Some("ebok") => Some(CdeType::Ebok),
        Some("pdoc") => Some(CdeType::Pdoc),
        _ => None,
    };
    RunOptions {
        cde_type: CdeType::Pdoc,
        cde_override: wishes.cde_override.or(device_override),
        amazon_match: false,
        metadata_fill: false,
        typography: false,
        popup_notes: false,
        verified_real_asin: profile.is_some_and(|p| p.verified_real_asin),
        asin: None,
        pages: wishes.pages,
        target: target.clone(),
        previewer: None,
        keep_work_dir: false,
        xray: wishes.xray,
        llm: None,
        wikipedia: false,
        word_wise: wishes.word_wise.map(|level| WordWiseOptions {
            level,
            ..finishing.word_wise.clone()
        }),
        replaces: Vec::new(),
    }
}

/// Finishes a prepared entry for `conn`'s Kindle and delivers it as `name`; if finishing
/// fails, delivers its `fallback.kfx` under the same name instead. The warnings say which.
fn deliver_prepared(
    entry: &Path,
    name: &str,
    conn: &mut Conn<'_>,
    finishing: &Finishing,
) -> Result<(PathBuf, Vec<String>), DeliverError> {
    let failed = match load_prepared(entry, name) {
        Ok(prepared) => {
            let opts = push_options(&prepared.wishes, finishing, conn.target);
            let r = finish_with(&prepared, conn, &opts, &mut |_| {}, &mut |_| {
                Err("a push doesn't download it; the next `deckle run` with Word Wise does".into())
            });
            match (r.error, r.delivered) {
                (None, Some(path)) => return Ok((path, r.warnings)),
                (e, _) => e.unwrap_or_else(|| "not delivered".into()),
            }
        }
        Err(e) => e,
    };
    let fallback = entry.join("fallback.kfx");
    let bytes = std::fs::read(&fallback).map_err(|source| DeliverError::Io {
        path: fallback.clone(),
        source,
    })?;
    let path = deliver_named_on(&bytes, name, conn, &DeliverOptions::default())?;
    Ok((
        path,
        vec![format!(
            "not finished for this Kindle ({failed}); sent as a personal document with its \
             cover, without X-Ray or Word Wise"
        )],
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_new_edition_replaces_the_undelivered_one_and_flush_prunes() {
        let tmp = tempfile::tempdir().unwrap();
        let src = tmp.path().join("book.kfx");
        std::fs::write(&src, b"KFX").unwrap();
        let q = Queue::new(tmp.path().join("outbox"));
        q.add(&src, "deckle-daily-2026-10-19.kfx", Some("deckle-daily-"))
            .unwrap();
        q.add(&src, "deckle-daily-2026-10-20.kfx", Some("deckle-daily-"))
            .unwrap();
        q.add(&src, "other.kfx", None).unwrap();
        let names: Vec<_> = q
            .pending()
            .iter()
            .map(|p| p.file_name().unwrap().to_string_lossy().into_owned())
            .collect();
        assert_eq!(names, ["deckle-daily-2026-10-20.kfx", "other.kfx"]);

        let kindle = tmp.path().join("kindle");
        std::fs::create_dir_all(&kindle).unwrap();
        for old in ["deckle-daily-2026-10-17.kfx", "deckle-daily-2026-10-18.kfx"] {
            std::fs::write(kindle.join(old), b"KFX").unwrap();
        }
        let prune = Prune {
            prefix: "deckle-daily-".into(),
            keep: 2,
        };
        let r = q
            .flush(&Target::Folder(kindle.clone()), Some(&prune))
            .unwrap();
        assert_eq!(r.delivered.len(), 2);
        assert_eq!(r.pruned, ["deckle-daily-2026-10-17.kfx"]);
        assert!(q.pending().is_empty());
        assert!(kindle.join("deckle-daily-2026-10-20.kfx").exists());
        assert!(kindle.join("other.kfx").exists());
    }

    // Device-aware build (§7): prepared books are finished for the Kindle that takes them.
    // Unix only: the fake Previewer is a shell script.
    #[cfg(unix)]
    mod device_aware {
        use super::*;

        use std::cell::Cell;

        use deckle_transport::ptp::op;
        use deckle_transport::usb::MtpInfo;

        use crate::devices::DeviceProfile;
        use crate::fake_mtp as fake;
        use crate::pipeline::{Prepared, RunOptions, XRayMode, prepare};
        use crate::wordwise::WordWiseOptions;
        use deckle_kfx::convert::CdeType;

        const LONG_READ: &str = "deckle-reads-2026-10-21-mini.kfx";

        /// The mini fixture up to the KPF (the fake Previewer), asking for X-Ray and Word Wise as
        /// `digest run` asks for long reads.
        fn prepared_mini() -> Prepared {
            let opts = RunOptions {
                cde_type: CdeType::Pdoc,
                cde_override: None,
                amazon_match: false,
                metadata_fill: false,
                typography: false,
                popup_notes: false,
                verified_real_asin: false,
                asin: None,
                pages: None,
                target: Target::Folder(std::env::temp_dir()),
                previewer: Some(
                    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fake_previewer/ok.sh"),
                ),
                keep_work_dir: false,
                xray: XRayMode::Auto,
                llm: None,
                wikipedia: false,
                word_wise: Some(WordWiseOptions::default()),
                replaces: Vec::new(),
            };
            prepare(
                &deckle_kfx::fixtures::path("deckle-mini.epub"),
                &opts,
                &mut |_| {},
            )
            .expect("prepared")
        }

        /// A Word Wise database with one hint in the mini fixture.
        fn lexicon(dir: &Path) -> PathBuf {
            let db = dir.join("ww.sqlite");
            let c = deckle_wordwise::lexicon::create(&db, "test").unwrap();
            c.execute_batch(
                "INSERT INTO senses VALUES (1, 'universally', 'adverb', 'everywhere', 'everywhere', NULL, 4);
                 INSERT INTO forms VALUES ('universally', 1);",
            )
            .unwrap();
            db
        }

        /// Word Wise with nothing cached and (for a push) no download.
        fn uncached(dir: &Path) -> Finishing {
            Finishing {
                profile: None,
                word_wise: WordWiseOptions {
                    level: 5,
                    database: None,
                    cache_dir: dir.join("empty-cache"),
                },
            }
        }

        fn mass_storage_kindle(root: &Path) -> Target {
            std::fs::create_dir_all(root.join("documents")).unwrap();
            std::fs::create_dir_all(root.join("system/thumbnails")).unwrap();
            std::fs::write(
                root.join("system/version.txt"),
                "Kindle 5.16.2.1.1 (4184940002)\n",
            )
            .unwrap();
            Target::MassStorage(crate::deliver::detect_in(&[root.to_path_buf()]).remove(0))
        }

        fn cde_of(kfx: &[u8]) -> String {
            let doc = deckle_kfx::container::read_kfx(kfx).unwrap();
            let md = doc
                .fragments
                .get_single(&deckle_kfx::ion::Sym::new("$490"))
                .unwrap();
            let s = format!("{:?}", md.value);
            if s.contains("EBOK") { "EBOK" } else { "PDOC" }.into()
        }

        #[test]
        fn a_prepared_book_ships_as_ebok_with_a_thumbnail_and_xray_to_a_writable_kindle() {
            let tmp = tempfile::tempdir().unwrap();
            let q = Queue::new(tmp.path().join("outbox"));
            let entry = q.add_prepared(&prepared_mini(), LONG_READ, None).unwrap();
            assert!(entry.join("book.kpf").is_file());
            assert!(entry.join("prepared.json").is_file());
            assert_eq!(
                cde_of(&std::fs::read(entry.join("fallback.kfx")).unwrap()),
                "PDOC"
            );
            assert_eq!(q.pending(), std::slice::from_ref(&entry));

            let root = tmp.path().join("Kindle");
            let target = mass_storage_kindle(&root);
            let mut seen = Vec::new();
            let r = q
                .flush_with(
                    &mut Conn::new(&target),
                    None,
                    &uncached(tmp.path()),
                    &mut |i, n, name| seen.push(format!("{i}/{n} {name}")),
                )
                .unwrap();
            assert_eq!(seen, [format!("1/1 {LONG_READ}")]);
            let book = root.join("documents").join(LONG_READ);
            assert_eq!(r.delivered, std::slice::from_ref(&book));
            assert_eq!(cde_of(&std::fs::read(&book).unwrap()), "EBOK");
            let sdr: Vec<String> = std::fs::read_dir(book.with_extension("sdr"))
                .unwrap()
                .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
                .collect();
            assert!(
                sdr.iter().any(|n| n.starts_with("XRAY.entities.")),
                "{sdr:?}"
            );
            let thumbs: Vec<String> = std::fs::read_dir(root.join("system/thumbnails"))
                .unwrap()
                .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
                .collect();
            assert!(
                thumbs.iter().any(|n| n.ends_with("_EBOK_portrait.jpg")),
                "{thumbs:?}"
            );
            assert!(q.pending().is_empty(), "delivered entries leave the outbox");
            assert!(!entry.exists());
        }

        #[test]
        fn with_no_network_and_no_cached_lexicon_the_book_ships_with_xray_and_no_word_wise() {
            let tmp = tempfile::tempdir().unwrap();
            let q = Queue::new(tmp.path().join("outbox"));
            q.add_prepared(&prepared_mini(), LONG_READ, None).unwrap();
            let root = tmp.path().join("Kindle");
            let target = mass_storage_kindle(&root);
            let r = q
                .flush_with(
                    &mut Conn::new(&target),
                    None,
                    &uncached(tmp.path()),
                    &mut |_, _, _| {},
                )
                .unwrap();
            assert_eq!(r.delivered.len(), 1);
            let sdr = root.join("documents").join(LONG_READ).with_extension("sdr");
            let names: Vec<String> = std::fs::read_dir(&sdr)
                .unwrap()
                .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
                .collect();
            assert!(names.iter().any(|n| n.starts_with("XRAY.")), "{names:?}");
            assert!(
                !names.iter().any(|n| n.starts_with("LanguageLayer.")),
                "{names:?}"
            );
            assert!(
                r.warnings.iter().any(|w| w.starts_with(LONG_READ)
                    && w.contains("Word Wise skipped")
                    && w.contains("push doesn't download")),
                "{:?}",
                r.warnings
            );
            assert!(
                !tmp.path()
                    .join("empty-cache")
                    .join(crate::wordwise::DB_FILE)
                    .exists(),
                "nothing was downloaded"
            );
        }

        #[test]
        fn a_prepared_book_to_an_mtp_kindle_probes_sends_and_prunes_in_one_session() {
            let tmp = tempfile::tempdir().unwrap();
            let q = Queue::new(tmp.path().join("outbox"));
            q.add_prepared(&prepared_mini(), LONG_READ, None).unwrap();
            fake::reset(&["deckle-daily-2026-10-17.kfx", "deckle-daily-2026-10-18.kfx"]);
            let target = Target::Mtp(MtpInfo {
                vendor_id: deckle_transport::usb::AMAZON_VENDOR_ID,
                product_id: 1,
                serial: Some("G0001".into()),
                product: Some("Kindle".into()),
                bus: 1,
                address: 2,
            });
            // An MTP Kindle has no thumbnail folder: the owner chose features first for it.
            let mut profile = DeviceProfile::new("mtp:G0001", "Kindle");
            profile.cde_override = Some("ebok".into());
            let finishing = Finishing {
                profile: Some(profile),
                word_wise: WordWiseOptions {
                    level: 5,
                    database: Some(lexicon(tmp.path())),
                    cache_dir: tmp.path().join("cache"),
                },
            };
            let prune = Prune {
                prefix: "deckle-daily-".into(),
                keep: 1,
            };
            let r = {
                let mut conn = Conn::with_opener(&target, fake::open);
                q.flush_with(&mut conn, Some(&prune), &finishing, &mut |_, _, _| {})
                    .unwrap()
            };
            assert_eq!(
                r.delivered,
                [PathBuf::from(format!("mtp://G0001/documents/{LONG_READ}"))]
            );
            assert_eq!(r.pruned, ["deckle-daily-2026-10-17.kfx"]);
            let sdr = LONG_READ.replace(".kfx", ".sdr");
            assert!(
                fake::in_documents(&sdr)
                    .iter()
                    .any(|n| n.starts_with("XRAY.entities.")),
                "{:?}",
                fake::in_documents(&sdr)
            );
            // The Word Wise probe read this Kindle (it hides `system/`) on the same session.
            assert!(
                r.warnings
                    .iter()
                    .any(|w| w.contains("hides its Word Wise dictionaries")),
                "{:?}",
                r.warnings
            );
            let ops = fake::ops();
            assert_eq!(fake::opens(), 1, "one connection");
            assert_eq!(ops.iter().filter(|&&o| o == op::OPEN_SESSION).count(), 1);
            assert_eq!(ops.first(), Some(&op::OPEN_SESSION));
            assert_eq!(ops.last(), Some(&op::CLOSE_SESSION));
            let first_send = ops.iter().position(|&o| o == op::SEND_OBJECT_INFO).unwrap();
            let last_send = ops.iter().rposition(|&o| o == op::SEND_OBJECT).unwrap();
            let probe = ops.iter().position(|&o| o == op::GET_STORAGE_IDS).unwrap();
            let pruned = ops.iter().rposition(|&o| o == op::DELETE_OBJECT).unwrap();
            assert!(probe < first_send, "the probe comes before the sends");
            assert!(last_send < pruned, "prune last");
            assert!(q.pending().is_empty());
        }

        #[test]
        fn a_corrupt_kpf_ships_the_fallback_under_the_same_name_with_a_warning() {
            let tmp = tempfile::tempdir().unwrap();
            let q = Queue::new(tmp.path().join("outbox"));
            let entry = q.add_prepared(&prepared_mini(), LONG_READ, None).unwrap();
            let fallback = std::fs::read(entry.join("fallback.kfx")).unwrap();
            std::fs::write(entry.join("book.kpf"), b"not a KPF").unwrap();
            let root = tmp.path().join("Kindle");
            let target = mass_storage_kindle(&root);
            let r = q
                .flush_with(
                    &mut Conn::new(&target),
                    None,
                    &uncached(tmp.path()),
                    &mut |_, _, _| {},
                )
                .unwrap();
            let book = root.join("documents").join(LONG_READ);
            assert_eq!(r.delivered, std::slice::from_ref(&book));
            assert_eq!(std::fs::read(&book).unwrap(), fallback);
            assert!(
                r.warnings
                    .iter()
                    .any(|w| w.starts_with(LONG_READ) && w.contains("repackage")),
                "{:?}",
                r.warnings
            );
            assert!(q.pending().is_empty());
        }

        #[test]
        fn an_unknown_prepared_version_ships_the_fallback() {
            let tmp = tempfile::tempdir().unwrap();
            let q = Queue::new(tmp.path().join("outbox"));
            let entry = q.add_prepared(&prepared_mini(), LONG_READ, None).unwrap();
            let fallback = std::fs::read(entry.join("fallback.kfx")).unwrap();
            let json = std::fs::read_to_string(entry.join("prepared.json")).unwrap();
            let mut v: serde_json::Value = serde_json::from_str(&json).unwrap();
            v["version"] = 99.into();
            std::fs::write(entry.join("prepared.json"), v.to_string()).unwrap();
            let kindle = tmp.path().join("kindle");
            let r = q
                .flush_with(
                    &mut Conn::new(&Target::Folder(kindle.clone())),
                    None,
                    &uncached(tmp.path()),
                    &mut |_, _, _| {},
                )
                .unwrap();
            assert_eq!(std::fs::read(kindle.join(LONG_READ)).unwrap(), fallback);
            assert!(
                r.warnings.iter().any(|w| w.contains("version 99")),
                "{:?}",
                r.warnings
            );
        }

        #[test]
        fn replaces_removes_prepared_entries_by_prefix_and_a_name_has_one_entry() {
            let tmp = tempfile::tempdir().unwrap();
            let q = Queue::new(tmp.path().join("outbox"));
            let p = prepared_mini();
            q.add_prepared(&p, "deckle-reads-2026-10-20-a.kfx", None)
                .unwrap();
            q.add_prepared(&p, "keep-me.kfx", None).unwrap();
            let src = tmp.path().join("plain.kfx");
            std::fs::write(&src, b"KFX").unwrap();
            // A finished file and a prepared book never share a name: the newer one wins.
            q.add(&src, "keep-me.kfx", None).unwrap();
            q.add_prepared(
                &p,
                "deckle-reads-2026-10-21-b.kfx",
                Some("deckle-reads-2026-10-20"),
            )
            .unwrap();
            let names: Vec<String> = q
                .pending()
                .iter()
                .map(|p| p.file_name().unwrap().to_string_lossy().into_owned())
                .collect();
            assert_eq!(names, ["deckle-reads-2026-10-21-b.book", "keep-me.kfx"]);
            let opened = Cell::new(0);
            let r = q
                .flush_with(
                    &mut Conn::new(&Target::Folder(tmp.path().join("k"))),
                    None,
                    &uncached(tmp.path()),
                    &mut |_, _, _| opened.set(opened.get() + 1),
                )
                .unwrap();
            assert_eq!(opened.get(), 2);
            assert_eq!(r.delivered.len(), 2);
        }
    }
}
