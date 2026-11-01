// SPDX-License-Identifier: GPL-3.0-or-later
//! The import stage's entry point (MOBI import design §2): a DRM-free MOBI, AZW, AZW3 or PRC
//! routed to the KF8 rebuild or the MOBI 6 one. DRM and fixed layout are refused before any
//! text is read.

use std::path::Path;

use crate::MobiError;

/// Larger than any real book; anything bigger is almost certainly not one.
const MAX_BYTES: u64 = 50 * 1024 * 1024;

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ImportReport {
    pub title: String,
    /// XHTML files in the spine.
    pub parts: usize,
    pub images: usize,
    pub warnings: Vec<String>,
}

/// Inputs the importer takes: `.mobi`, `.azw`, `.azw3` and `.prc`, any case.
pub fn is_mobi_file(path: &Path) -> bool {
    path.extension().and_then(|e| e.to_str()).is_some_and(|e| {
        ["mobi", "azw", "azw3", "prc"]
            .iter()
            .any(|x| e.eq_ignore_ascii_case(x))
    })
}

/// Converts the book at `src` and writes the EPUB to `epub`.
pub fn convert(src: &Path, epub: &Path) -> Result<ImportReport, MobiError> {
    let size = std::fs::metadata(src)?.len();
    if size > MAX_BYTES {
        return Err(MobiError::Unsupported(format!(
            "it is {} MB; the limit is 50 MB",
            size / (1024 * 1024)
        )));
    }
    let bytes = std::fs::read(src)?;
    let stem = src
        .file_stem()
        .map(|s| s.to_string_lossy().into_owned())
        .unwrap_or_default();
    to_epub(&bytes, epub, &stem)
}

/// Routes `bytes`: refused when DRM-protected or fixed-layout, else rebuilt from KF8 when the
/// book has a KF8 part, else from its MOBI 6 or PalmDOC text.
pub fn to_epub(bytes: &[u8], epub: &Path, fallback_title: &str) -> Result<ImportReport, MobiError> {
    if bytes.starts_with(b"CONT") || bytes.starts_with(b"\xeaDRMION") {
        return Err(MobiError::Unsupported(
            "this is a KFX book; send it as a .kfx file and it goes over as is".into(),
        ));
    }
    if !crate::is_mobi(bytes) {
        return Err(MobiError::NotMobi("no BOOKMOBI type and creator"));
    }
    let meta = crate::read_meta(bytes)?;
    if meta.drm {
        return Err(MobiError::Drm);
    }
    if meta.fixed_layout {
        return Err(MobiError::Unsupported(
            "a fixed-layout Kindle book: Deckle sends it to a Kindle as it is, but can't make an \
             EPUB of it for other readers"
                .into(),
        ));
    }
    let r = if meta.kf8 {
        crate::kf8::rebuild(bytes, epub)?
    } else {
        crate::mobi6::rebuild(bytes, epub, fallback_title)?
    };
    Ok(ImportReport {
        title: r.title,
        parts: r.parts,
        images: r.images,
        warnings: r.warnings,
    })
}
