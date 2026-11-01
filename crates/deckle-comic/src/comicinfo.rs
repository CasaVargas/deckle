// SPDX-License-Identifier: GPL-3.0-or-later
//! `ComicInfo.xml`, the metadata file comic tools put in a CBZ (the Anansi Project schema). Only
//! what the EPUB uses is read; a file that won't parse is the same as none.

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ComicInfo {
    pub title: Option<String>,
    pub series: Option<String>,
    pub number: Option<String>,
    pub writer: Option<String>,
    pub language: Option<String>,
    pub summary: Option<String>,
    /// `<Manga>YesAndRightToLeft</Manga>`. A plain `Yes` says nothing about direction.
    pub right_to_left: bool,
}

impl ComicInfo {
    pub fn parse(bytes: &[u8]) -> Self {
        let bytes = bytes.strip_prefix(&[0xEF, 0xBB, 0xBF]).unwrap_or(bytes);
        let Ok(text) = std::str::from_utf8(bytes) else {
            return Self::default();
        };
        let Ok(doc) = roxmltree::Document::parse(text) else {
            return Self::default();
        };
        let field = |name: &str| {
            doc.root_element()
                .children()
                .find(|n| n.has_tag_name(name))
                .and_then(|n| n.text())
                .map(str::trim)
                .filter(|s| !s.is_empty())
                .map(str::to_owned)
        };
        ComicInfo {
            title: field("Title"),
            series: field("Series"),
            number: field("Number"),
            writer: field("Writer"),
            language: field("LanguageISO"),
            summary: field("Summary"),
            right_to_left: field("Manga").is_some_and(|m| m == "YesAndRightToLeft"),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::ComicInfo;

    #[test]
    fn reads_the_fields_the_epub_uses() {
        let info = ComicInfo::parse(
            "\u{feff}<?xml version=\"1.0\"?><ComicInfo xmlns:xsi=\"http://www.w3.org/2001/XMLSchema-instance\">\
             <Title> Tetsuo </Title><Series>Akira</Series><Number>3</Number>\
             <Writer>Katsuhiro Otomo</Writer><LanguageISO>ja</LanguageISO>\
             <Summary>Neo-Tokyo.</Summary><Manga>YesAndRightToLeft</Manga></ComicInfo>"
                .as_bytes(),
        );
        assert_eq!(info.title.as_deref(), Some("Tetsuo"));
        assert_eq!(info.series.as_deref(), Some("Akira"));
        assert_eq!(info.number.as_deref(), Some("3"));
        assert_eq!(info.writer.as_deref(), Some("Katsuhiro Otomo"));
        assert_eq!(info.language.as_deref(), Some("ja"));
        assert_eq!(info.summary.as_deref(), Some("Neo-Tokyo."));
        assert!(info.right_to_left);
    }

    #[test]
    fn manga_yes_alone_keeps_left_to_right_and_junk_is_empty() {
        assert!(!ComicInfo::parse(b"<ComicInfo><Manga>Yes</Manga></ComicInfo>").right_to_left);
        assert_eq!(ComicInfo::parse(b"not xml"), ComicInfo::default());
    }
}
