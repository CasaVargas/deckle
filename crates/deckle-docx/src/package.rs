// SPDX-License-Identifier: GPL-3.0-or-later
//! The DOCX zip: parts, relationships and the core properties.

use std::collections::HashMap;
use std::io::{Cursor, Read};

use crate::DocxError;

const OLE_MAGIC: [u8; 8] = [0xD0, 0xCF, 0x11, 0xE0, 0xA1, 0xB1, 0x1A, 0xE1];

pub struct Package<'a> {
    zip: zip::ZipArchive<Cursor<&'a [u8]>>,
}

#[derive(Clone, Debug)]
pub struct Rel {
    /// The last segment of the relationship type, e.g. `image`, `footnotes`, `hyperlink`.
    pub kind: String,
    /// A zip path for internal targets, the URL as written for external ones.
    pub target: String,
    pub external: bool,
}

#[derive(Debug, Default)]
pub struct Core {
    pub title: Option<String>,
    pub creator: Option<String>,
    pub language: Option<String>,
    pub modified: Option<String>,
}

impl<'a> Package<'a> {
    pub fn open(bytes: &'a [u8]) -> Result<Self, DocxError> {
        // Word wraps a password-protected .docx in an OLE compound file, the same container a
        // Word 97-2003 .doc uses; the encrypted one has an `EncryptedPackage` stream.
        if bytes.starts_with(&OLE_MAGIC) {
            let marker: Vec<u8> = "EncryptedPackage"
                .encode_utf16()
                .flat_map(u16::to_le_bytes)
                .collect();
            return Err(if bytes.windows(marker.len()).any(|w| w == marker) {
                DocxError::Encrypted
            } else {
                DocxError::LegacyDoc
            });
        }
        if !bytes.starts_with(b"PK") {
            return Err(DocxError::NotZip);
        }
        let zip = zip::ZipArchive::new(Cursor::new(bytes))
            .map_err(|e| DocxError::Corrupt(e.to_string()))?;
        Ok(Self { zip })
    }

    pub fn names(&self) -> Vec<String> {
        self.zip
            .file_names()
            .filter_map(Result::ok)
            .map(|n| n.into_owned())
            .collect()
    }

    pub fn read(&mut self, name: &str) -> Result<Option<Vec<u8>>, DocxError> {
        let mut f = match self.zip.by_name(name) {
            Ok(f) => f,
            Err(zip::result::ZipError::FileNotFound) => return Ok(None),
            Err(e) => return Err(DocxError::Corrupt(format!("{name}: {e}"))),
        };
        let mut buf = Vec::new();
        f.read_to_end(&mut buf)
            .map_err(|e| DocxError::Corrupt(format!("{name}: {e}")))?;
        Ok(Some(buf))
    }

    pub fn read_string(&mut self, name: &str) -> Result<Option<String>, DocxError> {
        Ok(match self.read(name)? {
            Some(b) => Some(
                String::from_utf8(b)
                    .map_err(|_| DocxError::Corrupt(format!("{name} is not UTF-8")))?,
            ),
            None => None,
        })
    }

    /// The relationships of `part` (`""` for the package), keyed by id.
    pub fn rels(&mut self, part: &str) -> Result<HashMap<String, Rel>, DocxError> {
        let (dir, file) = match part.rsplit_once('/') {
            Some((d, f)) => (d, f),
            None => ("", part),
        };
        let rels_path = if dir.is_empty() {
            format!("_rels/{file}.rels")
        } else {
            format!("{dir}/_rels/{file}.rels")
        };
        let Some(xml) = self.read_string(&rels_path)? else {
            return Ok(HashMap::new());
        };
        let doc = parse(&xml, &rels_path)?;
        let mut out = HashMap::new();
        for r in doc
            .root_element()
            .children()
            .filter(|n| n.has_tag_name("Relationship"))
        {
            let (Some(id), Some(target)) = (attr(r, "Id"), attr(r, "Target")) else {
                continue;
            };
            let external = attr(r, "TargetMode") == Some("External");
            let kind = attr(r, "Type")
                .and_then(|t| t.rsplit('/').next())
                .unwrap_or("")
                .to_owned();
            let target = if external {
                target.to_owned()
            } else {
                resolve(dir, target)
            };
            out.insert(
                id.to_owned(),
                Rel {
                    kind,
                    target,
                    external,
                },
            );
        }
        Ok(out)
    }

    pub fn core(&mut self, root_rels: &HashMap<String, Rel>) -> Result<Core, DocxError> {
        let path = root_rels
            .values()
            .find(|r| r.kind == "core-properties")
            .map(|r| r.target.clone())
            .unwrap_or_else(|| "docProps/core.xml".into());
        let Some(xml) = self.read_string(&path)? else {
            return Ok(Core::default());
        };
        let doc = parse(&xml, &path)?;
        let get = |name: &str| {
            doc.root_element()
                .children()
                .find(|n| n.tag_name().name() == name)
                .and_then(|n| n.text())
                .map(str::trim)
                .filter(|t| !t.is_empty())
                .map(str::to_owned)
        };
        Ok(Core {
            title: get("title"),
            creator: get("creator"),
            language: get("language"),
            modified: get("modified"),
        })
    }
}

pub fn parse<'i>(xml: &'i str, name: &str) -> Result<roxmltree::Document<'i>, DocxError> {
    roxmltree::Document::parse(xml).map_err(|e| DocxError::Corrupt(format!("{name}: {e}")))
}

/// An attribute by local name, whatever its namespace (`w:val`, `r:id`, plain `Id`).
pub fn attr<'a>(n: roxmltree::Node<'a, '_>, name: &str) -> Option<&'a str> {
    n.attributes().find(|a| a.name() == name).map(|a| a.value())
}

/// A target relative to `dir`, as a zip path (`..` resolved, a leading `/` is the package root).
fn resolve(dir: &str, target: &str) -> String {
    let mut parts: Vec<&str> = if target.starts_with('/') || dir.is_empty() {
        Vec::new()
    } else {
        dir.split('/').collect()
    };
    for seg in target.split('/') {
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

#[cfg(test)]
mod tests {
    use super::resolve;

    #[test]
    fn targets_resolve_against_the_part_folder() {
        assert_eq!(resolve("word", "media/image1.png"), "word/media/image1.png");
        assert_eq!(
            resolve("word", "../customXml/item1.xml"),
            "customXml/item1.xml"
        );
        assert_eq!(resolve("word", "/word/styles.xml"), "word/styles.xml");
        assert_eq!(resolve("", "word/document.xml"), "word/document.xml");
    }
}
