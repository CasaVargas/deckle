// SPDX-License-Identifier: GPL-3.0-or-later
//! The few facts kfxlib reads back from the source EPUB inside a KPF (`original_source_epub.py`).

use std::io::Read;

#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct SourceEpub {
    /// `dc:creator` texts in display order (kfxlib ignores `display-seq` ties beyond order).
    pub authors: Vec<String>,
    /// First `YYYY-MM-DD` of `dc:date`, unless it is `0101-01-01`.
    pub issue_date: Option<String>,
    /// `<meta name="original-language" content="…">`.
    pub original_language: Option<String>,
}

impl SourceEpub {
    /// Parses the OPF of an EPUB held in memory. Any failure yields the default (empty) facts.
    pub fn parse(epub: &[u8]) -> SourceEpub {
        Self::try_parse(epub).unwrap_or_default()
    }

    fn try_parse(epub: &[u8]) -> Option<SourceEpub> {
        let mut zip = zip::ZipArchive::new(std::io::Cursor::new(epub)).ok()?;
        let mut container = String::new();
        zip.by_name("META-INF/container.xml")
            .ok()?
            .read_to_string(&mut container)
            .ok()?;
        let cdoc = roxmltree::Document::parse(&container).ok()?;
        let opf_path = cdoc
            .descendants()
            .find(|n| n.has_tag_name("rootfile"))
            .and_then(|n| n.attribute("full-path"))?
            .to_owned();
        let mut opf = String::new();
        zip.by_name(&opf_path).ok()?.read_to_string(&mut opf).ok()?;
        let doc = roxmltree::Document::parse(&opf).ok()?;
        let metadata = doc.descendants().find(|n| n.has_tag_name("metadata"))?;
        let mut out = SourceEpub::default();
        let mut seq: Vec<(i64, String)> = Vec::new();
        let mut next_seq = 0i64;
        for creator in metadata.children().filter(|n| n.has_tag_name("creator")) {
            let Some(text) = creator.text().map(str::trim).filter(|t| !t.is_empty()) else {
                continue;
            };
            if let Some(role) = creator
                .attributes()
                .find(|a| a.name() == "role")
                .map(|a| a.value())
                && role != "aut"
            {
                continue;
            }
            if seq.iter().any(|(_, t)| t == text) {
                continue;
            }
            let mut display_seq = next_seq;
            if let Some(id) = creator.attribute("id") {
                let frag = format!("#{id}");
                for meta in metadata.children().filter(|n| n.has_tag_name("meta")) {
                    if meta.attribute("property") == Some("display-seq")
                        && meta.attribute("refines") == Some(frag.as_str())
                        && let Some(n) = meta.text().and_then(|t| t.trim().parse::<i64>().ok())
                    {
                        display_seq = n;
                        break;
                    }
                }
            }
            seq.push((display_seq, text.to_owned()));
            next_seq = display_seq + 1;
        }
        seq.sort();
        out.authors = seq.into_iter().map(|(_, t)| t).collect();
        if let Some(date) = metadata
            .children()
            .find(|n| n.has_tag_name("date"))
            .and_then(|n| n.text())
        {
            let date = date.trim();
            if date.len() >= 10 {
                let d = &date[..10];
                let b = d.as_bytes();
                let is_date = b.iter().enumerate().all(|(i, c)| {
                    if i == 4 || i == 7 {
                        *c == b'-'
                    } else {
                        c.is_ascii_digit()
                    }
                });
                if is_date && d != "0101-01-01" {
                    out.issue_date = Some(d.to_owned());
                }
            }
        }
        for meta in metadata.children().filter(|n| n.has_tag_name("meta")) {
            if meta.attribute("name") == Some("original-language")
                && let Some(c) = meta.attribute("content")
            {
                out.original_language = Some(c.to_owned());
            }
        }
        Some(out)
    }
}
