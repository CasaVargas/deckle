// SPDX-License-Identifier: GPL-3.0-or-later
//! The cookbook's pages as XHTML (cookbook design §6.1): recipe sections, chapter title pages,
//! the contents and the index. Everything that comes from a saved page is escaped here.

use std::fmt::Write as _;
use std::sync::LazyLock;

use chrono::{DateTime, Duration, Utc};
use dom_query::Document;
use regex::Regex;

use crate::cookbook::recipe::{Group, SAVED_PAGE};
use crate::cookbook::store::{Picture, Saved};
use crate::esc;

/// A recipe ready to print: how it is shown, and the picture file it embeds (if any).
pub(super) struct Entry<'a> {
    pub saved: &'a Saved,
    pub title: String,
    /// The picture's file name inside `OEBPS/images/`, when one is printed.
    pub image: Option<String>,
}

/// A chapter with something in it.
pub(super) struct Part<'a> {
    pub name: String,
    /// The chapter's file-name stem: `chapter-<slug>.xhtml`.
    pub slug: String,
    pub file: String,
    pub entries: Vec<Entry<'a>>,
}

/// A page's XHTML, with the Daily's stylesheet and the cookbook's after it.
pub(super) fn xhtml(title: &str, body: &str) -> Vec<u8> {
    format!(
        "<?xml version=\"1.0\" encoding=\"utf-8\"?>\n\
         <html xmlns=\"http://www.w3.org/1999/xhtml\" xmlns:epub=\"http://www.idpf.org/2007/ops\" xml:lang=\"en\" lang=\"en\">\n\
         <head><meta charset=\"utf-8\"/><title>{}</title>\
         <link rel=\"stylesheet\" type=\"text/css\" href=\"style.css\"/>\
         <link rel=\"stylesheet\" type=\"text/css\" href=\"cookbook.css\"/></head>\n<body>\n{body}\n</body>\n</html>\n",
        esc(title)
    )
    .into_bytes()
}

/// "1 recipe", "2 recipes".
pub(super) fn recipes(n: usize) -> String {
    match n {
        1 => "1 recipe".to_owned(),
        n => format!("{n} recipes"),
    }
}

/// Added within 7 days of the newest recipe (which counts itself): the book's "latest
/// additions". Anchored to the store, not to a clock, so a rebuilt book never changes by itself.
pub(super) fn is_latest(added: DateTime<Utc>, newest: DateTime<Utc>) -> bool {
    newest.signed_duration_since(added) < Duration::days(7)
}

/// `href` of a recipe: its chapter's file and its section.
pub(super) fn link(file: &str, id: &str) -> String {
    format!("{file}#r-{id}")
}

fn minutes(m: u32) -> String {
    match (m / 60, m % 60) {
        (0, r) => format!("{r} min"),
        (h, 0) => format!("{h} hr"),
        (h, r) => format!("{h} hr {r} min"),
    }
}

/// The yield as the facts line says it: a bare count or servings is "Serves 4", a count of
/// something else "Makes 24 cookies", and anything else as the page gave it.
fn yield_phrase(y: &str) -> String {
    static SERVINGS: LazyLock<Regex> = LazyLock::new(|| {
        Regex::new(r"(?i)^(\d+(?:\s*[-–]\s*\d+)?)(?:\s*(?:servings?|portions?|people|persons?))?$")
            .unwrap()
    });
    let y = y.trim();
    if let Some(c) = SERVINGS.captures(y) {
        return format!("Serves {}", &c[1]);
    }
    let lower = y.to_lowercase();
    if ["serves", "makes", "yields"]
        .iter()
        .any(|p| lower.starts_with(p))
    {
        return y.to_owned();
    }
    match y.starts_with(|c: char| c.is_ascii_digit()) {
        true => format!("Makes {y}"),
        false => y.to_owned(),
    }
}

/// "Serves 4 · Prep 15 min · Cook 50 min · Thai", leaving out what is missing. The total time
/// stands in when there is no cook time.
fn facts(s: &Saved) -> String {
    let r = &s.recipe;
    let time = |label: &str, m: Option<u32>| {
        m.filter(|&m| m > 0)
            .map(|m| format!("{label} {}", minutes(m)))
    };
    let parts = [
        r.yield_
            .as_deref()
            .map(yield_phrase)
            .filter(|y| !y.is_empty()),
        time("Prep", r.prep),
        time("Cook", r.cook).or_else(|| time("Total", r.total)),
        r.cuisine.clone().filter(|c| !c.trim().is_empty()),
    ];
    parts.into_iter().flatten().collect::<Vec<_>>().join(" · ")
}

/// A page's body text with its photos taken out: the extractor leaves `__DECKLE_IMG_k__`
/// placeholders and a lead figure in it, and the recipe prints its own stored picture.
///
/// The body is parsed and every `img`, `figure` and `div.figure` element is removed with
/// everything inside it (a gallery is nested figure divs, which no regex can balance), then
/// written back out as XHTML. If that doesn't parse as XML, the body's text stands in as plain
/// paragraphs, so one odd page never breaks the chapter file.
fn without_photos(xhtml: &str) -> String {
    static VOID: LazyLock<Regex> =
        LazyLock::new(|| Regex::new(r"<(br|hr|wbr)((?:\s[^<>]*?)?)\s*/?>").unwrap());
    // Relies on `article::extract`'s writer emitting `br` as its only void element.
    let doc = Document::from(format!("<body>{xhtml}</body>"));
    doc.select(r#"img, figure, div[class^="figure"]"#).remove();
    let body = doc.select("body");
    let html = body.inner_html().replace("&nbsp;", "&#160;");
    // The HTML serializer writes `<br>`; XHTML wants `<br/>`.
    let out = VOID.replace_all(&html, "<$1$2/>").into_owned();
    if roxmltree::Document::parse(&format!("<div>{out}</div>")).is_ok() {
        return out;
    }
    // Block boundaries become line breaks, the rest of the markup goes, and each line is a
    // paragraph.
    static BLOCK: LazyLock<Regex> = LazyLock::new(|| {
        Regex::new(r"(?i)</?(?:p|div|li|ul|ol|h[1-6]|blockquote|pre|tr|table|br|hr)\b[^<>]*>")
            .unwrap()
    });
    static TAG: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"<[^<>]*>").unwrap());
    let lines = BLOCK.replace_all(&html, "\n");
    let lines = html_escape::decode_html_entities(&TAG.replace_all(&lines, "")).into_owned();
    lines
        .lines()
        .map(str::trim)
        .filter(|l| !l.is_empty())
        .map(|l| format!("<p>{}</p>", esc(l)))
        .collect::<Vec<_>>()
        .join("\n")
}

fn groups(out: &mut String, groups: &[Group], list: &str, class: &str) {
    for g in groups.iter().filter(|g| !g.items.is_empty()) {
        if let Some(h) = g.heading.as_deref().filter(|h| !h.trim().is_empty()) {
            let _ = writeln!(out, "<h4>{}</h4>", esc(h));
        }
        let _ = writeln!(out, "<{list} class=\"{class}\">");
        for item in &g.items {
            let _ = writeln!(out, "<li>{}</li>", esc(item));
        }
        let _ = writeln!(out, "</{list}>");
    }
}

fn foot(s: &Saved) -> String {
    let r = &s.recipe;
    let mut bits: Vec<String> = Vec::new();
    // A saved page with no canonical link or site name has only the placeholder.
    let site = r.site_name.trim();
    if !site.is_empty() && site != SAVED_PAGE {
        bits.push(format!("From <em>{}</em>", esc(site)));
    }
    if let Some(a) = r.author.as_deref().filter(|a| !a.trim().is_empty()) {
        bits.push(format!("by {}", esc(a.trim())));
    }
    bits.push(format!("saved {}", s.added.format("%b %-d, %Y")));
    if !bits[0].starts_with("From") {
        let first = bits[0].remove(0).to_ascii_uppercase();
        bits[0].insert(0, first);
    }
    let mut out = format!("<div class=\"foot\">\n<p>{}</p>\n", bits.join(" · "));
    // A saved page's address is a path on the owner's disk, which is no use in a book.
    let url = r.source_url.trim();
    if !url.is_empty() && !url.to_ascii_lowercase().starts_with("file:") {
        let _ = writeln!(out, "<p class=\"url\">{}</p>", esc(url));
    }
    out.push_str("</div>\n");
    out
}

/// One recipe: `<section class="recipe" id="r-<id>">`.
pub(super) fn recipe(e: &Entry) -> String {
    let (s, r) = (e.saved, &e.saved.recipe);
    let mut out = format!(
        "<section class=\"recipe\" id=\"r-{}\" epub:type=\"subchapter\">\n",
        esc(&s.id)
    );
    if let Some(file) = &e.image {
        let _ = writeln!(
            out,
            "<div class=\"figure\"><img src=\"images/{}\" alt=\"{}\"/></div>",
            esc(file),
            esc(&e.title)
        );
    }
    let _ = writeln!(out, "<h2 class=\"recipe-title\">{}</h2>", esc(&e.title));
    if let Some(d) = r.description.as_deref().filter(|d| !d.trim().is_empty()) {
        let _ = writeln!(
            out,
            "<p class=\"description\"><em>{}</em></p>",
            esc(d.trim())
        );
    }
    let facts = facts(s);
    if !facts.is_empty() {
        let _ = writeln!(out, "<p class=\"facts\">{}</p>", esc(&facts));
    }
    match (&r.body_xhtml, r.structured) {
        (Some(body), false) => {
            let _ = writeln!(
                out,
                "<div class=\"body\">\n{}\n</div>",
                without_photos(body)
            );
        }
        _ => {
            if r.ingredients.iter().any(|g| !g.items.is_empty()) {
                out.push_str("<h3>Ingredients</h3>\n");
                groups(&mut out, &r.ingredients, "ul", "ingredients");
            }
            if r.steps.iter().any(|g| !g.items.is_empty()) {
                out.push_str("<h3>Method</h3>\n");
                groups(&mut out, &r.steps, "ol", "method");
            }
        }
    }
    if let Some(n) = s.note.as_deref().filter(|n| !n.trim().is_empty()) {
        let _ = writeln!(
            out,
            "<div class=\"note\">\n<p class=\"label\">Your note</p>\n<p>{}</p>\n</div>",
            esc(n.trim())
        );
    }
    out.push_str(&foot(s));
    out.push_str("</section>\n");
    out
}

/// A chapter's file: its title page, then its recipes.
pub(super) fn chapter(part: &Part) -> Vec<u8> {
    let mut body = format!(
        "<section class=\"chapter-title\" epub:type=\"chapter\" id=\"c-{}\">\n<h1>{}</h1>\n<p class=\"count\">{}</p>\n</section>\n",
        esc(&part.slug),
        esc(&part.name),
        recipes(part.entries.len())
    );
    for e in &part.entries {
        body.push_str(&recipe(e));
    }
    xhtml(&part.name, &body)
}

/// The contents: each chapter with its count and its recipes, a star on the latest additions,
/// the index, and a line counting them (left out of an empty book).
pub(super) fn contents(parts: &[Part], has_index: bool, newest: Option<DateTime<Utc>>) -> Vec<u8> {
    let mut body = String::from("<h1>Contents</h1>\n");
    let mut new = 0;
    if parts.is_empty() {
        body.push_str("<p>No recipes yet.</p>\n");
    }
    for p in parts {
        let _ = writeln!(
            body,
            "<div class=\"toc-chapter\">\n<h2><a href=\"{}\">{}</a> <span class=\"count\">· {}</span></h2>\n<ul class=\"toc\">",
            esc(&p.file),
            esc(&p.name),
            recipes(p.entries.len())
        );
        for e in &p.entries {
            let star = match newest.is_some_and(|n| is_latest(e.saved.added, n)) {
                true => {
                    new += 1;
                    " <span class=\"star\">★</span>"
                }
                false => "",
            };
            let _ = writeln!(
                body,
                "<li><a href=\"{}\">{}</a>{star}</li>",
                esc(&link(&p.file, &e.saved.id)),
                esc(&e.title)
            );
        }
        body.push_str("</ul>\n</div>\n");
    }
    if has_index {
        body.push_str(
            "<p class=\"toc-index\"><a href=\"index.xhtml\">Index by ingredient</a></p>\n",
        );
    }
    if newest.is_some() {
        let _ = writeln!(body, "<p class=\"latest\">Latest additions: {new}</p>");
    }
    xhtml("Contents", &body)
}

/// The index by ingredient: keys alphabetically under their first letter, each with its
/// recipes as links. `places` maps a recipe id to its chapter file and title.
pub(super) fn index(
    keys: &[(String, Vec<String>)],
    place: &dyn Fn(&str) -> Option<(String, String)>,
) -> Vec<u8> {
    let mut body = String::from("<h1>Index by ingredient</h1>\n");
    let mut letter = String::new();
    for (key, ids) in keys {
        let first = key
            .chars()
            .next()
            .filter(|c| c.is_alphabetic())
            .map(|c| c.to_uppercase().collect())
            .unwrap_or_else(|| "#".to_owned());
        if first != letter {
            let _ = writeln!(body, "<h2 class=\"ixletter\">{}</h2>", esc(&first));
            letter = first;
        }
        let links: Vec<String> = ids
            .iter()
            .filter_map(|id| place(id).map(|(file, title)| (link(&file, id), title)))
            .map(|(href, title)| format!("<a href=\"{}\">{}</a>", esc(&href), esc(&title)))
            .collect();
        let _ = writeln!(
            body,
            "<p class=\"ixrow\"><span class=\"ixkey\">{}</span> {}</p>",
            esc(key),
            links.join(", ")
        );
    }
    xhtml("Index by ingredient", &body)
}

/// A picture never resolves to `Default` when it is printed.
pub(super) fn image_name(id: &str, picture: Picture) -> Option<String> {
    match picture {
        Picture::Site => Some(format!("{id}.jpg")),
        Picture::Illustration => Some(format!("{id}.ill.jpg")),
        Picture::None | Picture::Default => None,
    }
}
