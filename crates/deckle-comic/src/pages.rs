// SPDX-License-Identifier: GPL-3.0-or-later
//! What happens to a page image (fixed-layout spec §7.1): a spread cut in half at import, and a
//! page fitted to a device's screen, grey on a black-and-white one, at finish.

use std::io::Cursor;

use image::{DynamicImage, ImageFormat};

/// A device's screen in pixels, portrait.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Screen {
    pub width: u32,
    pub height: u32,
    pub color: bool,
}

/// JPEG quality for a fitted page.
const FIT_QUALITY: u8 = 85;
/// JPEG quality for a spread's halves, which may be fitted again later.
const SPLIT_QUALITY: u8 = 90;

/// `bytes` fitted to `screen`: scaled down to fit (never up), grey on a black-and-white screen,
/// and re-encoded in its own format (KF8 markup names each image's type). `None` when it already
/// fits as it is; an error when it isn't an image that decodes.
pub fn fit(bytes: &[u8], screen: Screen) -> Result<Option<Vec<u8>>, String> {
    let format = image::guess_format(bytes).map_err(|e| e.to_string())?;
    let img = image::load_from_memory_with_format(bytes, format).map_err(|e| e.to_string())?;
    let too_big = img.width() > screen.width || img.height() > screen.height;
    let grey = matches!(img.color(), image::ColorType::L8 | image::ColorType::La8);
    if !too_big && (screen.color || grey) {
        return Ok(None);
    }
    let img = if too_big {
        img.resize(
            screen.width,
            screen.height,
            image::imageops::FilterType::Lanczos3,
        )
    } else {
        img
    };
    let img = if screen.color {
        img
    } else {
        DynamicImage::ImageLuma8(img.to_luma8())
    };
    encode(&img, format, FIT_QUALITY).map(Some)
}

/// Which half of a spread.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Half {
    Left,
    Right,
}

/// The `half` of a landscape page, re-encoded in its own format (a GIF's as PNG), with that
/// format.
pub fn split(bytes: &[u8], half: Half) -> Result<(Vec<u8>, ImageFormat), String> {
    let format = image::guess_format(bytes).map_err(|e| e.to_string())?;
    let img = image::load_from_memory_with_format(bytes, format).map_err(|e| e.to_string())?;
    let (w, h) = (img.width(), img.height());
    let left = w / 2;
    let cut = match half {
        Half::Left => img.crop_imm(0, 0, left, h),
        Half::Right => img.crop_imm(left, 0, w - left, h),
    };
    let format = if format == ImageFormat::Gif {
        ImageFormat::Png
    } else {
        format
    };
    Ok((encode(&cut, format, SPLIT_QUALITY)?, format))
}

/// The pixel size of `half` of a `width`-wide page.
pub fn half_width(width: u32, half: Half) -> u32 {
    match half {
        Half::Left => width / 2,
        Half::Right => width - width / 2,
    }
}

fn encode(img: &DynamicImage, format: ImageFormat, quality: u8) -> Result<Vec<u8>, String> {
    let mut buf = Cursor::new(Vec::new());
    match format {
        ImageFormat::Jpeg => {
            let enc = image::codecs::jpeg::JpegEncoder::new_with_quality(&mut buf, quality);
            // JPEG has no alpha: grey stays one channel, anything else becomes RGB.
            let r = match img {
                DynamicImage::ImageLuma8(l) => l.write_with_encoder(enc),
                other => other.to_rgb8().write_with_encoder(enc),
            };
            r.map_err(|e| e.to_string())?;
            return Ok(ensure_jfif(buf.into_inner()));
        }
        ImageFormat::Gif => DynamicImage::ImageRgba8(img.to_rgba8()).write_to(&mut buf, format),
        _ => img.write_to(&mut buf, format),
    }
    .map_err(|e| e.to_string())?;
    Ok(buf.into_inner())
}

/// A JFIF APP0 segment after SOI when the encoder wrote none (as `normalize` does for covers:
/// Kindles read JFIF most reliably).
fn ensure_jfif(jpeg: Vec<u8>) -> Vec<u8> {
    if jpeg.len() > 4 && jpeg[0..2] == [0xFF, 0xD8] && jpeg[2..4] != [0xFF, 0xE0] {
        let mut out = Vec::with_capacity(jpeg.len() + 18);
        out.extend_from_slice(&[0xFF, 0xD8]);
        out.extend_from_slice(&[
            0xFF, 0xE0, 0x00, 0x10, b'J', b'F', b'I', b'F', 0x00, 0x01, 0x01, 0x00, 0x00, 0x01,
            0x00, 0x01, 0x00, 0x00,
        ]);
        out.extend_from_slice(&jpeg[2..]);
        out
    } else {
        jpeg
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn image(w: u32, h: u32, format: ImageFormat) -> Vec<u8> {
        let img = image::RgbImage::from_fn(w, h, |x, _| {
            if x < w / 2 {
                image::Rgb([200, 30, 30])
            } else {
                image::Rgb([30, 30, 200])
            }
        });
        let mut out = Cursor::new(Vec::new());
        img.write_to(&mut out, format).unwrap();
        out.into_inner()
    }

    const KINDLE: Screen = Screen {
        width: 1264,
        height: 1680,
        color: false,
    };

    #[test]
    fn a_big_page_is_scaled_to_fit_keeping_its_shape_and_turned_grey() {
        // A quarter-size stand-in for a 3000×4000 scan on a 1264×1680 Kindle (fast in debug).
        let small = Screen {
            width: 316,
            height: 420,
            color: false,
        };
        let fitted = fit(&image(750, 1000, ImageFormat::Jpeg), small)
            .unwrap()
            .unwrap();
        assert_eq!(&fitted[2..4], [0xFF, 0xE0], "JFIF");
        let img = image::load_from_memory(&fitted).unwrap();
        assert_eq!(image::guess_format(&fitted).unwrap(), ImageFormat::Jpeg);
        assert!(img.width() <= 316 && img.height() <= 420);
        // 3:4 kept: the height is the limit.
        assert_eq!((img.width(), img.height()), (315, 420));
        assert_eq!(img.color(), image::ColorType::L8);
    }

    #[test]
    fn a_small_page_is_never_scaled_up_but_still_greyed() {
        let png = image(300, 400, ImageFormat::Png);
        let fitted = fit(&png, KINDLE).unwrap().unwrap();
        assert_eq!(image::guess_format(&fitted).unwrap(), ImageFormat::Png);
        let img = image::load_from_memory(&fitted).unwrap();
        assert_eq!((img.width(), img.height()), (300, 400));
        assert_eq!(img.color(), image::ColorType::L8);
        // Already grey and small: left alone. On a colour screen, a small colour page too.
        assert_eq!(fit(&fitted, KINDLE).unwrap(), None);
        let colour = Screen {
            color: true,
            ..KINDLE
        };
        assert_eq!(fit(&png, colour).unwrap(), None);
    }

    #[test]
    fn a_gif_stays_a_gif_and_junk_is_an_error() {
        let fitted = fit(
            &image(300, 450, ImageFormat::Gif),
            Screen {
                width: 200,
                ..KINDLE
            },
        )
        .unwrap()
        .unwrap();
        assert_eq!(image::guess_format(&fitted).unwrap(), ImageFormat::Gif);
        assert!(fit(b"not an image", KINDLE).is_err());
    }

    #[test]
    fn a_spread_splits_into_its_halves() {
        let spread = image(201, 100, ImageFormat::Png);
        let (left, f) = split(&spread, Half::Left).unwrap();
        let (right, _) = split(&spread, Half::Right).unwrap();
        assert_eq!(f, ImageFormat::Png);
        let (l, r) = (
            image::load_from_memory(&left).unwrap().to_rgb8(),
            image::load_from_memory(&right).unwrap().to_rgb8(),
        );
        assert_eq!((l.width(), r.width()), (100, 101));
        assert_eq!(
            (half_width(201, Half::Left), half_width(201, Half::Right)),
            (100, 101)
        );
        assert_eq!(l.get_pixel(50, 50).0, [200, 30, 30]);
        assert_eq!(r.get_pixel(50, 50).0, [30, 30, 200]);
        let (_, f) = split(&image(40, 20, ImageFormat::Gif), Half::Left).unwrap();
        assert_eq!(f, ImageFormat::Png);
    }
}
