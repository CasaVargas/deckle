// SPDX-License-Identifier: GPL-3.0-or-later
//! Kindle cover thumbnails for mass-storage devices (calibre-compatible naming), plus the
//! restore cache that defeats the Amazon "no image" overwrite.

use std::io::Cursor;
use std::path::{Path, PathBuf};

use deckle_kfx::convert::CdeType;
use deckle_kfx::ion::{Sym, Value};

/// Height calibre uses on Paperwhite 3 and later.
pub const THUMBNAIL_HEIGHT: u32 = 500;
pub const THUMBNAIL_DIR: &str = "system/thumbnails";
/// Our restore copies (calibre's equivalent is `amazon-cover-bug/`).
pub const CACHE_DIR: &str = "deckle-covers";

#[derive(Debug, thiserror::Error)]
pub enum ThumbnailError {
    #[error("cover image could not be decoded: {0}")]
    Decode(String),
    #[error("I/O error at {path}: {source}")]
    Io {
        path: PathBuf,
        source: std::io::Error,
    },
}

pub fn thumbnail_name(content_id: &str, cde: CdeType) -> String {
    format!("thumbnail_{content_id}_{}_portrait.jpg", cde.as_str())
}

/// Scales the cover to `height` pixels tall and encodes it as baseline JPEG.
pub fn render(cover: &[u8], height: u32) -> Result<Vec<u8>, ThumbnailError> {
    let img = image::load_from_memory(cover).map_err(|e| ThumbnailError::Decode(e.to_string()))?;
    let width = ((f64::from(img.width()) * f64::from(height)) / f64::from(img.height()))
        .round()
        .max(1.0) as u32;
    let scaled = img
        .resize_exact(width, height, image::imageops::FilterType::Lanczos3)
        .to_rgb8();
    let mut buf = Cursor::new(Vec::new());
    let enc = image::codecs::jpeg::JpegEncoder::new_with_quality(&mut buf, 85);
    scaled
        .write_with_encoder(enc)
        .map_err(|e| ThumbnailError::Decode(e.to_string()))?;
    Ok(crate::normalize::ensure_jfif(buf.into_inner()))
}

fn write(path: &Path, data: &[u8]) -> Result<(), ThumbnailError> {
    let io = |source| ThumbnailError::Io {
        path: path.to_path_buf(),
        source,
    };
    if let Some(p) = path.parent() {
        std::fs::create_dir_all(p).map_err(io)?;
    }
    std::fs::write(path, data).map_err(io)
}

/// Writes the thumbnail into `system/thumbnails/` and a restore copy into `deckle-covers/`.
pub fn install(device_root: &Path, name: &str, jpeg: &[u8]) -> Result<PathBuf, ThumbnailError> {
    let target = device_root.join(THUMBNAIL_DIR).join(name);
    write(&target, jpeg)?;
    write(&device_root.join(CACHE_DIR).join(name), jpeg)?;
    Ok(target)
}

/// Restores every cached thumbnail that is missing from, or differs in size in,
/// `system/thumbnails/`. Returns how many were restored.
pub fn sync(device_root: &Path) -> Result<usize, ThumbnailError> {
    let cache = device_root.join(CACHE_DIR);
    let Ok(rd) = std::fs::read_dir(&cache) else {
        return Ok(0);
    };
    let mut restored = 0;
    for entry in rd.flatten() {
        let src = entry.path();
        let Some(name) = src.file_name() else {
            continue;
        };
        let dst = device_root.join(THUMBNAIL_DIR).join(name);
        let src_len = std::fs::metadata(&src).map(|m| m.len()).unwrap_or(0);
        let dst_len = std::fs::metadata(&dst).map(|m| m.len()).ok();
        if dst_len != Some(src_len) {
            let data = std::fs::read(&src).map_err(|source| ThumbnailError::Io {
                path: src.clone(),
                source,
            })?;
            write(&dst, &data)?;
            restored += 1;
        }
    }
    Ok(restored)
}

/// The cover image bytes stored inside a monolithic KFX (JPEG after our metadata pass).
pub fn cover_jpeg_from_kfx(kfx: &[u8]) -> Option<Vec<u8>> {
    let doc = deckle_kfx::container::read_kfx(kfx).ok()?;
    let md = doc.fragments.get_single(&Sym::new("$490"))?;
    let cats = md.value.as_struct()?.get("$491")?.as_list()?;
    let mut cover_name: Option<String> = None;
    for cat in cats {
        let Some(c) = cat.as_struct() else { continue };
        if c.get("$495").and_then(Value::as_str) != Some("kindle_title_metadata") {
            continue;
        }
        for kv in c.get("$258").and_then(Value::as_list).into_iter().flatten() {
            let Some(s) = kv.as_struct() else { continue };
            if s.get("$492").and_then(Value::as_str) == Some("cover_image") {
                cover_name = match s.get("$307") {
                    Some(Value::String(t)) => Some(t.clone()),
                    Some(Value::Symbol(t)) => Some(t.0.clone()),
                    _ => None,
                };
            }
        }
    }
    let cover_name = cover_name?;
    let res = doc
        .fragments
        .get(&Sym::new("$164"), Some(&Sym::new(&cover_name)))?;
    let location = res.value.as_struct()?.get("$165")?.as_str()?.to_owned();
    let raw = doc
        .fragments
        .get(&Sym::new("$417"), Some(&Sym::new(&location)))?;
    raw.value.as_blob().map(|b| b.to_vec())
}
