// SPDX-License-Identifier: GPL-3.0-or-later
//! Stage 5 (enrich): the Word Wise sidecar for an EBOK book (design §4.5, M4 plan).
//!
//! Deckle matches the book's text against its own Wiktionary-derived database, then points the
//! hints at a dictionary the Kindle can read: its own English dictionary when it has one
//! (`kll.en.en.klld`, read at send time and never kept), else Deckle's `.klld` installed in the
//! Chinese-gloss slot, which is only written when empty or already Deckle's.

use std::io::Read;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use sha2::Digest;

use crate::deliver::{Conn, Target};
use deckle_kfx::text::Run;
use deckle_wordwise::device::DeviceDictionary;
use deckle_wordwise::kll::{DictionaryRef, LanguageLayer, sidecar_name, write_kll};
use deckle_wordwise::klld;
use deckle_wordwise::lexicon::Lexicon;

/// Deckle's database release asset (built by `data/wordwise`).
pub const DB_FILE: &str = "deckle-wordwise-en-v1.sqlite";
pub const DB_URL: &str = "https://github.com/prjoni99/deckle/releases/download/wordwise-v1/deckle-wordwise-en-v1.sqlite.gz";
/// SHA-256 of the `.gz` at [`DB_URL`].
pub const DB_SHA256: &str = "2fdc729af1a2411dd4706eda183cc7475e908816013a1d4ec1cb10b2bedc7c34";
/// Points at a local database instead of the cached download.
pub const DB_ENV: &str = "DECKLE_WORDWISE_DB";

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct WordWiseOptions {
    /// Highest difficulty written (1 = rarest words only … 5 = every hint; the Kindle's own
    /// slider filters further).
    pub level: u8,
    /// A local database; `None` uses [`DB_ENV`], else the cached download.
    pub database: Option<PathBuf>,
    /// Where the download and Deckle's derived `.klld` are cached.
    pub cache_dir: PathBuf,
}

impl Default for WordWiseOptions {
    fn default() -> Self {
        WordWiseOptions {
            level: 5,
            database: None,
            cache_dir: default_cache_dir(),
        }
    }
}

/// `~/Library/Caches/deckle`, `%LOCALAPPDATA%\deckle`, or `$XDG_CACHE_HOME/deckle` (`~/.cache`).
pub fn default_cache_dir() -> PathBuf {
    let home = || std::env::var_os("HOME").map(PathBuf::from);
    let base = if cfg!(target_os = "macos") {
        home().map(|h| h.join("Library/Caches"))
    } else if cfg!(target_os = "windows") {
        std::env::var_os("LOCALAPPDATA").map(PathBuf::from)
    } else {
        std::env::var_os("XDG_CACHE_HOME")
            .map(PathBuf::from)
            .or_else(|| home().map(|h| h.join(".cache")))
    };
    base.unwrap_or_else(std::env::temp_dir).join("deckle")
}

pub type FetchBytes<'a> = dyn FnMut(&str) -> Result<Vec<u8>, String> + 'a;

/// Real HTTP GET of a binary body.
pub fn http_get_bytes(url: &str) -> Result<Vec<u8>, String> {
    let agent = ureq::Agent::config_builder()
        .timeout_global(Some(std::time::Duration::from_secs(300)))
        .user_agent("Deckle/0.1 (+https://github.com/prjoni99/deckle)")
        // The OS certificate store, so TLS-inspecting proxies (Zscaler, …) the machine trusts work.
        .tls_config(
            ureq::tls::TlsConfig::builder()
                .root_certs(ureq::tls::RootCerts::PlatformVerifier)
                .build(),
        )
        .build()
        .new_agent();
    let mut resp = agent.get(url).call().map_err(|e| e.to_string())?;
    resp.body_mut()
        .with_config()
        .limit(200 * 1024 * 1024)
        .read_to_vec()
        .map_err(|e| e.to_string())
}

/// The database to use and whether this call downloaded it.
pub fn ensure_database(
    opts: &WordWiseOptions,
    fetch: &mut FetchBytes<'_>,
) -> Result<(PathBuf, bool), String> {
    ensure_database_pinned(opts, fetch, DB_SHA256)
}

/// The version of the database a send would use now, without downloading one: `None` when
/// there is none yet.
pub fn current_database_version(opts: &WordWiseOptions) -> Option<String> {
    let path = opts
        .database
        .clone()
        .or_else(|| {
            std::env::var_os(DB_ENV)
                .filter(|p| !p.is_empty())
                .map(PathBuf::from)
        })
        .unwrap_or_else(|| opts.cache_dir.join(DB_FILE));
    path.is_file()
        .then(|| Lexicon::open(&path).ok().map(|l| l.version))
        .flatten()
}

/// [`ensure_database`] against another pinned digest (tests).
pub fn ensure_database_pinned(
    opts: &WordWiseOptions,
    fetch: &mut FetchBytes<'_>,
    sha256: &str,
) -> Result<(PathBuf, bool), String> {
    if let Some(p) = &opts.database {
        return Ok((p.clone(), false));
    }
    if let Some(p) = std::env::var_os(DB_ENV).filter(|p| !p.is_empty()) {
        return Ok((PathBuf::from(p), false));
    }
    let cached = opts.cache_dir.join(DB_FILE);
    if cached.is_file() {
        return Ok((cached, false));
    }
    let gz =
        fetch(DB_URL).map_err(|e| format!("could not download the Word Wise database: {e}"))?;
    let digest = hex(&sha2::Sha256::digest(&gz));
    if digest != sha256 {
        return Err(format!(
            "the downloaded Word Wise database is corrupt (sha256 {digest})"
        ));
    }
    let mut raw = Vec::new();
    flate2::read::GzDecoder::new(gz.as_slice())
        .read_to_end(&mut raw)
        .map_err(|e| format!("Word Wise database: {e}"))?;
    std::fs::create_dir_all(&opts.cache_dir).map_err(|e| e.to_string())?;
    let tmp = opts.cache_dir.join(format!("{DB_FILE}.part"));
    std::fs::write(&tmp, &raw).map_err(|e| e.to_string())?;
    std::fs::rename(&tmp, &cached).map_err(|e| e.to_string())?;
    Ok((cached, true))
}

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

/// Deckle's `.klld` for a database, built once per database version into the cache.
pub fn deckle_klld(db: &Path, cache_dir: &Path) -> Result<PathBuf, String> {
    let version = Lexicon::open(db).map_err(|e| e.to_string())?.version;
    let path = cache_dir.join(format!("kll.en.zh-{}.klld", sanitize(&version)));
    // the marker inside the .klld is the same `version+build` string the device slot is
    // compared against
    if klld::deckle_version(&path).as_deref() == Some(version.as_str()) {
        return Ok(path);
    }
    std::fs::create_dir_all(cache_dir).map_err(|e| e.to_string())?;
    let tmp = cache_dir.join("kll.en.zh.klld.part");
    klld::write_from_db(db, &tmp).map_err(|e| e.to_string())?;
    std::fs::rename(&tmp, &path).map_err(|e| e.to_string())?;
    Ok(path)
}

fn sanitize(s: &str) -> String {
    s.chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || c == '-' {
                c
            } else {
                '_'
            }
        })
        .collect()
}

/// What sits in the Kindle's `kll.en.zh.klld` slot.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Slot {
    Empty,
    /// Deckle's, at this database version.
    Deckle(String),
    /// Amazon's Chinese dictionary (or anyone else's): never replaced.
    Other,
    /// Out of reach: the Kindle hides `system/` over MTP (Scribe 2024).
    Hidden,
}

/// Where the reader stages a copy of the Kindle's `kll.en.en.klld` when MTP hides `system/`.
pub const STAGED_KINDLE_EN_PATH: [&str; 3] = ["documents", "deckle", "kll.en.en.klld"];

/// The dictionaries already on the target.
#[derive(Clone, Debug)]
pub struct DeviceDicts {
    /// Bytes of the Kindle's `kll.en.en.klld`, if it has one.
    pub kindle_en: Option<Vec<u8>>,
    pub zh_slot: Slot,
}

fn slot_of(bytes: Option<&[u8]>) -> Slot {
    let Some(bytes) = bytes else {
        return Slot::Empty;
    };
    let Ok(dir) = tempfile::tempdir() else {
        return Slot::Other;
    };
    let p = dir.path().join("slot.klld");
    if std::fs::write(&p, bytes).is_err() {
        return Slot::Other;
    }
    match klld::deckle_version(&p) {
        Some(v) => Slot::Deckle(v),
        None => Slot::Other,
    }
}

fn read_opt(path: &Path) -> Option<Vec<u8>> {
    std::fs::read(path).ok()
}

fn join(root: &Path, parts: &[&str]) -> PathBuf {
    parts.iter().fold(root.to_path_buf(), |p, s| p.join(s))
}

/// Reads the dictionaries on a folder export, a mass-storage Kindle, or an MTP Kindle.
pub fn probe(target: &Target) -> Result<DeviceDicts, String> {
    probe_on(&mut Conn::new(target))
}

/// [`probe`] over a connection the delivery will reuse.
pub fn probe_on(conn: &mut Conn<'_>) -> Result<DeviceDicts, String> {
    match conn.target {
        Target::Folder(dir) => Ok(DeviceDicts {
            kindle_en: None,
            zh_slot: slot_of(read_opt(&join(dir, &klld::DEVICE_PATH)).as_deref()),
        }),
        Target::MassStorage(dev) => Ok(DeviceDicts {
            kindle_en: read_opt(&join(&dev.root, &klld::KINDLE_EN_PATH)),
            zh_slot: slot_of(read_opt(&join(&dev.root, &klld::DEVICE_PATH)).as_deref()),
        }),
        Target::Mtp(_) => {
            let m = |e: &dyn std::fmt::Display| format!("MTP: {e}");
            let dev = match conn.mtp() {
                Some(r) => r.map_err(|e| m(&e))?,
                None => unreachable!("an MTP target always has a connection"),
            };
            let storage = *dev
                .storages()
                .map_err(|e| m(&e))?
                .first()
                .ok_or("MTP: device reports no storage")?;
            let root = dev.content_root(storage).map_err(|e| m(&e))?;
            // The Scribe 2024 hides `system/` over MTP: read the reader's staged copy instead.
            let hidden = dev
                .find_child(storage, root, "system")
                .map_err(|e| m(&e))?
                .is_none();
            let mut get = |parts: &[&str]| -> Result<Option<Vec<u8>>, String> {
                match dev.find_path(storage, root, parts).map_err(|e| m(&e))? {
                    Some(h) => dev.get_object(h).map(Some).map_err(|e| m(&e)),
                    None => Ok(None),
                }
            };
            if hidden {
                return Ok(DeviceDicts {
                    kindle_en: get(&STAGED_KINDLE_EN_PATH)?,
                    zh_slot: Slot::Hidden,
                });
            }
            let kindle_en = get(&klld::KINDLE_EN_PATH)?;
            let zh = get(&klld::DEVICE_PATH)?;
            Ok(DeviceDicts {
                kindle_en,
                zh_slot: slot_of(zh.as_deref()),
            })
        }
    }
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct WordWiseSummary {
    pub hints: usize,
    pub level: u8,
    /// `device` (the Kindle's English dictionary) | `deckle` (Deckle's, in the 中文 slot)
    pub dictionary: String,
    /// The database was downloaded during this run.
    pub downloaded: bool,
    /// The database's `version+build`, stamped on the send (Kindle-updates design §3).
    pub database: String,
}

#[derive(Debug)]
pub struct WordWiseOutput {
    pub file_name: String,
    pub bytes: Vec<u8>,
    pub summary: WordWiseSummary,
    /// Deckle's `.klld` to install at `system/kll/kll.en.zh.klld`, when the slot needs it.
    pub dictionary_file: Option<(Vec<String>, Vec<u8>)>,
    pub warnings: Vec<String>,
}

/// Builds the sidecar (and, in Deckle-dictionary mode, the dictionary to install). `Ok(None)`
/// when the book has no hint-worthy words at this level.
pub fn build(
    asin: &str,
    acr: &str,
    runs: &[Run],
    db: &Path,
    downloaded: bool,
    opts: &WordWiseOptions,
    dicts: &DeviceDicts,
) -> Result<Option<WordWiseOutput>, String> {
    let lexicon = Lexicon::open(db).map_err(|e| e.to_string())?;
    let level = opts.level.clamp(1, 5);
    let found = deckle_wordwise::matcher::glosses(runs, &lexicon, level as i64);
    let mut warnings = Vec::new();
    let tmp = tempfile::tempdir().map_err(|e| e.to_string())?;

    let device = dicts.kindle_en.as_ref().and_then(|bytes| {
        let p = tmp.path().join("kll.en.en.klld");
        std::fs::write(&p, bytes).ok()?;
        match DeviceDictionary::open(&p) {
            Ok(d) => Some(d),
            Err(e) => {
                warnings.push(format!(
                    "Word Wise: not using the Kindle's dictionary ({e}); using Deckle's"
                ));
                None
            }
        }
    });
    let (glosses, dictionary, dictionary_file) = match device {
        Some(d) => (d.remap(found, &lexicon), "device", None),
        None => {
            let install = match &dicts.zh_slot {
                Slot::Other => {
                    return Err(
                        "the Kindle has no English Word Wise dictionary and its Chinese one \
                         is Amazon's, which Deckle will not replace. Open Word Wise on a store \
                         book once (Wi-Fi on) so the Kindle downloads its English dictionary"
                            .into(),
                    );
                }
                Slot::Hidden => {
                    let staged = STAGED_KINDLE_EN_PATH.join("/");
                    return Err(if dicts.kindle_en.is_some() {
                        format!(
                            "the Kindle's dictionary copied to {staged} can't be read; copy it again"
                        )
                    } else {
                        format!(
                            "this Kindle hides its Word Wise dictionaries over USB. Copy \
                             system/kll/kll.en.en.klld to {staged} once (KOReader's file manager \
                             on a jailbroken Kindle)"
                        )
                    });
                }
                Slot::Deckle(v) if *v == lexicon.version => false,
                _ => true,
            };
            let file = if install {
                let path = deckle_klld(db, &opts.cache_dir)?;
                let bytes = std::fs::read(&path).map_err(|e| e.to_string())?;
                Some((
                    klld::DEVICE_PATH.iter().map(|s| s.to_string()).collect(),
                    bytes,
                ))
            } else {
                None
            };
            (found, "deckle", file)
        }
    };
    if glosses.is_empty() {
        return Ok(None);
    }
    let path = tmp.path().join(sidecar_name(asin));
    write_kll(
        &path,
        &LanguageLayer {
            acr: acr.to_owned(),
            dictionary: DictionaryRef::default(),
            glosses: glosses.clone(),
        },
    )
    .map_err(|e| e.to_string())?;
    let bytes = std::fs::read(&path).map_err(|e| e.to_string())?;
    Ok(Some(WordWiseOutput {
        file_name: sidecar_name(asin),
        bytes,
        summary: WordWiseSummary {
            hints: glosses.len(),
            level,
            dictionary: dictionary.into(),
            downloaded,
            database: lexicon.version.clone(),
        },
        dictionary_file,
        warnings,
    }))
}

/// The whole stage: database (downloading it on first use), the target's dictionaries, sidecar.
pub fn enrich(
    runs: &[Run],
    asin: &str,
    acr: &str,
    opts: &WordWiseOptions,
    conn: &mut Conn<'_>,
    fetch: &mut FetchBytes<'_>,
) -> Result<Option<WordWiseOutput>, String> {
    let (db, downloaded) = ensure_database(opts, fetch)?;
    let dicts = probe_on(conn)?;
    build(asin, acr, runs, &db, downloaded, opts, &dicts)
}
