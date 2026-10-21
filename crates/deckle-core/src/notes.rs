// SPDX-License-Identifier: GPL-3.0-or-later
//! Opt-in pop-up notes pass (#31, `2026-10-21-popup-notes-design.md`): footnotes and endnotes
//! become `epub:type="noteref"` links to `aside epub:type="footnote"` notes, which the Kindle
//! opens in a pop-up. Works across every spine document at once; edits are text splices on the
//! existing markup, so everything else stays byte for byte, and running it twice changes nothing.

use std::collections::{BTreeMap, HashMap};
use std::ops::Range;
use std::sync::LazyLock;

use regex::Regex;
use serde::Serialize;

use crate::epub::{join, percent_decode};
use crate::typography::{markup_len, tag_name};

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize)]
pub struct NotesReport {
    /// Notes the book already marked (`epub:type` or ARIA role) and that were converted.
    pub semantic: u32,
    /// Notes found by their two-way link or note class and converted.
    pub heuristic: u32,
    /// Note-looking links whose target has no way back, left as they are.
    pub skipped_one_way: u32,
}

/// Elements that can become the note's `aside` (design §2).
const NOTE_BLOCKS: &[&str] = &["p", "li", "div", "aside"];
/// Elements a target `id` may sit on inside a note block; the `id` then moves to the block.
const INLINE: &[&str] = &[
    "a", "span", "sup", "sub", "b", "i", "em", "strong", "small", "cite", "abbr", "dfn", "mark",
    "q", "s", "u",
];
const VOID: &[&str] = &[
    "area", "base", "br", "col", "embed", "hr", "img", "input", "link", "meta", "param", "source",
    "track", "wbr",
];
/// Longer than this (in characters) isn't a note (design §3.4).
const MAX_NOTE_CHARS: usize = 2000;

static ATTR: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r#"(?s)([^\s=/>]+)\s*=\s*(?:"([^"]*)"|'([^']*)')"#).unwrap());
/// A noteref's visible text: 1–4 digits, a lowercase letter, a Roman numeral up to `xx`, or a
/// reference mark, optionally in brackets (design §3.2).
static LABEL: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"^[\[(]?(?:\d{1,4}|[a-z]|x?(?:ix|iv|v?i{0,3})|xx|[*†‡§¶#])[\])]?$").unwrap()
});
static REF_CLASS: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"(?i)note|fn|ref").unwrap());
static NOTE_NAME: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"(?i)foot.?note|end.?note|rearnote").unwrap());
static NOTES_TOKEN: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"(?i)^notes?$").unwrap());

struct El {
    /// Local name, lowercased.
    name: String,
    open: Range<usize>,
    /// The end tag; `None` when self-closing or never closed.
    close: Option<Range<usize>>,
    parent: Option<usize>,
}

struct Doc {
    name: String,
    els: Vec<El>,
    /// First element carrying each `id`.
    ids: HashMap<String, usize>,
}

impl Doc {
    fn parse(name: &str, text: &str) -> Doc {
        let mut els: Vec<El> = Vec::new();
        let mut stack: Vec<usize> = Vec::new();
        let mut pos = 0;
        while pos < text.len() {
            let rest = &text[pos..];
            if let Some(markup) = rest.strip_prefix('<') {
                let len = 1 + markup_len(markup);
                if let Some((tag, closing, self_closing)) = tag_name(&rest[..len]) {
                    if closing {
                        if let Some(at) = stack.iter().rposition(|&i| els[i].name == tag) {
                            els[stack[at]].close = Some(pos..pos + len);
                            stack.truncate(at);
                        }
                    } else {
                        let open_ended = !self_closing && !VOID.contains(&tag.as_str());
                        els.push(El {
                            name: tag,
                            open: pos..pos + len,
                            close: None,
                            parent: stack.last().copied(),
                        });
                        if open_ended {
                            stack.push(els.len() - 1);
                        }
                    }
                }
                pos += len;
            } else {
                pos += rest.find('<').unwrap_or(rest.len());
            }
        }
        let mut ids = HashMap::new();
        for (i, el) in els.iter().enumerate() {
            if let Some(id) = attr(&text[el.open.clone()], "id") {
                ids.entry(id.to_owned()).or_insert(i);
            }
        }
        Doc {
            name: name.to_owned(),
            els,
            ids,
        }
    }

    fn ancestors(&self, i: usize) -> impl Iterator<Item = usize> + '_ {
        std::iter::successors(self.els[i].parent, |&p| self.els[p].parent)
    }

    /// Whether element `j` sits inside element `i`.
    fn contains(&self, i: usize, j: usize) -> bool {
        self.ancestors(j).any(|a| a == i)
    }

    fn inner<'t>(&self, text: &'t str, i: usize) -> &'t str {
        let el = &self.els[i];
        match &el.close {
            Some(c) => &text[el.open.end..c.start],
            None => "",
        }
    }
}

/// The value of attribute `name` in a start tag (names compare case-insensitively).
fn attr<'t>(tag: &'t str, name: &str) -> Option<&'t str> {
    let after_name = tag.find(|c: char| c.is_whitespace()).unwrap_or(tag.len());
    ATTR.captures_iter(&tag[after_name..])
        .find(|c| c[1].eq_ignore_ascii_case(name))
        .map(|c| c.get(2).or(c.get(3)).unwrap().as_str())
}

fn tokens<'t>(tag: &'t str, name: &str) -> Vec<&'t str> {
    attr(tag, name)
        .map(|v| v.split_whitespace().collect())
        .unwrap_or_default()
}

/// Adds `token` to the tag's `epub:type`, or adds the attribute.
fn add_epub_type(tag: &str, token: &str) -> String {
    let after_name = tag.find(|c: char| c.is_whitespace()).unwrap_or(tag.len());
    if let Some(c) = ATTR
        .captures_iter(&tag[after_name..])
        .find(|c| c[1].eq_ignore_ascii_case("epub:type"))
    {
        let value = c.get(2).or(c.get(3)).unwrap();
        if value.as_str().split_whitespace().any(|t| t == token) {
            return tag.to_owned();
        }
        let at = after_name + value.end();
        let sep = if value.as_str().trim().is_empty() {
            ""
        } else {
            " "
        };
        return format!("{}{sep}{token}{}", &tag[..at], &tag[at..]);
    }
    add_attr(tag, "epub:type", token)
}

fn add_attr(tag: &str, name: &str, value: &str) -> String {
    let body = tag
        .strip_suffix("/>")
        .or(tag.strip_suffix('>'))
        .unwrap_or(tag);
    let end = &tag[body.len()..];
    format!("{} {name}=\"{value}\"{end}", body.trim_end())
}

fn remove_attr(tag: &str, name: &str) -> String {
    let re = Regex::new(&format!(
        r#"(?i)\s+{}\s*=\s*(?:"[^"]*"|'[^']*')"#,
        regex::escape(name)
    ))
    .unwrap();
    re.replace(tag, "").into_owned()
}

/// Renames a start or end tag, keeping everything after the name.
fn rename(tag: &str, to: &str) -> String {
    let start = if tag.starts_with("</") { 2 } else { 1 };
    let len = tag[start..]
        .find(|c: char| c.is_whitespace() || c == '/' || c == '>')
        .unwrap_or(tag.len() - start);
    format!("{}{to}{}", &tag[..start], &tag[start + len..])
}

/// Visible text with tags stripped and the common entities decoded.
fn visible_text(markup: &str) -> String {
    let mut out = String::new();
    let mut rest = markup;
    while !rest.is_empty() {
        if let Some(m) = rest.strip_prefix('<') {
            rest = &rest[1 + markup_len(m)..];
        } else if rest.starts_with('&')
            && let Some(semi) = rest[..rest.len().min(12)].find(';')
        {
            let ent = &rest[1..semi];
            let ch = match ent {
                "amp" => Some('&'),
                "lt" => Some('<'),
                "gt" => Some('>'),
                "quot" => Some('"'),
                "apos" => Some('\''),
                "nbsp" => Some('\u{a0}'),
                "dagger" => Some('†'),
                "Dagger" => Some('‡'),
                "sect" => Some('§'),
                "para" => Some('¶'),
                _ => ent
                    .strip_prefix("#x")
                    .or(ent.strip_prefix("#X"))
                    .and_then(|h| u32::from_str_radix(h, 16).ok())
                    .or_else(|| ent.strip_prefix('#').and_then(|d| d.parse().ok()))
                    .and_then(char::from_u32),
            };
            match ch {
                Some(c) => {
                    out.push(c);
                    rest = &rest[semi + 1..];
                }
                None => {
                    out.push('&');
                    rest = &rest[1..];
                }
            }
        } else {
            let len = rest.find(['<', '&']).unwrap_or(rest.len()).max(1);
            let len = (1..=len).rev().find(|l| rest.is_char_boundary(*l)).unwrap();
            out.push_str(&rest[..len]);
            rest = &rest[len..];
        }
    }
    out
}

fn is_label(text: &str) -> bool {
    let t = text.trim_matches(|c: char| c.is_whitespace());
    let inner = t.trim_matches(['[', ']', '(', ')']);
    !inner.is_empty() && LABEL.is_match(t)
}

/// The pop-up kind a note marks itself as: `footnote`, `endnote` or `rearnote`, from its
/// `epub:type` or ARIA role; `Some("")` for a plain `epub:type="note"`.
fn note_marker(tag: &str) -> Option<&'static str> {
    let types = tokens(tag, "epub:type");
    let roles = tokens(tag, "role");
    for kind in ["footnote", "endnote", "rearnote"] {
        if types.contains(&kind) || roles.contains(&format!("doc-{kind}").as_str()) {
            return Some(kind);
        }
    }
    types.contains(&"note").then_some("")
}

/// Whether a section marks itself as endnotes (`epub:type="endnotes"`, `role="doc-endnotes"`).
fn endnotes_section(tag: &str) -> bool {
    let types = tokens(tag, "epub:type");
    types.contains(&"endnotes")
        || types.contains(&"rearnotes")
        || tokens(tag, "role").contains(&"doc-endnotes")
}

fn is_noteref(tag: &str) -> bool {
    tokens(tag, "epub:type").contains(&"noteref") || tokens(tag, "role").contains(&"doc-noteref")
}

/// A link's target: (document index, element index), when it is a fragment in the spine.
fn resolve(docs: &[Doc], from: usize, href: &str) -> Option<(usize, usize)> {
    let (file, frag) = href.split_once('#')?;
    if frag.is_empty() || file.contains(':') {
        return None;
    }
    let name = if file.is_empty() {
        docs[from].name.clone()
    } else {
        let dir = docs[from].name.rsplit_once('/').map_or("", |(d, _)| d);
        join(dir, &percent_decode(file))
    };
    let d = docs.iter().position(|doc| doc.name == name)?;
    let el = *docs[d].ids.get(&percent_decode(frag))?;
    Some((d, el))
}

/// A note-looking link and where it points.
struct Ref {
    doc: usize,
    a: usize,
    semantic: bool,
}

/// All links to one target `id`.
struct Target {
    doc: usize,
    /// The element carrying the `id`.
    id_el: usize,
    /// The block that becomes the `aside`.
    block: usize,
    refs: Vec<Ref>,
}

impl Target {
    fn semantic(&self, docs: &[Doc], texts: &[String]) -> bool {
        let tag = |i: usize| &texts[self.doc][docs[self.doc].els[i].open.clone()];
        self.refs.iter().any(|r| r.semantic)
            || note_marker(tag(self.block)).is_some()
            || note_marker(tag(self.id_el)).is_some()
    }

    fn contains_ref(&self, docs: &[Doc], r: &Ref) -> bool {
        r.doc == self.doc && docs[self.doc].contains(self.block, r.a)
    }
}

/// Converts the notes in `docs` (zip entry name, XHTML text), in spine order.
pub fn convert(docs: &mut [(String, String)]) -> NotesReport {
    let texts: Vec<String> = docs.iter().map(|(_, t)| t.clone()).collect();
    let parsed: Vec<Doc> = docs.iter().map(|(n, t)| Doc::parse(n, t)).collect();
    let mut report = NotesReport::default();

    // Note-looking links, grouped by the id they point at, in document order.
    let mut targets: BTreeMap<(usize, usize), Target> = BTreeMap::new();
    for (d, doc) in parsed.iter().enumerate() {
        let text = &texts[d];
        for (a, el) in doc.els.iter().enumerate().filter(|(_, el)| el.name == "a") {
            let tag = &text[el.open.clone()];
            let Some((td, t)) = attr(tag, "href").and_then(|h| resolve(&parsed, d, h)) else {
                continue;
            };
            let target_doc = &parsed[td];
            let t_el = &target_doc.els[t];
            let block = if NOTE_BLOCKS.contains(&t_el.name.as_str()) {
                t
            } else if INLINE.contains(&t_el.name.as_str()) {
                match target_doc
                    .ancestors(t)
                    .find(|&b| NOTE_BLOCKS.contains(&target_doc.els[b].name.as_str()))
                {
                    Some(b) => b,
                    None => continue,
                }
            } else {
                continue;
            };
            if td == d && doc.contains(block, a) {
                continue;
            }
            let semantic = is_noteref(tag);
            let styled = REF_CLASS.is_match(attr(tag, "class").unwrap_or(""))
                || doc.els[a + 1..]
                    .iter()
                    .take_while(|e| e.open.start < el.close.as_ref().map_or(0, |c| c.start))
                    .any(|e| e.name == "sup")
                || el.parent.is_some_and(|p| doc.els[p].name == "sup");
            let marked = note_marker(&texts[td][target_doc.els[block].open.clone()]).is_some()
                || note_marker(&texts[td][t_el.open.clone()]).is_some();
            let labelled = is_label(&visible_text(doc.inner(text, a)));
            if !(semantic || (labelled && (styled || marked))) {
                continue;
            }
            targets
                .entry((td, t))
                .or_insert_with(|| Target {
                    doc: td,
                    id_el: t,
                    block,
                    refs: Vec::new(),
                })
                .refs
                .push(Ref {
                    doc: d,
                    a,
                    semantic,
                });
        }
    }

    // Which targets are notes: semantic ones always, heuristic ones with a way back or a
    // note class, under the size limit.
    let mut accepted: Vec<&Target> = Vec::new();
    for target in targets.values() {
        if target.semantic(&parsed, &texts) {
            accepted.push(target);
            continue;
        }
        let doc = &parsed[target.doc];
        let text = &texts[target.doc];
        if visible_text(doc.inner(text, target.block)).chars().count() >= MAX_NOTE_CHARS {
            continue;
        }
        if two_way(&parsed, &texts, target) || note_named(doc, text, target.block) {
            accepted.push(target);
        } else {
            report.skipped_one_way += target.refs.len() as u32;
        }
    }

    // A note's anchor that links back with the same number looks like a noteref to the body
    // paragraph: of two targets that hold each other's refs, the one before its ref is the body.
    let position = |t: &Target| (t.doc, parsed[t.doc].els[t.block].open.start);
    let ref_position = |r: &Ref| (r.doc, parsed[r.doc].els[r.a].open.start);
    let mutual_backwards: Vec<bool> = accepted
        .iter()
        .map(|t| {
            !t.semantic(&parsed, &texts)
                && position(t) < t.refs.iter().map(ref_position).min().unwrap()
                && accepted.iter().any(|o| {
                    !std::ptr::eq(*o, *t)
                        && t.refs.iter().any(|r| o.contains_ref(&parsed, r))
                        && o.refs.iter().any(|r| t.contains_ref(&parsed, r))
                })
        })
        .collect();
    let accepted: Vec<&Target> = accepted
        .into_iter()
        .zip(mutual_backwards)
        .filter_map(|(t, drop)| (!drop).then_some(t))
        .collect();

    // Edits, per document: start offset → (end offset, replacement), plus insertions.
    let mut edits: Vec<BTreeMap<usize, (usize, String)>> = vec![BTreeMap::new(); docs.len()];
    let mut claimed: Vec<(usize, usize)> = Vec::new();
    for target in accepted {
        let doc = &parsed[target.doc];
        let text = &texts[target.doc];
        let block_el = &doc.els[target.block];
        let inline = target.id_el != target.block;
        let block_tag = &text[block_el.open.clone()];
        // One note per block; an inline id can only move to a block without one.
        if claimed.contains(&(target.doc, target.block))
            || (inline && attr(block_tag, "id").is_some())
            || block_el.close.is_none()
        {
            continue;
        }
        claimed.push((target.doc, target.block));
        let mut changed = false;
        let mut edit_tag = |edits: &mut Vec<BTreeMap<usize, (usize, String)>>,
                            d: usize,
                            range: Range<usize>,
                            f: &dyn Fn(&str) -> String| {
            let current = edits[d]
                .get(&range.start)
                .map(|(_, s)| s.clone())
                .unwrap_or_else(|| texts[d][range.clone()].to_owned());
            let new = f(&current);
            if new != current {
                edits[d].insert(range.start, (range.end, new));
                changed = true;
            }
        };
        for r in &target.refs {
            let range = parsed[r.doc].els[r.a].open.clone();
            edit_tag(&mut edits, r.doc, range, &|t| add_epub_type(t, "noteref"));
        }
        let kind = match note_marker(block_tag)
            .or(note_marker(&text[doc.els[target.id_el].open.clone()]))
        {
            Some(k) if !k.is_empty() => k,
            _ if endnotes_section(block_tag)
                || doc
                    .ancestors(target.block)
                    .any(|a| endnotes_section(&text[doc.els[a].open.clone()])) =>
            {
                "endnote"
            }
            _ => "footnote",
        };
        let id = attr(&text[doc.els[target.id_el].open.clone()], "id")
            .unwrap_or_default()
            .to_owned();
        if inline {
            let range = doc.els[target.id_el].open.clone();
            edit_tag(&mut edits, target.doc, range, &|t| remove_attr(t, "id"));
        }
        let number = list_number(doc, text, target.block);
        let first_p = doc
            .els
            .iter()
            .enumerate()
            .skip(target.block + 1)
            .find(|(_, e)| e.parent == Some(target.block))
            .filter(|(_, e)| {
                e.name == "p" && text[block_el.open.end..e.open.start].trim().is_empty()
            })
            .map(|(i, _)| i);
        edit_tag(&mut edits, target.doc, block_el.open.clone(), &|t| {
            let mut t = if block_el.name == "aside" {
                t.to_owned()
            } else {
                rename(t, "aside")
            };
            let typed = tokens(&t, "epub:type");
            if !["footnote", "endnote", "rearnote"]
                .iter()
                .any(|k| typed.contains(k))
            {
                t = add_epub_type(&t, kind);
            }
            if inline {
                t = add_attr(&t, "id", &id);
            }
            t
        });
        if block_el.name != "aside" {
            let close = block_el.close.clone().unwrap();
            edit_tag(&mut edits, target.doc, close, &|t| rename(t, "aside"));
        }
        if let Some(n) = number {
            let at = first_p.map_or(block_el.open.end, |p| doc.els[p].open.end);
            edits[target.doc].insert(at, (at, format!("{n}. ")));
        }
        if changed {
            if target.semantic(&parsed, &texts) {
                report.semantic += 1;
            } else {
                report.heuristic += 1;
            }
        }
    }

    for (d, doc_edits) in edits.into_iter().enumerate() {
        if doc_edits.is_empty() {
            continue;
        }
        let text = &texts[d];
        let mut doc_edits = doc_edits;
        if let Some(html) = parsed[d].els.iter().find(|e| e.name == "html") {
            let tag = &text[html.open.clone()];
            if attr(tag, "xmlns:epub").is_none() {
                doc_edits.insert(
                    html.open.start,
                    (
                        html.open.end,
                        add_attr(tag, "xmlns:epub", "http://www.idpf.org/2007/ops"),
                    ),
                );
            }
        }
        let mut out = String::with_capacity(text.len() + 256);
        let mut last = 0;
        // Insertions sort before the tag that starts at the same offset.
        let mut ordered: Vec<(usize, usize, String)> =
            doc_edits.into_iter().map(|(s, (e, t))| (s, e, t)).collect();
        ordered.sort_by_key(|(s, e, _)| (*s, *e));
        for (start, end, new) in ordered {
            out.push_str(&text[last..start]);
            out.push_str(&new);
            last = end;
        }
        out.push_str(&text[last..]);
        docs[d].1 = out;
    }
    report
}

/// Whether the target block links back to one of its refs: the ref's own `id`, or the nearest
/// ancestor's with one (design §3.3).
fn two_way(docs: &[Doc], texts: &[String], target: &Target) -> bool {
    let doc = &docs[target.doc];
    let backs: Vec<(usize, usize)> = target
        .refs
        .iter()
        .filter_map(|r| {
            let rd = &docs[r.doc];
            std::iter::once(r.a)
                .chain(rd.ancestors(r.a))
                .find(|&i| attr(&texts[r.doc][rd.els[i].open.clone()], "id").is_some())
                .map(|i| (r.doc, i))
        })
        .collect();
    // The block's descendants are the elements after it that start before its end tag.
    let end = doc.els[target.block].close.as_ref().map_or(0, |c| c.start);
    doc.els[target.block + 1..]
        .iter()
        .take_while(|e| e.open.start < end)
        .filter(|e| e.name == "a")
        .filter_map(|e| attr(&texts[target.doc][e.open.clone()], "href"))
        .filter_map(|h| resolve(docs, target.doc, h))
        .any(|back| backs.contains(&back))
}

/// Whether the block's class, or an enclosing section's id or class, names it a note.
fn note_named(doc: &Doc, text: &str, block: usize) -> bool {
    let names_a_note =
        |v: &str| NOTE_NAME.is_match(v) || v.split_whitespace().any(|t| NOTES_TOKEN.is_match(t));
    let tag = |i: usize| &text[doc.els[i].open.clone()];
    attr(tag(block), "class").is_some_and(names_a_note)
        || doc
            .ancestors(block)
            .filter(|&a| !matches!(doc.els[a].name.as_str(), "html" | "body"))
            .any(|a| {
                ["class", "id"]
                    .iter()
                    .any(|n| attr(tag(a), n).is_some_and(names_a_note))
            })
}

/// The number an `ol` shows for this `li`, when its text doesn't already start with it.
fn list_number(doc: &Doc, text: &str, block: usize) -> Option<i64> {
    let el = &doc.els[block];
    let ol = el.parent.filter(|&p| doc.els[p].name == "ol")?;
    let ol_tag = &text[doc.els[ol].open.clone()];
    let items: Vec<usize> = (ol + 1..doc.els.len())
        .filter(|&i| doc.els[i].parent == Some(ol) && doc.els[i].name == "li")
        .collect();
    let reversed = attr(ol_tag, "reversed").is_some();
    let step = if reversed { -1 } else { 1 };
    let mut n = attr(ol_tag, "start")
        .and_then(|s| s.trim().parse().ok())
        .unwrap_or(if reversed { items.len() as i64 } else { 1 });
    for i in items {
        if let Some(v) =
            attr(&text[doc.els[i].open.clone()], "value").and_then(|v| v.trim().parse().ok())
        {
            n = v;
        }
        if i == block {
            let shown = visible_text(doc.inner(text, block));
            let shown = shown.trim_start();
            let label = n.to_string();
            let has_it = shown
                .strip_prefix(&label)
                .is_some_and(|r| !r.starts_with(|c: char| c.is_ascii_digit()));
            return (!has_it).then_some(n);
        }
        n += step;
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    fn one(body: &str) -> (String, NotesReport) {
        let doc = format!(
            "<html xmlns=\"http://www.w3.org/1999/xhtml\" xmlns:epub=\"http://www.idpf.org/2007/ops\"><body>{body}</body></html>"
        );
        let mut docs = vec![("OEBPS/a.xhtml".to_owned(), doc)];
        let r = convert(&mut docs);
        let out = &docs[0].1;
        let start = out.find("<body>").unwrap() + 6;
        let end = out.rfind("</body>").unwrap();
        (out[start..end].to_owned(), r)
    }

    #[test]
    fn labels() {
        for ok in [
            "1", "12", "9999", "[3]", "(4)", "a", "iv", "xix", "xx", "*", "†", "‡", "§", "¶", "#",
        ] {
            assert!(is_label(ok), "{ok}");
        }
        for no in ["", "12345", "A", "xxi", "ab", "1.", "[]", "note", "↩"] {
            assert!(!is_label(no), "{no}");
        }
    }

    #[test]
    fn inline_target_id_moves_to_the_new_aside() {
        let (out, r) = one(
            r##"<p>Claim<sup><a href="#n3" id="r3">3</a></sup>.</p><p class="x"><a id="n3" href="#r3">3</a> The note.</p>"##,
        );
        assert_eq!(
            out,
            r##"<p>Claim<sup><a href="#n3" id="r3" epub:type="noteref">3</a></sup>.</p><aside class="x" epub:type="footnote" id="n3"><a href="#r3">3</a> The note.</aside>"##
        );
        assert_eq!(r.heuristic, 1);
    }

    #[test]
    fn note_class_without_backlink_converts_and_long_blocks_dont() {
        let long = "x".repeat(MAX_NOTE_CHARS);
        let (out, r) = one(&format!(
            r##"<p>a<sup><a href="#n1">1</a></sup> b<sup><a href="#n2">2</a></sup></p><div class="endnotes"><p id="n1">One.</p><p id="n2">{long}</p></div>"##
        ));
        assert!(
            out.contains(r#"<aside id="n1" epub:type="footnote">One.</aside>"#),
            "{out}"
        );
        assert!(out.contains(r#"<p id="n2">"#), "{out}");
        assert_eq!((r.heuristic, r.skipped_one_way), (1, 0));
    }

    #[test]
    fn epub_type_is_extended_not_replaced() {
        assert_eq!(
            add_epub_type(r##"<a epub:type="biblioref" href="#x">"##, "noteref"),
            r##"<a epub:type="biblioref noteref" href="#x">"##
        );
        assert_eq!(add_epub_type("<br/>", "x"), r#"<br epub:type="x"/>"#);
        assert_eq!(rename("</p >", "aside"), "</aside >");
        assert_eq!(rename(r#"<li id="a">"#, "aside"), r#"<aside id="a">"#);
    }

    #[test]
    fn reversed_and_valued_lists_number_right() {
        let (out, _) = one(
            r##"<p>a<sup><a href="#a" id="ra">1</a></sup><sup><a href="#b" id="rb">2</a></sup></p><ol reversed="reversed"><li id="a">A <a href="#ra">^</a></li><li id="b" value="7">B <a href="#rb">^</a></li></ol>"##,
        );
        assert!(out.contains(r#"epub:type="footnote">2. A "#), "{out}");
        assert!(out.contains(r#"epub:type="footnote">7. B "#), "{out}");
    }
}
