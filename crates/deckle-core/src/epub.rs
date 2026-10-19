// SPDX-License-Identifier: GPL-3.0-or-later
//! Minimal EPUB container access shared by the pipeline stages.

use std::io::Read;
use std::path::Path;

use crate::inspect::InspectError;

/// An EPUB opened for reading: zip archive plus the OPF location.
pub struct EpubReader {
    pub zip: zip::ZipArchive<std::fs::File>,
    pub opf_path: String,
    pub opf_dir: String,
}

impl EpubReader {
    pub fn open(path: &Path) -> Result<Self, InspectError> {
        let file = std::fs::File::open(path).map_err(InspectError::Io)?;
        let mut zip = zip::ZipArchive::new(file).map_err(|_| InspectError::NotZip)?;
        if let Ok(mut f) = zip.by_name("META-INF/encryption.xml") {
            let mut xml = String::new();
            let _ = f.read_to_string(&mut xml);
            if encryption_is_drm(&xml) {
                return Err(InspectError::Drm);
            }
        }
        let container = {
            let mut f = zip
                .by_name("META-INF/container.xml")
                .map_err(|_| InspectError::NoContainerXml)?;
            let mut s = String::new();
            f.read_to_string(&mut s).map_err(InspectError::Io)?;
            s
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
        })
    }

    pub fn read_string(&mut self, name: &str) -> Result<String, InspectError> {
        let mut f = self
            .zip
            .by_name(name)
            .map_err(|_| InspectError::MissingFile(name.to_owned()))?;
        let mut s = String::new();
        f.read_to_string(&mut s).map_err(InspectError::Io)?;
        Ok(s)
    }

    pub fn read_bytes(&mut self, name: &str) -> Result<Vec<u8>, InspectError> {
        let mut f = self
            .zip
            .by_name(name)
            .map_err(|_| InspectError::MissingFile(name.to_owned()))?;
        let mut v = Vec::new();
        f.read_to_end(&mut v).map_err(InspectError::Io)?;
        Ok(v)
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
