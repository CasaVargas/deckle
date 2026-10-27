// SPDX-License-Identifier: GPL-3.0-or-later
//! The picture a recipe prints: the site's photo, cropped to 3:2 and made e-ink (cookbook design §7.1).

use image::ImageFormat;

use crate::article::eink_photo;

/// The printed picture for a site photo: the middle 3:2 of it, as [`eink_photo`] makes it
/// (grayscale JPEG, at most 1000 px wide). `None` for tiny images and bytes that aren't one.
pub fn site_picture(bytes: &[u8]) -> Option<Vec<u8>> {
    let img = image::load_from_memory(bytes).ok()?;
    let (w, h) = (img.width(), img.height());
    // Cross-multiplied, so 3:2 is exact; the odd pixel rounds to the nearest.
    let (cw, ch) = if u64::from(w) * 2 > u64::from(h) * 3 {
        ((h * 3).div_ceil(2), h)
    } else {
        (w, (w * 2 + 1) / 3)
    };
    let (cw, ch) = (cw.min(w), ch.min(h));
    let crop = img.crop_imm((w - cw) / 2, (h - ch) / 2, cw, ch);
    // Lossless, so the crop is only compressed once, by the JPEG that is printed.
    let mut png = Vec::new();
    crop.write_to(&mut std::io::Cursor::new(&mut png), ImageFormat::Png)
        .ok()?;
    eink_photo(&png)
}
