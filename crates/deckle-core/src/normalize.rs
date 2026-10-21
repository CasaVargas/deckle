// SPDX-License-Identifier: GPL-3.0-or-later
//! Stage 2: write a cleaned EPUB that Kindle Previewer converts well. v1 scope: JPEG/JFIF cover
//! with the `cover-image` property, image downscaling, script and remote-resource removal, and a
//! synthesized nav TOC when the book has none, and a generated cover when it has none of those
//! either (#27). Everything else passes through byte for byte, unless the opt-in typography
//! pass (#30, [`crate::typography`]) is on.

use std::collections::BTreeMap;
use std::io::{Cursor, Read, Write};
use std::path::Path;

use regex::Regex;
use serde::Serialize;

use crate::epub::{EpubReader, join};
use crate::inspect::InspectError;
use crate::typography::{self, TypographyReport};

#[derive(Clone, Debug)]
pub struct NormalizeOptions {
    /// Longest side above which raster images are downscaled.
    pub max_image_px: u32,
    pub jpeg_quality: u8,
    /// Metadata the Open Library fill found for fields the book leaves empty (issue #28),
    /// added to the OPF; empty adds nothing.
    pub fill: crate::metadata_fill::Metadata,
    /// Curly quotes, dashes, ellipses and empty-paragraph cleanup in the text (issue #30).
    pub typography: bool,
    /// A cover to add when the book has none (issue #27); ignored when it has one.
    pub generated_cover: Option<GeneratedCover>,
}

/// A cover made for a coverless book (`deckle_cover`).
#[derive(Clone, Debug, Default)]
pub struct GeneratedCover {
    /// The template it was made with, for the report.
    pub style: String,
    pub jpeg: Vec<u8>,
}

impl Default for NormalizeOptions {
    fn default() -> Self {
        NormalizeOptions {
            max_image_px: 1920,
            jpeg_quality: 85,
            fill: Default::default(),
            typography: false,
            generated_cover: None,
        }
    }
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize)]
pub struct NormalizeReport {
    pub images_downscaled: u32,
    pub scripts_removed: u32,
    pub remote_refs_removed: u32,
    pub cover_converted_to_jpeg: bool,
    pub toc_synthesized: bool,
    /// What the typography pass changed, when it was on.
    pub typography: Option<TypographyReport>,
    /// The template of the cover added to a coverless book, when one was.
    pub cover_generated: Option<String>,
    pub warnings: Vec<String>,
}

#[derive(Debug, thiserror::Error)]
pub enum NormalizeError {
    #[error(transparent)]
    Inspect(#[from] InspectError),
    #[error("image error in {0}: {1}")]
    Image(String, String),
    #[error("zip error: {0}")]
    Zip(#[from] zip::result::ZipError),
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),
    #[error("XML error: {0}")]
    Xml(String),
}

#[derive(Clone, Debug)]
struct Item {
    id: String,
    href: String,
    media_type: String,
    properties: String,
    /// The literal `<item .../>` text in the OPF, for in-place rewriting.
    tag: String,
}

fn attr_of(tag: &str, name: &str) -> Option<String> {
    let re = Regex::new(&format!(r#"\b{name}\s*=\s*["']([^"']*)["']"#)).ok()?;
    re.captures(tag).map(|c| c[1].to_owned())
}

fn set_attr(tag: &str, name: &str, value: &str) -> String {
    let re = Regex::new(&format!(r#"\b{name}\s*=\s*["'][^"']*["']"#)).unwrap();
    if re.is_match(tag) {
        re.replace(tag, format!(r#"{name}="{value}""#).as_str())
            .into_owned()
    } else {
        // insert before the closing `/>` or `>`
        let insert_at = tag
            .rfind("/>")
            .or_else(|| tag.rfind('>'))
            .unwrap_or(tag.len());
        format!(
            "{} {name}=\"{value}\"{}",
            tag[..insert_at].trim_end(),
            &tag[insert_at..]
        )
    }
}

/// Inserts a JFIF APP0 segment after SOI when the encoder did not write one.
pub fn ensure_jfif(jpeg: Vec<u8>) -> Vec<u8> {
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

fn encode_jpeg(img: &image::DynamicImage, quality: u8) -> Result<Vec<u8>, String> {
    let mut buf = Cursor::new(Vec::new());
    let enc = image::codecs::jpeg::JpegEncoder::new_with_quality(&mut buf, quality);
    img.to_rgb8()
        .write_with_encoder(enc)
        .map_err(|e| e.to_string())?;
    Ok(ensure_jfif(buf.into_inner()))
}

fn encode_png(img: &image::DynamicImage) -> Result<Vec<u8>, String> {
    let mut buf = Cursor::new(Vec::new());
    img.write_to(&mut buf, image::ImageFormat::Png)
        .map_err(|e| e.to_string())?;
    Ok(buf.into_inner())
}

/// Normalizes `src` into `dst`.
pub fn normalize(
    src: &Path,
    dst: &Path,
    opts: &NormalizeOptions,
) -> Result<NormalizeReport, NormalizeError> {
    let mut report = NormalizeReport::default();
    let mut epub = EpubReader::open(src)?;
    let opf_path = epub.opf_path.clone();
    let opf_dir = epub.opf_dir.clone();

    // Load every entry (order preserved).
    let mut order: Vec<String> = Vec::new();
    let mut files: BTreeMap<String, Vec<u8>> = BTreeMap::new();
    for i in 0..epub.zip.len() {
        let mut f = epub.zip.by_index(i)?;
        if f.is_dir() {
            continue;
        }
        let name = f.name()?.into_owned();
        let mut data = Vec::with_capacity(f.size() as usize);
        f.read_to_end(&mut data)?;
        order.push(name.clone());
        files.insert(name, data);
    }

    let mut opf = String::from_utf8(files.get(&opf_path).cloned().unwrap_or_default())
        .map_err(|_| NormalizeError::Xml("OPF is not UTF-8".into()))?;
    let item_re = Regex::new(r"(?s)<item\b[^>]*?/>|<item\b[^>]*?>\s*</item>").unwrap();
    let mut items: Vec<Item> = item_re
        .find_iter(&opf)
        .map(|m| {
            let tag = m.as_str().to_owned();
            Item {
                id: attr_of(&tag, "id").unwrap_or_default(),
                href: attr_of(&tag, "href").unwrap_or_default(),
                media_type: attr_of(&tag, "media-type").unwrap_or_default(),
                properties: attr_of(&tag, "properties").unwrap_or_default(),
                tag,
            }
        })
        .collect();

    // Cover item.
    let cover_meta_re = Regex::new(r#"<meta\b[^>]*\bname\s*=\s*["']cover["'][^>]*>"#).unwrap();
    let cover_meta_id = cover_meta_re
        .find(&opf)
        .and_then(|m| attr_of(m.as_str(), "content"));
    let cover_idx = items
        .iter()
        .position(|it| it.properties.split_whitespace().any(|p| p == "cover-image"))
        .or_else(|| {
            cover_meta_id
                .as_ref()
                .and_then(|cid| items.iter().position(|it| &it.id == cid))
        });

    let xhtml_names: Vec<String> = items
        .iter()
        .filter(|it| it.media_type == "application/xhtml+xml")
        .map(|it| join(&opf_dir, &crate::epub::percent_decode(&it.href)))
        .collect();

    // Images: cover conversion and downscaling.
    let mut renames: Vec<(String, String)> = Vec::new(); // (old href, new href) relative to OPF dir
    for (idx, item) in items.clone().iter().enumerate() {
        let is_raster = matches!(
            item.media_type.as_str(),
            "image/png" | "image/jpeg" | "image/jpg" | "image/gif" | "image/bmp" | "image/webp"
        );
        if !is_raster {
            continue;
        }
        let name = join(&opf_dir, &crate::epub::percent_decode(&item.href));
        let Some(data) = files.get(&name).cloned() else {
            report
                .warnings
                .push(format!("manifest image {} is missing", item.href));
            continue;
        };
        let is_cover = Some(idx) == cover_idx;
        let Ok(mut img) = image::load_from_memory(&data) else {
            report
                .warnings
                .push(format!("could not decode image {}", item.href));
            continue;
        };
        let longest = img.width().max(img.height());
        let mut changed = false;
        if longest > opts.max_image_px {
            let scale = opts.max_image_px as f64 / longest as f64;
            let (w, h) = (
                ((img.width() as f64) * scale).round().max(1.0) as u32,
                ((img.height() as f64) * scale).round().max(1.0) as u32,
            );
            img = img.resize_exact(w, h, image::imageops::FilterType::Lanczos3);
            report.images_downscaled += 1;
            changed = true;
        }
        let is_jpeg = item.media_type == "image/jpeg" || item.media_type == "image/jpg";
        if is_cover && !is_jpeg {
            // Re-encode as JPEG/JFIF and rename the entry.
            let jpeg = encode_jpeg(&img, opts.jpeg_quality)
                .map_err(|e| NormalizeError::Image(item.href.clone(), e))?;
            let new_href = match item.href.rfind('.') {
                Some(i) => format!("{}.jpg", &item.href[..i]),
                None => format!("{}.jpg", item.href),
            };
            let new_name = join(&opf_dir, &new_href);
            files.remove(&name);
            if let Some(pos) = order.iter().position(|n| *n == name) {
                order[pos] = new_name.clone();
            }
            files.insert(new_name, jpeg);
            let mut tag = set_attr(&item.tag, "href", &new_href);
            tag = set_attr(&tag, "media-type", "image/jpeg");
            let props = if item
                .properties
                .split_whitespace()
                .any(|p| p == "cover-image")
            {
                item.properties.clone()
            } else {
                format!("{} cover-image", item.properties).trim().to_owned()
            };
            tag = set_attr(&tag, "properties", &props);
            opf = opf.replace(&item.tag, &tag);
            items[idx].tag = tag;
            items[idx].href = new_href.clone();
            renames.push((item.href.clone(), new_href));
            report.cover_converted_to_jpeg = true;
            continue;
        }
        if is_cover && is_jpeg && data.len() > 4 && data[2..4] != [0xFF, 0xE0] {
            // Make the cover JFIF without re-encoding.
            files.insert(name.clone(), ensure_jfif(data.clone()));
            report.cover_converted_to_jpeg = true;
        }
        if is_cover
            && !item
                .properties
                .split_whitespace()
                .any(|p| p == "cover-image")
        {
            let props = format!("{} cover-image", item.properties).trim().to_owned();
            let tag = set_attr(&item.tag, "properties", &props);
            opf = opf.replace(&item.tag, &tag);
            items[idx].tag = tag;
        }
        if changed {
            let bytes = if is_jpeg {
                encode_jpeg(&img, opts.jpeg_quality)
            } else {
                encode_png(&img)
            }
            .map_err(|e| NormalizeError::Image(item.href.clone(), e))?;
            files.insert(name, bytes);
        }
    }

    // XHTML: scripts and remote resources.
    let script_re = Regex::new(r"(?is)<script\b[^>]*/>|<script\b[^>]*>.*?</script>").unwrap();
    let remote_self_re = Regex::new(r#"(?is)<(img|link|source|iframe|object|embed|audio|video|track)\b[^>]*\b(?:src|href|data)\s*=\s*["']https?://[^"']*["'][^>]*/>"#).unwrap();
    let remote_pair_re = Regex::new(r#"(?is)<(img|link|source|iframe|object|embed|audio|video|track)\b[^>]*\b(?:src|href|data)\s*=\s*["']https?://[^"']*["'][^>]*>.*?</(?:img|link|source|iframe|object|embed|audio|video|track)>"#).unwrap();
    for name in &xhtml_names {
        let Some(bytes) = files.get(name) else {
            continue;
        };
        let Ok(mut text) = String::from_utf8(bytes.clone()) else {
            continue;
        };
        let before = text.clone();
        let n = script_re.find_iter(&text).count();
        if n > 0 {
            text = script_re.replace_all(&text, "").into_owned();
            report.scripts_removed += n as u32;
        }
        let n = remote_self_re.find_iter(&text).count();
        if n > 0 {
            text = remote_self_re.replace_all(&text, "").into_owned();
            report.remote_refs_removed += n as u32;
        }
        let n = remote_pair_re.find_iter(&text).count();
        if n > 0 {
            text = remote_pair_re.replace_all(&text, "").into_owned();
            report.remote_refs_removed += n as u32;
        }
        if text != before {
            files.insert(name.clone(), text.into_bytes());
        }
    }

    if opts.typography {
        let mut counts = TypographyReport::default();
        let book_lang = Regex::new(r"(?is)<dc:language\b[^>]*>([^<]*)</dc:language>")
            .unwrap()
            .captures(&opf)
            .map(|c| c[1].trim().to_owned());
        for name in &xhtml_names {
            let Some(text) = files
                .get(name)
                .and_then(|b| String::from_utf8(b.clone()).ok())
            else {
                continue;
            };
            let lang = typography::document_lang(&text).or_else(|| book_lang.clone());
            let tidied = typography::tidy_xhtml(
                &text,
                typography::curls_quotes(lang.as_deref()),
                &mut counts,
            );
            if tidied != text {
                files.insert(name.clone(), tidied.into_bytes());
            }
        }
        let curl = typography::curls_quotes(book_lang.as_deref());
        for item in items
            .iter()
            .filter(|it| it.media_type == "application/x-dtbncx+xml")
        {
            let name = join(&opf_dir, &crate::epub::percent_decode(&item.href));
            let Some(text) = files
                .get(&name)
                .and_then(|b| String::from_utf8(b.clone()).ok())
            else {
                continue;
            };
            let tidied = typography::tidy_ncx(&text, curl, &mut counts);
            if tidied != text {
                files.insert(name, tidied.into_bytes());
            }
        }
        report.typography = Some(counts);
    }

    // Renamed resources: rewrite every reference in XHTML, SVG and CSS, resolved against the
    // referencing document's own location (publisher layouts nest Text/, Images/, Styles/).
    if !renames.is_empty() {
        let full_renames: Vec<(String, String)> = renames
            .iter()
            .map(|(o, n)| (join(&opf_dir, o), join(&opf_dir, n)))
            .collect();
        let text_like: Vec<String> = order
            .iter()
            .filter(|n| {
                let lower = n.to_lowercase();
                lower.ends_with(".xhtml")
                    || lower.ends_with(".html")
                    || lower.ends_with(".htm")
                    || lower.ends_with(".svg")
                    || lower.ends_with(".css")
            })
            .cloned()
            .collect();
        for name in text_like {
            let Some(bytes) = files.get(&name) else {
                continue;
            };
            let Ok(text) = String::from_utf8(bytes.clone()) else {
                continue;
            };
            let rewritten = rewrite_references(&text, &name, &full_renames);
            if rewritten != text {
                files.insert(name.clone(), rewritten.into_bytes());
            }
        }
    }

    // TOC: synthesize a nav document when there is neither nav nor NCX.
    let has_nav = items
        .iter()
        .any(|it| it.properties.split_whitespace().any(|p| p == "nav"));
    let has_ncx = items
        .iter()
        .any(|it| it.media_type == "application/x-dtbncx+xml");
    if !has_nav && !has_ncx {
        let spine_re =
            Regex::new(r#"<itemref\b[^>]*\bidref\s*=\s*["']([^"']*)["'][^>]*>"#).unwrap();
        let heading_re = Regex::new(r"(?is)<h[1-3]\b[^>]*>(.*?)</h[1-3]>").unwrap();
        let title_re = Regex::new(r"(?is)<title\b[^>]*>(.*?)</title>").unwrap();
        let tag_re = Regex::new(r"(?s)<[^>]*>").unwrap();
        let mut entries = Vec::new();
        for cap in spine_re.captures_iter(&opf) {
            let idref = &cap[1];
            let Some(item) = items.iter().find(|it| it.id == idref) else {
                continue;
            };
            if item.media_type != "application/xhtml+xml" {
                continue;
            }
            let name = join(&opf_dir, &crate::epub::percent_decode(&item.href));
            let text = files
                .get(&name)
                .and_then(|b| String::from_utf8(b.clone()).ok())
                .unwrap_or_default();
            let raw = heading_re
                .captures(&text)
                .or_else(|| title_re.captures(&text))
                .map(|c| c[1].to_owned())
                .unwrap_or_else(|| {
                    item.href
                        .rsplit('/')
                        .next()
                        .unwrap_or(&item.href)
                        .to_owned()
                });
            let label: String = tag_re
                .replace_all(&raw, "")
                .split_whitespace()
                .collect::<Vec<_>>()
                .join(" ");
            let label = if label.is_empty() {
                item.href.clone()
            } else {
                label
            };
            entries.push((label, item.href.clone()));
        }
        if !entries.is_empty() {
            let mut nav = String::from(
                "<?xml version=\"1.0\" encoding=\"utf-8\"?>\n<html xmlns=\"http://www.w3.org/1999/xhtml\" xmlns:epub=\"http://www.idpf.org/2007/ops\"><head><title>Contents</title></head><body><nav epub:type=\"toc\" id=\"toc\"><h1>Contents</h1><ol>",
            );
            for (label, href) in &entries {
                nav.push_str(&format!(
                    "<li><a href=\"{}\">{}</a></li>",
                    xml_escape(href),
                    xml_escape(label)
                ));
            }
            nav.push_str("</ol></nav></body></html>\n");
            let nav_href = "deckle-nav.xhtml";
            let nav_name = join(&opf_dir, nav_href);
            files.insert(nav_name.clone(), nav.into_bytes());
            order.push(nav_name);
            let item_tag = format!(
                "<item id=\"deckle-nav\" href=\"{nav_href}\" media-type=\"application/xhtml+xml\" properties=\"nav\"/>"
            );
            if let Some(pos) = opf.find("</manifest>") {
                opf.insert_str(pos, &item_tag);
            }
            report.toc_synthesized = true;
        }
    }

    if !opts.fill.is_empty() {
        match crate::metadata_fill::add_to_opf(&opf, &opts.fill) {
            Some(filled) => opf = filled,
            None => report
                .warnings
                .push("metadata fill not written: the OPF has no </metadata>".into()),
        }
    }

    if let (None, Some(cover)) = (cover_idx, &opts.generated_cover) {
        match add_generated_cover(&opf, &opf_dir, cover, &mut files, &mut order) {
            Some(with_cover) => {
                opf = with_cover;
                report.cover_generated = Some(cover.style.clone());
            }
            None => report
                .warnings
                .push("generated cover not added: the OPF has no manifest or spine".into()),
        }
    }

    files.insert(opf_path.clone(), opf.into_bytes());

    // Write: mimetype first and stored, everything else deflated.
    let out = std::fs::File::create(dst)?;
    let mut zw = zip::ZipWriter::new(out);
    let stored =
        zip::write::SimpleFileOptions::default().compression_method(zip::CompressionMethod::Stored);
    let deflated = zip::write::SimpleFileOptions::default()
        .compression_method(zip::CompressionMethod::Deflated);
    zw.start_file("mimetype", stored)?;
    zw.write_all(
        files
            .get("mimetype")
            .map(Vec::as_slice)
            .unwrap_or(b"application/epub+zip"),
    )?;
    for name in &order {
        if name == "mimetype" {
            continue;
        }
        let Some(data) = files.get(name) else {
            continue;
        };
        zw.start_file(name, deflated)?;
        zw.write_all(data)?;
    }
    zw.finish()?;
    Ok(report)
}

const COVER_IMAGE: &str = "deckle-cover.jpg";
const COVER_PAGE: &str = "deckle-cover.xhtml";

/// Adds `cover` as the book's cover: a `cover-image` JPEG, `<meta name="cover">` for EPUB 2
/// readers, and a cover page first in the spine. `None` when the OPF lacks a place for them.
fn add_generated_cover(
    opf: &str,
    opf_dir: &str,
    cover: &GeneratedCover,
    files: &mut BTreeMap<String, Vec<u8>>,
    order: &mut Vec<String>,
) -> Option<String> {
    let manifest = opf.find("</manifest>")?;
    let metadata = opf.find("</metadata>")?;
    let spine = Regex::new(r"<spine\b[^>]*>").unwrap().find(opf)?.end();
    let mut out = opf.to_owned();
    // Back to front, so the earlier offsets stay valid.
    let mut edits = [
        (
            manifest,
            format!(
                "<item id=\"deckle-cover-image\" href=\"{COVER_IMAGE}\" media-type=\"image/jpeg\" properties=\"cover-image\"/>\
                 <item id=\"deckle-cover\" href=\"{COVER_PAGE}\" media-type=\"application/xhtml+xml\"/>"
            ),
        ),
        (
            metadata,
            "<meta name=\"cover\" content=\"deckle-cover-image\"/>".to_owned(),
        ),
        (spine, "<itemref idref=\"deckle-cover\"/>".to_owned()),
    ];
    edits.sort_by_key(|(at, _)| std::cmp::Reverse(*at));
    for (at, text) in edits {
        out.insert_str(at, &text);
    }
    let page = format!(
        "<?xml version=\"1.0\" encoding=\"utf-8\"?>\n<html xmlns=\"http://www.w3.org/1999/xhtml\" xmlns:epub=\"http://www.idpf.org/2007/ops\"><head><title>Cover</title><style>body{{margin:0;padding:0;text-align:center}}img{{max-width:100%;max-height:100%}}</style></head><body epub:type=\"cover\"><img src=\"{COVER_IMAGE}\" alt=\"Cover\"/></body></html>\n"
    );
    for (href, data) in [
        (COVER_IMAGE, ensure_jfif(cover.jpeg.clone())),
        (COVER_PAGE, page.into_bytes()),
    ] {
        let name = join(opf_dir, href);
        order.push(name.clone());
        files.insert(name, data);
    }
    Some(out)
}

fn xml_escape(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}

/// Directory part of a zip entry name (`OEBPS/Text/x.xhtml` → `OEBPS/Text`).
fn dir_of(name: &str) -> &str {
    match name.rfind('/') {
        Some(i) => &name[..i],
        None => "",
    }
}

/// Relative path from `from_dir` to `to` (both zip-style, `/`-separated).
pub fn relative_path(from_dir: &str, to: &str) -> String {
    let from: Vec<&str> = from_dir.split('/').filter(|p| !p.is_empty()).collect();
    let to_parts: Vec<&str> = to.split('/').filter(|p| !p.is_empty()).collect();
    let mut common = 0;
    while common < from.len()
        && common < to_parts.len().saturating_sub(1)
        && from[common] == to_parts[common]
    {
        common += 1;
    }
    let mut out: Vec<String> = vec!["..".to_owned(); from.len() - common];
    out.extend(to_parts[common..].iter().map(|s| (*s).to_owned()));
    out.join("/")
}

/// Rewrites `src`/`href`/`xlink:href` attributes and CSS `url()` values in `text` (the content
/// of zip entry `doc_name`) that resolve to a renamed entry.
pub fn rewrite_references(text: &str, doc_name: &str, renames: &[(String, String)]) -> String {
    let doc_dir = dir_of(doc_name);
    let attr_re =
        Regex::new(r#"(?i)\b(src|href|xlink:href)(\s*=\s*)(["'])([^"']*)(["'])"#).unwrap();
    let url_re = Regex::new(r#"(?i)url\(\s*(['"]?)([^'")]+)(['"]?)\s*\)"#).unwrap();
    let map_value = |value: &str| -> Option<String> {
        if value.starts_with("http://")
            || value.starts_with("https://")
            || value.starts_with("data:")
            || value.starts_with('#')
        {
            return None;
        }
        let (path, frag) = match value.split_once('#') {
            Some((p, f)) => (p, Some(f)),
            None => (value, None),
        };
        let resolved = join(doc_dir, &crate::epub::percent_decode(path));
        let (_, new) = renames.iter().find(|(old, _)| *old == resolved)?;
        let mut rel = relative_path(doc_dir, new);
        if let Some(f) = frag {
            rel.push('#');
            rel.push_str(f);
        }
        Some(rel)
    };
    let text = attr_re.replace_all(text, |c: &regex::Captures| match map_value(&c[4]) {
        Some(new) => format!("{}{}{}{}{}", &c[1], &c[2], &c[3], new, &c[5]),
        None => c[0].to_owned(),
    });
    url_re
        .replace_all(&text, |c: &regex::Captures| match map_value(c[2].trim()) {
            Some(new) => format!("url({}{}{})", &c[1], new, &c[3]),
            None => c[0].to_owned(),
        })
        .into_owned()
}
