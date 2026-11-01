// SPDX-License-Identifier: GPL-3.0-or-later
//! Import a text PDF as a clean EPUB 3, so every later stage (inspect, normalize, Previewer,
//! repackage, deliver) runs on it unchanged (docs/superpowers/specs/2026-10-23-pdf-reflow-design.md).
//!
//! PDFium reads each page's characters with their positions and fonts; [`layout`] puts them back
//! into reading order (columns, running heads and page numbers out, broken words rejoined) and
//! [`epub`] writes the book. Tables, displayed equations, images and drawings are cut from the
//! rendered page as pictures, since reflowed table text is unreadable. A reflowed PDF is never
//! as good as a book made for reading, and the report says so.

pub mod engine;
mod epub;
pub mod layout;

use std::path::Path;

pub use engine::ENV;

/// Larger than any real book a person would read on a Kindle.
const MAX_BYTES: u64 = 200 * 1024 * 1024;

/// Said with every PDF import: the reader should know what to check.
pub const QUALITY_NOTE: &str = "reflowed from a PDF: the text is rebuilt from the page layout, \
    so check headings, footnotes and page breaks; tables, equations and figures are pictures";

#[derive(Debug, thiserror::Error)]
pub enum PdfError {
    #[error(
        "reading a PDF needs the PDFium library, and it isn't installed. Get it for your system \
         from https://github.com/bblanchon/pdfium-binaries/releases and set {ENV} to the file \
         (looked in: {0})"
    )]
    NoEngine(String),
    #[error("the PDF is password-protected")]
    Locked,
    #[error("can't read the PDF: {0}")]
    Unreadable(String),
    #[error(
        "this PDF has no text in it, only pictures of pages (a scan); reading scans needs OCR, \
         which Deckle doesn't do yet"
    )]
    Scanned,
    #[error("the PDF is {} MB; the limit is 200 MB", .0 / (1024 * 1024))]
    TooLarge(u64),
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ImportReport {
    pub title: String,
    pub pages: usize,
    /// XHTML files in the spine.
    pub chapters: usize,
    /// Entries in the contents.
    pub contents: usize,
    pub stats: layout::Stats,
    pub warnings: Vec<String>,
}

/// Inputs the importer takes: `.pdf`, any case.
pub fn is_pdf(path: &Path) -> bool {
    path.extension()
        .and_then(|e| e.to_str())
        .is_some_and(|e| e.eq_ignore_ascii_case("pdf"))
}

/// Converts the PDF at `pdf` and writes the EPUB to `epub`.
pub fn convert(pdf: &Path, epub: &Path) -> Result<ImportReport, PdfError> {
    let size = std::fs::metadata(pdf)?.len();
    if size > MAX_BYTES {
        return Err(PdfError::TooLarge(size));
    }
    let pdfium = engine::pdfium()?;
    let mut doc = engine::Document::open(pdfium, pdf)?;
    let pages = doc.pages()?;
    let info = doc.info();
    let chars: usize = pages
        .iter()
        .map(|p| p.glyphs.iter().filter(|g| !g.c.is_whitespace()).count())
        .sum();
    // Fewer than a line's worth of text a page, with pictures: a scan.
    if chars < 20 * pages.len().max(1) && pages.iter().any(|p| !p.images.is_empty()) || chars == 0 {
        return Err(PdfError::Scanned);
    }
    let (blocks, stats) = layout::blocks(&pages);
    // The document's own bookmarks, when it has them, are better chapters than any guess.
    let outlined = layout::apply_outline(&blocks, &doc.outline());
    let from_outline = outlined.is_some();
    let blocks = outlined.unwrap_or(blocks);
    let stem = pdf
        .file_stem()
        .map(|s| s.to_string_lossy().into_owned())
        .unwrap_or_default();
    let mut pictures = Vec::new();
    for b in &blocks {
        if let layout::Block::Figure { page, rect, .. } = b {
            pictures.push(doc.picture(*page, *rect)?);
        }
    }
    let (files, mut report) = epub::write(&blocks, &pictures, &info, &stem, from_outline);
    report.pages = pages.len();
    report.stats = stats;
    report.warnings.insert(0, QUALITY_NOTE.to_owned());
    epub::write_zip(epub, &files)?;
    Ok(report)
}
