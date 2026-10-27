// SPDX-License-Identifier: GPL-3.0-or-later
//! Reading a recipe from a page (cookbook design §4): the schema.org `Recipe` in its JSON-LD
//! first (§4.1), the page's text when it has none (§4.2).

use std::sync::LazyLock;

use dom_query::Document;
use regex::Regex;
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::sources::feeds::host;
use crate::{article, web_book};

/// The longest description kept, in characters (§4.1).
const DESCRIPTION_MAX: usize = 300;

/// A run of list items under an optional sub-heading: one group per `HowToSection`, or a single
/// heading-less group for a plain list (§6.1).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Group {
    pub heading: Option<String>,
    pub items: Vec<String>,
}

/// One recipe, as the book prints it (§4.1).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Recipe {
    pub title: String,
    pub description: Option<String>,
    /// As the page gives it ("4 servings", "1 loaf").
    #[serde(rename = "yield")]
    pub yield_: Option<String>,
    /// Minutes.
    pub prep: Option<u32>,
    pub cook: Option<u32>,
    pub total: Option<u32>,
    #[serde(default)]
    pub ingredients: Vec<Group>,
    #[serde(default)]
    pub steps: Vec<Group>,
    #[serde(default)]
    pub category: Vec<String>,
    pub cuisine: Option<String>,
    pub author: Option<String>,
    pub image_url: Option<String>,
    pub source_url: String,
    pub site_name: String,
    /// `false` for the page-text fallback (§4.2): no ingredient list, the method is `body_xhtml`.
    pub structured: bool,
    /// Fallback only: the article extractor's body.
    #[serde(default)]
    pub body_xhtml: Option<String>,
}

/// Reads the recipe on a page. `url` is where the page came from (`file://…` for a saved
/// page). Fails with the web-article reason when a page without recipe data is a stub.
pub fn parse_recipe(html: &str, url: &str) -> Result<Recipe, String> {
    let doc = Document::from(html);
    let source_url = canonical(&doc, url).unwrap_or_else(|| url.to_owned());
    let site_name = meta(&doc, "og:site_name")
        .map(|s| clean(&s))
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| match host(&source_url) {
            "" => "a saved page".to_owned(),
            h => h.to_owned(),
        });

    let blocks: Vec<Value> = doc
        .select(r#"script[type="application/ld+json"]"#)
        .nodes()
        .iter()
        .filter_map(|n| serde_json::from_str(n.text().trim()).ok())
        .collect();
    let mut nodes = Vec::new();
    for block in &blocks {
        recipe_nodes(block, &mut nodes);
    }
    // The first node that has everything a recipe needs; the rest are teasers.
    if let Some(r) = nodes
        .into_iter()
        .find_map(|n| map_node(n, url, &source_url, &site_name))
    {
        return Ok(r);
    }

    // §4.2: no usable recipe data, so the page's text stands in as the method.
    let x = article::extract(html, url).ok_or_else(web_book::stub_reason)?;
    let image_url = if x.lead {
        x.photos.first().cloned()
    } else {
        None
    };
    Ok(Recipe {
        title: x.title.unwrap_or_else(|| site_name.clone()),
        description: None,
        yield_: None,
        prep: None,
        cook: None,
        total: None,
        ingredients: Vec::new(),
        steps: Vec::new(),
        category: Vec::new(),
        cuisine: None,
        author: x.byline,
        image_url,
        source_url,
        site_name,
        structured: false,
        body_xhtml: Some(x.xhtml),
    })
}

/// Whole minutes in an ISO 8601 duration (`PT1H30M`, `P1DT2H`); `None` when it isn't one.
/// Seconds are dropped, and the date part reads weeks and days only (a month isn't a fixed
/// length).
pub fn iso_minutes(s: &str) -> Option<u32> {
    static ISO: LazyLock<Regex> = LazyLock::new(|| {
        Regex::new(r"^P(?:(\d+)W)?(?:(\d+)D)?(?:T(?:(\d+)H)?(?:(\d+)M)?(?:(\d+)(?:\.\d+)?S)?)?$")
            .unwrap()
    });
    let s = s.trim().to_ascii_uppercase();
    if s.ends_with('T') {
        return None;
    }
    let c = ISO.captures(&s)?;
    let part = |i: usize, per: u64| -> Option<Option<u64>> {
        match c.get(i) {
            None => Some(None),
            Some(m) => m
                .as_str()
                .parse::<u64>()
                .ok()
                .and_then(|n| n.checked_mul(per))
                .map(Some),
        }
    };
    let parts = [
        part(1, 7 * 24 * 60)?,
        part(2, 24 * 60)?,
        part(3, 60)?,
        part(4, 1)?,
        part(5, 0)?,
    ];
    // Bare "P" / "PT" match the pattern but name no time at all.
    if parts.iter().all(Option::is_none) {
        return None;
    }
    let seconds = c
        .get(5)
        .and_then(|m| m.as_str().parse::<u64>().ok())
        .unwrap_or(0);
    // The pieces come from untrusted pages: an overflow is no time at all, not a panic.
    let total = parts
        .iter()
        .flatten()
        .try_fold(seconds / 60, |sum, n| sum.checked_add(*n))?;
    u32::try_from(total).ok()
}

/// Every Recipe node in a JSON-LD block, in document order: top-level, in an array, or in an
/// `@graph`.
fn recipe_nodes<'a>(v: &'a Value, out: &mut Vec<&'a Value>) {
    match v {
        Value::Array(items) => items.iter().for_each(|i| recipe_nodes(i, out)),
        Value::Object(map) => {
            if is_recipe(v) {
                out.push(v);
            }
            if let Some(graph) = map.get("@graph") {
                recipe_nodes(graph, out);
            }
        }
        _ => {}
    }
}

/// `@type` is, or contains, `Recipe` (also spelled `http://schema.org/Recipe`).
fn is_recipe(v: &Value) -> bool {
    has_type(v, "Recipe")
}

fn has_type(v: &Value, name: &str) -> bool {
    let is = |t: &Value| {
        t.as_str()
            .is_some_and(|t| t.rsplit(['/', '#']).next() == Some(name))
    };
    match v.get("@type") {
        Some(Value::Array(types)) => types.iter().any(is),
        Some(t) => is(t),
        None => false,
    }
}

/// A node's mapped recipe, or `None` when it lacks a title, an ingredient or a step (§4.1).
fn map_node(n: &Value, base: &str, source_url: &str, site_name: &str) -> Option<Recipe> {
    let title = n.get("name").and_then(text).filter(|t| !t.is_empty())?;
    let ingredients = n
        .get("recipeIngredient")
        .or_else(|| n.get("ingredients"))
        .map(string_list)
        .unwrap_or_default();
    if ingredients.is_empty() {
        return None;
    }
    let steps = n
        .get("recipeInstructions")
        .map(step_groups)
        .unwrap_or_default();
    if steps.is_empty() {
        return None;
    }
    let minutes = |key: &str| n.get(key).and_then(Value::as_str).and_then(iso_minutes);
    let (prep, cook) = (minutes("prepTime"), minutes("cookTime"));
    let total = minutes("totalTime").or_else(|| prep?.checked_add(cook?));
    Some(Recipe {
        title,
        description: n
            .get("description")
            .and_then(text)
            .filter(|d| !d.is_empty())
            .map(|d| cap_description(&d)),
        yield_: n.get("recipeYield").and_then(yield_text),
        prep,
        cook,
        total,
        ingredients: vec![Group {
            heading: None,
            items: ingredients,
        }],
        steps,
        category: n.get("recipeCategory").map(string_list).unwrap_or_default(),
        cuisine: n
            .get("recipeCuisine")
            .map(string_list)
            .and_then(|c| c.into_iter().next()),
        author: n.get("author").and_then(author_name),
        image_url: n.get("image").and_then(|i| image_url(i, base)),
        source_url: source_url.to_owned(),
        site_name: site_name.to_owned(),
        structured: true,
        body_xhtml: None,
    })
}

/// A JSON string or number as clean text.
fn text(v: &Value) -> Option<String> {
    match v {
        Value::String(s) => Some(clean(s)),
        Value::Number(n) => Some(n.to_string()),
        _ => None,
    }
}

/// A string or an array of strings, cleaned, with the empty entries dropped. A single string
/// is split on its line breaks.
fn string_list(v: &Value) -> Vec<String> {
    match v {
        Value::String(s) => lines(s),
        Value::Array(items) => items
            .iter()
            .filter_map(text)
            .filter(|s| !s.is_empty())
            .collect(),
        other => text(other).filter(|s| !s.is_empty()).into_iter().collect(),
    }
}

/// `recipeYield`: the first entry with a digit, else the first entry.
fn yield_text(v: &Value) -> Option<String> {
    let entries: Vec<String> = match v {
        Value::Array(items) => items.iter().filter_map(text).collect(),
        other => text(other).into_iter().collect(),
    };
    let entries: Vec<String> = entries.into_iter().filter(|s| !s.is_empty()).collect();
    entries
        .iter()
        .find(|s| s.chars().any(|c| c.is_ascii_digit()))
        .or_else(|| entries.first())
        .cloned()
}

fn author_name(v: &Value) -> Option<String> {
    match v {
        Value::String(_) => text(v),
        Value::Object(map) => map.get("name").and_then(text),
        Value::Array(items) => items.iter().find_map(author_name),
        _ => None,
    }
    .filter(|s| !s.is_empty())
}

/// `image`: a string, an `ImageObject`, or an array of either (the widest one, else the first).
fn image_url(v: &Value, base: &str) -> Option<String> {
    fn one(v: &Value) -> Option<(Option<u64>, String)> {
        match v {
            Value::String(s) => Some((None, s.trim().to_owned())),
            Value::Object(map) => {
                let url = map.get("url").or_else(|| map.get("contentUrl"))?.as_str()?;
                let width = match map.get("width") {
                    Some(Value::Number(n)) => n.as_u64(),
                    Some(Value::String(s)) => s.trim().parse().ok(),
                    _ => None,
                };
                Some((width, url.trim().to_owned()))
            }
            _ => None,
        }
    }
    let candidates: Vec<(Option<u64>, String)> = match v {
        Value::Array(items) => items.iter().filter_map(one).collect(),
        other => one(other).into_iter().collect(),
    };
    let candidates: Vec<_> = candidates
        .into_iter()
        .filter(|(_, u)| !u.is_empty())
        .collect();
    let mut best: Option<&(Option<u64>, String)> = None;
    for c in &candidates {
        // The first widest one wins; entries with no width only count when none has one.
        if best.is_none_or(|b| c.0 > b.0) {
            best = Some(c);
        }
    }
    resolve(base, &best?.1)
}

/// `recipeInstructions` as groups: a string, an array of strings, `HowToStep`s and
/// `HowToSection`s (a section's name is its sub-heading).
fn step_groups(v: &Value) -> Vec<Group> {
    fn walk(v: &Value, groups: &mut Vec<Group>) {
        match v {
            Value::Array(items) => items.iter().for_each(|i| walk(i, groups)),
            Value::Object(map) if is_section(v) => {
                let mut items = Vec::new();
                if let Some(list) = map.get("itemListElement") {
                    flat_steps(list, &mut items);
                }
                let heading = map.get("name").and_then(text).filter(|h| !h.is_empty());
                if !items.is_empty() {
                    groups.push(Group { heading, items });
                }
            }
            other => {
                let mut items = Vec::new();
                flat_steps(other, &mut items);
                for item in items {
                    match groups.last_mut() {
                        Some(g) if g.heading.is_none() => g.items.push(item),
                        _ => groups.push(Group {
                            heading: None,
                            items: vec![item],
                        }),
                    }
                }
            }
        }
    }
    let mut groups = Vec::new();
    walk(v, &mut groups);
    groups
}

fn is_section(v: &Value) -> bool {
    has_type(v, "HowToSection") || (v.get("itemListElement").is_some() && v.get("text").is_none())
}

/// The steps in `v`, flattening any sections nested inside it.
fn flat_steps(v: &Value, out: &mut Vec<String>) {
    match v {
        Value::String(s) => out.extend(lines(s)),
        Value::Array(items) => items.iter().for_each(|i| flat_steps(i, out)),
        Value::Object(map) => {
            if is_section(v) {
                if let Some(list) = map.get("itemListElement") {
                    flat_steps(list, out);
                }
            } else if let Some(step) = ["text", "name"]
                .iter()
                .filter_map(|k| map.get(*k).and_then(text))
                .find(|s| !s.is_empty())
            {
                out.push(step);
            }
        }
        _ => {}
    }
}

/// Tags that end a line of text when removed.
static BREAK_TAG: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"(?i)</?(?:p|br|div|li|ul|ol|h[1-6]|tr|td)\b[^>]*>").unwrap());
static TAG: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"(?s)</?[A-Za-z][^>]*>|<!--.*?-->").unwrap());

/// Text from markup: tags stripped, entities decoded, whitespace collapsed.
fn clean(s: &str) -> String {
    let s = BREAK_TAG.replace_all(s, " ");
    let s = TAG.replace_all(&s, "");
    html_escape::decode_html_entities(&s)
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
}

/// The non-empty lines of a string (a line break, `<br>` or paragraph end all split), each cleaned.
fn lines(s: &str) -> Vec<String> {
    BREAK_TAG
        .replace_all(s, "\n")
        .split(['\n', '\r'])
        .map(clean)
        .filter(|l| !l.is_empty())
        .collect()
}

/// At most [`DESCRIPTION_MAX`] characters, cut after the last sentence end that fits; with no
/// sentence end, at a word boundary with an ellipsis.
fn cap_description(d: &str) -> String {
    if d.chars().count() <= DESCRIPTION_MAX {
        return d.to_owned();
    }
    let head: String = d.chars().take(DESCRIPTION_MAX).collect();
    let after = d[head.len()..].chars().next();
    let cut = head
        .char_indices()
        .filter(|&(i, c)| {
            matches!(c, '.' | '!' | '?')
                && head[i + c.len_utf8()..]
                    .chars()
                    .next()
                    .or(after)
                    .is_none_or(char::is_whitespace)
        })
        .map(|(i, c)| i + c.len_utf8())
        .next_back();
    match cut {
        Some(end) => head[..end].to_owned(),
        None => {
            let words = head
                .rsplit_once(char::is_whitespace)
                .map_or(&*head, |(w, _)| w);
            format!("{}\u{2026}", words.trim_end_matches([',', ';', ':', '-']))
        }
    }
}

/// `<meta property="…" content="…">`.
fn meta(doc: &Document, property: &str) -> Option<String> {
    doc.select(&format!(r#"meta[property="{property}"]"#))
        .nodes()
        .iter()
        .find_map(|n| n.attr("content").map(|c| c.to_string()))
}

/// The page's `rel=canonical` address, resolved against the page's own.
fn canonical(doc: &Document, base: &str) -> Option<String> {
    let href = doc
        .select(r#"link[rel="canonical"]"#)
        .nodes()
        .iter()
        .find_map(|n| n.attr("href").map(|h| h.to_string()))?;
    resolve(base, &href).filter(|u| u.starts_with("http"))
}

/// `href` against `base`: an http(s) address, or a file beside a saved page.
fn resolve(base: &str, href: &str) -> Option<String> {
    let href = href.trim();
    if href.is_empty() || href.starts_with("data:") {
        return None;
    }
    let base = url::Url::parse(base).ok()?;
    let joined = base.join(href).ok()?;
    match joined.scheme() {
        "http" | "https" => Some(joined.to_string()),
        "file" if base.scheme() == "file" => Some(joined.to_string()),
        _ => None,
    }
}
