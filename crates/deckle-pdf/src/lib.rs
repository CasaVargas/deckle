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

/// Height a scanned page is rendered at: the tallest e-reader panel (Kindle Scribe). Each device
/// gets it scaled down to its own screen later (fixed-layout spec §7.1).
const SCAN_HEIGHT: u32 = 2480;

/// What [`render_scan`] wrote.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Scan {
    /// The PDF's own title and author, when it has real ones.
    pub title: Option<String>,
    pub author: Option<String>,
    pub pages: usize,
}

/// Renders every page of the PDF at `pdf` into `dir` as `p0001.jpg`, `p0002.jpg`…: a scan
/// [`convert`] refuses becomes a folder of page images, which `deckle-comic` makes a fixed-layout
/// book of (fixed-layout spec §9, phase 4b). No OCR: the pages stay pictures.
pub fn render_scan(pdf: &Path, dir: &Path) -> Result<Scan, PdfError> {
    let size = std::fs::metadata(pdf)?.len();
    if size > MAX_BYTES {
        return Err(PdfError::TooLarge(size));
    }
    let pdfium = engine::pdfium()?;
    let doc = engine::Document::open(pdfium, pdf)?;
    let info = doc.info();
    let pages = doc.page_count();
    std::fs::create_dir_all(dir)?;
    for i in 0..pages {
        let img = doc.render(i, SCAN_HEIGHT)?;
        let mut jpeg = Vec::new();
        let enc = image::codecs::jpeg::JpegEncoder::new_with_quality(&mut jpeg, 85);
        img.to_rgb8()
            .write_with_encoder(enc)
            .map_err(|e| PdfError::Unreadable(e.to_string()))?;
        std::fs::write(dir.join(format!("p{:04}.jpg", i + 1)), jpeg)?;
    }
    Ok(Scan {
        title: info.title,
        author: info.author,
        pages,
    })
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
    let (files, mut report) = epub::write(&blocks, &pictures, &info, &stem);
    report.pages = pages.len();
    report.stats = stats;
    report.warnings.insert(0, QUALITY_NOTE.to_owned());
    epub::write_zip(epub, &files)?;
    Ok(report)
}
