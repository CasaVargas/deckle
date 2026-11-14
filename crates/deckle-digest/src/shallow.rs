// SPDX-License-Identifier: GPL-3.0-or-later
//! A fetched page, made shallow enough to parse.
//!
//! The HTML parser and readability both slow down with nesting: the parser checks the open
//! elements for each `<div>` (quadratic, 40,000 levels took 2.7 s), and readability is about
//! cubic (2,000 levels took 14 s). A page of tens of kilobytes could stall an edition
//! (GHSA-5mmx-qpc6-3x6r). [`shallow`] reads the page once, linearly, and drops the tags nested
//! past [`MAX_DEPTH`], keeping their text, as the story writer already does with what readability
//! gives it. A page within the limit comes back as it was.
//!
//! The depth is the parser's, closely enough: void elements and the elements whose end tag may be
//! left out (`<p>`, `<li>`, …, which close the one before) add none, raw text (`<script>`,
//! `<style>`, …) and comments are skipped, and `/>` closes only inside `<svg>` or `<math>`.

use std::borrow::Cow;

use deckle_limits::MAX_DEPTH;

/// Elements with no end tag.
const VOID: &[&str] = &[
    "area", "base", "br", "col", "embed", "hr", "img", "input", "link", "meta", "param", "source",
    "track", "wbr", "keygen", "frame", "basefont", "bgsound", "image",
];

/// Elements a following sibling closes, so a run of them doesn't nest.
const SELF_CLOSING_RUNS: &[&str] = &[
    "p", "li", "dt", "dd", "option", "optgroup", "tr", "td", "th", "rb", "rt", "rp", "rtc",
];

/// Elements whose content is text up to their own end tag.
const RAW_TEXT: &[&str] = &[
    "script",
    "style",
    "textarea",
    "title",
    "xmp",
    "iframe",
    "noembed",
    "noframes",
    "plaintext",
];

/// `html` with nothing nested more than [`MAX_DEPTH`] deep.
pub fn shallow(html: &str) -> Cow<'_, str> {
    shallow_to(html, MAX_DEPTH)
}

fn shallow_to(html: &str, limit: usize) -> Cow<'_, str> {
    let mut out: Option<String> = None;
    let mut depth = 0usize;
    // The depth an `<svg>` or `<math>` opened at, while inside one.
    let mut foreign: Option<usize> = None;
    // Bytes of `html` already copied to `out`, when copying.
    let mut copied = 0usize;
    let b = html.as_bytes();
    let mut i = 0usize;
    while let Some(off) = html[i..].find('<') {
        let start = i + off;
        i = start + 1;
        if html[i..].starts_with("!--") {
            i = html[i..].find("-->").map_or(html.len(), |e| i + e + 3);
            continue;
        }
        if matches!(b.get(i), Some(b'!' | b'?')) {
            i = html[i..].find('>').map_or(html.len(), |e| i + e + 1);
            continue;
        }
        let closing = b.get(i) == Some(&b'/');
        let name_at = i + usize::from(closing);
        let name_len = html[name_at..]
            .bytes()
            .take_while(|c| c.is_ascii_alphanumeric() || *c == b'-' || *c == b':')
            .count();
        if name_len == 0 || !b[name_at].is_ascii_alphabetic() {
            continue;
        }
        let name = html[name_at..name_at + name_len].to_ascii_lowercase();
        let end = tag_end(html, name_at + name_len);
        let self_closed = html[..end].ends_with("/>");
        i = end;

        let keep = if closing {
            if VOID.contains(&name.as_str()) || SELF_CLOSING_RUNS.contains(&name.as_str()) {
                true
            } else {
                let keep = depth <= limit;
                depth = depth.saturating_sub(1);
                if foreign.is_some_and(|f| depth < f) {
                    foreign = None;
                }
                keep
            }
        } else if VOID.contains(&name.as_str())
            || (SELF_CLOSING_RUNS.contains(&name.as_str()) && foreign.is_none())
            || (self_closed && foreign.is_some())
        {
            depth < limit
        } else {
            depth += 1;
            if foreign.is_none() && (name == "svg" || name == "math") {
                foreign = Some(depth);
            }
            if foreign.is_none() && RAW_TEXT.contains(&name.as_str()) {
                // Its text runs to its own end tag, which then closes it.
                i = raw_text_end(html, i, &name);
            }
            depth <= limit
        };
        if !keep {
            let o = out.get_or_insert_with(|| String::with_capacity(html.len()));
            o.push_str(&html[copied..start]);
            copied = end;
        }
    }
    match out {
        None => Cow::Borrowed(html),
        Some(mut o) => {
            o.push_str(&html[copied..]);
            Cow::Owned(o)
        }
    }
}

/// Just past the `>` that ends the tag whose attributes start at `from`; quoted values may
/// hold a `>`.
fn tag_end(html: &str, from: usize) -> usize {
    let mut quote: Option<u8> = None;
    for (k, c) in html.as_bytes()[from..].iter().enumerate() {
        match (quote, c) {
            (Some(q), c) if *c == q => quote = None,
            (Some(_), _) => {}
            (None, b'"' | b'\'') => quote = Some(*c),
            (None, b'>') => return from + k + 1,
            _ => {}
        }
    }
    html.len()
}

/// Where the `</name` that ends a raw-text element starts, or the end of the page. Searched
/// without copying, so many such elements still cost one pass.
fn raw_text_end(html: &str, from: usize, name: &str) -> usize {
    let b = html.as_bytes();
    let mut i = from;
    while let Some(k) = html[i..].find("</") {
        let at = i + k;
        let tag = at + 2;
        if b.len() >= tag + name.len()
            && b[tag..tag + name.len()].eq_ignore_ascii_case(name.as_bytes())
        {
            return at;
        }
        i = at + 2;
    }
    html.len()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Deepest element nesting the parser builds from `html`.
    fn parsed_depth(html: &str) -> usize {
        let doc = dom_query::Document::from(html);
        let mut deepest = 0;
        let mut stack = vec![(doc.root(), 0usize)];
        while let Some((node, d)) = stack.pop() {
            deepest = deepest.max(d);
            let mut c = node.first_child();
            while let Some(n) = c {
                if n.is_element() {
                    stack.push((n, d + 1));
                }
                c = n.next_sibling();
            }
        }
        deepest
    }

    #[test]
    fn an_ordinary_page_comes_back_as_it_was() {
        let page = r#"<!DOCTYPE html><html><head><title>A <b> title</title>
            <script>if (a < b) { document.write("<div><div><div>") }</script>
            <style>div > p { color: red }</style></head>
            <body><!-- <div><div> --><ul><li>one<li>two</ul><p>a<p>b<br><img src="x.jpg" alt=">">
            <svg><path d="M0 0"/><path/><circle/></svg><table><tr><td>1<td>2</table></body></html>"#;
        assert!(matches!(shallow_to(page, 6), Cow::Borrowed(_)));
    }

    #[test]
    fn deep_nesting_is_cut_to_the_limit_and_keeps_its_text() {
        for tag in ["div", "em", "span", "section"] {
            let page = format!(
                "<html><body><p>before</p>{}deep words{}<p>after</p></body></html>",
                format!("<{tag} class=\"a\">").repeat(5_000),
                format!("</{tag}>").repeat(5_000)
            );
            let s = shallow(&page);
            assert!(s.contains("deep words") && s.contains("after"), "{tag}");
            assert!(
                parsed_depth(&s) <= MAX_DEPTH + 3,
                "{tag}: {}",
                parsed_depth(&s)
            );
        }
    }

    #[test]
    fn self_closing_html_tags_still_nest() {
        // The parser ignores `/>` on a `<div>`, so these nest.
        let page = format!("<body>{}x</body>", "<div/>".repeat(1_000));
        assert!(parsed_depth(&shallow(&page)) <= MAX_DEPTH + 3);
    }

    #[test]
    fn runs_of_list_items_and_paragraphs_dont_count() {
        let page = format!(
            "<body><ul>{}</ul>{}</body>",
            "<li>x".repeat(1_000),
            "<p>y".repeat(1_000)
        );
        assert!(matches!(shallow_to(&page, 8), Cow::Borrowed(_)));
    }
}
