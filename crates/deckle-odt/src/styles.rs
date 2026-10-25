// SPDX-License-Identifier: GPL-3.0-or-later
//! ODF styles: paragraph and text formatting along the parent chain, list levels, and the
//! document's default language (ODT spec §2, Styles).

use std::collections::HashMap;

use roxmltree::Node;

/// Inline formatting that becomes XHTML elements.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Fmt {
    pub bold: bool,
    pub italic: bool,
    pub underline: bool,
    pub strike: bool,
    pub small_caps: bool,
    pub sup: bool,
    pub sub: bool,
}

/// What one style sets; `None` inherits.
#[derive(Clone, Debug, Default)]
struct Patch {
    bold: Option<bool>,
    italic: Option<bool>,
    underline: Option<bool>,
    strike: Option<bool>,
    small_caps: Option<bool>,
    /// (sup, sub)
    position: Option<(bool, bool)>,
}

#[derive(Clone, Debug, Default)]
struct Def {
    parent: Option<String>,
    /// Lowercased display name (or name).
    display: String,
    patch: Patch,
    align: Option<String>,
    outline: Option<u8>,
}

/// What a paragraph style chain says about the paragraph.
#[derive(Clone, Debug, Default)]
pub struct ParaInfo {
    pub fmt: Fmt,
    /// Lowercased display names along the chain, nearest first.
    pub names: Vec<String>,
    pub align: Option<String>,
    pub outline: Option<u8>,
}

#[derive(Default)]
pub struct Styles {
    /// (family, name) → definition; automatic styles shadow common ones of the same name.
    auto: HashMap<(String, String), Def>,
    common: HashMap<(String, String), Def>,
    /// List style name → ordered? per level (index 0 is level 1).
    lists: HashMap<String, Vec<bool>>,
    pub language: Option<String>,
}

pub fn local<'a>(n: Node<'a, '_>) -> &'a str {
    n.tag_name().name()
}

/// An attribute by local name, whatever its namespace prefix.
pub fn attr<'a>(n: Node<'a, '_>, name: &str) -> Option<&'a str> {
    n.attributes().find(|a| a.name() == name).map(|a| a.value())
}

pub fn child<'a, 'i>(n: Node<'a, 'i>, name: &str) -> Option<Node<'a, 'i>> {
    n.children().find(|c| c.is_element() && local(*c) == name)
}

fn flag(v: Option<&str>, on: impl Fn(&str) -> bool) -> Option<bool> {
    v.map(|v| on(v.trim()))
}

fn patch_of(props: Node) -> Patch {
    let weight = flag(attr(props, "font-weight"), |v| {
        v == "bold" || v.parse::<u32>().is_ok_and(|w| w >= 600)
    });
    let position = attr(props, "text-position").map(|v| {
        let first = v.split_whitespace().next().unwrap_or("");
        match first {
            "super" => (true, false),
            "sub" => (false, true),
            pct => {
                let n: f32 = pct.trim_end_matches('%').parse().unwrap_or(0.0);
                (n > 0.0, n < 0.0)
            }
        }
    });
    Patch {
        bold: weight,
        italic: flag(attr(props, "font-style"), |v| {
            v == "italic" || v == "oblique"
        }),
        underline: flag(attr(props, "text-underline-style"), |v| v != "none"),
        strike: flag(attr(props, "text-line-through-style"), |v| v != "none"),
        small_caps: flag(attr(props, "font-variant"), |v| v == "small-caps"),
        position,
    }
}

fn def_of(s: Node) -> Def {
    let name = attr(s, "name").unwrap_or("");
    Def {
        parent: attr(s, "parent-style-name").map(str::to_owned),
        display: attr(s, "display-name").unwrap_or(name).to_lowercase(),
        patch: child(s, "text-properties")
            .map(patch_of)
            .unwrap_or_default(),
        align: child(s, "paragraph-properties")
            .and_then(|p| attr(p, "text-align"))
            .map(str::to_owned),
        outline: attr(s, "default-outline-level").and_then(|v| v.parse().ok()),
    }
}

impl Styles {
    /// `automatic` holds `office:automatic-styles` elements; `common` holds `office:styles`.
    pub fn parse(automatic: &[Node], common: &[Node]) -> Self {
        let mut st = Styles::default();
        for (containers, auto) in [(automatic, true), (common, false)] {
            for c in containers {
                for s in c.children().filter(|s| s.is_element()) {
                    match local(s) {
                        "style" => {
                            let key = (
                                attr(s, "family").unwrap_or("").to_owned(),
                                attr(s, "name").unwrap_or("").to_owned(),
                            );
                            let map = if auto { &mut st.auto } else { &mut st.common };
                            map.insert(key, def_of(s));
                        }
                        "list-style" => {
                            let mut levels = vec![false; 10];
                            for l in s.children().filter(|l| l.is_element()) {
                                let level: usize =
                                    attr(l, "level").and_then(|v| v.parse().ok()).unwrap_or(1);
                                if (1..=levels.len()).contains(&level) {
                                    levels[level - 1] = local(l) == "list-level-style-number";
                                }
                            }
                            st.lists
                                .insert(attr(s, "name").unwrap_or("").to_owned(), levels);
                        }
                        "default-style" if attr(s, "family") == Some("paragraph") => {
                            if let Some(p) = child(s, "text-properties") {
                                st.language = language(p);
                            }
                        }
                        _ => {}
                    }
                }
            }
        }
        st
    }

    /// The chain from `name` up through its parents (at most 20, against cycles).
    fn chain(&self, family: &str, name: Option<&str>) -> Vec<&Def> {
        let mut out = Vec::new();
        let mut next = name.map(str::to_owned);
        let mut first = true;
        while let Some(n) = next.take() {
            let key = (family.to_owned(), n);
            let def = if first {
                self.auto.get(&key).or_else(|| self.common.get(&key))
            } else {
                self.common.get(&key).or_else(|| self.auto.get(&key))
            };
            first = false;
            let Some(def) = def else { break };
            out.push(def);
            if out.len() >= 20 {
                break;
            }
            next = def.parent.clone();
        }
        out
    }

    fn apply(chain: &[&Def], base: Fmt) -> Fmt {
        let pick = |get: &dyn Fn(&Patch) -> Option<bool>, dflt: bool| {
            chain.iter().find_map(|d| get(&d.patch)).unwrap_or(dflt)
        };
        let position = chain.iter().find_map(|d| d.patch.position);
        Fmt {
            bold: pick(&|p| p.bold, base.bold),
            italic: pick(&|p| p.italic, base.italic),
            underline: pick(&|p| p.underline, base.underline),
            strike: pick(&|p| p.strike, base.strike),
            small_caps: pick(&|p| p.small_caps, base.small_caps),
            sup: position.map_or(base.sup, |p| p.0),
            sub: position.map_or(base.sub, |p| p.1),
        }
    }

    pub fn paragraph(&self, name: Option<&str>) -> ParaInfo {
        let chain = self.chain("paragraph", name);
        ParaInfo {
            fmt: Self::apply(&chain, Fmt::default()),
            names: chain.iter().map(|d| d.display.clone()).collect(),
            align: chain.iter().find_map(|d| d.align.clone()),
            outline: chain.iter().find_map(|d| d.outline),
        }
    }

    /// A span's formatting on top of the paragraph's.
    pub fn text(&self, name: Option<&str>, base: Fmt) -> Fmt {
        Self::apply(&self.chain("text", name), base)
    }

    /// Whether `level` (0-based) of list style `name` is numbered.
    pub fn ordered(&self, name: Option<&str>, level: usize) -> bool {
        name.and_then(|n| self.lists.get(n))
            .and_then(|l| l.get(level).copied())
            .unwrap_or(false)
    }
}

/// `fo:language` + `fo:country` as a BCP 47 tag; `none` and missing parts are left out.
pub fn language(props: Node) -> Option<String> {
    let lang = attr(props, "language").filter(|l| !l.is_empty() && *l != "none")?;
    Some(
        match attr(props, "country").filter(|c| !c.is_empty() && *c != "none") {
            Some(c) => format!("{lang}-{c}"),
            None => lang.to_owned(),
        },
    )
}
