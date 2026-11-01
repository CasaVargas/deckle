// SPDX-License-Identifier: GPL-3.0-or-later
//! Who's who (docs/superpowers/specs/2026-10-30-whos-who-design.md): X-Ray's people and terms
//! put inside the EPUB, for readers that aren't Kindles. A page at the end lists each with its
//! description, and the first mention of each name in each spine document links to its entry.
//!
//! The text is read and the links are written in one pass over each document's raw XHTML, as
//! [`crate::kepub`] does: nothing is parsed into a tree and written back.

use std::collections::{HashMap, HashSet};

use deckle_kfx::text::Run;
use deckle_xray::asc::{DescriptionSource, Entity, EntityKind};
use deckle_xray::llm;
use deckle_xray::wiki;

use crate::xray::{Entities, Fetch, Post, XRayOptions, entities_with};

/// The page's file name, beside the OPF.
pub const PAGE: &str = "deckle-whoswho.xhtml";
/// Fewer entities than this and the page isn't worth adding.
pub const MIN_ENTITIES: usize = 3;

/// What the page got.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Summary {
    pub people: usize,
    pub terms: usize,
    /// Links put in the text.
    pub links: usize,
    /// `llm` | `wikipedia` | `basic`
    pub source: String,
    pub warnings: Vec<String>,
}

/// Adds the page and links to the EPUB `bytes`; `None` (and the bytes unchanged) when the book
/// has fewer than [`MIN_ENTITIES`] people and terms. Real HTTP for the LLM and Wikipedia.
pub fn add(bytes: &[u8], opts: &XRayOptions) -> Result<(Vec<u8>, Option<Summary>), String> {
    add_with(bytes, opts, &mut llm::http_post, &mut wiki::http_get)
}

/// [`add`] with injected network calls (tests).
pub fn add_with(
    bytes: &[u8],
    opts: &XRayOptions,
    post: &mut Post<'_>,
    fetch: &mut Fetch<'_>,
) -> Result<(Vec<u8>, Option<Summary>), String> {
    let mut files = unzip(bytes)?;
    let book = Book::read(&files)?;
    if files.iter().any(|(n, _)| *n == book.page_path()) {
        return Ok((bytes.to_vec(), None));
    }
    let docs: Vec<Doc> = book
        .spine
        .iter()
        .map(|p| {
            let xhtml = files
                .iter()
                .find(|(n, _)| n == p)
                .map(|(_, d)| String::from_utf8_lossy(d).into_owned())
                .unwrap_or_default();
            scan(&xhtml)
        })
        .collect();
    let runs = runs(&docs);
    let Some(found) = entities_with(
        &book.title,
        &book.author,
        &book.lang,
        &runs.runs,
        opts,
        post,
        fetch,
    ) else {
        return Ok((bytes.to_vec(), None));
    };
    let Entities {
        mut entities,
        source,
        warnings,
        lang,
    } = found;
    if entities.len() < MIN_ENTITIES {
        return Ok((bytes.to_vec(), None));
    }
    // People first, then terms; each by first appearance.
    for e in &mut entities {
        e.occurrences.sort_unstable();
    }
    entities.sort_by_key(|e| {
        (
            e.kind != EntityKind::Person,
            e.occurrences.first().map_or(i64::MAX, |o| o.0),
        )
    });

    // The first linkable mention of each entity in each chapter: a document, split again at
    // each `h1`/`h2` (one file often holds several chapters).
    let mut inserts: Vec<Vec<Insert>> = vec![Vec::new(); docs.len()];
    let mut first_link: Vec<Option<(usize, usize)>> = vec![None; entities.len()];
    for (ei, e) in entities.iter().enumerate() {
        let mut done: HashSet<(usize, usize)> = HashSet::new();
        for &(start, len) in &e.occurrences {
            let Some((di, section, raw)) = runs.locate(start, len, &docs) else {
                continue;
            };
            if !done.insert((di, section)) {
                continue;
            }
            inserts[di].push(Insert {
                raw,
                entity: ei,
                section,
            });
            if first_link[ei].is_none() {
                first_link[ei] = Some((di, section));
            }
        }
    }

    let page_rel = |doc: &str| relative(dir_of(doc), &book.page_path());
    let mut links = 0;
    for (di, path) in book.spine.iter().enumerate() {
        if inserts[di].is_empty() {
            continue;
        }
        let Some(slot) = files.iter_mut().find(|(n, _)| n == path) else {
            continue;
        };
        let xhtml = String::from_utf8_lossy(&slot.1).into_owned();
        let (out, n) = link(&xhtml, &mut inserts[di], &page_rel(path), di);
        links += n;
        slot.1 = out.into_bytes();
    }

    let labels = book.nav_labels(&files);
    let page = page(&entities, &first_link, &book, &labels, &lang);
    files.push((book.page_path(), page.into_bytes()));
    book.register_page(&mut files, "deckle-whoswho", PAGE, "Who's who")?;

    let people = entities
        .iter()
        .filter(|e| e.kind == EntityKind::Person)
        .count();
    Ok((
        crate::kepub::write(&files)?,
        Some(Summary {
            people,
            terms: entities.len() - people,
            links,
            source: source.to_owned(),
            warnings,
        }),
    ))
}

pub(crate) fn unzip(bytes: &[u8]) -> Result<Vec<(String, Vec<u8>)>, String> {
    use std::io::Read;
    let mut zip = zip::ZipArchive::new(std::io::Cursor::new(bytes)).map_err(|e| e.to_string())?;
    let mut files = Vec::with_capacity(zip.len());
    for i in 0..zip.len() {
        let mut f = zip.by_index(i).map_err(|e| e.to_string())?;
        if f.is_dir() {
            continue;
        }
        let name = f.name().map_err(|e| e.to_string())?.into_owned();
        let mut data = Vec::with_capacity(f.size() as usize);
        f.read_to_end(&mut data).map_err(|e| e.to_string())?;
        files.push((name, data));
    }
    Ok(files)
}

// ---------------------------------------------------------------------------------------------
// The package

pub(crate) struct Book {
    pub(crate) opf_path: String,
    pub(crate) title: String,
    pub(crate) author: String,
    pub(crate) lang: String,
    /// Spine XHTML documents, as paths in the zip, in reading order.
    pub(crate) spine: Vec<String>,
    /// Their manifest ids, in the same order.
    pub(crate) spine_ids: Vec<String>,
    /// The nav document's path, when there is one.
    pub(crate) nav: Option<String>,
    /// The NCX's path, when there is one.
    pub(crate) ncx: Option<String>,
    /// Spine documents marked `linear="no"`.
    pub(crate) nonlinear: HashSet<String>,
    /// `rendition:layout` is `pre-paginated`.
    pub(crate) fixed_layout: bool,
}

impl Book {
    pub(crate) fn read(files: &[(String, Vec<u8>)]) -> Result<Book, String> {
        let find = |n: &str| files.iter().find(|(f, _)| f == n).map(|(_, d)| d);
        let container = find("META-INF/container.xml").ok_or("no META-INF/container.xml")?;
        let container = String::from_utf8_lossy(container);
        let doc = roxmltree::Document::parse(&container).map_err(|e| format!("container: {e}"))?;
        let opf_path = doc
            .descendants()
            .find(|n| n.has_tag_name("rootfile"))
            .and_then(|n| n.attribute("full-path"))
            .ok_or("container.xml names no OPF")?
            .to_owned();
        let opf = find(&opf_path).ok_or_else(|| format!("{opf_path} is missing"))?;
        let opf = String::from_utf8_lossy(opf);
        let doc = roxmltree::Document::parse(&opf).map_err(|e| format!("{opf_path}: {e}"))?;
        let dir = dir_of(&opf_path).to_owned();
        let text = |tag: &str| {
            doc.descendants()
                .find(|n| n.tag_name().name() == tag)
                .and_then(|n| n.text())
                .map(|t| t.trim().to_owned())
                .unwrap_or_default()
        };
        let items: HashMap<&str, (String, &str, &str)> = doc
            .descendants()
            .filter(|n| n.has_tag_name("item"))
            .filter_map(|n| {
                Some((
                    n.attribute("id")?,
                    (
                        crate::epub::join(&dir, &crate::epub::percent_decode(n.attribute("href")?)),
                        n.attribute("media-type").unwrap_or(""),
                        n.attribute("properties").unwrap_or(""),
                    ),
                ))
            })
            .collect();
        let (spine_ids, spine): (Vec<String>, Vec<String>) = doc
            .descendants()
            .filter(|n| n.has_tag_name("itemref"))
            .filter_map(|n| {
                let id = n.attribute("idref")?;
                Some((id, items.get(id)?))
            })
            .filter(|(_, (_, mt, _))| *mt == "application/xhtml+xml")
            .map(|(id, (p, _, _))| (id.to_owned(), p.clone()))
            .unzip();
        let nav = items
            .values()
            .find(|(_, _, props)| props.split_whitespace().any(|p| p == "nav"))
            .map(|(p, _, _)| p.clone());
        let ncx = items
            .values()
            .find(|(_, mt, _)| *mt == "application/x-dtbncx+xml")
            .map(|(p, _, _)| p.clone());
        let nonlinear = doc
            .descendants()
            .filter(|n| n.has_tag_name("itemref") && n.attribute("linear") == Some("no"))
            .filter_map(|n| items.get(n.attribute("idref")?))
            .map(|(p, _, _)| p.clone())
            .collect();
        let fixed_layout = doc.descendants().any(|n| {
            n.tag_name().name() == "meta"
                && n.attribute("property") == Some("rendition:layout")
                && n.text().map(str::trim) == Some("pre-paginated")
        });
        Ok(Book {
            title: text("title"),
            author: text("creator"),
            lang: text("language"),
            opf_path,
            spine,
            spine_ids,
            nav,
            ncx,
            nonlinear,
            fixed_layout,
        })
    }

    fn page_path(&self) -> String {
        self.page_at(PAGE)
    }

    /// A page Deckle adds, `file` next to the OPF, as a path in the zip.
    pub(crate) fn page_at(&self, file: &str) -> String {
        crate::epub::join(dir_of(&self.opf_path), file)
    }

    /// Spine document path → its label in the nav (else the NCX), for "First mentioned in …".
    pub(crate) fn nav_labels(&self, files: &[(String, Vec<u8>)]) -> HashMap<String, String> {
        let mut out = HashMap::new();
        let read = |p: &str| {
            files
                .iter()
                .find(|(n, _)| n == p)
                .map(|(_, d)| String::from_utf8_lossy(d).into_owned())
        };
        if let Some(path) = &self.nav
            && let Some(xml) = read(path)
            && let Ok(doc) = roxmltree::Document::parse(&xml)
        {
            for a in doc.descendants().filter(|n| n.tag_name().name() == "a") {
                let (Some(href), label) = (a.attribute("href"), collect_text(a)) else {
                    continue;
                };
                let target = href.split('#').next().unwrap_or("");
                let target = crate::epub::join(dir_of(path), &crate::epub::percent_decode(target));
                if !label.is_empty() {
                    out.entry(target).or_insert(label);
                }
            }
        }
        if out.is_empty()
            && let Some(path) = &self.ncx
            && let Some(xml) = read(path)
            && let Ok(doc) = roxmltree::Document::parse(&xml)
        {
            for np in doc
                .descendants()
                .filter(|n| n.tag_name().name() == "navPoint")
            {
                let label = np
                    .children()
                    .find(|n| n.tag_name().name() == "navLabel")
                    .map(collect_text)
                    .unwrap_or_default();
                let src = np
                    .children()
                    .find(|n| n.tag_name().name() == "content")
                    .and_then(|n| n.attribute("src"))
                    .unwrap_or("");
                let target = src.split('#').next().unwrap_or("");
                let target = crate::epub::join(dir_of(path), &crate::epub::percent_decode(target));
                if !label.is_empty() {
                    out.entry(target).or_insert(label);
                }
            }
        }
        out
    }

    /// The page `file` (next to the OPF, manifest id `id`) in the manifest, at the end of the
    /// spine, and last in the nav and the NCX as `label`.
    pub(crate) fn register_page(
        &self,
        files: &mut [(String, Vec<u8>)],
        id: &str,
        file: &str,
        label: &str,
    ) -> Result<(), String> {
        let opf = slot(files, &self.opf_path)?;
        let mut s = String::from_utf8_lossy(&opf.1).into_owned();
        let item =
            format!("<item id=\"{id}\" href=\"{file}\" media-type=\"application/xhtml+xml\"/>");
        insert_before_close(&mut s, "manifest", &item)?;
        insert_before_close(&mut s, "spine", &format!("<itemref idref=\"{id}\"/>"))?;
        opf.1 = s.into_bytes();
        let page = self.page_at(file);
        let label = esc(label);

        if let Some(path) = &self.nav {
            let nav = slot(files, path)?;
            let mut s = String::from_utf8_lossy(&nav.1).into_owned();
            let href = relative(dir_of(path), &page);
            add_to_toc(&mut s, &format!("<li><a href=\"{href}\">{label}</a></li>"));
            nav.1 = s.into_bytes();
        }
        if let Some(path) = &self.ncx {
            let ncx = slot(files, path)?;
            let mut s = String::from_utf8_lossy(&ncx.1).into_owned();
            let order = s
                .match_indices("playOrder=\"")
                .filter_map(|(i, m)| {
                    let rest = &s[i + m.len()..];
                    rest[..rest.find('"')?].parse::<u32>().ok()
                })
                .max()
                .unwrap_or(0)
                + 1;
            let href = relative(dir_of(path), &page);
            let point = format!(
                "<navPoint id=\"{id}\" playOrder=\"{order}\"><navLabel><text>{label}</text>\
                 </navLabel><content src=\"{href}\"/></navPoint>"
            );
            // A NCX without a navMap is left alone; the nav document still lists the page.
            let _ = insert_before_close(&mut s, "navMap", &point);
            ncx.1 = s.into_bytes();
        }
        Ok(())
    }
}

impl Book {
    /// The page `file` (next to the OPF, manifest id `id`) in the manifest, and before the spine
    /// document `before` in the spine, the nav and the NCX (as `label`, taking that document's
    /// `playOrder`; later points move up one). Where `before` isn't listed, the page goes first in
    /// the nav and the NCX's place for it is left alone.
    pub(crate) fn register_page_before(
        &self,
        files: &mut [(String, Vec<u8>)],
        (id, file, label): (&str, &str, &str),
        before: &str,
    ) -> Result<(), String> {
        let at = self
            .spine
            .iter()
            .position(|p| p == before)
            .ok_or_else(|| format!("{before} is not in the spine"))?;
        let opf = slot(files, &self.opf_path)?;
        let mut s = String::from_utf8_lossy(&opf.1).into_owned();
        let item =
            format!("<item id=\"{id}\" href=\"{file}\" media-type=\"application/xhtml+xml\"/>");
        insert_before_close(&mut s, "manifest", &item)?;
        let idref = format!("idref=\"{}\"", self.spine_ids[at]);
        let ref_at = s
            .find(&idref)
            .and_then(|i| s[..i].rfind('<'))
            .ok_or_else(|| format!("no itemref for {before}"))?;
        s.insert_str(ref_at, &format!("<itemref idref=\"{id}\"/>"));
        opf.1 = s.into_bytes();
        let page = self.page_at(file);
        let label = esc(label);

        if let Some(path) = &self.nav {
            let nav = slot(files, path)?;
            let mut s = String::from_utf8_lossy(&nav.1).into_owned();
            let li = format!(
                "<li><a href=\"{}\">{label}</a></li>",
                relative(dir_of(path), &page)
            );
            let target = |href: &str| {
                crate::epub::join(
                    dir_of(path),
                    &crate::epub::percent_decode(href.split('#').next().unwrap_or("")),
                )
            };
            let li_at = s
                .match_indices("href=\"")
                .find(|(i, m)| {
                    let rest = &s[i + m.len()..];
                    rest.find('"').is_some_and(|e| target(&rest[..e]) == before)
                })
                .and_then(|(i, _)| s[..i].rfind("<li"));
            match li_at {
                Some(i) => s.insert_str(i, &li),
                None => {
                    let start = s
                        .find("epub:type=\"toc\"")
                        .or_else(|| s.find("<nav"))
                        .unwrap_or(0);
                    if let Some(ol) = s[start..].find("<ol") {
                        let open = tag_end(&s, start + ol);
                        s.insert_str(open, &li);
                    }
                }
            }
            nav.1 = s.into_bytes();
        }
        if let Some(path) = &self.ncx {
            let ncx = slot(files, path)?;
            let mut s = String::from_utf8_lossy(&ncx.1).into_owned();
            let href = relative(dir_of(path), &page);
            let point_at = s.match_indices("src=\"").find_map(|(i, m)| {
                let rest = &s[i + m.len()..];
                let src = &rest[..rest.find('"')?];
                let target = crate::epub::join(
                    dir_of(path),
                    &crate::epub::percent_decode(src.split('#').next().unwrap_or("")),
                );
                (target == before)
                    .then(|| s[..i].rfind("<navPoint"))
                    .flatten()
            });
            if let Some(i) = point_at {
                let order = s[i..]
                    .find("playOrder=\"")
                    .and_then(|o| {
                        let rest = &s[i + o + 11..];
                        rest[..rest.find('"')?].parse::<u32>().ok()
                    })
                    .unwrap_or(1);
                let mut out = renumber(&s, order);
                out.insert_str(
                    i,
                    &format!(
                        "<navPoint id=\"{id}\" playOrder=\"{order}\"><navLabel><text>{label}\
                         </text></navLabel><content src=\"{href}\"/></navPoint>"
                    ),
                );
                s = out;
            }
            ncx.1 = s.into_bytes();
        }
        Ok(())
    }
}

/// `xml` with every `playOrder` of `from` or more one higher. Byte offsets before the first
/// changed number stay put; the inserted navPoint goes before it.
fn renumber(xml: &str, from: u32) -> String {
    let mut out = String::with_capacity(xml.len() + 16);
    let mut rest = xml;
    while let Some(i) = rest.find("playOrder=\"") {
        let num_at = i + 11;
        out.push_str(&rest[..num_at]);
        rest = &rest[num_at..];
        let end = rest.find('"').unwrap_or(0);
        match rest[..end].parse::<u32>() {
            Ok(n) if n >= from => out.push_str(&(n + 1).to_string()),
            _ => out.push_str(&rest[..end]),
        }
        rest = &rest[end..];
    }
    out.push_str(rest);
    out
}

fn slot<'a>(
    files: &'a mut [(String, Vec<u8>)],
    path: &str,
) -> Result<&'a mut (String, Vec<u8>), String> {
    files
        .iter_mut()
        .find(|(n, _)| n == path)
        .ok_or_else(|| format!("{path} is missing"))
}

fn collect_text(n: roxmltree::Node) -> String {
    let t: String = n
        .descendants()
        .filter(|d| d.is_text())
        .filter_map(|d| d.text())
        .collect();
    t.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// Inserts `what` before the last `</tag>` or `</prefix:tag>`.
fn insert_before_close(s: &mut String, tag: &str, what: &str) -> Result<(), String> {
    let at = s
        .rmatch_indices("</")
        .find(|(i, _)| {
            let rest = &s[i + 2..];
            let name = rest[..rest.find('>').unwrap_or(rest.len())].trim();
            name == tag || name.rsplit(':').next() == Some(tag)
        })
        .map(|(i, _)| i)
        .ok_or_else(|| format!("no </{tag}>"))?;
    s.insert_str(at, what);
    Ok(())
}

/// Appends `li` to the TOC nav's list: before the last `</ol>` ahead of the `</nav>` that ends
/// the `epub:type="toc"` nav (the first nav when none says so).
fn add_to_toc(s: &mut String, li: &str) {
    let start = s
        .find("epub:type=\"toc\"")
        .or_else(|| s.find("<nav"))
        .unwrap_or(0);
    let Some(end) = s[start..].find("</nav>").map(|e| start + e) else {
        return;
    };
    if let Some(ol) = s[start..end].rfind("</ol>") {
        s.insert_str(start + ol, li);
    }
}

fn dir_of(path: &str) -> &str {
    path.rsplit_once('/').map(|(d, _)| d).unwrap_or("")
}

/// `to` (a path in the zip) as seen from the folder `from_dir`.
fn relative(from_dir: &str, to: &str) -> String {
    let from: Vec<&str> = from_dir.split('/').filter(|p| !p.is_empty()).collect();
    let to_parts: Vec<&str> = to.split('/').filter(|p| !p.is_empty()).collect();
    let common = from
        .iter()
        .zip(&to_parts)
        .take_while(|(a, b)| a == b)
        .count();
    let mut out: Vec<&str> = vec![".."; from.len() - common];
    out.extend(&to_parts[common..]);
    out.join("/")
}

// ---------------------------------------------------------------------------------------------
// Reading the text

/// One text node: its decoded text, and where each decoded character starts in the document's
/// raw XHTML.
#[derive(Clone, Debug)]
pub(crate) struct Seg {
    pub(crate) text: String,
    /// Raw byte offset of each decoded char, plus the end.
    pub(crate) map: Vec<usize>,
    pub(crate) linkable: bool,
    /// Index of the run (block) it belongs to, within its document.
    pub(crate) block: usize,
    /// Chapter within its document: one more at each `h1` or `h2`.
    pub(crate) section: usize,
}

#[derive(Clone, Debug, Default)]
pub(crate) struct Doc {
    pub(crate) segs: Vec<Seg>,
    pub(crate) blocks: usize,
}

/// Elements whose text isn't read.
const HIDDEN: &[&str] = &["script", "style", "svg", "math", "head", "title"];
/// Elements whose text is read but never linked.
const NO_LINK: &[&str] = &[
    "a", "h1", "h2", "h3", "h4", "h5", "h6", "pre", "code", "table",
];
/// Elements that end one run and start the next.
const BLOCKS: &[&str] = &[
    "p",
    "h1",
    "h2",
    "h3",
    "h4",
    "h5",
    "h6",
    "li",
    "dt",
    "dd",
    "blockquote",
    "figcaption",
    "div",
    "aside",
    "section",
    "br",
    "td",
    "th",
    "tr",
    "body",
];

/// The text nodes of a document's body, in order.
pub(crate) fn scan(xhtml: &str) -> Doc {
    let mut doc = Doc::default();
    let mut hidden = 0usize;
    let mut no_link = 0usize;
    let mut block = 0usize;
    let mut block_used = false;
    let mut section = 0usize;
    let mut i = 0;
    let b = xhtml.as_bytes();
    while i < b.len() {
        if b[i] == b'<' {
            let end = tag_end(xhtml, i);
            let tag = &xhtml[i..end];
            if let Some((name, closing, selfclosing)) = tag_name(tag) {
                if HIDDEN.contains(&name) && !selfclosing {
                    if closing {
                        hidden = hidden.saturating_sub(1);
                    } else {
                        hidden += 1;
                    }
                }
                if NO_LINK.contains(&name) && !selfclosing {
                    if closing {
                        no_link = no_link.saturating_sub(1);
                    } else {
                        no_link += 1;
                    }
                }
                if (name == "h1" || name == "h2") && !closing {
                    section += 1;
                }
                if BLOCKS.contains(&name) && block_used {
                    block += 1;
                    block_used = false;
                }
            }
            i = end;
            continue;
        }
        let end = xhtml[i..].find('<').map_or(b.len(), |e| i + e);
        if hidden == 0 {
            let (text, map) = decode(&xhtml[i..end], i);
            if !text.is_empty() {
                doc.segs.push(Seg {
                    text,
                    map,
                    linkable: no_link == 0,
                    block,
                    section,
                });
                block_used = true;
            }
        }
        i = end;
    }
    doc.blocks = block + 1;
    doc
}

/// The byte after a tag, comment, CDATA section or processing instruction starting at `i`.
pub(crate) fn tag_end(s: &str, i: usize) -> usize {
    let rest = &s[i..];
    let close = if rest.starts_with("<!--") {
        "-->"
    } else if rest.starts_with("<![CDATA[") {
        "]]>"
    } else if rest.starts_with("<?") {
        "?>"
    } else {
        ">"
    };
    rest[1..]
        .find(close)
        .map_or(s.len(), |e| i + 1 + e + close.len())
}

/// `(local name, closing, self-closing)` of an element tag; `None` for comments and the like.
pub(crate) fn tag_name(tag: &str) -> Option<(&str, bool, bool)> {
    let inner = tag.strip_prefix('<')?.strip_suffix('>')?;
    if inner.starts_with('!') || inner.starts_with('?') {
        return None;
    }
    let closing = inner.starts_with('/');
    let selfclosing = inner.ends_with('/');
    let inner = inner.trim_start_matches('/');
    let name = inner
        .split(|c: char| c.is_whitespace() || c == '/')
        .next()?;
    let name = name.rsplit(':').next()?;
    Some((name, closing, selfclosing))
}

/// Decodes XML entities in `raw` (which starts at byte `base` of its document).
pub(crate) fn decode(raw: &str, base: usize) -> (String, Vec<usize>) {
    let mut text = String::with_capacity(raw.len());
    let mut map = Vec::with_capacity(raw.len() + 1);
    let mut i = 0;
    while i < raw.len() {
        let c = raw[i..].chars().next().unwrap_or('\u{fffd}');
        if c == '&'
            && let Some(semi) = raw[i..].find(';').filter(|&s| s <= 10)
            && let Some(d) = entity(&raw[i + 1..i + semi])
        {
            map.push(base + i);
            text.push(d);
            i += semi + 1;
            continue;
        }
        map.push(base + i);
        text.push(c);
        i += c.len_utf8();
    }
    map.push(base + raw.len());
    (text, map)
}

fn entity(name: &str) -> Option<char> {
    match name {
        "amp" => Some('&'),
        "lt" => Some('<'),
        "gt" => Some('>'),
        "quot" => Some('"'),
        "apos" => Some('\''),
        "nbsp" => Some('\u{a0}'),
        _ => {
            let n = name.strip_prefix('#')?;
            let v = match n.strip_prefix(['x', 'X']) {
                Some(h) => u32::from_str_radix(h, 16).ok()?,
                None => n.parse().ok()?,
            };
            char::from_u32(v)
        }
    }
}

// ---------------------------------------------------------------------------------------------
// Runs: one per block, with positions mapped back to text nodes

struct Runs {
    runs: Vec<Run>,
    /// For each run: `(document, [(segment index, char offset of the segment in the run)])`.
    parts: Vec<(usize, Vec<(usize, i64)>)>,
}

fn runs(docs: &[Doc]) -> Runs {
    let mut runs = Vec::new();
    let mut parts = Vec::new();
    let mut pid = 0i64;
    for (di, doc) in docs.iter().enumerate() {
        let mut by_block: Vec<Vec<usize>> = vec![Vec::new(); doc.blocks];
        for (si, s) in doc.segs.iter().enumerate() {
            by_block[s.block].push(si);
        }
        for segs in by_block.into_iter().filter(|v| !v.is_empty()) {
            let mut text = String::new();
            let mut at = Vec::with_capacity(segs.len());
            let mut len = 0i64;
            for si in segs {
                at.push((si, len));
                text.push_str(&doc.segs[si].text);
                len += doc.segs[si].text.chars().count() as i64;
            }
            runs.push(Run { pid, text });
            parts.push((di, at));
            // A gap, so no position is shared by two runs.
            pid += len + 1;
        }
    }
    Runs { runs, parts }
}

impl Runs {
    /// The document, chapter and raw byte range of the mention `(start, len)`, when it sits in
    /// one linkable text node.
    fn locate(&self, start: i64, len: i64, docs: &[Doc]) -> Option<(usize, usize, (usize, usize))> {
        let ri = self
            .runs
            .partition_point(|r| r.pid <= start)
            .checked_sub(1)?;
        let (di, at) = &self.parts[ri];
        let off = start - self.runs[ri].pid;
        let k = at.partition_point(|&(_, o)| o <= off).checked_sub(1)?;
        let (si, seg_off) = at[k];
        let seg = &docs[*di].segs[si];
        let from = (off - seg_off) as usize;
        let to = from + len as usize;
        if !seg.linkable || to > seg.map.len() - 1 {
            return None;
        }
        Some((*di, seg.section, (seg.map[from], seg.map[to])))
    }
}

// ---------------------------------------------------------------------------------------------
// Writing

#[derive(Clone, Debug)]
struct Insert {
    raw: (usize, usize),
    entity: usize,
    section: usize,
}

/// A mention reads as the text around it, with a dotted rule under it (an inline style, so the
/// book's own stylesheets need no change).
const LINK_STYLE: &str = "color: inherit; text-decoration: none; border-bottom: 1px dotted";

/// Link id of entity `e`'s mention in chapter `section` of spine document `doc`.
fn mention_id(e: usize, doc: usize, section: usize) -> String {
    format!("deckle-ww-{e}-d{doc}-s{section}")
}

fn entry_id(e: usize) -> String {
    format!("deckle-ww-{e}")
}

/// The document with its inserts made; returns how many.
fn link(xhtml: &str, inserts: &mut [Insert], page_href: &str, doc: usize) -> (String, usize) {
    inserts.sort_by_key(|i| i.raw.0);
    let mut out = String::with_capacity(xhtml.len() + inserts.len() * 120);
    let mut at = 0;
    let mut n = 0;
    for ins in inserts.iter() {
        let (s, e) = ins.raw;
        if s < at {
            continue; // overlaps the one before
        }
        out.push_str(&xhtml[at..s]);
        out.push_str(&format!(
            "<a class=\"deckle-ww\" epub:type=\"noteref\" style=\"{LINK_STYLE}\" \
             href=\"{page_href}#{}\" id=\"{}\">",
            entry_id(ins.entity),
            mention_id(ins.entity, doc, ins.section)
        ));
        out.push_str(&xhtml[s..e]);
        out.push_str("</a>");
        at = e;
        n += 1;
    }
    out.push_str(&xhtml[at..]);
    (with_epub_ns(out), n)
}

/// Declares the `epub` namespace on `<html>` when it isn't.
fn with_epub_ns(mut s: String) -> String {
    let Some(start) = s.find("<html") else {
        return s;
    };
    let end = start + s[start..].find('>').unwrap_or(0);
    if !s[start..end].contains("xmlns:epub") {
        s.insert_str(start + 5, " xmlns:epub=\"http://www.idpf.org/2007/ops\"");
    }
    s
}

pub(crate) fn esc(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}

fn page(
    entities: &[Entity],
    first_link: &[Option<(usize, usize)>],
    book: &Book,
    labels: &HashMap<String, String>,
    lang: &str,
) -> String {
    let mut s = format!(
        "<?xml version=\"1.0\" encoding=\"utf-8\"?>\n<!DOCTYPE html>\n<html \
         xmlns=\"http://www.w3.org/1999/xhtml\" xmlns:epub=\"http://www.idpf.org/2007/ops\" \
         xml:lang=\"{l}\" lang=\"{l}\"><head><title>Who's who</title><style>\
         ol.deckle-ww-list {{ list-style: none; padding: 0; margin: 0; }}\
         ol.deckle-ww-list li {{ margin: 0 0 0.8em 0; }}\
         ol.deckle-ww-list p {{ margin: 0; text-indent: 0; }}\
         p.deckle-ww-back {{ font-size: 0.85em; }}\
         </style></head><body><section epub:type=\"appendix\" id=\"deckle-whoswho\">\
         <h1>Who's who</h1>",
        l = esc(lang)
    );
    let page_dir = dir_of(&book.page_path()).to_owned();
    for (kind, heading) in [
        (EntityKind::Person, "People"),
        (EntityKind::Term, "Places and terms"),
    ] {
        let group: Vec<usize> = (0..entities.len())
            .filter(|&i| entities[i].kind == kind)
            .collect();
        if group.is_empty() {
            continue;
        }
        s.push_str(&format!(
            "<h2>{heading}</h2><ol epub:type=\"endnotes\" class=\"deckle-ww-list\">"
        ));
        for i in group {
            let e = &entities[i];
            s.push_str(&format!(
                "<li epub:type=\"endnote\" id=\"{}\"><p><b>{}</b> — {}</p>",
                entry_id(i),
                esc(&e.label),
                esc(e.description.trim())
            ));
            if e.source == DescriptionSource::Wikipedia {
                let article = e.label.replace(' ', "_");
                s.push_str(&format!(
                    "<p class=\"deckle-ww-back\"><a href=\"https://{}.wikipedia.org/wiki/{}\">From \
                     Wikipedia (CC BY-SA).</a></p>",
                    esc(lang),
                    esc(&article)
                ));
            }
            if let Some((di, section)) = first_link[i] {
                let doc = &book.spine[di];
                let where_ = labels
                    .get(doc)
                    .cloned()
                    .unwrap_or_else(|| "the text".into());
                s.push_str(&format!(
                    "<p class=\"deckle-ww-back\"><a href=\"{}#{}\">First mentioned in {}</a></p>",
                    relative(&page_dir, doc),
                    mention_id(i, di, section),
                    esc(&where_)
                ));
            }
            s.push_str("</li>");
        }
        s.push_str("</ol>");
    }
    s.push_str("</section></body></html>\n");
    s
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn relative_paths_climb_and_descend() {
        assert_eq!(
            relative("OEBPS", "OEBPS/deckle-whoswho.xhtml"),
            "deckle-whoswho.xhtml"
        );
        assert_eq!(
            relative("OEBPS/text", "OEBPS/deckle-whoswho.xhtml"),
            "../deckle-whoswho.xhtml"
        );
        assert_eq!(relative("", "OEBPS/a.xhtml"), "OEBPS/a.xhtml");
    }

    #[test]
    fn entities_decode_with_their_raw_offsets() {
        let (t, map) = decode("Tom &amp; Jerry&#8217;s", 10);
        assert_eq!(t, "Tom & Jerry’s");
        assert_eq!(map[4], 14); // '&' at raw "&amp;"
        assert_eq!(map[6], 20); // 'J'
        assert_eq!(*map.last().unwrap(), 10 + "Tom &amp; Jerry&#8217;s".len());
    }

    #[test]
    fn headings_and_links_are_read_but_not_linkable() {
        let d = scan(
            "<html><head><title>T</title></head><body><h1>Emma</h1><p>Emma met \
             <a href=\"x\">Knightley</a>.</p></body></html>",
        );
        let texts: Vec<_> = d
            .segs
            .iter()
            .map(|s| (s.text.as_str(), s.linkable))
            .collect();
        assert_eq!(
            texts,
            [
                ("Emma", false),
                ("Emma met ", true),
                ("Knightley", false),
                (".", true)
            ]
        );
        assert_ne!(d.segs[0].block, d.segs[1].block);
        assert_eq!(d.segs[1].block, d.segs[2].block);
    }

    #[test]
    fn the_toc_gets_a_last_entry() {
        let mut s = String::from(
            "<nav epub:type=\"toc\"><ol><li><a href=\"a\">A</a><ol><li>x</li></ol></li></ol></nav>",
        );
        add_to_toc(&mut s, "<li>W</li>");
        assert!(s.ends_with("</li><li>W</li></ol></nav>"), "{s}");
    }
}
