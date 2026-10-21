// SPDX-License-Identifier: GPL-3.0-or-later
//! A typographic cover for a book that has none (cover design, issue #27): the title, author
//! and series set on white in one of three templates, as a 1264 × 1680 grayscale JPEG. Pure:
//! metadata in, JPEG bytes out, with bundled OFL fonts only (Old Standard, Inter) so a cover
//! comes out the same on every machine.

use ab_glyph::{Font, FontRef, PxScale, ScaleFont, point};
use image::{GrayImage, Luma};

pub const WIDTH: u32 = 1264;
pub const HEIGHT: u32 = 1680;
const MARGIN: f32 = 64.0;
const INNER: f32 = WIDTH as f32 - 2.0 * MARGIN;
const BLACK: u8 = 0;
const WHITE: u8 = 255;
const JPEG_QUALITY: u8 = 90;
/// Text is never cut off: below a template's smallest size it keeps shrinking, down to this.
const FLOOR: f32 = 24.0;
/// A template's size range runs from its largest size down to this share of it (160 → 96 px).
const MIN_SHARE: f32 = 0.6;

const SERIF: &[u8] = include_bytes!("../fonts/OldStandard-Regular.ttf");
const SERIF_BOLD: &[u8] = include_bytes!("../fonts/OldStandard-Bold.ttf");
const SERIF_ITALIC: &[u8] = include_bytes!("../fonts/OldStandard-Italic.ttf");
const SANS: &[u8] = include_bytes!("../fonts/Inter-Regular.ttf");
const SANS_BOLD: &[u8] = include_bytes!("../fonts/Inter-Bold.ttf");

pub const TITLE_NOT_RENDERABLE: &str = "cover title not renderable in the bundled fonts";

/// What goes on the cover.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct CoverText {
    /// `dc:title`; a subtitle after the first `:` is set smaller on its own line.
    pub title: String,
    /// `dc:creator`s, in order.
    pub authors: Vec<String>,
    /// The series line, e.g. "Hainish Cycle · 5" ([`series_line`]).
    pub series: Option<String>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Template {
    /// Old Standard, centred: title, a thin rule, the author in small caps, the series at the foot.
    Classic,
    /// A black band across the upper third with the title reversed out in Inter Bold.
    Band,
    /// Inter Bold ranged left against a heavy left rule; author and series at the foot.
    Rule,
}

impl Template {
    pub const ALL: [Template; 3] = [Template::Classic, Template::Band, Template::Rule];

    pub fn name(self) -> &'static str {
        match self {
            Template::Classic => "classic",
            Template::Band => "band",
            Template::Rule => "rule",
        }
    }

    /// The template for the book with this identifier: a stable hash (FNV-1a), so the same
    /// book always gets the same cover and a shelf of documents doesn't all look alike.
    pub fn for_book(identifier: &str) -> Template {
        let mut h: u64 = 0xcbf2_9ce4_8422_2325;
        for b in identifier.bytes() {
            h ^= u64::from(b);
            h = h.wrapping_mul(0x0000_0100_0000_01b3);
        }
        Template::ALL[(h % Template::ALL.len() as u64) as usize]
    }

    /// [`Template::for_book`], unless that template's fonts can't set this title and another
    /// template's can: Old Standard has Latin and Cyrillic, Inter Greek as well.
    pub fn choose(identifier: &str, text: &CoverText) -> Template {
        let first = Template::for_book(identifier);
        let Ok(f) = fonts() else {
            return first;
        };
        let at = Template::ALL.iter().position(|t| *t == first).unwrap_or(0);
        (0..Template::ALL.len())
            .map(|i| Template::ALL[(at + i) % Template::ALL.len()])
            .find(|t| renderable(&text.title, &f.of(*t)))
            .unwrap_or(first)
    }
}

impl std::str::FromStr for Template {
    type Err = String;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Template::ALL
            .into_iter()
            .find(|t| t.name() == s)
            .ok_or_else(|| format!("unknown cover style {s:?}: classic, band or rule"))
    }
}

/// A rendered cover.
#[derive(Clone, Debug)]
pub struct Cover {
    pub jpeg: Vec<u8>,
    pub template: Template,
    /// Lines left off because the bundled fonts can't draw them.
    pub warnings: Vec<String>,
}

/// "Hainish Cycle · 5"; a fractional index keeps its decimals ("Discworld · 1.5").
pub fn series_line(name: &str, index: Option<f32>) -> String {
    match index {
        Some(i) if i.fract() == 0.0 => format!("{name} · {}", i as i64),
        Some(i) => format!("{name} · {i}"),
        None => name.to_owned(),
    }
}

/// Up to two names joined by "and", otherwise "First Author et al.".
pub fn authors_line(authors: &[String]) -> String {
    let names: Vec<&str> = authors
        .iter()
        .map(|a| a.trim())
        .filter(|a| !a.is_empty())
        .collect();
    match names.as_slice() {
        [] => String::new(),
        [one] => (*one).to_owned(),
        [a, b] => format!("{a} and {b}"),
        [first, ..] => format!("{first} et al."),
    }
}

/// The title and its subtitle (after the first `:`, when both halves have text).
fn split_title(title: &str) -> (String, Option<String>) {
    let tidy = |s: &str| s.split_whitespace().collect::<Vec<_>>().join(" ");
    match title.split_once(':') {
        Some((main, sub)) if !main.trim().is_empty() && !sub.trim().is_empty() => {
            (tidy(main), Some(tidy(sub)))
        }
        _ => (tidy(title), None),
    }
}

struct Fonts<'a> {
    serif: FontRef<'a>,
    serif_bold: FontRef<'a>,
    serif_italic: FontRef<'a>,
    sans: FontRef<'a>,
    sans_bold: FontRef<'a>,
}

impl<'f> Fonts<'f> {
    /// The faces `template` sets its text in.
    fn of(&self, template: Template) -> Vec<&FontRef<'f>> {
        match template {
            Template::Classic => vec![&self.serif_bold, &self.serif_italic, &self.serif],
            Template::Band | Template::Rule => vec![&self.sans_bold, &self.sans],
        }
    }
}

fn fonts() -> Result<Fonts<'static>, String> {
    let font = |b| FontRef::try_from_slice(b).map_err(|e| e.to_string());
    Ok(Fonts {
        serif: font(SERIF)?,
        serif_bold: font(SERIF_BOLD)?,
        serif_italic: font(SERIF_ITALIC)?,
        sans: font(SANS)?,
        sans_bold: font(SANS_BOLD)?,
    })
}

/// Whether every letter and digit in `text` has a glyph in all of `fonts`. Other characters
/// a font lacks (stray symbols) are skipped when drawn rather than shown as boxes.
fn renderable(text: &str, fonts: &[&FontRef]) -> bool {
    text.chars()
        .filter(|c| c.is_alphanumeric())
        .all(|c| fonts.iter().all(|f| f.glyph_id(c).0 != 0))
}

/// One run of text at one size (small caps mix two).
struct Run<'a, 'f> {
    font: &'a FontRef<'f>,
    size: f32,
    text: String,
}

/// Small caps: capitals at `size`, lower case as capitals at 80 % of it.
fn small_caps<'a, 'f>(font: &'a FontRef<'f>, size: f32, text: &str) -> Vec<Run<'a, 'f>> {
    let mut runs: Vec<Run> = Vec::new();
    for c in text.chars() {
        let lower = c.is_lowercase();
        let s = if lower { size * 0.8 } else { size };
        let up: String = c.to_uppercase().collect();
        match runs.last_mut() {
            Some(r) if r.size == s => r.text.push_str(&up),
            _ => runs.push(Run {
                font,
                size: s,
                text: up,
            }),
        }
    }
    runs
}

fn width_of(font: &FontRef, size: f32, text: &str, tracking: f32) -> f32 {
    let f = font.as_scaled(PxScale::from(size));
    let mut w = 0.0;
    let mut prev = None;
    for c in text.chars() {
        let id = f.glyph_id(c);
        if id.0 == 0 {
            continue;
        }
        if let Some(p) = prev {
            w += f.kern(p, id) + tracking;
        }
        w += f.h_advance(id);
        prev = Some(id);
    }
    w
}

fn runs_width(runs: &[Run], tracking: f32) -> f32 {
    let gaps = runs.len().saturating_sub(1) as f32 * tracking;
    runs.iter()
        .map(|r| width_of(r.font, r.size, &r.text, tracking))
        .sum::<f32>()
        + gaps
}

/// Greedy word wrap to `width`; a word wider than `width` gets a line to itself.
fn wrap(font: &FontRef, size: f32, text: &str, width: f32) -> Vec<String> {
    let mut lines: Vec<String> = Vec::new();
    let mut line = String::new();
    for word in text.split_whitespace() {
        let candidate = if line.is_empty() {
            word.to_owned()
        } else {
            format!("{line} {word}")
        };
        if line.is_empty() || width_of(font, size, &candidate, 0.0) <= width {
            line = candidate;
        } else {
            lines.push(std::mem::replace(&mut line, word.to_owned()));
        }
    }
    if !line.is_empty() {
        lines.push(line);
    }
    lines
}

/// Text set in a block: the size it fits at and its lines.
struct Block {
    size: f32,
    lines: Vec<String>,
}

impl Block {
    fn height(&self, leading: f32) -> f32 {
        self.size * leading * self.lines.len() as f32
    }
}

/// The largest size from `max` down at which `text` wraps into lines no wider than `width` and
/// no taller than `max_height`: at most `max_lines` of them down to [`MIN_SHARE`] of `max` (the
/// template's range), then as many as fit. Nothing is ever cut off (design §2).
fn fit(
    font: &FontRef,
    text: &str,
    max: f32,
    width: f32,
    max_lines: usize,
    max_height: f32,
    leading: f32,
) -> Block {
    let mut size = max;
    loop {
        let lines = wrap(font, size, text, width);
        let block = Block { size, lines };
        // Below its range a long title takes more lines rather than shrink out of legibility.
        let fits = (block.lines.len() <= max_lines || size < max * MIN_SHARE)
            && block.height(leading) <= max_height
            && block
                .lines
                .iter()
                .all(|l| width_of(font, size, l, 0.0) <= width);
        if fits || size <= FLOOR {
            return block;
        }
        size -= 2.0;
    }
}

struct Canvas {
    img: GrayImage,
}

impl Canvas {
    fn new() -> Self {
        Canvas {
            img: GrayImage::from_pixel(WIDTH, HEIGHT, Luma([WHITE])),
        }
    }

    /// Draws `text` in `ink` with its baseline at `y`, starting at `x`; returns the caret.
    #[allow(clippy::too_many_arguments)]
    fn text(
        &mut self,
        font: &FontRef,
        size: f32,
        x: f32,
        y: f32,
        text: &str,
        ink: u8,
        tracking: f32,
    ) -> f32 {
        let scale = PxScale::from(size);
        let f = font.as_scaled(scale);
        let mut caret = x;
        let mut prev = None;
        for c in text.chars() {
            let id = f.glyph_id(c);
            if id.0 == 0 {
                continue;
            }
            if let Some(p) = prev {
                caret += f.kern(p, id) + tracking;
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
        caret
    }

    fn runs(&mut self, runs: &[Run], x: f32, y: f32, ink: u8, tracking: f32) {
        let mut caret = x;
        for r in runs {
            caret = self.text(r.font, r.size, caret, y, &r.text, ink, tracking) + tracking;
        }
    }

    fn centered(&mut self, font: &FontRef, size: f32, y: f32, text: &str, ink: u8) {
        let w = width_of(font, size, text, 0.0);
        self.text(font, size, (WIDTH as f32 - w) / 2.0, y, text, ink, 0.0);
    }

    fn fill(&mut self, x0: f32, y0: f32, x1: f32, y1: f32, ink: u8) {
        for y in y0.max(0.0) as u32..(y1 as u32).min(HEIGHT) {
            for x in x0.max(0.0) as u32..(x1 as u32).min(WIDTH) {
                self.img.put_pixel(x, y, Luma([ink]));
            }
        }
    }

    /// A rectangular outline `thickness` wide, inset `inset` from the edges.
    fn frame(&mut self, inset: f32, thickness: f32) {
        let (w, h) = (WIDTH as f32, HEIGHT as f32);
        self.fill(inset, inset, w - inset, inset + thickness, BLACK);
        self.fill(inset, h - inset - thickness, w - inset, h - inset, BLACK);
        self.fill(inset, inset, inset + thickness, h - inset, BLACK);
        self.fill(w - inset - thickness, inset, w - inset, h - inset, BLACK);
    }
}

/// The lines a template sets, with those the fonts can't draw already left off.
struct Lines {
    title: Option<String>,
    subtitle: Option<String>,
    author: Option<String>,
    series: Option<String>,
}

fn lines_for(text: &CoverText, fonts: &[&FontRef], warnings: &mut Vec<String>) -> Lines {
    let keep = |s: String| (!s.trim().is_empty() && renderable(&s, fonts)).then_some(s);
    let (title, subtitle) = split_title(&text.title);
    let whole = format!("{title} {}", subtitle.as_deref().unwrap_or_default());
    let (title, subtitle) = if renderable(&whole, fonts) {
        (keep(title), subtitle.and_then(keep))
    } else {
        warnings.push(TITLE_NOT_RENDERABLE.to_owned());
        (None, None)
    };
    Lines {
        title,
        subtitle,
        author: keep(authors_line(&text.authors)),
        series: text.series.clone().and_then(keep),
    }
}

/// Old Standard, centred: a double frame, the title in the upper middle, a thin rule, the
/// author in small caps, the series in italics near the foot.
fn classic(f: &Fonts, l: &Lines) -> Canvas {
    let mut c = Canvas::new();
    c.frame(32.0, 4.0);
    c.frame(46.0, 1.0);
    let width = INNER - 48.0;
    let title = l
        .title
        .as_deref()
        .map(|t| fit(&f.serif_bold, t, 160.0, width, 4, 760.0, 1.12));
    let subtitle = l.subtitle.as_deref().map(|s| {
        let max = title
            .as_ref()
            .map_or(64.0, |t| (t.size * 0.45).clamp(44.0, 64.0));
        fit(&f.serif_italic, s, max, width, 3, 240.0, 1.25)
    });
    let block = title.as_ref().map_or(0.0, |t| t.height(1.12))
        + subtitle.as_ref().map_or(0.0, |s| 28.0 + s.height(1.25));
    // The title block sits on a line at 42 % of the height, in the upper middle.
    let mut y = (HEIGHT as f32 * 0.42 - block / 2.0).max(150.0);
    if let Some(t) = &title {
        for line in &t.lines {
            y += t.size * 1.12;
            c.centered(&f.serif_bold, t.size, y - t.size * 0.24, line, BLACK);
        }
    }
    if let Some(s) = &subtitle {
        y += 28.0;
        for line in &s.lines {
            y += s.size * 1.25;
            c.centered(&f.serif_italic, s.size, y - s.size * 0.3, line, BLACK);
        }
    }
    y += 56.0;
    let rule = 220.0;
    c.fill(
        (WIDTH as f32 - rule) / 2.0,
        y,
        (WIDTH as f32 + rule) / 2.0,
        y + 3.0,
        BLACK,
    );
    if let Some(a) = &l.author {
        let mut size = 64.0;
        let mut runs = small_caps(&f.serif, size, a);
        while runs_width(&runs, 4.0) > width && size > FLOOR {
            size -= 2.0;
            runs = small_caps(&f.serif, size, a);
        }
        let w = runs_width(&runs, 4.0);
        c.runs(&runs, (WIDTH as f32 - w) / 2.0, y + 56.0 + size, BLACK, 4.0);
    }
    if let Some(s) = &l.series {
        let size = fit(&f.serif_italic, s, 46.0, width, 1, 60.0, 1.0).size;
        c.centered(&f.serif_italic, size, HEIGHT as f32 - 130.0, s, BLACK);
    }
    c
}

/// A black band across the upper third with the title reversed out in white Inter Bold, the
/// series along the band's lower edge, the author underneath.
fn band(f: &Fonts, l: &Lines) -> Canvas {
    let mut c = Canvas::new();
    let (top, bottom) = (180.0, 900.0);
    c.fill(0.0, top, WIDTH as f32, bottom, BLACK);
    let series_room = if l.series.is_some() { 90.0 } else { 0.0 };
    let room = bottom - top - 120.0 - series_room;
    let title = l
        .title
        .as_deref()
        .map(|t| fit(&f.sans_bold, t, 140.0, INNER, 4, room * 0.8, 1.08));
    let used = title.as_ref().map_or(0.0, |t| t.height(1.08));
    let subtitle = l.subtitle.as_deref().map(|s| {
        let max = title
            .as_ref()
            .map_or(56.0, |t| (t.size * 0.42).clamp(36.0, 56.0));
        fit(
            &f.sans,
            s,
            max,
            INNER,
            3,
            (room - used - 24.0).max(60.0),
            1.25,
        )
    });
    let block = used + subtitle.as_ref().map_or(0.0, |s| 24.0 + s.height(1.25));
    let mut y = top + 60.0 + ((room - block) / 2.0).max(0.0);
    if let Some(t) = &title {
        for line in &t.lines {
            y += t.size * 1.08;
            c.centered(&f.sans_bold, t.size, y - t.size * 0.22, line, WHITE);
        }
    }
    if let Some(s) = &subtitle {
        y += 24.0;
        for line in &s.lines {
            y += s.size * 1.25;
            c.centered(&f.sans, s.size, y - s.size * 0.3, line, WHITE);
        }
    }
    if let Some(s) = &l.series {
        let label = s.to_uppercase();
        let size = fit(&f.sans, &label, 34.0, INNER, 1, 50.0, 1.0).size;
        let w = width_of(&f.sans, size, &label, 3.0);
        c.text(
            &f.sans,
            size,
            (WIDTH as f32 - w) / 2.0,
            bottom - 56.0,
            &label,
            WHITE,
            3.0,
        );
    }
    if let Some(a) = &l.author {
        let block = fit(&f.sans, a, 64.0, INNER, 2, 160.0, 1.2);
        let mut y = bottom + 130.0;
        for line in &block.lines {
            c.centered(&f.sans, block.size, y, line, BLACK);
            y += block.size * 1.2;
        }
    }
    c
}

/// Inter Bold ranged left against a heavy left rule; the series above the author at the foot.
fn rule(f: &Fonts, l: &Lines) -> Canvas {
    let mut c = Canvas::new();
    let (top, foot) = (128.0, HEIGHT as f32 - 128.0);
    c.fill(MARGIN, top, MARGIN + 24.0, foot, BLACK);
    let x = MARGIN + 24.0 + 52.0;
    let width = WIDTH as f32 - MARGIN - x;
    let mut y = top + 40.0;
    let title = l
        .title
        .as_deref()
        .map(|t| fit(&f.sans_bold, t, 150.0, width, 5, 820.0, 1.06));
    if let Some(t) = &title {
        for line in &t.lines {
            y += t.size * 1.06;
            c.text(&f.sans_bold, t.size, x, y - t.size * 0.18, line, BLACK, 0.0);
        }
    }
    if let Some(s) = &l.subtitle {
        let max = title
            .as_ref()
            .map_or(56.0, |t| (t.size * 0.4).clamp(36.0, 56.0));
        let block = fit(&f.sans, s, max, width, 4, 300.0, 1.25);
        y += 30.0;
        for line in &block.lines {
            y += block.size * 1.25;
            c.text(
                &f.sans,
                block.size,
                x,
                y - block.size * 0.3,
                line,
                BLACK,
                0.0,
            );
        }
    }
    let mut base = foot;
    if let Some(a) = &l.author {
        let block = fit(&f.sans, a, 64.0, width, 2, 160.0, 1.2);
        for line in block.lines.iter().rev() {
            c.text(&f.sans, block.size, x, base, line, BLACK, 0.0);
            base -= block.size * 1.2;
        }
        base -= 24.0;
    }
    if let Some(s) = &l.series {
        let label = s.to_uppercase();
        let size = fit(&f.sans_bold, &label, 32.0, width, 1, 50.0, 1.0).size;
        c.text(&f.sans_bold, size, x, base, &label, BLACK, 3.0);
    }
    c
}

/// The cover as a grayscale image, with warnings for lines left off.
pub fn render_image(
    text: &CoverText,
    template: Template,
) -> Result<(GrayImage, Vec<String>), String> {
    let f = fonts()?;
    let mut warnings = Vec::new();
    let lines = lines_for(text, &f.of(template), &mut warnings);
    let canvas = match template {
        Template::Classic => classic(&f, &lines),
        Template::Band => band(&f, &lines),
        Template::Rule => rule(&f, &lines),
    };
    Ok((canvas.img, warnings))
}

/// The cover as a grayscale JPEG.
pub fn render(text: &CoverText, template: Template) -> Result<Cover, String> {
    let (img, warnings) = render_image(text, template)?;
    let mut jpeg = Vec::new();
    image::DynamicImage::ImageLuma8(img)
        .write_with_encoder(image::codecs::jpeg::JpegEncoder::new_with_quality(
            &mut jpeg,
            JPEG_QUALITY,
        ))
        .map_err(|e| e.to_string())?;
    Ok(Cover {
        jpeg,
        template,
        warnings,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_subtitle_splits_at_the_first_colon() {
        assert_eq!(
            split_title("Thinking in Systems:  A Primer"),
            ("Thinking in Systems".into(), Some("A Primer".into()))
        );
        assert_eq!(split_title("Re: Zero"), ("Re".into(), Some("Zero".into())));
        assert_eq!(split_title("Ends with:"), ("Ends with:".into(), None));
    }

    #[test]
    fn authors_join_two_and_shorten_more() {
        let a = |v: &[&str]| authors_line(&v.iter().map(|s| (*s).to_owned()).collect::<Vec<_>>());
        assert_eq!(a(&[]), "");
        assert_eq!(a(&["Ann Leckie"]), "Ann Leckie");
        assert_eq!(
            a(&["Terry Pratchett", "Neil Gaiman"]),
            "Terry Pratchett and Neil Gaiman"
        );
        assert_eq!(a(&["A", "B", "C"]), "A et al.");
    }

    #[test]
    fn fitting_shrinks_rather_than_cuts_off() {
        let f = fonts().unwrap();
        let long = "The Remarkable and Entirely Unabridged Account of a Voyage Around the Known \
                    World in Eighty Days, with Notes, Maps, Charts and a Very Long Appendix";
        assert!(long.len() > 120);
        let b = fit(&f.serif_bold, long, 160.0, INNER, 4, 760.0, 1.12);
        assert!(
            b.lines.len() > 4,
            "below 96 px it takes more lines: {:?}",
            b.lines
        );
        assert!(b.size >= 90.0, "and stays legible: {}", b.size);
        assert!(b.height(1.12) <= 760.0);
        assert_eq!(b.lines.join(" "), long, "every word is kept");
        for l in &b.lines {
            assert!(width_of(&f.serif_bold, b.size, l, 0.0) <= INNER, "{l}");
        }
        let short = fit(&f.serif_bold, "Dune", 160.0, INNER, 4, 760.0, 1.12);
        assert_eq!((short.size, short.lines.len()), (160.0, 1));
    }

    #[test]
    fn cjk_isnt_in_the_bundled_fonts_cyrillic_is_and_greek_is_in_inter() {
        let f = fonts().unwrap();
        for t in Template::ALL {
            assert!(!renderable("吾輩は猫である", &f.of(t)));
            assert!(renderable("Война и мир", &f.of(t)));
        }
        assert!(renderable("Ὀδύσσεια", &f.of(Template::Band)));
        assert!(!renderable("Ὀδύσσεια", &f.of(Template::Classic)));
    }
}
