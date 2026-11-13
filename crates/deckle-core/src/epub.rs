// SPDX-License-Identifier: GPL-3.0-or-later
//! Minimal EPUB container access shared by the pipeline stages.

use std::io::{Read, Seek};
use std::path::Path;

use deckle_limits::{Budget, MAX_BOOK_BYTES, MAX_ENTRY_BYTES, TooLarge, read_capped};

use crate::inspect::InspectError;

/// An EPUB opened for reading: zip archive plus the OPF location.
pub struct EpubReader {
    pub zip: zip::ZipArchive<std::fs::File>,
    pub opf_path: String,
    pub opf_dir: String,
    /// Most bytes one entry may unpack to; [`read_string`](Self::read_string) and
    /// [`read_bytes`](Self::read_bytes) stop there.
    pub(crate) entry_limit: u64,
    /// Most bytes the whole book may unpack to, checked against the declared sizes at open.
    pub(crate) book_limit: u64,
}

impl EpubReader {
    /// Opens the EPUB at `path`, refusing one whose entries declare more than
    /// [`MAX_ENTRY_BYTES`] each or [`MAX_BOOK_BYTES`] together.
    pub fn open(path: &Path) -> Result<Self, InspectError> {
        Self::open_with(path, MAX_ENTRY_BYTES, MAX_BOOK_BYTES)
    }

    pub(crate) fn open_with(
        path: &Path,
        entry_limit: u64,
        book_limit: u64,
    ) -> Result<Self, InspectError> {
        let file = std::fs::File::open(path).map_err(InspectError::Io)?;
        let mut zip = zip::ZipArchive::new(file).map_err(|_| InspectError::NotZip)?;
        // `zip` stops each entry at its declared size, so these bound every later read.
        check_declared_sizes(&zip, entry_limit, book_limit)
            .map_err(|e| too_large(e, entry_limit))?;
        if let Ok(mut f) = zip.by_name("META-INF/encryption.xml") {
            let xml = read_capped(&mut f, entry_limit).unwrap_or_default();
            if encryption_is_drm(&String::from_utf8_lossy(&xml)) {
                return Err(InspectError::Drm);
            }
        }
        let container = {
            let mut f = zip
                .by_name("META-INF/container.xml")
                .map_err(|_| InspectError::NoContainerXml)?;
            read_text(&mut f, entry_limit)?
        };
        let doc =
            roxmltree::Document::parse(&container).map_err(|e| InspectError::Xml(e.to_string()))?;
        let opf_path = doc
            .descendants()
            .find(|n| n.has_tag_name("rootfile"))
            .and_then(|n| n.attribute("full-path"))
            .ok_or(InspectError::NoOpf)?
            .to_owned();
        if zip.by_name(&opf_path).is_err() {
            return Err(InspectError::NoOpf);
        }
        let opf_dir = match opf_path.rfind('/') {
            Some(i) => opf_path[..i + 1].to_owned(),
            None => String::new(),
        };
        Ok(EpubReader {
            zip,
            opf_path,
            opf_dir,
            entry_limit,
            book_limit,
        })
    }

    pub fn read_string(&mut self, name: &str) -> Result<String, InspectError> {
        let mut f = self
            .zip
            .by_name(name)
            .map_err(|_| InspectError::MissingFile(name.to_owned()))?;
        read_text(&mut f, self.entry_limit)
    }

    pub fn read_bytes(&mut self, name: &str) -> Result<Vec<u8>, InspectError> {
        let mut f = self
            .zip
            .by_name(name)
            .map_err(|_| InspectError::MissingFile(name.to_owned()))?;
        read_entry(&mut f, self.entry_limit)
    }

    /// Resolves an href relative to the OPF directory, dropping any fragment.
    pub fn resolve(&self, href: &str) -> String {
        let href = href.split('#').next().unwrap_or("");
        let href = percent_decode(href);
        join(&self.opf_dir, &href)
    }

    pub fn names(&self) -> Vec<String> {
        self.zip
            .file_names()
            .filter_map(|r| r.ok().map(|c| c.into_owned()))
            .collect()
    }
}

/// Fails when an entry of `zip` declares more than `entry_limit` bytes unpacked, or all of them
/// together more than `total_limit`: the cheap check before anything is read. `zip` stops an
/// entry at its declared size, so a zip that passes cannot unpack to more.
pub fn check_declared_sizes<R: Read + Seek>(
    zip: &zip::ZipArchive<R>,
    entry_limit: u64,
    total_limit: u64,
) -> Result<(), TooLarge> {
    let mut total = Budget::new(total_limit);
    for i in 0..zip.len() {
        let Ok(entry) = zip.by_index_data(i) else {
            continue;
        };
        if entry.size() > entry_limit {
            return Err(TooLarge { limit: entry_limit });
        }
        total.add(entry.size())?;
    }
    Ok(())
}

/// Reads one opened zip entry, at most `limit` bytes.
pub fn read_entry(f: impl Read, limit: u64) -> Result<Vec<u8>, InspectError> {
    read_capped(f, limit).map_err(|e| io_error(e, limit))
}

fn read_text(f: impl Read, limit: u64) -> Result<String, InspectError> {
    String::from_utf8(read_entry(f, limit)?).map_err(|_| {
        InspectError::Io(std::io::Error::new(
            std::io::ErrorKind::InvalidData,
            "stream did not contain valid UTF-8",
        ))
    })
}

/// The error for `e`, from a read capped at `entry_limit` bytes.
pub(crate) fn io_error(e: std::io::Error, entry_limit: u64) -> InspectError {
    match TooLarge::in_io(&e) {
        Some(t) => too_large(t, entry_limit),
        None => InspectError::Io(e),
    }
}

/// [`InspectError::EntryTooLarge`] when `e` is the per-entry limit, else
/// [`InspectError::TooLarge`].
pub(crate) fn too_large(e: TooLarge, entry_limit: u64) -> InspectError {
    if e.limit == entry_limit {
        InspectError::EntryTooLarge(e)
    } else {
        InspectError::TooLarge(e)
    }
}

pub fn join(dir: &str, rel: &str) -> String {
    let mut parts: Vec<&str> = dir
        .trim_end_matches('/')
        .split('/')
        .filter(|p| !p.is_empty())
        .collect();
    for seg in rel.split('/') {
        match seg {
            "" | "." => {}
            ".." => {
                parts.pop();
            }
            s => parts.push(s),
        }
    }
    parts.join("/")
}

pub fn percent_decode(s: &str) -> String {
    let bytes = s.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%'
            && i + 2 < bytes.len()
            && let Ok(v) = u8::from_str_radix(
                std::str::from_utf8(&bytes[i + 1..i + 3]).unwrap_or("zz"),
                16,
            )
        {
            out.push(v);
            i += 3;
            continue;
        }
        out.push(bytes[i]);
        i += 1;
    }
    String::from_utf8(out).unwrap_or_else(|_| s.to_owned())
}

const FONT_OBFUSCATION_ALGORITHMS: [&str; 2] = [
    "http://www.idpf.org/2008/embedding",
    "http://ns.adobe.com/pdf/enc#RC",
];
const FONT_EXTENSIONS: [&str; 6] = [".ttf", ".otf", ".woff", ".woff2", ".eot", ".dfont"];

/// True when `META-INF/encryption.xml` encrypts anything beyond obfuscated fonts (IDPF/Adobe
/// font mangling is routine in DRM-free books). Unparseable files count as DRM.
pub fn encryption_is_drm(xml: &str) -> bool {
    let Ok(doc) = roxmltree::Document::parse(xml) else {
        return true;
    };
    let mut saw_any = false;
    for data in doc
        .descendants()
        .filter(|n| n.has_tag_name("EncryptedData"))
    {
        saw_any = true;
        let algorithm = data
            .descendants()
            .find(|n| n.has_tag_name("EncryptionMethod"))
            .and_then(|n| n.attribute("Algorithm"))
            .unwrap_or("");
        let uri = data
            .descendants()
            .find(|n| n.has_tag_name("CipherReference"))
            .and_then(|n| n.attribute("URI"))
            .unwrap_or("")
            .to_lowercase();
        let font_algorithm = FONT_OBFUSCATION_ALGORITHMS.contains(&algorithm);
        let font_file = FONT_EXTENSIONS.iter().any(|e| uri.ends_with(e));
        if !(font_algorithm && font_file) {
            return true;
        }
    }
    // An encryption.xml with no EncryptedData entries encrypts nothing.
    let _ = saw_any;
    false
}

#[cfg(test)]
mod tests {
    use super::*;
    use deckle_kfx::fixtures::stored_epub as epub_with;

    fn file(bytes: &[u8]) -> tempfile::NamedTempFile {
        let f = tempfile::NamedTempFile::new().unwrap();
        std::fs::write(f.path(), bytes).unwrap();
        f
    }

    #[test]
    fn an_entry_declaring_2_pow_60_bytes_is_refused_at_open() {
        let f = file(&epub_with(&[("big.bin", b"tiny", 1 << 60)]));
        let e = EpubReader::open(f.path()).err().unwrap();
        assert!(
            matches!(e, InspectError::EntryTooLarge(t) if t.limit == MAX_ENTRY_BYTES),
            "{e}"
        );
        assert_eq!(
            e.to_string(),
            "a file in this book unpacks to more than 256 MB"
        );
    }

    #[test]
    fn declared_sizes_past_the_book_limit_are_refused_at_open() {
        let f = file(&epub_with(&[("a.bin", b"a", 900), ("b.bin", b"b", 900)]));
        let e = EpubReader::open_with(f.path(), 1000, 1500).err().unwrap();
        assert!(
            matches!(e, InspectError::TooLarge(t) if t.limit == 1500),
            "{e}"
        );
        assert!(EpubReader::open_with(f.path(), 1000, 10_000).is_ok());
    }

    #[test]
    fn an_entry_longer_than_it_declares_is_not_read_whole() {
        // Declares 10 bytes and holds 5000: `zip` stops it at 10.
        let big = vec![b'x'; 5000];
        let f = file(&epub_with(&[("big.bin", &big, 10)]));
        let mut r = EpubReader::open_with(f.path(), 1000, 10_000).unwrap();
        let e = r.read_bytes("big.bin").unwrap_err();
        assert!(e.to_string().contains("declared"), "{e}");
    }

    #[test]
    fn a_read_stops_at_the_entry_limit() {
        let big = vec![b'x'; 5000];
        let f = file(&epub_with(&[("big.bin", &big, 5000)]));
        let mut r = EpubReader::open_with(f.path(), 10_000, 100_000).unwrap();
        r.entry_limit = 100;
        let e = r.read_bytes("big.bin").unwrap_err();
        assert!(
            matches!(e, InspectError::EntryTooLarge(t) if t.limit == 100),
            "{e}"
        );
    }

    #[test]
    fn a_book_inside_the_limits_reads_as_before() {
        let f = file(&epub_with(&[]));
        let mut r = EpubReader::open(f.path()).unwrap();
        assert_eq!(r.opf_path, "content.opf");
        assert!(r.read_string("c.xhtml").unwrap().contains("<p>Hi.</p>"));
        assert_eq!(r.read_bytes("mimetype").unwrap(), b"application/epub+zip");
    }
}
