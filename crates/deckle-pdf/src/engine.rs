// SPDX-License-Identifier: GPL-3.0-or-later
//! PDFium: finding the library, reading a document into [`Page`]s and cutting pictures out of a
//! rendered page (PDF spec §3).

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::OnceLock;

use pdfium_render::prelude::*;

use crate::PdfError;
use crate::layout::{Glyph, Page, Rect};

/// Points to pixels when a page is rendered for its pictures: 144 dpi, sharp on any Kindle.
const RENDER_SCALE: f32 = 2.0;

/// `DECKLE_PDFIUM`: the PDFium library file, or the folder it is in.
pub const ENV: &str = "DECKLE_PDFIUM";

/// Where PDFium may be, in order: `DECKLE_PDFIUM`, beside the executable, the app bundle's
/// `Frameworks` (macOS), and in a development build the workspace's `target/pdfium`, where
/// `tools/fetch-pdfium.sh` puts it.
pub fn candidates() -> Vec<PathBuf> {
    let name = Pdfium::pdfium_platform_library_name();
    let mut out = Vec::new();
    if let Some(p) = std::env::var_os(ENV).filter(|p| !p.is_empty()) {
        let p = PathBuf::from(p);
        out.push(if p.is_dir() { p.join(&name) } else { p });
    }
    if let Some(dir) = std::env::current_exe()
        .ok()
        .and_then(|e| e.parent().map(Path::to_path_buf))
    {
        out.push(dir.join(&name));
        out.push(dir.join("../Frameworks").join(&name));
    }
    if cfg!(debug_assertions) {
        let target = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../target/pdfium");
        out.push(target.join("lib").join(&name));
        out.push(target.join("bin").join(&name));
    }
    out
}

static PDFIUM: OnceLock<Result<Pdfium, String>> = OnceLock::new();

/// The process's PDFium, bound on first use: PDFium is initialised once per process.
pub fn pdfium() -> Result<&'static Pdfium, PdfError> {
    PDFIUM
        .get_or_init(|| {
            let tried = candidates();
            for path in tried.iter().filter(|p| p.is_file()) {
                if let Ok(bindings) = Pdfium::bind_to_library(path) {
                    return Ok(Pdfium::new(bindings));
                }
            }
            match Pdfium::bind_to_system_library() {
                Ok(bindings) => Ok(Pdfium::new(bindings)),
                Err(_) => Err(tried
                    .iter()
                    .map(|p| p.display().to_string())
                    .collect::<Vec<_>>()
                    .join(", ")),
            }
        })
        .as_ref()
        .map_err(|tried| PdfError::NoEngine(tried.clone()))
}

/// An open document and the pages it renders, kept for cutting pictures.
pub struct Document<'a> {
    doc: PdfDocument<'a>,
    rendered: HashMap<usize, image::DynamicImage>,
}

/// Title and author from the document's info, when they look like a person wrote them.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Info {
    pub title: Option<String>,
    pub author: Option<String>,
}

fn real(value: Option<PdfDocumentMetadataTag>) -> Option<String> {
    let v = value?.value().trim().to_owned();
    let lower = v.to_lowercase();
    let junk = v.is_empty()
        || lower == "untitled"
        || lower.starts_with("microsoft word - ")
        || [".doc", ".docx", ".pdf", ".indd", ".tex", ".dvi", ".qxd"]
            .iter()
            .any(|ext| lower.ends_with(ext));
    (!junk).then_some(v)
}

/// Fonts whose glyphs are mathematics.
fn math_font(name: &str) -> bool {
    let n = name.to_lowercase();
    [
        "cmmi", "cmsy", "cmex", "msam", "msbm", "math", "symbol", "stix", "euclid", "mt extra",
        "rsfs", "esint",
    ]
    .iter()
    .any(|m| n.contains(m))
}

impl<'a> Document<'a> {
    pub fn open(pdfium: &'a Pdfium, path: &Path) -> Result<Self, PdfError> {
        let doc = pdfium.load_pdf_from_file(path, None).map_err(|e| match e {
            PdfiumError::PdfiumLibraryInternalError(PdfiumInternalError::PasswordError) => {
                PdfError::Locked
            }
            e => PdfError::Unreadable(e.to_string()),
        })?;
        Ok(Document {
            doc,
            rendered: HashMap::new(),
        })
    }

    pub fn info(&self) -> Info {
        let m = self.doc.metadata();
        Info {
            title: real(m.get(PdfDocumentMetadataTagType::Title)),
            author: real(m.get(PdfDocumentMetadataTagType::Author)),
        }
    }

    /// Every page's glyphs, images and drawings.
    pub fn pages(&self) -> Result<Vec<Page>, PdfError> {
        let mut out = Vec::new();
        for page in self.doc.pages().iter() {
            out.push(read_page(&page).map_err(|e| PdfError::Unreadable(e.to_string()))?);
        }
        Ok(out)
    }

    /// How many pages the document has.
    pub fn page_count(&self) -> usize {
        self.doc.pages().len() as usize
    }

    /// Page `index` rendered whole, `height` pixels tall, on white: a scanned page as a picture
    /// (fixed-layout spec §9, phase 4b).
    pub fn render(&self, index: usize, height: u32) -> Result<image::DynamicImage, PdfError> {
        let err = |e: PdfiumError| PdfError::Unreadable(e.to_string());
        let page = self.doc.pages().get(index as PdfPageIndex).map_err(err)?;
        let config = PdfRenderConfig::new()
            .set_target_height(height as Pixels)
            .set_clear_color(PdfColor::WHITE)
            .render_form_data(true);
        page.render_with_config(&config)
            .map_err(err)?
            .as_image()
            .map_err(err)
    }

    /// `rect` on page `index` as a PNG, rendered at [`RENDER_SCALE`].
    pub fn picture(&mut self, index: usize, rect: Rect) -> Result<Vec<u8>, PdfError> {
        let err = |e: PdfiumError| PdfError::Unreadable(e.to_string());
        if !self.rendered.contains_key(&index) {
            let page = self.doc.pages().get(index as PdfPageIndex).map_err(err)?;
            let config = PdfRenderConfig::new()
                .scale_page_by_factor(RENDER_SCALE)
                .set_clear_color(PdfColor::WHITE)
                .render_form_data(true);
            let image = page
                .render_with_config(&config)
                .map_err(err)?
                .as_image()
                .map_err(err)?;
            self.rendered.insert(index, image);
        }
        let image = &self.rendered[&index];
        let px = |v: f32| (v.max(0.0) * RENDER_SCALE).round() as u32;
        let x = px(rect.x0).min(image.width().saturating_sub(1));
        let y = px(rect.y0).min(image.height().saturating_sub(1));
        let w = px(rect.width()).clamp(1, image.width() - x);
        let h = px(rect.height()).clamp(1, image.height() - y);
        let mut png = Vec::new();
        image
            .crop_imm(x, y, w, h)
            .to_rgb8()
            .write_to(&mut std::io::Cursor::new(&mut png), image::ImageFormat::Png)
            .map_err(|e| PdfError::Unreadable(e.to_string()))?;
        Ok(png)
    }
}

fn read_page(page: &PdfPage) -> Result<Page, PdfiumError> {
    let (width, height) = (page.width().value, page.height().value);
    let flip = |r: PdfRect| Rect {
        x0: r.left().value,
        y0: height - r.top().value,
        x1: r.right().value,
        y1: height - r.bottom().value,
    };
    let mut glyphs: Vec<Glyph> = Vec::new();
    // Whitespace (often generated by PDFium, with no box) only marks a space before what follows.
    let mut space = false;
    let text = page.text()?;
    for ch in text.chars().iter() {
        let Some(raw) = ch.unicode_char() else {
            continue;
        };
        // PDFium reports a hyphen it takes for a line-end break as U+0002; it was drawn as a
        // hyphen, and whether it stays is the layout's call. Only U+00AD is soft for certain.
        let line_end_hyphen = raw == '\u{2}' || ch.is_hyphen().unwrap_or(false);
        let c = if line_end_hyphen { '-' } else { raw };
        if c.is_whitespace() || (c.is_control() && !line_end_hyphen) {
            space = true;
            continue;
        }
        // Rotated text (a chart's axis label, a margin note) stays out of the flow.
        if ch
            .angle_degrees()
            .is_ok_and(|a| a.abs() > 1.0 && (a - 360.0).abs() > 1.0)
        {
            continue;
        }
        let Ok(bounds) = ch.loose_bounds() else {
            continue;
        };
        let name = ch.font_name();
        let weight = ch.font_weight();
        let r = flip(bounds);
        glyphs.push(Glyph {
            c,
            x0: r.x0,
            y0: r.y0,
            x1: r.x1,
            y1: r.y1,
            size: ch.scaled_font_size().value,
            bold: name.contains("Bold")
                || name.contains("Black")
                || name.contains("Heavy")
                || matches!(
                    weight,
                    Some(
                        PdfFontWeight::Weight600
                            | PdfFontWeight::Weight700Bold
                            | PdfFontWeight::Weight800
                            | PdfFontWeight::Weight900
                    )
                ),
            italic: ch.font_is_italic() || name.contains("Italic") || name.contains("Oblique"),
            math: math_font(&name),
            soft_hyphen: raw == '\u{ad}',
            space_before: std::mem::take(&mut space),
        });
    }
    let mut images = Vec::new();
    let mut paths = Vec::new();
    for object in page.objects().iter() {
        let Ok(bounds) = object.bounds() else {
            continue;
        };
        let r = flip(bounds.to_rect());
        match object.object_type() {
            PdfPageObjectType::Image => images.push(r),
            PdfPageObjectType::Path
            | PdfPageObjectType::Shading
            | PdfPageObjectType::XObjectForm => paths.push(r),
            _ => {}
        }
    }
    Ok(Page {
        width,
        height,
        glyphs,
        images,
        paths,
    })
}
