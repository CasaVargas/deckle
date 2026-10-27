// SPDX-License-Identifier: GPL-3.0-or-later
//! The cookbook EPUB (cookbook design §6.1): one book with a fixed identifier, a cover that
//! counts the recipes, the contents, a file per non-empty chapter, and the index by ingredient.

mod pages;

use std::collections::{HashMap, HashSet};
use std::fmt::Write as _;
use std::path::{Path, PathBuf};

use chrono::{DateTime, Utc};
use sha2::{Digest, Sha256};

use self::pages::{Entry, Part};
use super::filing::{Chapter, EVERYTHING_ELSE};
use super::index::index;
use super::store::{BookConfig, ImageKind, Picture, Saved, Store};
use crate::esc;
use crate::render::{Files, STYLE};

/// The book's identifier. It never changes, so the Kindle's ASIN for it never changes either
/// and a rebuilt book replaces the old one (§6.1).
pub const BOOK_UID: &str = "urn:deckle:cookbook:default";

/// The book's file name on the device, whatever its title (§6.1).
pub const DEVICE_NAME: &str = "Deckle Kitchen Book.kfx";

const CSS: &str = include_str!("book/cookbook.css");

/// A finished build.
#[derive(Clone, Debug)]
pub struct Built {
    pub path: PathBuf,
    /// The first 16 hex digits of the SHA-256 of the EPUB's sorted `(name, bytes)` entries: the
    /// same recipes, notes and pictures give the same id, whatever the zip's timestamps say.
    pub build_id: String,
    pub recipes: usize,
    pub title: String,
}

/// The picture `saved` prints: its own setting, else the book's. An illustration that hasn't
/// been made prints the site's photo, and a site photo that isn't there prints nothing. Never
/// `Default`.
pub fn printed_picture(store: &Store, saved: &Saved, cfg: &BookConfig) -> Picture {
    let wanted = match saved.picture {
        Picture::Default => cfg.picture,
        own => own,
    };
    let exists = |kind| store.image_path(&saved.id, kind).is_file();
    match wanted {
        Picture::None => Picture::None,
        Picture::Illustration if exists(ImageKind::Ill) => Picture::Illustration,
        Picture::Illustration | Picture::Site | Picture::Default => {
            match exists(ImageKind::Printed) {
                true => Picture::Site,
                false => Picture::None,
            }
        }
    }
}

/// The title the book shows for a recipe.
fn shown_title(s: &Saved) -> String {
    [s.title_override.as_deref(), Some(&s.recipe.title)]
        .into_iter()
        .flatten()
        .map(str::trim)
        .find(|t| !t.is_empty())
        .unwrap_or("Untitled recipe")
        .to_owned()
}

/// A file-name stem from a chapter id: letters, digits, `-` and `_`.
fn slug(id: &str, taken: &HashSet<String>) -> String {
    let mut base: String = id
        .chars()
        .filter(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '_'))
        .collect::<String>()
        .to_lowercase();
    if base.is_empty() {
        base = "x".to_owned();
    }
    let mut out = base.clone();
    let mut n = 2;
    while taken.contains(&out) {
        out = format!("{base}-{n}");
        n += 1;
    }
    out
}

/// The chapters, in book order, with the catch-all last if the config lost it.
fn chapters(cfg: &BookConfig) -> Vec<Chapter> {
    let mut chapters = cfg.chapters.clone();
    if !chapters.iter().any(|c| c.id == EVERYTHING_ELSE) {
        chapters.push(Chapter {
            id: EVERYTHING_ELSE.to_owned(),
            name: "Everything else".to_owned(),
            match_: Vec::new(),
        });
    }
    chapters
}

fn hash(files: &Files) -> String {
    let mut sorted: Vec<&(String, Vec<u8>)> = files.iter().collect();
    sorted.sort_by(|a, b| a.0.cmp(&b.0));
    let mut h = Sha256::new();
    for (name, bytes) in sorted {
        h.update((name.len() as u64).to_le_bytes());
        h.update(name.as_bytes());
        h.update((bytes.len() as u64).to_le_bytes());
        h.update(bytes);
    }
    h.finalize().iter().take(8).fold(String::new(), |mut s, b| {
        let _ = write!(s, "{b:02x}");
        s
    })
}

const CONTAINER: &str = "<?xml version=\"1.0\" encoding=\"utf-8\"?>\n\
    <container version=\"1.0\" xmlns=\"urn:oasis:names:tc:opendocument:xmlns:container\">\
    <rootfiles><rootfile full-path=\"OEBPS/content.opf\" media-type=\"application/oebps-package+xml\"/>\
    </rootfiles></container>\n";

/// Writes the whole book to `out`. This is an export: it leaves the store's dirty flag alone,
/// which only a delivery clears.
pub fn build(store: &Store, now: DateTime<Utc>, out: &Path) -> Result<Built, String> {
    let cfg = store.config()?;
    let all = store.all()?;
    let title = match cfg.title.trim() {
        "" => BookConfig::default().title,
        t => t.to_owned(),
    };

    // File each recipe under its chapter; one whose chapter is gone goes under Everything else.
    let chapters = chapters(&cfg);
    let mut slot: HashMap<&str, usize> = HashMap::new();
    for (i, c) in chapters.iter().enumerate() {
        slot.entry(c.id.as_str()).or_insert(i);
    }
    let other = slot[EVERYTHING_ELSE];
    let mut images: Files = Vec::new();
    let mut bins: Vec<Vec<Entry>> = chapters.iter().map(|_| Vec::new()).collect();
    for saved in &all {
        let picture = printed_picture(store, saved, &cfg);
        let kind = match picture {
            Picture::Illustration => ImageKind::Ill,
            _ => ImageKind::Printed,
        };
        let image = pages::image_name(&saved.id, picture).and_then(|name| {
            let bytes = std::fs::read(store.image_path(&saved.id, kind)).ok()?;
            images.push((format!("OEBPS/images/{name}"), bytes));
            Some(name)
        });
        let at = slot.get(saved.chapter.as_str()).copied().unwrap_or(other);
        bins[at].push(Entry {
            saved,
            title: shown_title(saved),
            image,
        });
    }

    let mut taken = HashSet::new();
    let mut parts: Vec<Part> = Vec::new();
    for (chapter, mut entries) in chapters.iter().zip(bins) {
        if entries.is_empty() {
            continue;
        }
        entries.sort_by_cached_key(|e| (e.title.to_lowercase(), e.saved.id.clone()));
        let slug = slug(&chapter.id, &taken);
        taken.insert(slug.clone());
        parts.push(Part {
            name: chapter.name.trim().to_owned(),
            file: format!("chapter-{slug}.xhtml"),
            slug,
            entries,
        });
    }

    let refs: Vec<&Saved> = all.iter().collect();
    let keys = index(&refs);
    let place: HashMap<&str, (&str, &str)> = parts
        .iter()
        .flat_map(|p| {
            p.entries
                .iter()
                .map(move |e| (e.saved.id.as_str(), (p.file.as_str(), e.title.as_str())))
        })
        .collect();
    let index_page = pages::index(&keys, &|id| {
        place
            .get(id)
            .map(|(f, t)| ((*f).to_owned(), (*t).to_owned()))
    });
    let has_index = !keys.is_empty();

    // The newest recipe is the last one `all` returns.
    let last_added = all.last().map(shown_title);
    let cover =
        crate::cover::render_cookbook(&title, &cfg.subtitle, all.len(), last_added.as_deref())?;

    let modified = all
        .iter()
        .map(|s| s.added)
        .max()
        .unwrap_or_default()
        .format("%Y-%m-%dT%H:%M:%SZ")
        .to_string();

    let mut files: Files = vec![
        ("mimetype".into(), b"application/epub+zip".to_vec()),
        (
            "META-INF/container.xml".into(),
            CONTAINER.as_bytes().to_vec(),
        ),
        (
            "OEBPS/content.opf".into(),
            opf(&title, &modified, &parts, &images, has_index).into_bytes(),
        ),
        (
            "OEBPS/toc.ncx".into(),
            ncx(&title, &parts, has_index).into_bytes(),
        ),
        ("OEBPS/nav.xhtml".into(), nav(&title, &parts, has_index)),
        ("OEBPS/style.css".into(), STYLE.as_bytes().to_vec()),
        ("OEBPS/cookbook.css".into(), CSS.as_bytes().to_vec()),
        ("OEBPS/cover.jpg".into(), cover),
        (
            "OEBPS/contents.xhtml".into(),
            pages::contents(&parts, has_index, now),
        ),
    ];
    for p in &parts {
        files.push((format!("OEBPS/{}", p.file), pages::chapter(p)));
    }
    if has_index {
        files.push(("OEBPS/index.xhtml".into(), index_page));
    }
    files.extend(images);

    if let Some(parent) = out.parent().filter(|p| !p.as_os_str().is_empty()) {
        std::fs::create_dir_all(parent).map_err(|e| format!("{}: {e}", parent.display()))?;
    }
    crate::epub::write(out, &files).map_err(|e| format!("{}: {e}", out.display()))?;
    Ok(Built {
        path: out.to_path_buf(),
        build_id: hash(&files),
        recipes: all.len(),
        title,
    })
}

fn opf(title: &str, modified: &str, parts: &[Part], images: &Files, has_index: bool) -> String {
    let mut manifest = String::from(
        "<item id=\"nav\" href=\"nav.xhtml\" media-type=\"application/xhtml+xml\" properties=\"nav\"/>\n\
         <item id=\"ncx\" href=\"toc.ncx\" media-type=\"application/x-dtbncx+xml\"/>\n\
         <item id=\"css\" href=\"style.css\" media-type=\"text/css\"/>\n\
         <item id=\"cookbook-css\" href=\"cookbook.css\" media-type=\"text/css\"/>\n\
         <item id=\"cover-image\" href=\"cover.jpg\" media-type=\"image/jpeg\" properties=\"cover-image\"/>\n\
         <item id=\"contents\" href=\"contents.xhtml\" media-type=\"application/xhtml+xml\"/>\n",
    );
    let mut spine = String::from("<itemref idref=\"contents\"/>\n");
    for (i, p) in parts.iter().enumerate() {
        let _ = writeln!(
            manifest,
            "<item id=\"ch{i}\" href=\"{}\" media-type=\"application/xhtml+xml\"/>",
            esc(&p.file)
        );
        let _ = writeln!(spine, "<itemref idref=\"ch{i}\"/>");
    }
    if has_index {
        manifest.push_str(
            "<item id=\"index\" href=\"index.xhtml\" media-type=\"application/xhtml+xml\"/>\n",
        );
        spine.push_str("<itemref idref=\"index\"/>\n");
    }
    for (i, (name, _)) in images.iter().enumerate() {
        let _ = writeln!(
            manifest,
            "<item id=\"img{i}\" href=\"{}\" media-type=\"image/jpeg\"/>",
            esc(name.trim_start_matches("OEBPS/"))
        );
    }
    spine.push_str("<itemref idref=\"nav\" linear=\"no\"/>\n");
    format!(
        "<?xml version=\"1.0\" encoding=\"utf-8\"?>\n\
         <package xmlns=\"http://www.idpf.org/2007/opf\" version=\"3.0\" unique-identifier=\"uid\" xml:lang=\"en\">\n\
         <metadata xmlns:dc=\"http://purl.org/dc/elements/1.1/\">\n\
         <dc:identifier id=\"uid\">{BOOK_UID}</dc:identifier>\n<dc:title>{title}</dc:title>\n\
         <dc:creator id=\"creator\">Deckle</dc:creator>\n<dc:language>en</dc:language>\n\
         <dc:date>{date}</dc:date>\n<meta property=\"dcterms:modified\">{modified}</meta>\n\
         <meta name=\"cover\" content=\"cover-image\"/>\n</metadata>\n\
         <manifest>\n{manifest}</manifest>\n<spine toc=\"ncx\">\n{spine}</spine>\n</package>\n",
        title = esc(title),
        date = &modified[..10],
    )
}

fn nav(title: &str, parts: &[Part], has_index: bool) -> Vec<u8> {
    let mut list = String::from("<li><a href=\"contents.xhtml\">Contents</a></li>\n");
    for p in parts {
        let _ = writeln!(
            list,
            "<li><a href=\"{}\">{}</a>\n<ol>",
            esc(&p.file),
            esc(&p.name)
        );
        for e in &p.entries {
            let _ = writeln!(
                list,
                "<li><a href=\"{}\">{}</a></li>",
                esc(&pages::link(&p.file, &e.saved.id)),
                esc(&e.title)
            );
        }
        list.push_str("</ol></li>\n");
    }
    if has_index {
        list.push_str("<li><a href=\"index.xhtml\">Index by ingredient</a></li>\n");
    }
    let start = parts.first().map_or("contents.xhtml", |p| p.file.as_str());
    pages::xhtml(
        title,
        &format!(
            "<nav epub:type=\"toc\" id=\"toc\"><h1>Contents</h1><ol>\n{list}</ol></nav>\n\
             <nav epub:type=\"landmarks\" id=\"landmarks\" hidden=\"hidden\"><ol>\n\
             <li><a epub:type=\"toc\" href=\"contents.xhtml\">Contents</a></li>\n\
             <li><a epub:type=\"bodymatter\" href=\"{}\">Start of the book</a></li>\n</ol></nav>",
            esc(start)
        ),
    )
}

/// One NCX entry, numbered before the entries nested in it.
fn point(
    order: &mut u32,
    label: &str,
    src: &str,
    kids: &mut dyn FnMut(&mut u32) -> String,
) -> String {
    *order += 1;
    let n = *order;
    let inner = kids(order);
    format!(
        "<navPoint id=\"np{n}\" playOrder=\"{n}\"><navLabel><text>{}</text></navLabel><content src=\"{}\"/>{inner}</navPoint>\n",
        esc(label),
        esc(src)
    )
}

fn ncx(title: &str, parts: &[Part], has_index: bool) -> String {
    let mut order = 0;
    let none = &mut |_: &mut u32| String::new();
    let mut points = point(&mut order, "Contents", "contents.xhtml", none);
    for p in parts {
        points.push_str(&point(&mut order, &p.name, &p.file, &mut |order| {
            p.entries
                .iter()
                .map(|e| {
                    let href = pages::link(&p.file, &e.saved.id);
                    point(order, &e.title, &href, &mut |_| String::new())
                })
                .collect()
        }));
    }
    if has_index {
        points.push_str(&point(
            &mut order,
            "Index by ingredient",
            "index.xhtml",
            none,
        ));
    }
    format!(
        "<?xml version=\"1.0\" encoding=\"utf-8\"?>\n\
         <ncx xmlns=\"http://www.daisy.org/z3986/2005/ncx/\" version=\"2005-1\">\n\
         <head><meta name=\"dtb:uid\" content=\"{BOOK_UID}\"/><meta name=\"dtb:depth\" content=\"2\"/></head>\n\
         <docTitle><text>{}</text></docTitle>\n<navMap>\n{points}</navMap>\n</ncx>\n",
        esc(title)
    )
}
