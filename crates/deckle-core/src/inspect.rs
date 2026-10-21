// SPDX-License-Identifier: GPL-3.0-or-later
//! Stage 1: read an EPUB's metadata and structure, refuse what we cannot convert.

use std::path::Path;

use serde::{Deserialize, Serialize};

use crate::epub::EpubReader;

#[derive(Debug, thiserror::Error)]
pub enum InspectError {
    #[error("not a zip/EPUB file")]
    NotZip,
    #[error("EPUB has no META-INF/container.xml")]
    NoContainerXml,
    #[error("EPUB has no package document (OPF)")]
    NoOpf,
    #[error(
        "EPUB is DRM-protected (META-INF/encryption.xml); only DRM-free books can be converted"
    )]
    Drm,
    #[error("XML error: {0}")]
    Xml(String),
    #[error("missing file in EPUB: {0}")]
    MissingFile(String),
    #[error("I/O error: {0}")]
    Io(std::io::Error),
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Creator {
    pub name: String,
    pub file_as: Option<String>,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Series {
    pub name: String,
    pub index: Option<f32>,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct TocEntry {
    pub title: String,
    pub href: String,
    pub depth: u8,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct BookInfo {
    pub title: String,
    pub creators: Vec<Creator>,
    pub language: String,
    pub identifiers: Vec<String>,
    pub isbn: Option<String>,
    pub series: Option<Series>,
    /// `dc:description`, as written (it may hold escaped HTML).
    pub description: Option<String>,
    /// `dc:subject`s, in order.
    pub subjects: Vec<String>,
    /// Cover image href relative to the OPF directory.
    pub cover_href: Option<String>,
    /// Spine item hrefs relative to the OPF directory, in reading order.
    pub spine: Vec<String>,
    pub toc: Vec<TocEntry>,
    pub epub_version: String,
    pub warnings: Vec<String>,
}

/// The warning `inspect` gives when the title is the file name; the metadata fill treats such
/// a title as empty.
pub const NO_TITLE_WARNING: &str = "no dc:title; using the file name";

fn text_of(n: roxmltree::Node) -> String {
    n.text().map(str::trim).unwrap_or("").to_owned()
}

fn attr<'a>(n: roxmltree::Node<'a, 'a>, name: &str) -> Option<&'a str> {
    n.attributes().find(|a| a.name() == name).map(|a| a.value())
}

/// Parses the EPUB at `path`.
pub fn inspect(path: &Path) -> Result<BookInfo, InspectError> {
    let mut epub = EpubReader::open(path)?;
    let opf = epub.read_string(&epub.opf_path.clone())?;
    let doc = roxmltree::Document::parse(&opf).map_err(|e| InspectError::Xml(e.to_string()))?;
    let package = doc.root_element();
    let mut info = BookInfo {
        epub_version: attr(package, "version").unwrap_or("2.0").to_owned(),
        ..Default::default()
    };

    let Some(metadata) = package.children().find(|n| n.has_tag_name("metadata")) else {
        return Err(InspectError::Xml("OPF has no metadata element".into()));
    };
    let Some(manifest) = package.children().find(|n| n.has_tag_name("manifest")) else {
        return Err(InspectError::Xml("OPF has no manifest element".into()));
    };
    let spine_el = package.children().find(|n| n.has_tag_name("spine"));

    // refinements: id -> [(property, text)]
    let refines = |id: &str, property: &str| -> Option<String> {
        let frag = format!("#{id}");
        metadata
            .children()
            .filter(|n| n.has_tag_name("meta"))
            .find(|m| {
                attr(*m, "refines") == Some(frag.as_str()) && attr(*m, "property") == Some(property)
            })
            .map(text_of)
    };

    for n in metadata.children().filter(|n| n.is_element()) {
        match n.tag_name().name() {
            "title" => {
                if info.title.is_empty() {
                    info.title = text_of(n);
                }
            }
            "creator" => {
                let name = text_of(n);
                if name.is_empty() {
                    continue;
                }
                let role = attr(n, "role")
                    .map(str::to_owned)
                    .or_else(|| attr(n, "id").and_then(|id| refines(id, "role")));
                if let Some(r) = &role
                    && r != "aut"
                {
                    continue;
                }
                let file_as = attr(n, "file-as")
                    .map(str::to_owned)
                    .or_else(|| attr(n, "id").and_then(|id| refines(id, "file-as")));
                info.creators.push(Creator { name, file_as });
            }
            "description" => {
                let d = text_of(n);
                if info.description.is_none() && !d.is_empty() {
                    info.description = Some(d);
                }
            }
            "subject" => {
                let s = text_of(n);
                if !s.is_empty() {
                    info.subjects.push(s);
                }
            }
            "language" => {
                if info.language.is_empty() {
                    info.language = text_of(n);
                }
            }
            "identifier" => {
                let value = text_of(n);
                if value.is_empty() {
                    continue;
                }
                let scheme = attr(n, "scheme").map(str::to_lowercase).or_else(|| {
                    attr(n, "id")
                        .and_then(|id| refines(id, "identifier-type"))
                        .map(|s| s.to_lowercase())
                });
                let digits: String = value
                    .chars()
                    .filter(|c| c.is_ascii_digit() || *c == 'X' || *c == 'x')
                    .collect();
                let looks_isbn = value.to_lowercase().contains("isbn")
                    || scheme.as_deref() == Some("isbn")
                    || scheme.as_deref() == Some("15");
                if looks_isbn && (digits.len() == 10 || digits.len() == 13) && info.isbn.is_none() {
                    info.isbn = Some(digits.to_uppercase());
                }
                info.identifiers.push(value);
            }
            "meta" => {
                let name = attr(n, "name").unwrap_or("");
                let content = attr(n, "content").unwrap_or("");
                match name {
                    "calibre:series" => {
                        let s = info.series.get_or_insert_with(Series::default);
                        if s.name.is_empty() {
                            s.name = content.to_owned();
                        }
                    }
                    "calibre:series_index" => {
                        let s = info.series.get_or_insert_with(Series::default);
                        s.index = content.trim().parse().ok();
                    }
                    _ => {}
                }
                if attr(n, "property") == Some("belongs-to-collection")
                    && let Some(id) = attr(n, "id")
                {
                    let ctype = refines(id, "collection-type");
                    if ctype.as_deref().is_none_or(|t| t == "series") {
                        let name = text_of(n);
                        let index =
                            refines(id, "group-position").and_then(|p| p.trim().parse().ok());
                        info.series = Some(Series { name, index });
                    }
                }
            }
            _ => {}
        }
    }
    if info.series.as_ref().is_some_and(|s| s.name.is_empty()) {
        info.series = None;
    }
    if info.title.is_empty() {
        info.warnings.push(NO_TITLE_WARNING.into());
        info.title = path
            .file_stem()
            .map(|s| s.to_string_lossy().into_owned())
            .unwrap_or_default();
    }
    if info.language.is_empty() {
        info.warnings.push("no dc:language; assuming en".into());
        info.language = "en".into();
    }

    // manifest: id -> (href, media-type, properties)
    let mut items: Vec<(String, String, String, String)> = Vec::new();
    for item in manifest.children().filter(|n| n.has_tag_name("item")) {
        items.push((
            attr(item, "id").unwrap_or("").to_owned(),
            attr(item, "href").unwrap_or("").to_owned(),
            attr(item, "media-type").unwrap_or("").to_owned(),
            attr(item, "properties").unwrap_or("").to_owned(),
        ));
    }
    let cover_meta = metadata
        .children()
        .filter(|n| n.has_tag_name("meta"))
        .find(|m| attr(*m, "name") == Some("cover"))
        .and_then(|m| attr(m, "content"))
        .map(str::to_owned);
    info.cover_href = items
        .iter()
        .find(|(_, _, _, props)| props.split_whitespace().any(|p| p == "cover-image"))
        .or_else(|| {
            cover_meta
                .as_ref()
                .and_then(|cid| items.iter().find(|(id, _, _, _)| id == cid))
        })
        .map(|(_, href, _, _)| href.clone());

    if let Some(spine_el) = spine_el {
        for itemref in spine_el.children().filter(|n| n.has_tag_name("itemref")) {
            let Some(idref) = attr(itemref, "idref") else {
                continue;
            };
            if attr(itemref, "linear") == Some("no") {
                continue;
            }
            if let Some((_, href, _, _)) = items.iter().find(|(id, _, _, _)| id == idref) {
                info.spine.push(href.clone());
            }
        }
        if info.spine.is_empty() {
            info.warnings.push("spine is empty".into());
        }
        // TOC: EPUB 3 nav, else NCX
        let nav_href = items
            .iter()
            .find(|(_, _, _, props)| props.split_whitespace().any(|p| p == "nav"))
            .map(|(_, h, _, _)| h.clone());
        let ncx_href = attr(spine_el, "toc")
            .and_then(|id| items.iter().find(|(iid, _, _, _)| iid == id))
            .map(|(_, h, _, _)| h.clone())
            .or_else(|| {
                items
                    .iter()
                    .find(|(_, _, mt, _)| mt == "application/x-dtbncx+xml")
                    .map(|(_, h, _, _)| h.clone())
            });
        if let Some(nav) = nav_href {
            let full = epub.resolve(&nav);
            match epub.read_string(&full) {
                Ok(xml) => info.toc = parse_nav_toc(&xml, &nav),
                Err(_) => info.warnings.push(format!("nav document {nav} missing")),
            }
        } else if let Some(ncx) = ncx_href {
            let full = epub.resolve(&ncx);
            match epub.read_string(&full) {
                Ok(xml) => info.toc = parse_ncx_toc(&xml, &ncx),
                Err(_) => info.warnings.push(format!("NCX {ncx} missing")),
            }
        }
        if info.toc.is_empty() {
            info.warnings.push("no table of contents".into());
        }
    } else {
        info.warnings.push("OPF has no spine".into());
    }
    Ok(info)
}

/// Href of `target` relative to the OPF directory, given the document (`base`) it appears in.
fn relative_to_opf(base: &str, target: &str) -> String {
    let base_dir = match base.rfind('/') {
        Some(i) => &base[..i + 1],
        None => "",
    };
    let (path, frag) = match target.split_once('#') {
        Some((p, f)) => (p, Some(f)),
        None => (target, None),
    };
    let joined = crate::epub::join(base_dir, &crate::epub::percent_decode(path));
    match frag {
        Some(f) => format!("{joined}#{f}"),
        None => joined,
    }
}

fn parse_nav_toc(xml: &str, nav_href: &str) -> Vec<TocEntry> {
    let Ok(doc) = roxmltree::Document::parse(xml) else {
        return Vec::new();
    };
    let toc_nav = doc
        .descendants()
        .filter(|n| n.has_tag_name("nav"))
        .find(|n| {
            n.attributes()
                .any(|a| a.name() == "type" && a.value().split_whitespace().any(|t| t == "toc"))
        });
    let Some(toc_nav) = toc_nav else {
        return Vec::new();
    };
    let mut out = Vec::new();
    fn walk(ol: roxmltree::Node, depth: u8, nav_href: &str, out: &mut Vec<TocEntry>) {
        for li in ol.children().filter(|n| n.has_tag_name("li")) {
            if let Some(a) = li
                .children()
                .find(|n| n.has_tag_name("a") || n.has_tag_name("span"))
            {
                let title: String = a
                    .descendants()
                    .filter(|t| t.is_text())
                    .filter_map(|t| t.text())
                    .collect::<Vec<_>>()
                    .join("")
                    .split_whitespace()
                    .collect::<Vec<_>>()
                    .join(" ");
                let href = a
                    .attribute("href")
                    .map(|h| relative_to_opf(nav_href, h))
                    .unwrap_or_default();
                out.push(TocEntry { title, href, depth });
            }
            if let Some(sub) = li.children().find(|n| n.has_tag_name("ol")) {
                walk(sub, depth + 1, nav_href, out);
            }
        }
    }
    if let Some(ol) = toc_nav.children().find(|n| n.has_tag_name("ol")) {
        walk(ol, 0, nav_href, &mut out);
    }
    out
}

fn parse_ncx_toc(xml: &str, ncx_href: &str) -> Vec<TocEntry> {
    let Ok(doc) = roxmltree::Document::parse(xml) else {
        return Vec::new();
    };
    let mut out = Vec::new();
    fn walk(parent: roxmltree::Node, depth: u8, ncx_href: &str, out: &mut Vec<TocEntry>) {
        for np in parent.children().filter(|n| n.has_tag_name("navPoint")) {
            let title = np
                .children()
                .find(|n| n.has_tag_name("navLabel"))
                .and_then(|l| l.children().find(|n| n.has_tag_name("text")))
                .map(text_of)
                .unwrap_or_default();
            let href = np
                .children()
                .find(|n| n.has_tag_name("content"))
                .and_then(|c| c.attribute("src"))
                .map(|h| relative_to_opf(ncx_href, h))
                .unwrap_or_default();
            out.push(TocEntry { title, href, depth });
            walk(np, depth + 1, ncx_href, out);
        }
    }
    if let Some(nav_map) = doc.descendants().find(|n| n.has_tag_name("navMap")) {
        walk(nav_map, 0, ncx_href, &mut out);
    }
    out
}

/// What a finished KFX says about itself: enough to list it and send it unchanged.
#[derive(Clone, Debug, Default, Serialize)]
pub struct KfxInfo {
    pub info: BookInfo,
    /// `PDOC` or `EBOK`, from `cde_content_type`.
    pub cde_type: Option<String>,
    /// The `ASIN` (else `content_id`) the Kindle files it under.
    pub asin: Option<String>,
    /// The cover image bytes (JPEG), when the KFX carries one.
    #[serde(skip)]
    pub cover: Option<Vec<u8>>,
}

/// Reads title, authors, content type and cover from a KFX's `kindle_title_metadata`. An
/// unreadable KFX gives an `Err`; missing fields stay empty.
pub fn inspect_kfx(kfx: &[u8]) -> Result<KfxInfo, String> {
    use deckle_kfx::ion::{Sym, Value};
    let doc = deckle_kfx::container::read_kfx(kfx).map_err(|e| e.to_string())?;
    let mut out = KfxInfo::default();
    let mut cover_name = None;
    let cats = doc
        .fragments
        .get_single(&Sym::new("$490"))
        .and_then(|md| md.value.as_struct())
        .and_then(|s| s.get("$491"))
        .and_then(Value::as_list)
        .ok_or("no book metadata")?;
    for cat in cats {
        let Some(c) = cat.as_struct() else { continue };
        if c.get("$495").and_then(Value::as_str) != Some("kindle_title_metadata") {
            continue;
        }
        for kv in c.get("$258").and_then(Value::as_list).into_iter().flatten() {
            let Some(s) = kv.as_struct() else { continue };
            let (Some(key), Some(value)) = (s.get("$492").and_then(Value::as_str), s.get("$307"))
            else {
                continue;
            };
            let text = match value {
                Value::Symbol(sym) => Some(sym.0.clone()),
                v => v.as_str().map(str::to_owned),
            };
            match (key, text) {
                ("title", Some(t)) => out.info.title = t,
                ("author", Some(a)) => out.info.creators.push(Creator {
                    file_as: Some(a.clone()),
                    name: a,
                }),
                ("language", Some(l)) => out.info.language = l,
                ("cde_content_type", Some(t)) => out.cde_type = Some(t),
                ("ASIN", Some(a)) => out.asin = Some(a),
                ("content_id", Some(a)) if out.asin.is_none() => out.asin = Some(a),
                ("cover_image", Some(n)) => cover_name = Some(n),
                _ => {}
            }
        }
    }
    out.cover = cover_name.and_then(|name| {
        let res = doc
            .fragments
            .get(&Sym::new("$164"), Some(&Sym::new(&name)))?;
        let location = res.value.as_struct()?.get("$165")?.as_str()?.to_owned();
        let raw = doc
            .fragments
            .get(&Sym::new("$417"), Some(&Sym::new(&location)))?;
        raw.value.as_blob().map(<[u8]>::to_vec)
    });
    Ok(out)
}
