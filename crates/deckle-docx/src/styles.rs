// SPDX-License-Identifier: GPL-3.0-or-later
//! `styles.xml` and `numbering.xml`: what a style id means for headings, inline formatting,
//! alignment and lists.

use std::collections::HashMap;

use roxmltree::Node;

use crate::package::attr;

/// The inline formatting Deckle carries over.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Fmt {
    pub bold: bool,
    pub italic: bool,
    pub underline: bool,
    pub strike: bool,
    pub small_caps: bool,
    pub sup: bool,
    pub sub: bool,
    pub hidden: bool,
    /// Font size in half-points (`w:sz`), when anything on the chain sets one.
    pub size: Option<u16>,
}

/// The formatting one `w:rPr` sets; `None` leaves the inherited value.
#[derive(Clone, Copy, Debug, Default)]
pub struct FmtPatch {
    bold: Option<bool>,
    italic: Option<bool>,
    underline: Option<bool>,
    strike: Option<bool>,
    small_caps: Option<bool>,
    vert: Option<(bool, bool)>,
    hidden: Option<bool>,
    size: Option<u16>,
}

pub fn child<'a, 'i>(n: Node<'a, 'i>, name: &str) -> Option<Node<'a, 'i>> {
    n.children().find(|c| c.tag_name().name() == name)
}

/// `<w:b/>` is on; `w:val="0"`, `"false"` or `"off"` turns it off.
fn toggle(n: Node) -> bool {
    !matches!(attr(n, "val"), Some("0" | "false" | "off"))
}

impl FmtPatch {
    pub fn parse(rpr: Node) -> Self {
        let mut p = FmtPatch::default();
        for c in rpr.children().filter(Node::is_element) {
            match c.tag_name().name() {
                "b" => p.bold = Some(toggle(c)),
                "i" => p.italic = Some(toggle(c)),
                "u" => p.underline = Some(attr(c, "val").is_none_or(|v| v != "none")),
                "strike" | "dstrike" => p.strike = Some(toggle(c)),
                "smallCaps" => p.small_caps = Some(toggle(c)),
                "vanish" => p.hidden = Some(toggle(c)),
                "sz" => p.size = attr(c, "val").and_then(|v| v.parse().ok()),
                "vertAlign" => {
                    p.vert = Some(match attr(c, "val") {
                        Some("superscript") => (true, false),
                        Some("subscript") => (false, true),
                        _ => (false, false),
                    })
                }
                _ => {}
            }
        }
        p
    }

    pub fn apply(&self, f: &mut Fmt) {
        let set = |dst: &mut bool, v: Option<bool>| {
            if let Some(v) = v {
                *dst = v;
            }
        };
        set(&mut f.bold, self.bold);
        set(&mut f.italic, self.italic);
        set(&mut f.underline, self.underline);
        set(&mut f.strike, self.strike);
        set(&mut f.small_caps, self.small_caps);
        set(&mut f.hidden, self.hidden);
        if let Some((sup, sub)) = self.vert {
            f.sup = sup;
            f.sub = sub;
        }
        if self.size.is_some() {
            f.size = self.size;
        }
    }
}

#[derive(Clone, Debug, Default)]
struct Style {
    name: String,
    based_on: Option<String>,
    rpr: FmtPatch,
    outline: Option<u8>,
    num: Option<(String, u8)>,
    jc: Option<String>,
}

#[derive(Debug, Default)]
pub struct Styles {
    map: HashMap<String, Style>,
    pub default_paragraph: Option<String>,
    /// `w:docDefaults` language, e.g. `en-US`.
    pub language: Option<String>,
    /// `w:docDefaults` font size in half-points.
    pub default_size: Option<u16>,
}

/// `w:numPr` as (numId, ilvl).
pub fn num_pr(ppr: Node) -> Option<(String, u8)> {
    let n = child(ppr, "numPr")?;
    let id = child(n, "numId").and_then(|c| attr(c, "val"))?.to_owned();
    let lvl = child(n, "ilvl")
        .and_then(|c| attr(c, "val"))
        .and_then(|v| v.parse().ok())
        .unwrap_or(0);
    Some((id, lvl))
}

pub fn outline_level(ppr: Node) -> Option<u8> {
    child(ppr, "outlineLvl")
        .and_then(|c| attr(c, "val"))
        .and_then(|v| v.parse().ok())
}

impl Styles {
    pub fn parse(doc: Option<&roxmltree::Document>) -> Self {
        let mut out = Styles::default();
        let Some(doc) = doc else { return out };
        let root = doc.root_element();
        let defaults = root
            .descendants()
            .find(|n| n.tag_name().name() == "docDefaults");
        out.language = defaults
            .and_then(|d| d.descendants().find(|n| n.tag_name().name() == "lang"))
            .and_then(|l| attr(l, "val"))
            .map(str::to_owned);
        out.default_size = defaults
            .and_then(|d| d.descendants().find(|n| n.tag_name().name() == "sz"))
            .and_then(|s| attr(s, "val"))
            .and_then(|v| v.parse().ok());
        for s in root.children().filter(|n| n.tag_name().name() == "style") {
            let Some(id) = attr(s, "styleId") else {
                continue;
            };
            if attr(s, "type") == Some("paragraph") && attr(s, "default") == Some("1") {
                out.default_paragraph = Some(id.to_owned());
            }
            let ppr = child(s, "pPr");
            let style = Style {
                name: child(s, "name")
                    .and_then(|n| attr(n, "val"))
                    .unwrap_or(id)
                    .to_lowercase(),
                based_on: child(s, "basedOn")
                    .and_then(|n| attr(n, "val"))
                    .map(str::to_owned),
                rpr: child(s, "rPr").map(FmtPatch::parse).unwrap_or_default(),
                outline: ppr.and_then(outline_level),
                num: ppr.and_then(num_pr),
                jc: ppr
                    .and_then(|p| child(p, "jc"))
                    .and_then(|j| attr(j, "val"))
                    .map(str::to_owned),
            };
            out.map.insert(id.to_owned(), style);
        }
        out
    }

    /// The style and its ancestors, most derived first.
    fn chain(&self, id: &str) -> Vec<&Style> {
        let mut out = Vec::new();
        let mut cur = self.map.get(id);
        while let Some(s) = cur {
            if out.len() > 32 {
                break;
            }
            out.push(s);
            cur = s.based_on.as_deref().and_then(|b| self.map.get(b));
        }
        out
    }

    /// The style's name, lowercase (built-in names are English whatever the UI language).
    pub fn name(&self, id: &str) -> String {
        self.map
            .get(id)
            .map(|s| s.name.clone())
            .unwrap_or_else(|| id.to_lowercase())
    }

    /// 1-based heading level: a `heading N` style, else an outline level on the style chain.
    pub fn heading_level(&self, id: &str) -> Option<u8> {
        for s in self.chain(id) {
            if let Some(n) = s.name.strip_prefix("heading ")
                && let Ok(n) = n.trim().parse::<u8>()
                && (1..=9).contains(&n)
            {
                return Some(n);
            }
            if let Some(l) = s.outline {
                return (l < 9).then_some(l + 1);
            }
        }
        None
    }

    pub fn fmt(&self, id: &str) -> Fmt {
        let mut f = Fmt::default();
        for s in self.chain(id).into_iter().rev() {
            s.rpr.apply(&mut f);
        }
        f
    }

    pub fn num(&self, id: &str) -> Option<(String, u8)> {
        self.chain(id).into_iter().find_map(|s| s.num.clone())
    }

    pub fn jc(&self, id: &str) -> Option<String> {
        self.chain(id).into_iter().find_map(|s| s.jc.clone())
    }
}

#[derive(Debug, Default)]
pub struct Numbering {
    /// numId → abstractNumId
    nums: HashMap<String, String>,
    /// (abstractNumId, ilvl) → (numFmt, start)
    levels: HashMap<(String, u8), (String, u32)>,
}

impl Numbering {
    pub fn parse(doc: Option<&roxmltree::Document>) -> Self {
        let mut out = Numbering::default();
        let Some(doc) = doc else { return out };
        for n in doc.root_element().children().filter(Node::is_element) {
            match n.tag_name().name() {
                "abstractNum" => {
                    let Some(aid) = attr(n, "abstractNumId") else {
                        continue;
                    };
                    for l in n.children().filter(|c| c.tag_name().name() == "lvl") {
                        let Some(ilvl) = attr(l, "ilvl").and_then(|v| v.parse().ok()) else {
                            continue;
                        };
                        let fmt = child(l, "numFmt")
                            .and_then(|f| attr(f, "val"))
                            .unwrap_or("decimal")
                            .to_owned();
                        let start = child(l, "start")
                            .and_then(|f| attr(f, "val"))
                            .and_then(|v| v.parse().ok())
                            .unwrap_or(1);
                        out.levels.insert((aid.to_owned(), ilvl), (fmt, start));
                    }
                }
                "num" => {
                    if let (Some(id), Some(aid)) = (
                        attr(n, "numId"),
                        child(n, "abstractNumId").and_then(|a| attr(a, "val")),
                    ) {
                        out.nums.insert(id.to_owned(), aid.to_owned());
                    }
                }
                _ => {}
            }
        }
        out
    }

    /// `Some(ordered)` when (numId, ilvl) is a real list level; numId 0 means "no list".
    pub fn ordered(&self, num_id: &str, ilvl: u8) -> Option<bool> {
        if num_id == "0" {
            return None;
        }
        let aid = self.nums.get(num_id)?;
        let fmt = self
            .levels
            .get(&(aid.clone(), ilvl))
            .map(|(f, _)| f.as_str())
            .unwrap_or("bullet");
        Some(!matches!(fmt, "bullet" | "none"))
    }

    pub fn start(&self, num_id: &str, ilvl: u8) -> u32 {
        self.nums
            .get(num_id)
            .and_then(|aid| self.levels.get(&(aid.clone(), ilvl)))
            .map(|(_, s)| *s)
            .unwrap_or(1)
    }
}
