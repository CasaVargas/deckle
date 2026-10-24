// SPDX-License-Identifier: GPL-3.0-or-later
use crate::{MAX_BYTES, Note};
use pulldown_cmark::{Event, Options, Parser, Tag, TagEnd, html};
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};

pub struct Heading {
    pub level: u8,
    pub text: String,
    pub id: String,
}

fn options() -> Options {
    Options::ENABLE_TABLES | Options::ENABLE_STRIKETHROUGH | Options::ENABLE_WIKILINKS
}

fn slug(s: &str) -> String {
    let s: String = s
        .to_lowercase()
        .chars()
        .map(|c| if c.is_whitespace() { '-' } else { c })
        .filter(|c| c.is_alphanumeric() || matches!(c, '-' | '_'))
        .collect();
    if s.is_empty() { "section".into() } else { s }
}

pub fn headings(body: &str) -> Vec<Heading> {
    let mut headings = Vec::new();
    let mut current: Option<(u8, String)> = None;
    let mut used = HashSet::new();
    for event in Parser::new_ext(body, options()) {
        match event {
            Event::Start(Tag::Heading { level, .. }) => {
                current = Some((level as u8, String::new()))
            }
            Event::Text(t) | Event::Code(t) => {
                if let Some((_, text)) = &mut current {
                    text.push_str(&t);
                }
            }
            Event::SoftBreak | Event::HardBreak => {
                if let Some((_, text)) = &mut current {
                    text.push(' ');
                }
            }
            Event::End(TagEnd::Heading(_)) => {
                if let Some((level, text)) = current.take() {
                    let base = slug(&text);
                    let mut id = base.clone();
                    let mut suffix = 2;
                    while !used.insert(id.clone()) {
                        id = format!("{base}-{suffix}");
                        suffix += 1;
                    }
                    headings.push(Heading { level, text, id });
                }
            }
            _ => {}
        }
    }
    headings
}

pub fn chapter(i: usize) -> String {
    format!("chapter-{:03}.xhtml", i + 1)
}

fn decode(s: &str) -> Option<String> {
    let mut bytes = Vec::new();
    let mut chars = s.as_bytes().iter().copied();
    while let Some(c) = chars.next() {
        if c == b'%' {
            let a = (chars.next()? as char).to_digit(16)?;
            let b = (chars.next()? as char).to_digit(16)?;
            bytes.push((a * 16 + b) as u8);
        } else {
            bytes.push(c);
        }
    }
    String::from_utf8(bytes).ok()
}

pub struct Asset {
    pub name: String,
    pub mime: &'static str,
    pub bytes: Vec<u8>,
}

pub struct Renderer<'a> {
    notes: &'a [Note],
    root: &'a Path,
    warnings: &'a mut Vec<String>,
    total: u64,
    image_paths: HashMap<PathBuf, String>,
    pub assets: Vec<Asset>,
}

impl<'a> Renderer<'a> {
    pub fn new(
        notes: &'a [Note],
        root: &'a Path,
        total: u64,
        warnings: &'a mut Vec<String>,
    ) -> Self {
        Self {
            notes,
            root,
            warnings,
            total,
            image_paths: HashMap::new(),
            assets: Vec::new(),
        }
    }

    fn warn(&mut self, i: usize, message: String) {
        let message = format!(
            "{}: {message}",
            self.notes[i]
                .path
                .file_name()
                .unwrap_or_default()
                .to_string_lossy()
        );
        if !self.warnings.contains(&message) {
            self.warnings.push(message);
        }
    }

    fn link(&self, i: usize, dest: &str) -> Option<String> {
        let lower = dest.to_ascii_lowercase();
        if ["https://", "http://", "mailto:"]
            .iter()
            .any(|p| lower.starts_with(p))
        {
            return Some(dest.into());
        }
        let decoded = decode(dest)?;
        let (file, anchor) = decoded
            .split_once('#')
            .map_or((decoded.as_str(), None), |(f, a)| (f, Some(a)));
        let file = file.strip_prefix("./").unwrap_or(file);
        if file.contains(['/', '\\', ':']) || file == ".." {
            return None;
        }
        let index = if file.is_empty() {
            i
        } else {
            let key = file.to_lowercase();
            let candidates: Vec<usize> = self
                .notes
                .iter()
                .enumerate()
                .filter_map(|(n, note)| {
                    let name = note.path.file_name()?.to_str()?.to_lowercase();
                    let stem = note.path.file_stem()?.to_str()?.to_lowercase();
                    (key == name || key == stem).then_some(n)
                })
                .collect();
            if candidates.len() != 1 {
                return None;
            }
            candidates[0]
        };
        let mut href = chapter(index);
        if let Some(anchor) = anchor.filter(|a| !a.is_empty()) {
            let h = self.notes[index]
                .headings
                .iter()
                .find(|h| h.id == anchor || h.text.to_lowercase() == anchor.to_lowercase())
                .or_else(|| {
                    self.notes[index]
                        .headings
                        .iter()
                        .find(|h| h.id == slug(anchor))
                })?;
            href.push('#');
            href.push_str(&h.id);
        }
        Some(href)
    }

    fn image(&mut self, dest: &str) -> Option<String> {
        let decoded = decode(dest)?;
        if decoded.contains([':', '\\', '\0']) || decoded.starts_with('/') {
            return None;
        }
        let path = self.root.join(decoded).canonicalize().ok()?;
        if !path.starts_with(self.root) || !path.is_file() {
            return None;
        }
        if let Some(name) = self.image_paths.get(&path) {
            return Some(name.clone());
        }
        let size = std::fs::metadata(&path).ok()?.len();
        if size > 10 * 1024 * 1024 || self.total + size > MAX_BYTES {
            return None;
        }
        let bytes = std::fs::read(&path).ok()?;
        // Signatures, not extensions: malformed markup cannot be packaged as an image.
        let (ext, mime) = if bytes.starts_with(b"\x89PNG\r\n\x1a\n") {
            ("png", "image/png")
        } else if bytes.starts_with(b"\xff\xd8\xff") {
            ("jpg", "image/jpeg")
        } else if bytes.starts_with(b"GIF87a") || bytes.starts_with(b"GIF89a") {
            ("gif", "image/gif")
        } else {
            return None;
        };
        self.total += size;
        let name = format!("images/image-{:03}.{ext}", self.assets.len() + 1);
        self.image_paths.insert(path, name.clone());
        self.assets.push(Asset {
            name: name.clone(),
            mime,
            bytes,
        });
        Some(name)
    }

    pub fn render(&mut self, i: usize) -> String {
        let events: Vec<_> = Parser::new_ext(&self.notes[i].body, options())
            .map(Event::into_static)
            .collect();
        let mut out = Vec::new();
        let mut heading = 0;
        let mut links = Vec::new();
        let mut images = Vec::new();
        for event in events {
            match event {
                Event::Start(Tag::Heading { level, .. }) => {
                    let id = self.notes[i].headings[heading].id.clone();
                    heading += 1;
                    out.push(Event::Start(Tag::Heading {
                        level,
                        id: Some(id.into()),
                        classes: vec![],
                        attrs: vec![],
                    }));
                }
                Event::Start(Tag::Link {
                    link_type,
                    dest_url,
                    title,
                    id,
                }) => {
                    let href = self.link(i, &dest_url);
                    links.push(href.is_some());
                    if let Some(href) = href {
                        out.push(Event::Start(Tag::Link {
                            link_type,
                            dest_url: href.into(),
                            title,
                            id,
                        }));
                    } else {
                        self.warn(
                            i,
                            format!(
                                "unresolved or unsupported link {dest_url}; kept its label as text"
                            ),
                        );
                    }
                }
                Event::End(TagEnd::Link) => {
                    if links.pop().unwrap_or(false) {
                        out.push(event);
                    }
                }
                Event::Start(Tag::Image {
                    link_type,
                    dest_url,
                    title,
                    id,
                }) => {
                    let href = self.image(&dest_url);
                    images.push(href.is_some());
                    if let Some(href) = href {
                        out.push(Event::Start(Tag::Image {
                            link_type,
                            dest_url: href.into(),
                            title,
                            id,
                        }));
                    } else {
                        self.warn(i, format!("image or embed {dest_url} unavailable (use a local PNG, JPEG or GIF within the notes folder); kept its alt text"));
                    }
                }
                Event::End(TagEnd::Image) => {
                    if images.pop().unwrap_or(false) {
                        out.push(event);
                    }
                }
                Event::Html(text) | Event::InlineHtml(text) => {
                    self.warn(i, "raw HTML rendered as text".into());
                    out.push(Event::Text(text));
                }
                Event::Start(Tag::HtmlBlock) | Event::End(TagEnd::HtmlBlock) => {}
                other => out.push(other),
            }
        }
        let mut body = String::new();
        html::push_html(&mut body, out.into_iter());
        body
    }
}
