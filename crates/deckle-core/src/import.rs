// SPDX-License-Identifier: GPL-3.0-or-later
//! The `import` stage: a Word, OpenDocument or RTF file, a DRM-free Kindle book, text file, FB2,
//! PDF or comic becomes an EPUB first, and every later stage runs on that (DOCX spec §3, ODT spec
//! §3, RTF spec §3, MOBI spec §3, TXT spec §3, FB2 spec §3, PDF spec §6, fixed-layout spec §6).
//! Shared by the pipeline and bind.

use std::path::{Path, PathBuf};

/// An imported source: the EPUB written and what the importer had to say.
pub struct Imported {
    pub epub: PathBuf,
    pub warnings: Vec<String>,
}

/// True when `path` needs importing before it's an EPUB.
pub fn needs_import(path: &Path) -> bool {
    deckle_docx::is_word(path)
        || deckle_pdf::is_pdf(path)
        || deckle_txt::is_text(path)
        || deckle_fb2::is_fb2(path)
        || deckle_odt::is_odt(path)
        || deckle_rtf::is_rtf(path)
        || deckle_mobi::is_mobi_file(path)
        || deckle_markdown::is_markdown_file(path)
        || deckle_comic::is_cbz(path)
        || path.is_dir()
}

/// Imports `source` into `<dir>/<name>`; `None` when it is already an EPUB (or anything else the
/// importers don't take). The error is the importer's own message.
pub fn import(source: &Path, dir: &Path, name: &str) -> Option<Result<Imported, String>> {
    let epub = dir.join(name);
    // A folder of page images is a comic; any other folder is Markdown notes.
    let warnings = if deckle_comic::is_comic(source) {
        deckle_comic::convert(source, &epub)
            .map(|r| r.warnings)
            .map_err(|e| e.to_string())
    } else if deckle_markdown::is_markdown_file(source) || source.is_dir() {
        deckle_markdown::convert(source, &epub)
            .map(|r| r.warnings)
            .map_err(|e| e.to_string())
    } else if deckle_docx::is_word(source) {
        deckle_docx::convert(source, &epub)
            .map(|r| r.warnings)
            .map_err(|e| e.to_string())
    } else if deckle_pdf::is_pdf(source) {
        deckle_pdf::convert(source, &epub)
            .map(|r| r.warnings)
            .map_err(|e| e.to_string())
    } else if deckle_mobi::is_mobi_file(source) {
        deckle_mobi::convert(source, &epub)
            .map(|r| r.warnings)
            .map_err(|e| e.to_string())
    } else if deckle_rtf::is_rtf(source) {
        deckle_rtf::convert(source, &epub)
            .map(|r| r.warnings)
            .map_err(|e| e.to_string())
    } else if deckle_odt::is_odt(source) {
        deckle_odt::convert(source, &epub)
            .map(|r| r.warnings)
            .map_err(|e| e.to_string())
    } else if deckle_fb2::is_fb2(source) {
        deckle_fb2::convert(source, &epub)
            .map(|r| r.warnings)
            .map_err(|e| e.to_string())
    } else if deckle_txt::is_text(source) {
        deckle_txt::convert(source, &epub)
            .map(|r| r.warnings)
            .map_err(|e| e.to_string())
    } else {
        return None;
    };
    Some(warnings.map(|warnings| Imported { epub, warnings }))
}
