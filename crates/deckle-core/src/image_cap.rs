// SPDX-License-Identifier: GPL-3.0-or-later
//! Decoding a picture a file or a site gave us, held to
//! [`MAX_IMAGE_PIXELS`](deckle_limits::MAX_IMAGE_PIXELS) (size-limits design): the size is read
//! from the header first, so a small file claiming a huge picture is refused before anything is
//! allocated or converted. Comic pages are not decoded through here: `image`'s own 512 MiB limit
//! bounds one page's decode, and fitting it to a screen (`deckle_comic::pages::fit`) briefly
//! needs about as much again for the resize's floating-point copy, so a page can peak near twice
//! that.

use image::error::{LimitError, LimitErrorKind};
use image::{DynamicImage, ImageError, ImageResult};

/// Whether a `width` × `height` picture is within the pixel limit.
pub fn fits(width: u32, height: u32) -> bool {
    u64::from(width) * u64::from(height) <= deckle_limits::MAX_IMAGE_PIXELS
}

/// The error for a picture over the pixel limit.
pub fn too_large() -> ImageError {
    ImageError::Limits(LimitError::from_kind(LimitErrorKind::DimensionError))
}

/// [`image::load_from_memory`], refusing a picture over the pixel limit before decoding it.
pub fn load(bytes: &[u8]) -> ImageResult<DynamicImage> {
    let (width, height) = image::ImageReader::new(std::io::Cursor::new(bytes))
        .with_guessed_format()?
        .into_dimensions()?;
    if !fits(width, height) {
        return Err(too_large());
    }
    image::load_from_memory(bytes)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A 1×1 GIF whose header claims `w` × `h`.
    fn gif_claiming(w: u16, h: u16) -> Vec<u8> {
        let mut out = std::io::Cursor::new(Vec::new());
        image::RgbImage::from_pixel(1, 1, image::Rgb([1, 2, 3]))
            .write_to(&mut out, image::ImageFormat::Gif)
            .unwrap();
        let mut gif = out.into_inner();
        gif[6..8].copy_from_slice(&w.to_le_bytes());
        gif[8..10].copy_from_slice(&h.to_le_bytes());
        gif
    }

    #[test]
    fn a_picture_over_the_pixel_limit_is_refused_before_decoding() {
        // 110 megapixels: under `image`'s own 512 MiB, over Deckle's 100 megapixels.
        let e = load(&gif_claiming(11_000, 10_000)).unwrap_err();
        assert!(matches!(e, ImageError::Limits(_)), "{e}");
        assert!(load(&gif_claiming(1, 1)).is_ok());
        assert!(fits(10_000, 10_000) && !fits(10_001, 10_000));
    }
}
