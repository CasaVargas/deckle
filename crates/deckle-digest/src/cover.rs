// SPDX-License-Identifier: GPL-3.0-or-later
//! The cover: a front page in miniature, so the Kindle library tile is today's paper.
//! Fonts are bundled (OFL): UnifrakturMaguntia for the masthead, Old Standard for the rest.
//!
//! Layout, top to bottom: ears (edition number, place), masthead, a black date band sized to
//! stay legible on a library tile, the weather line, the lead story with its opening
//! paragraph, a two-column grid of headlines with section labels, and the "Inside" index.

use ab_glyph::{Font, FontRef, PxScale, ScaleFont, point};
use image::{GrayImage, Luma};

use crate::{Gathered, long_date};

pub const WIDTH: u32 = 1264;
pub const HEIGHT: u32 = 1680;
const MARGIN: f32 = 64.0;
/// Headlines considered for the lead plus the grid.
const HEADLINES: usize = 12;
/// Saved articles shown in the cover's grid.
const SAVED_ON_COVER: usize = 2;
const BLACK: u8 = 0;
const WHITE: u8 = 255;

const BLACKLETTER: &[u8] = include_bytes!("../fonts/UnifrakturMaguntia-Book.ttf");
const SERIF: &[u8] = include_bytes!("../fonts/OldStandard-Regular.ttf");
const SERIF_BOLD: &[u8] = include_bytes!("../fonts/OldStandard-Bold.ttf");
const SERIF_ITALIC: &[u8] = include_bytes!("../fonts/OldStandard-Italic.ttf");

struct Fonts<'a> {
    // `'static`: the fonts are bundled bytes.
    black: FontRef<'a>,
    serif: FontRef<'a>,
    bold: FontRef<'a>,
    italic: FontRef<'a>,
}

struct Canvas {
    img: GrayImage,
}

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

/// The largest size up to `max` at which `text` fits in `width`.
fn fit(font: &FontRef, max: f32, text: &str, width: f32) -> f32 {
    let w = width_of(font, max, text);
    if w <= width { max } else { max * width / w }
}

/// Greedy word wrap to `max_width`, at most `max_lines` lines (the last ends in "…" when cut).
fn wrap(font: &FontRef, size: f32, text: &str, max_width: f32, max_lines: usize) -> Vec<String> {
    let mut lines: Vec<String> = Vec::new();
    let mut line = String::new();
    for word in text.split_whitespace() {
        let candidate = if line.is_empty() {
            word.to_owned()
        } else {
            format!("{line} {word}")
        };
        if width_of(font, size, &candidate) <= max_width || line.is_empty() {
            line = candidate;
            continue;
        }
        lines.push(std::mem::take(&mut line));
        line = word.to_owned();
        if lines.len() == max_lines {
            let last = lines.last_mut().expect("just pushed");
            while !last.is_empty() && width_of(font, size, &format!("{last}…")) > max_width {
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

impl Canvas {
    /// Draws `text` in `ink` with its baseline at `y`, starting at `x`.
    fn text(&mut self, font: &FontRef, size: f32, x: f32, y: f32, text: &str, ink: u8) {
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
                let a = coverage.clamp(0.0, 1.0);
                p.0[0] = (f32::from(p.0[0]) * (1.0 - a) + f32::from(ink) * a).round() as u8;
            });
        }
    }

    fn centered(&mut self, font: &FontRef, size: f32, y: f32, text: &str, ink: u8) {
        let w = width_of(font, size, text);
        self.text(font, size, (WIDTH as f32 - w) / 2.0, y, text, ink);
    }

    fn right(&mut self, font: &FontRef, size: f32, y: f32, text: &str) {
        let w = width_of(font, size, text);
        self.text(font, size, WIDTH as f32 - MARGIN - w, y, text, BLACK);
    }

    fn fill(&mut self, x0: f32, y0: f32, x1: f32, y1: f32, ink: u8) {
        for y in y0.max(0.0) as u32..(y1 as u32).min(HEIGHT) {
            for x in x0.max(0.0) as u32..(x1 as u32).min(WIDTH) {
                self.img.put_pixel(x, y, Luma([ink]));
            }
        }
    }

    /// A full-width horizontal rule with its top at `y`.
    fn rule(&mut self, y: f32, thickness: f32) {
        self.fill(MARGIN, y, WIDTH as f32 - MARGIN, y + thickness, BLACK);
    }
}

/// Paragraph text: lines of `size` with `leading`, returns the y after the last line.
fn paragraph(
    c: &mut Canvas,
    font: &FontRef,
    size: f32,
    x: f32,
    mut y: f32,
    lines: &[String],
    leading: f32,
) -> f32 {
    for line in lines {
        y += size * leading;
        c.text(font, size, x, y, line, BLACK);
    }
    y
}

fn fonts() -> Result<Fonts<'static>, String> {
    let font = |b| FontRef::try_from_slice(b).map_err(|e| e.to_string());
    Ok(Fonts {
        black: font(BLACKLETTER)?,
        serif: font(SERIF)?,
        bold: font(SERIF_BOLD)?,
        italic: font(SERIF_ITALIC)?,
    })
}

fn jpeg(c: Canvas) -> Result<Vec<u8>, String> {
    let mut out = Vec::new();
    image::DynamicImage::ImageLuma8(c.img)
        .to_rgb8()
        .write_with_encoder(image::codecs::jpeg::JpegEncoder::new_with_quality(
            &mut out, 90,
        ))
        .map_err(|e| e.to_string())?;
    Ok(out)
}

/// A long read's cover (save-to-paper §5): "Deckle Reads" in the masthead face, the title
/// large, then the byline, outlet and reading time (then `via`, how it came).
pub fn render_reads(
    title: &str,
    byline: Option<&str>,
    outlet: &str,
    minutes: usize,
    via: Option<&str>,
) -> Result<Vec<u8>, String> {
    let f = fonts()?;
    let inner = WIDTH as f32 - 2.0 * MARGIN;
    let mut c = Canvas {
        img: GrayImage::from_pixel(WIDTH, HEIGHT, Luma([WHITE])),
    };
    let mut y = 220.0;
    c.centered(&f.black, 120.0, y, "Deckle Reads", BLACK);
    y += 50.0;
    c.rule(y, 6.0);
    c.rule(y + 12.0, 2.0);
    let size = 96.0;
    let lines = wrap(&f.bold, size, title, inner, 6);
    // Title, byline and outlet centered in the space between the rules.
    let block =
        size * 1.15 * lines.len() as f32 + 40.0 + if byline.is_some() { 70.0 } else { 0.0 } + 40.0;
    let foot = HEIGHT as f32 - 110.0;
    y = (y + 12.0 + foot) / 2.0 - block / 2.0 + size;
    for line in &lines {
        c.centered(&f.bold, size, y, line, BLACK);
        y += size * 1.15;
    }
    y += 40.0;
    if let Some(b) = byline {
        c.centered(&f.italic, 46.0, y, b, BLACK);
        y += 70.0;
    }
    c.centered(&f.serif, 38.0, y, outlet, BLACK);
    c.rule(foot, 2.0);
    c.centered(
        &f.serif,
        34.0,
        foot + 60.0,
        &match via {
            Some(v) => format!("About {minutes} min · {v}"),
            None => format!("About {minutes} min"),
        },
        BLACK,
    );
    jpeg(c)
}

/// The outlet in a story's meta line ("By Sentinel Staff · Sandhills Sentinel · Oct 20"):
/// the first part that isn't a byline.
fn outlet_of(meta: &str) -> Option<&str> {
    meta.split(" · ").find(|p| !p.starts_with("By "))
}

/// The cover as a JPEG.
pub fn render(g: &Gathered) -> Result<Vec<u8>, String> {
    let f = fonts()?;
    let e = &g.edition;
    let inner = WIDTH as f32 - 2.0 * MARGIN;
    let mut c = Canvas {
        img: GrayImage::from_pixel(WIDTH, HEIGHT, Luma([WHITE])),
    };

    // Ears and masthead.
    let mut y = 78.0;
    c.text(
        &f.bold,
        26.0,
        MARGIN,
        y,
        &format!("NO. {}", e.number()),
        BLACK,
    );
    if !e.place.is_empty() {
        c.right(&f.bold, 26.0, y, &e.place.to_uppercase());
    }
    c.centered(&f.italic, 26.0, y, "All the news that fits", BLACK);
    y += 18.0;
    c.rule(y, 2.0);
    let size = fit(&f.black, 200.0, &e.paper, inner);
    y += size * 0.95;
    c.centered(&f.black, size, y, &e.paper, BLACK);
    y += size * 0.3;

    // The date band: the one thing that must read on a library tile.
    c.fill(MARGIN, y, WIDTH as f32 - MARGIN, y + 140.0, BLACK);
    let date = long_date(e.date).to_uppercase();
    let size = fit(&f.bold, 84.0, &date, inner - 60.0);
    c.centered(&f.bold, size, y + 70.0 + size * 0.36, &date, WHITE);
    y += 140.0;

    if let Some(w) = &e.weather_line {
        y += 54.0;
        c.centered(&f.bold, 38.0, y, &format!("TODAY  ·  {w}"), BLACK);
        y += 22.0;
        c.rule(y, 2.0);
        c.rule(y + 8.0, 6.0);
        y += 14.0;
    } else {
        y += 10.0;
    }

    let heads = e.headlines(&g.news, HEADLINES);
    let label = |s: usize, a: usize| {
        let section = &e.sections[s];
        match section.articles[a]
            .meta
            .as_deref()
            .and_then(|m| outlet_of(m))
        {
            // Briefs carry their outlet as the first meta part (Local: "WXII · Oct 20").
            Some(src) if section.title == "Local" => format!("LOCAL · {}", src.to_uppercase()),
            _ if Some(s) == g.reading => "YOUR READING".to_owned(),
            _ => section.title.to_uppercase(),
        }
    };
    let footer_top = HEIGHT as f32 - 110.0;

    // Lead story: the first headline with an opening paragraph, else the first headline.
    let lead = heads
        .iter()
        .position(|&(s, a)| e.sections[s].articles[a].teaser.is_some())
        .or((!heads.is_empty()).then_some(0));
    if let Some(i) = lead {
        let (s, a) = heads[i];
        let art = &e.sections[s].articles[a];
        y += 52.0;
        c.text(&f.bold, 26.0, MARGIN, y, &label(s, a), BLACK);
        let lines = wrap(&f.bold, 76.0, &art.title, inner, 3);
        y = paragraph(&mut c, &f.bold, 76.0, MARGIN, y, &lines, 1.12);
        if let Some(t) = &art.teaser {
            y += 12.0;
            let lines = wrap(&f.italic, 34.0, t, inner, 3);
            y = paragraph(&mut c, &f.italic, 34.0, MARGIN, y, &lines, 1.35);
        }
        y += 34.0;
        c.rule(y, 2.0);
    }

    // Two columns of the remaining headlines, each item going to the shorter column.
    let gutter = 48.0;
    let col_w = (inner - gutter) / 2.0;
    let top = y + 6.0;
    let mut col_y = [top, top];
    let mut drawn = [false, false];
    // The news keeps the lead; up to two saved articles take the first grid spots.
    let saved = g
        .reading
        .map(|r| (0..e.sections[r].articles.len().min(SAVED_ON_COVER)).map(move |a| (r, a)));
    let grid: Vec<(usize, usize)> = saved
        .into_iter()
        .flatten()
        .chain(
            heads
                .iter()
                .enumerate()
                .filter(|(k, _)| Some(*k) != lead)
                .map(|(_, h)| *h),
        )
        .collect();
    for &(s, a) in &grid {
        let col = if col_y[0] <= col_y[1] { 0 } else { 1 };
        let x = MARGIN + col as f32 * (col_w + gutter);
        let lines = wrap(&f.bold, 40.0, &e.sections[s].articles[a].title, col_w, 3);
        let needed = 30.0 + 22.0 + 40.0 * 1.15 * lines.len() as f32 + 26.0;
        if col_y[col] + needed > footer_top - 10.0 {
            // This column is full; try the other one once.
            let other = 1 - col;
            if col_y[other] + needed > footer_top - 10.0 {
                break;
            }
            col_y[col] = f32::MAX;
            continue;
        }
        let mut cy = col_y[col];
        if drawn[col] {
            c.fill(x, cy, x + col_w, cy + 1.0, BLACK);
        }
        cy += 52.0;
        c.text(&f.bold, 22.0, x, cy, &label(s, a), BLACK);
        cy = paragraph(&mut c, &f.bold, 40.0, x, cy, &lines, 1.15);
        col_y[col] = cy + 26.0;
        drawn[col] = true;
    }
    if drawn[0] && drawn[1] {
        let bottom = col_y
            .iter()
            .copied()
            .filter(|v| *v < f32::MAX)
            .fold(top, f32::max);
        let x = MARGIN + col_w + gutter / 2.0;
        c.fill(x, top + 24.0, x + 1.0, bottom, BLACK);
    }

    // Footer index.
    c.rule(footer_top, 6.0);
    c.rule(footer_top + 12.0, 2.0);
    let inside: Vec<&str> = e.sections.iter().map(|s| s.title.as_str()).collect();
    let line = format!("INSIDE  ·  {}", inside.join("  ·  ").to_uppercase());
    let size = fit(&f.serif, 28.0, &line, inner);
    c.centered(&f.serif, size, footer_top + 64.0, &line, BLACK);

    jpeg(c)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_outlet_label_skips_the_byline() {
        assert_eq!(
            outlet_of("By Sentinel Staff · Sandhills Sentinel · Oct 20, 6:52 PM"),
            Some("Sandhills Sentinel")
        );
        assert_eq!(outlet_of("WXII · Oct 20, 4:04 PM"), Some("WXII"));
    }
}
