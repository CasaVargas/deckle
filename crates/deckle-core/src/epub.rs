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
        if zip.by_name("META-INF/encryption.xml").is_ok() {
            return Err(InspectError::Drm);
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
