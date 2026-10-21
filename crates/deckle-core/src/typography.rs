// SPDX-License-Identifier: GPL-3.0-or-later
//! Opt-in typography pass (#30, `2026-10-21-typography-cleanup-design.md`): curly quotes,
//! dashes and ellipses in XHTML text, and empty-paragraph cleanup. Tags, attributes, comments
//! and code-like elements pass through byte for byte; running it twice changes nothing.

use regex::Regex;
use serde::Serialize;

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize)]
pub struct TypographyReport {
    pub quotes: u32,
    pub dashes: u32,
    pub ellipses: u32,
    pub empty_paragraphs: u32,
}

/// Elements whose text is never rewritten.
const SKIP: &[&str] = &[
    "pre", "code", "kbd", "samp", "tt", "var", "script", "style", "math", "svg",
];

/// Elements that end a run of text: quote context does not carry across them.
const BLOCK: &[&str] = &[
    "p",
    "div",
    "h1",
    "h2",
    "h3",
    "h4",
    "h5",
    "h6",
    "li",
    "blockquote",
    "td",
    "th",
    "dd",
    "dt",
    "br",
    "figcaption",
    "section",
    "aside",
];

/// Leading-apostrophe words: `'tis` gets `’`, not an opening quote.
const ELISIONS: &[&str] = &["tis", "twas", "til", "em", "cause", "round"];

/// Quotes are curled only for English, or a document that names no language (design §2).
pub fn curls_quotes(lang: Option<&str>) -> bool {
    match lang.map(str::trim) {
        None | Some("") => true,
        Some(l) => l.get(..2).is_some_and(|p| p.eq_ignore_ascii_case("en")),
    }
}

/// The `xml:lang` or `lang` on a document's `<html>` element.
pub fn document_lang(doc: &str) -> Option<String> {
    let html = Regex::new(r"(?is)<html\b[^>]*>").unwrap().find(doc)?;
    let re = Regex::new(r#"(?i)\b(?:xml:)?lang\s*=\s*["']([^"']*)["']"#).unwrap();
    re.captures(html.as_str()).map(|c| c[1].to_owned())
}

/// Tidies the `<body>` of one XHTML document; a document without one is returned unchanged.
pub fn tidy_xhtml(doc: &str, curl_quotes: bool, report: &mut TypographyReport) -> String {
    let Some(open) = Regex::new(r"(?is)<body\b[^>]*>").unwrap().find(doc) else {
        return doc.to_owned();
    };
    let start = open.end();
    let end = doc[start..]
        .to_ascii_lowercase()
        .rfind("</body")
        .map_or(doc.len(), |i| start + i);
    let body = remove_empty_paragraphs(&doc[start..end], report);
    let body = rewrite_text(&body, curl_quotes, report);
    format!("{}{}{}", &doc[..start], body, &doc[end..])
}

/// Tidies the `<text>` labels of an NCX, so the contents list matches the headings.
pub fn tidy_ncx(ncx: &str, curl_quotes: bool, report: &mut TypographyReport) -> String {
    let re = Regex::new(r"(?s)(<text\b[^>]*>)(.*?)(</text>)").unwrap();
    re.replace_all(ncx, |c: &regex::Captures| {
        let mut prev = None;
        let label = tidy_run(&c[2], curl_quotes, &mut prev, report);
        format!("{}{}{}", &c[1], label, &c[3])
    })
    .into_owned()
}

/// Drops `<p></p>`, `<p/>` and whitespace-only paragraphs, and collapses runs of `&nbsp;`
/// paragraphs to one (a single one is a scene break). A paragraph with an `id` always stays.
fn remove_empty_paragraphs(body: &str, report: &mut TypographyReport) -> String {
    let empty = Regex::new(r"(?i)<p\b([^>]*?)(?:/>|>[ \t\r\n]*</p\s*>)").unwrap();
    let id = Regex::new(r#"(?i)\bid\s*="#).unwrap();
    let body = empty.replace_all(body, |c: &regex::Captures| {
        if id.is_match(&c[1]) {
            c[0].to_owned()
        } else {
            report.empty_paragraphs += 1;
            String::new()
        }
    });
    let blank = Regex::new(
        r"(?i)<p\b([^>]*)>(?:[ \t\r\n]|&nbsp;|&#160;|&#xa0;|\x{a0})*(?:&nbsp;|&#160;|&#xa0;|\x{a0})(?:[ \t\r\n]|&nbsp;|&#160;|&#xa0;|\x{a0})*</p\s*>",
    )
    .unwrap();
    let mut out = String::with_capacity(body.len());
    let mut last = 0;
    // End of the previous blank paragraph, when only whitespace has followed it.
    let mut run_end: Option<usize> = None;
    for c in blank.captures_iter(&body) {
        let m = c.get(0).unwrap();
        let joined = run_end.is_some_and(|e| body[e..m.start()].trim().is_empty());
        if joined && !id.is_match(&c[1]) {
            // Drop the paragraph and the whitespace before it.
            out.push_str(&body[last..run_end.unwrap()]);
            report.empty_paragraphs += 1;
        } else {
            out.push_str(&body[last..m.end()]);
        }
        last = m.end();
        run_end = Some(m.end());
    }
    out.push_str(&body[last..]);
    out
}

/// Walks tags and text, rewriting text outside [`SKIP`] elements.
fn rewrite_text(body: &str, curl_quotes: bool, report: &mut TypographyReport) -> String {
    let mut out = String::with_capacity(body.len());
    let mut skip: Vec<String> = Vec::new();
    let mut prev: Option<char> = None;
    let mut rest = body;
    while !rest.is_empty() {
        if let Some(markup) = rest.strip_prefix('<') {
            let len = 1 + markup_len(markup);
            let tag = &rest[..len];
            if let Some((name, closing, self_closing)) = tag_name(tag) {
                if SKIP.contains(&name.as_str()) {
                    if closing {
                        if let Some(pos) = skip.iter().rposition(|n| *n == name) {
                            skip.truncate(pos);
                            // Code reads as a word: a quote after it closes.
                            prev = Some('x');
                        }
                    } else if !self_closing {
                        skip.push(name);
                    }
                } else if BLOCK.contains(&name.as_str()) {
                    prev = None;
                }
            }
            out.push_str(tag);
            rest = &rest[len..];
        } else {
            let len = rest.find('<').unwrap_or(rest.len());
            let text = &rest[..len];
            if skip.is_empty() {
                out.push_str(&tidy_run(text, curl_quotes, &mut prev, report));
            } else {
                out.push_str(text);
            }
            rest = &rest[len..];
        }
    }
    out
}

/// Length of the markup after a `<`: a comment, CDATA, processing instruction or tag
/// (quoted attribute values may hold `>`).
fn markup_len(s: &str) -> usize {
    let until = |end: &str| s.find(end).map_or(s.len(), |i| i + end.len());
    if s.starts_with("!--") {
        return until("-->");
    }
    if s.starts_with("![CDATA[") {
        return until("]]>");
    }
    if s.starts_with('?') {
        return until("?>");
    }
    let mut quote = None;
    for (i, ch) in s.char_indices() {
        match (quote, ch) {
            (None, '"' | '\'') => quote = Some(ch),
            (Some(q), c) if c == q => quote = None,
            (None, '>') => return i + 1,
            _ => {}
        }
    }
    s.len()
}

/// (local name lowercased, is closing, is self-closing) of an element tag; `None` for
/// comments, CDATA, doctype and processing instructions.
fn tag_name(tag: &str) -> Option<(String, bool, bool)> {
    let inner = tag.strip_prefix('<')?;
    let (closing, inner) = match inner.strip_prefix('/') {
        Some(r) => (true, r),
        None => (false, inner),
    };
    let name: String = inner
        .chars()
        .take_while(|c| !c.is_whitespace() && *c != '/' && *c != '>')
        .collect();
    if name.is_empty() || !name.starts_with(|c: char| c.is_alphabetic()) {
        return None;
    }
    let local = name.rsplit(':').next().unwrap_or(&name).to_lowercase();
    Some((local, closing, tag.ends_with("/>")))
}

/// One piece of text: a character, or an entity with the character it stands for.
enum Tok<'a> {
    Ch(char),
    Ent(&'a str, char),
}

impl Tok<'_> {
    fn ch(&self) -> char {
        match self {
            Tok::Ch(c) | Tok::Ent(_, c) => *c,
        }
    }
}

fn tokens(text: &str) -> Vec<Tok<'_>> {
    let mut toks = Vec::new();
    let mut rest = text;
    while let Some(ch) = rest.chars().next() {
        if ch == '&'
            && let Some(semi) = rest.as_bytes()[..rest.len().min(12)]
                .iter()
                .position(|b| *b == b';')
        {
            let raw = &rest[..=semi];
            let meaning = match raw.to_ascii_lowercase().as_str() {
                "&quot;" | "&#34;" | "&#x22;" => '"',
                "&apos;" | "&#39;" | "&#x27;" => '\'',
                "&nbsp;" | "&#160;" | "&#xa0;" => '\u{a0}',
                "&mdash;" | "&#8212;" | "&#x2014;" => '—',
                "&ndash;" | "&#8211;" | "&#x2013;" => '–',
                // Any other entity reads as a letter.
                _ => 'x',
            };
            toks.push(Tok::Ent(raw, meaning));
            rest = &rest[raw.len()..];
            continue;
        }
        toks.push(Tok::Ch(ch));
        rest = &rest[ch.len_utf8()..];
    }
    toks
}

/// Whether a quote between `prev` and `next` opens. After a dash it opens only when a word
/// follows (`—"Go"`); `stop—"` closes.
fn opens(prev: Option<char>, next: Option<char>) -> bool {
    match prev {
        None => true,
        Some('—' | '–') => next.is_some_and(char::is_alphanumeric),
        Some(c) => c.is_whitespace() || "([{“‘«/".contains(c),
    }
}

fn put(out: &mut String, prev: &mut Option<char>, c: char) {
    out.push(c);
    *prev = Some(c);
}

/// Rewrites one text run. `prev` is the last character before it in the same block, and is
/// updated to the last character written.
fn tidy_run(
    text: &str,
    curl_quotes: bool,
    prev: &mut Option<char>,
    report: &mut TypographyReport,
) -> String {
    let toks = tokens(text);
    let ch_at = |i: usize| toks.get(i).map(Tok::ch);
    let plain_at = |i: usize, want: char| matches!(toks.get(i), Some(Tok::Ch(c)) if *c == want);
    let mut out = String::with_capacity(text.len());
    let mut i = 0;
    while i < toks.len() {
        let c = toks[i].ch();
        match c {
            '"' if curl_quotes => {
                let open = opens(*prev, ch_at(i + 1));
                put(&mut out, prev, if open { '“' } else { '”' });
                report.quotes += 1;
                i += 1;
            }
            '\'' if curl_quotes => {
                let elided = ch_at(i + 1).is_some_and(|n| n.is_ascii_digit()) || {
                    let word: String = toks[i + 1..]
                        .iter()
                        .map(Tok::ch)
                        .take_while(|c| c.is_alphabetic())
                        .collect::<String>()
                        .to_lowercase();
                    ELISIONS.contains(&word.as_str()) || (word == "n" && ch_at(i + 2) == Some('\''))
                };
                let open = opens(*prev, ch_at(i + 1)) && !elided;
                put(&mut out, prev, if open { '‘' } else { '’' });
                report.quotes += 1;
                i += 1;
            }
            '-' if matches!(toks[i], Tok::Ch(_)) => {
                let n = (i..toks.len()).take_while(|&j| plain_at(j, '-')).count();
                let spaced = n == 1
                    && prev.is_some_and(char::is_whitespace)
                    && ch_at(i + 1).is_some_and(char::is_whitespace);
                if n == 2 || n == 3 {
                    put(&mut out, prev, '—');
                    report.dashes += 1;
                } else if spaced {
                    put(&mut out, prev, '–');
                    report.dashes += 1;
                } else {
                    for _ in 0..n {
                        put(&mut out, prev, '-');
                    }
                }
                i += n;
            }
            '.' if matches!(toks[i], Tok::Ch(_)) && *prev != Some('.') => {
                let n = (i..toks.len()).take_while(|&j| plain_at(j, '.')).count();
                // `. . .` exactly: not part of a longer spaced run on either side.
                let spaced = n == 1
                    && !(i >= 2 && plain_at(i - 1, ' ') && plain_at(i - 2, '.'))
                    && plain_at(i + 1, ' ')
                    && plain_at(i + 2, '.')
                    && plain_at(i + 3, ' ')
                    && plain_at(i + 4, '.')
                    && !(plain_at(i + 5, ' ') && plain_at(i + 6, '.'));
                if n == 3 {
                    put(&mut out, prev, '…');
                    report.ellipses += 1;
                    i += 3;
                } else if spaced {
                    put(&mut out, prev, '…');
                    report.ellipses += 1;
                    i += 5;
                } else {
                    for _ in 0..n {
                        put(&mut out, prev, '.');
                    }
                    i += n;
                }
            }
            _ => {
                match &toks[i] {
                    Tok::Ch(ch) => out.push(*ch),
                    Tok::Ent(raw, _) => out.push_str(raw),
                }
                *prev = Some(c);
                i += 1;
            }
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run(text: &str) -> String {
        let mut prev = None;
        tidy_run(text, true, &mut prev, &mut TypographyReport::default())
    }

    fn body(inner: &str) -> String {
        format!("<html><head><title>\"t\"</title></head><body>{inner}</body></html>")
    }

    fn tidy(inner: &str) -> (String, TypographyReport) {
        let mut r = TypographyReport::default();
        let out = tidy_xhtml(&body(inner), true, &mut r);
        let start = out.find("<body>").unwrap() + 6;
        let end = out.rfind("</body>").unwrap();
        (out[start..end].to_owned(), r)
    }

    #[test]
    fn quotes_open_and_close() {
        assert_eq!(run(r#""Hello," she said."#), "“Hello,” she said.");
        assert_eq!(run("'Quoted' and don't"), "‘Quoted’ and don’t");
        assert_eq!(run(r#"He said, "It's 'fine'.""#), "He said, “It’s ‘fine’.”");
        assert_eq!(run("the dogs' bowls"), "the dogs’ bowls");
        assert_eq!(
            run("back in '90s, 'tis 'Twas rock 'n' roll"),
            "back in ’90s, ’tis ’Twas rock ’n’ roll"
        );
        assert_eq!(run("(\"aside\")"), "(“aside”)");
        assert_eq!(
            run(r#""Don't--" he said, "--and go""#),
            "“Don’t—” he said, “—and go”"
        );
        assert_eq!(run(r#"stop—"Go!""#), "stop—“Go!”");
    }

    #[test]
    fn quote_entities_curl_and_others_stay() {
        assert_eq!(run("&quot;Hi&quot; &amp; it&#39;s"), "“Hi” &amp; it’s");
        assert_eq!(run("a&nbsp;\"b\""), "a&nbsp;“b”");
    }

    #[test]
    fn dashes_and_ellipses() {
        assert_eq!(run("wait--no---stop"), "wait—no—stop");
        assert_eq!(
            run("1990 - 2000 and well-known"),
            "1990 – 2000 and well-known"
        );
        assert_eq!(run("a ---- rule and -x"), "a ---- rule and -x");
        assert_eq!(run("so... and . . . end"), "so… and … end");
        assert_eq!(
            run("four.... dots, file.txt, . . . . four"),
            "four.... dots, file.txt, . . . . four"
        );
    }

    #[test]
    fn context_crosses_inline_tags_and_resets_at_blocks() {
        let (out, r) = tidy(r#"<p><i>"Hello,</i>" she said.</p><p>"Next"</p>"#);
        assert_eq!(out, "<p><i>“Hello,</i>” she said.</p><p>“Next”</p>");
        assert_eq!(r.quotes, 4);
        let (out, _) = tidy(r#"<p>end<br/>"new line"</p>"#);
        assert_eq!(out, "<p>end<br/>“new line”</p>");
    }

    #[test]
    fn code_attributes_comments_and_head_are_untouched() {
        let inner = r#"<p class="a--b" title="x 'y'">"t"</p><pre>"a" -- b...</pre><p>x <code>'c'--</code>" and</p><!-- "c" -- --><style>p::before{content:"--"}</style><svg:svg><svg:text>"s"</svg:text></svg:svg><![CDATA[ "d" ]]>"#;
        let (out, _) = tidy(inner);
        assert_eq!(
            out,
            r#"<p class="a--b" title="x 'y'">“t”</p><pre>"a" -- b...</pre><p>x <code>'c'--</code>” and</p><!-- "c" -- --><style>p::before{content:"--"}</style><svg:svg><svg:text>"s"</svg:text></svg:svg><![CDATA[ "d" ]]>"#
        );
        let mut r = TypographyReport::default();
        let whole = tidy_xhtml(&body("<p>x</p>"), true, &mut r);
        assert!(whole.contains("<title>\"t\"</title>"), "{whole}");
    }

    #[test]
    fn empty_paragraphs_go_scene_breaks_stay() {
        let (out, r) = tidy(
            "<p>a</p><p></p><p class=\"x\"/>\n<p>  </p><p>&nbsp;</p>\n<p>&#160;</p><p> \u{a0} </p><p>b</p><p id=\"keep\"></p><p><img src=\"i.png\"/></p><p>&nbsp;</p><p>c</p>",
        );
        assert_eq!(
            out,
            "<p>a</p>\n<p>&nbsp;</p><p>b</p><p id=\"keep\"></p><p><img src=\"i.png\"/></p><p>&nbsp;</p><p>c</p>"
        );
        assert_eq!(r.empty_paragraphs, 5);
    }

    #[test]
    fn twice_is_once() {
        let inner = r#"<p>"Don't--" he said... 'tis - so</p><p></p><p>&nbsp;</p><p></p><p>&nbsp;</p><p>"x"</p>"#;
        let mut r = TypographyReport::default();
        let once = tidy_xhtml(&body(inner), true, &mut r);
        let mut r2 = TypographyReport::default();
        let twice = tidy_xhtml(&once, true, &mut r2);
        assert_eq!(once, twice);
        assert_eq!(r2, TypographyReport::default());
    }

    #[test]
    fn no_curling_leaves_quotes_straight() {
        let mut r = TypographyReport::default();
        let out = tidy_xhtml(&body("<p>\"Ja\" -- &quot;so&quot;...</p>"), false, &mut r);
        assert!(out.contains("<p>\"Ja\" — &quot;so&quot;…</p>"), "{out}");
        assert_eq!(r.quotes, 0);
    }

    #[test]
    fn languages_and_ncx() {
        assert!(curls_quotes(None));
        assert!(curls_quotes(Some("en-GB")));
        assert!(curls_quotes(Some("EN")));
        assert!(!curls_quotes(Some("de")));
        assert_eq!(
            document_lang(r#"<?xml version="1.0"?><html xmlns="x" xml:lang="fr"><body/></html>"#)
                .as_deref(),
            Some("fr")
        );
        assert_eq!(document_lang("<html><body/></html>"), None);
        let mut r = TypographyReport::default();
        let ncx = tidy_ncx(
            r#"<docTitle><text>"Book"</text></docTitle><navLabel><text>Don't -- stop</text></navLabel>"#,
            true,
            &mut r,
        );
        assert_eq!(
            ncx,
            "<docTitle><text>“Book”</text></docTitle><navLabel><text>Don’t — stop</text></navLabel>"
        );
    }

    #[test]
    fn no_body_is_unchanged() {
        let doc = "<html><p>\"x\"</p></html>";
        assert_eq!(tidy_xhtml(doc, true, &mut TypographyReport::default()), doc);
    }
}
