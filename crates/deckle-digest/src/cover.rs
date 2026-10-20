// SPDX-License-Identifier: GPL-3.0-or-later
//! The cover: a front page in miniature, so the Kindle library tile is today's paper.
//! Fonts are bundled (OFL): UnifrakturMaguntia for the masthead, Old Standard for the rest.

use ab_glyph::{Font, FontRef, PxScale, ScaleFont, point};
use image::{GrayImage, Luma};

use crate::{Gathered, long_date};

pub const WIDTH: u32 = 1264;
pub const HEIGHT: u32 = 1680;
const MARGIN: f32 = 80.0;
const HEADLINES: usize = 8;

const BLACKLETTER: &[u8] = include_bytes!("../fonts/UnifrakturMaguntia-Book.ttf");
const SERIF: &[u8] = include_bytes!("../fonts/OldStandard-Regular.ttf");
const SERIF_BOLD: &[u8] = include_bytes!("../fonts/OldStandard-Bold.ttf");

struct Canvas {
    img: GrayImage,
}

impl Canvas {
    fn width_of(font: &FontRef, size: f32, text: &str) -> f32 {
        let f = font.as_scaled(PxScale::from(size));
        let mut w = 0.0;
        let mut prev = None;
        for c in text.chars() {
            let id = f.glyph_id(c);
            if let Some(p) = prev {
                w += f.kern(p, id);
            }
            w += f.h_advance(id);
            prev = Some(id);
        }
        w
    }

    /// Draws `text` with its baseline at `y`, starting at `x`.
    fn text(&mut self, font: &FontRef, size: f32, x: f32, y: f32, text: &str) {
        let scale = PxScale::from(size);
        let f = font.as_scaled(scale);
        let mut caret = x;
        let mut prev = None;
        for c in text.chars() {
            let id = f.glyph_id(c);
            if let Some(p) = prev {
                caret += f.kern(p, id);
            }
            let glyph = id.with_scale_and_position(scale, point(caret, y));
            caret += f.h_advance(id);
            prev = Some(id);
            let Some(outlined) = font.outline_glyph(glyph) else {
                continue;
            };
            let bounds = outlined.px_bounds();
            outlined.draw(|gx, gy, coverage| {
                let px = bounds.min.x as i64 + i64::from(gx);
                let py = bounds.min.y as i64 + i64::from(gy);
                if px < 0 || py < 0 || px >= i64::from(WIDTH) || py >= i64::from(HEIGHT) {
                    return;
                }
                let p = self.img.get_pixel_mut(px as u32, py as u32);
                let ink = 255.0 - coverage.clamp(0.0, 1.0) * 255.0;
                p.0[0] = p.0[0].min(ink as u8);
            });
        }
    }

    fn centered(&mut self, font: &FontRef, size: f32, y: f32, text: &str) {
        let w = Self::width_of(font, size, text);
        self.text(font, size, (WIDTH as f32 - w) / 2.0, y, text);
    }

    fn rule(&mut self, y: f32, thickness: u32) {
        for dy in 0..thickness {
            for x in MARGIN as u32..WIDTH - MARGIN as u32 {
                self.img.put_pixel(x, y as u32 + dy, Luma([0]));
            }
        }
    }
}

/// Greedy word wrap to `max_width`, at most `max_lines` lines (the last ends in "…" when cut).
fn wrap(font: &FontRef, size: f32, text: &str, max_width: f32, max_lines: usize) -> Vec<String> {
    let mut lines: Vec<String> = Vec::new();
    let mut line = String::new();
    let words = text.split_whitespace();
    for word in words {
        let candidate = if line.is_empty() {
            word.to_owned()
        } else {
            format!("{line} {word}")
        };
        if Canvas::width_of(font, size, &candidate) <= max_width || line.is_empty() {
            line = candidate;
            continue;
        }
        lines.push(std::mem::take(&mut line));
        line = word.to_owned();
        if lines.len() == max_lines {
            let last = lines.last_mut().expect("just pushed");
            while !last.is_empty() && Canvas::width_of(font, size, &format!("{last}…")) > max_width
            {
                last.pop();
            }
            last.push('…');
            return lines;
        }
    }
    if !line.is_empty() {
        lines.push(line);
    }
    lines
}

/// The largest size up to `max` at which `text` fits in `width`.
fn fit(font: &FontRef, max: f32, text: &str, width: f32) -> f32 {
    let w = Canvas::width_of(font, max, text);
    if w <= width { max } else { max * width / w }
}

/// The cover as a JPEG.
pub fn render(g: &Gathered) -> Result<Vec<u8>, String> {
    let font = |b| FontRef::try_from_slice(b).map_err(|e| e.to_string());
    let (black, serif, bold) = (font(BLACKLETTER)?, font(SERIF)?, font(SERIF_BOLD)?);
    let e = &g.edition;
    let inner = WIDTH as f32 - 2.0 * MARGIN;
    let mut c = Canvas {
        img: GrayImage::from_pixel(WIDTH, HEIGHT, Luma([255])),
    };

    let mut ears = format!("NO. {}", e.number());
    if !e.place.is_empty() {
        ears.push_str(&format!("  ·  {}", e.place.to_uppercase()));
    }
    c.centered(&serif, 30.0, 120.0, &ears);
    c.rule(140.0, 2);

    let size = fit(&black, 190.0, &e.paper, inner);
    c.centered(&black, size, 150.0 + size * 0.9, &e.paper);
    let mut y = 150.0 + size * 1.15;

    c.rule(y, 6);
    c.rule(y + 12.0, 2);
    c.centered(&serif, 44.0, y + 70.0, &long_date(e.date));
    y += 100.0;
    c.rule(y, 2);
    if let Some(w) = &e.weather_line {
        c.centered(&bold, 40.0, y + 60.0, w);
        y += 90.0;
        c.rule(y, 2);
    }

    y += 80.0;
    let heads = e.headlines(&g.news, HEADLINES);
    for (n, (s, a)) in heads.iter().enumerate() {
        let title = &e.sections[*s].articles[*a].title;
        let size = if n == 0 { 62.0 } else { 46.0 };
        let lines = wrap(&bold, size, title, inner, if n == 0 { 3 } else { 2 });
        // A separator above every headline but the first, drawn only when the headline fits.
        let gap = if n == 0 { 0.0 } else { 48.0 };
        if y + gap + size * 1.2 * lines.len() as f32 > HEIGHT as f32 - 160.0 {
            break;
        }
        if n > 0 {
            c.rule(y - 18.0, 1);
            y += gap;
        }
        for line in lines {
            c.text(&bold, size, MARGIN, y, &line);
            y += size * 1.2;
        }
    }

    c.rule(HEIGHT as f32 - 120.0, 2);
    let inside: Vec<&str> = e.sections.iter().map(|s| s.title.as_str()).collect();
    let line = format!("INSIDE: {}", inside.join(" · ").to_uppercase());
    let size = fit(&serif, 30.0, &line, inner);
    c.centered(&serif, size, HEIGHT as f32 - 75.0, &line);

    let mut jpeg = Vec::new();
    image::DynamicImage::ImageLuma8(c.img)
        .to_rgb8()
        .write_with_encoder(image::codecs::jpeg::JpegEncoder::new_with_quality(
            &mut jpeg, 90,
        ))
        .map_err(|e| e.to_string())?;
    Ok(jpeg)
}
