// SPDX-License-Identifier: GPL-3.0-or-later
//! EPUB → KEPUB (docs/superpowers/specs/2026-10-23-other-readers-design.md §8.1): the same
//! book with its text in `<span class="koboSpan" id="kobo.P.S">`, paragraph `P`, sentence `S`,
//! and the body in Kobo's two wrapper divs. With them a Kobo shows page and chapter counts and
//! reading statistics, and keeps progress per chapter; a plain EPUB goes to its older renderer.
//!
//! Text is wrapped where it stands, in a single pass over each XHTML file: nothing is parsed
//! into a tree and written back, so markup Deckle doesn't know comes through untouched. Text
//! inside `pre`, `code`, `script`, `style`, `svg`, `math` and tables is left alone (a table's
//! layout, the paper's weather strip and puzzle grid among them, doesn't need locating).

use std::io::{Read, Write};

/// Converts the EPUB `bytes` to a KEPUB. A book that is a KEPUB already comes back unchanged.
pub fn convert(bytes: &[u8]) -> Result<Vec<u8>, String> {
    let mut zip = zip::ZipArchive::new(std::io::Cursor::new(bytes)).map_err(|e| e.to_string())?;
    let mut files: Vec<(String, Vec<u8>)> = Vec::with_capacity(zip.len());
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
    let text = xhtml_files(&files)?;
    if files
        .iter()
        .filter(|(n, _)| text.contains(n))
        .any(|(_, d)| contains(d, b"koboSpan"))
    {
        return Ok(bytes.to_vec());
    }
    for (name, data) in &mut files {
        if text.contains(name)
            && let Ok(s) = std::str::from_utf8(data)
        {
            *data = spans(s).into_bytes();
        }
    }
    write(&files)
}

fn contains(hay: &[u8], needle: &[u8]) -> bool {
    hay.windows(needle.len()).any(|w| w == needle)
}

/// The XHTML documents the OPF's manifest lists, as paths inside the zip.
fn xhtml_files(files: &[(String, Vec<u8>)]) -> Result<Vec<String>, String> {
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
    let dir = opf_path.rsplit_once('/').map(|(d, _)| d).unwrap_or("");
    Ok(doc
        .descendants()
        .filter(|n| n.has_tag_name("item"))
        .filter(|n| n.attribute("media-type") == Some("application/xhtml+xml"))
        .filter_map(|n| n.attribute("href"))
        .map(|href| crate::epub::join(dir, &crate::epub::percent_decode(href)))
        .collect())
}

/// The zip again: `mimetype` first and stored, the rest deflated, in their old order.
fn write(files: &[(String, Vec<u8>)]) -> Result<Vec<u8>, String> {
    use zip::write::SimpleFileOptions;
    let mut out = zip::ZipWriter::new(std::io::Cursor::new(Vec::new()));
    let stored = SimpleFileOptions::default().compression_method(zip::CompressionMethod::Stored);
    let deflated =
        SimpleFileOptions::default().compression_method(zip::CompressionMethod::Deflated);
    let e = |e: zip::result::ZipError| e.to_string();
    let io = |e: std::io::Error| e.to_string();
    if let Some((_, m)) = files.iter().find(|(n, _)| n == "mimetype") {
        out.start_file("mimetype", stored).map_err(e)?;
        out.write_all(m).map_err(io)?;
    }
    for (name, data) in files.iter().filter(|(n, _)| n != "mimetype") {
        out.start_file(name.as_str(), deflated).map_err(e)?;
        out.write_all(data).map_err(io)?;
    }
    Ok(out.finish().map_err(e)?.into_inner())
}

/// Elements whose text is left as it is.
const SKIP: &[&str] = &["pre", "code", "script", "style", "svg", "math", "table"];

/// Elements that start a new paragraph number.
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
];

/// One XHTML document with its text in Kobo spans and its body in Kobo's wrapper divs.
pub fn spans(xhtml: &str) -> String {
    let mut out = String::with_capacity(xhtml.len() + xhtml.len() / 3);
    let mut in_body = false;
    let mut skip_depth = 0usize;
    let mut stack: Vec<String> = Vec::new();
    let mut para = 0u32;
    let mut sentence = 0u32;
    let mut rest = xhtml;
    while !rest.is_empty() {
        let Some(lt) = rest.find('<') else {
            text(
                rest,
                in_body && skip_depth == 0,
                para,
                &mut sentence,
                &mut out,
            );
            break;
        };
        text(
            &rest[..lt],
            in_body && skip_depth == 0,
            para,
            &mut sentence,
            &mut out,
        );
        rest = &rest[lt..];
        // Comments, CDATA, doctype and processing instructions pass through whole.
        let end = if rest.starts_with("<!--") {
            rest.find("-->").map(|i| i + 3)
        } else if rest.starts_with("<![CDATA[") {
            rest.find("]]>").map(|i| i + 3)
        } else {
            tag_end(rest)
        };
        let Some(end) = end else {
            out.push_str(rest);
            break;
        };
        let tag = &rest[..end];
        rest = &rest[end..];
        if tag.starts_with("<!") || tag.starts_with("<?") {
            out.push_str(tag);
            continue;
        }
        let closing = tag.starts_with("</");
        let self_closing = tag.ends_with("/>");
        let name = tag_name(tag);
        if closing {
            if name == "body" && in_body {
                out.push_str("</div></div>");
                in_body = false;
            }
            if let Some(at) = stack.iter().rposition(|n| *n == name) {
                for n in stack.drain(at..) {
                    if SKIP.contains(&n.as_str()) {
                        skip_depth = skip_depth.saturating_sub(1);
                    }
                }
            }
            out.push_str(tag);
            continue;
        }
        if name == "body" {
            out.push_str(tag);
            out.push_str("<div id=\"book-columns\"><div id=\"book-inner\">");
            in_body = true;
            continue;
        }
        if in_body && skip_depth == 0 && BLOCKS.contains(&name.as_str()) {
            para += 1;
            sentence = 0;
        }
        if in_body && skip_depth == 0 && name == "img" {
            // A picture gets a span of its own, so it can be located like text.
            para += 1;
            sentence = 1;
            out.push_str(&format!(
                "<span class=\"koboSpan\" id=\"kobo.{para}.1\">{tag}</span>"
            ));
            continue;
        }
        out.push_str(tag);
        if !self_closing {
            if SKIP.contains(&name.as_str()) {
                skip_depth += 1;
            }
            stack.push(name);
        }
    }
    out
}

/// Where the tag starting at `s[0] == '<'` ends, past its `>`, minding quoted attributes.
fn tag_end(s: &str) -> Option<usize> {
    let mut quote: Option<char> = None;
    for (i, c) in s.char_indices().skip(1) {
        match (quote, c) {
            (Some(q), c) if c == q => quote = None,
            (Some(_), _) => {}
            (None, '"' | '\'') => quote = Some(c),
            (None, '>') => return Some(i + 1),
            _ => {}
        }
    }
    None
}

/// A tag's element name, lowercase, without any namespace prefix.
fn tag_name(tag: &str) -> String {
    let inner = tag.trim_start_matches('<').trim_start_matches('/');
    let name: String = inner
        .chars()
        .take_while(|c| !c.is_whitespace() && *c != '/' && *c != '>')
        .collect();
    let name = name.rsplit(':').next().unwrap_or(&name);
    name.to_ascii_lowercase()
}

/// Writes `s`, each sentence of it in a span when `wrap` (and it isn't only whitespace).
fn text(s: &str, wrap: bool, para: u32, sentence: &mut u32, out: &mut String) {
    if !wrap || s.trim().is_empty() {
        out.push_str(s);
        return;
    }
    // Leading and trailing whitespace stays outside the spans, as it was.
    let start = s.len() - s.trim_start().len();
    let end = s.trim_end().len();
    out.push_str(&s[..start]);
    for piece in sentences(&s[start..end]) {
        if piece.trim().is_empty() {
            out.push_str(piece);
            continue;
        }
        *sentence += 1;
        out.push_str(&format!(
            "<span class=\"koboSpan\" id=\"kobo.{para}.{sentence}\">{piece}</span>"
        ));
    }
    out.push_str(&s[end..]);
}

/// `s` cut after each run of sentence-ending punctuation (and any closing quote or bracket)
/// that is followed by whitespace; the whitespace stays with the sentence before it.
fn sentences(s: &str) -> Vec<&str> {
    let mut out = Vec::new();
    let mut start = 0;
    let chars: Vec<(usize, char)> = s.char_indices().collect();
    let mut i = 0;
    while i < chars.len() {
        let (_, c) = chars[i];
        if matches!(c, '.' | '!' | '?' | '…') {
            let mut j = i + 1;
            while j < chars.len()
                && matches!(
                    chars[j].1,
                    '.' | '!' | '?' | '…' | '"' | '\'' | '”' | '’' | ')' | ']' | '»'
                )
            {
                j += 1;
            }
            if j < chars.len() && chars[j].1.is_whitespace() {
                while j < chars.len() && chars[j].1.is_whitespace() {
                    j += 1;
                }
                let cut = chars.get(j).map_or(s.len(), |(at, _)| *at);
                out.push(&s[start..cut]);
                start = cut;
            }
            i = j;
            continue;
        }
        i += 1;
    }
    if start < s.len() {
        out.push(&s[start..]);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    const PAGE: &str = "<?xml version=\"1.0\" encoding=\"utf-8\"?>\n<!DOCTYPE html>\n\
        <html xmlns=\"http://www.w3.org/1999/xhtml\"><head><title>T</title>\
        <style>p { margin: 0; }</style></head>\n\
        <body class=\"x\">\n<h1>Chapter One</h1>\n\
        <p>It was late. The mill was quiet! Was it? <em>Yes</em>, it was.</p>\n\
        <!-- a comment. With sentences. -->\n\
        <pre>code. stays.</pre>\n\
        <table class=\"strip\"><tr><td>Mon. Tue.</td></tr></table>\n\
        <p><img src=\"a.jpg\" alt=\"A > B\"/></p>\n\
        </body></html>";

    #[test]
    fn text_is_wrapped_by_paragraph_and_sentence() {
        let out = spans(PAGE);
        assert!(
            out.contains("<h1><span class=\"koboSpan\" id=\"kobo.1.1\">Chapter One</span></h1>")
        );
        assert!(out.contains("<span class=\"koboSpan\" id=\"kobo.2.1\">It was late. </span>"));
        assert!(out.contains("id=\"kobo.2.2\">The mill was quiet! </span>"));
        assert!(out.contains("id=\"kobo.2.3\">Was it?</span> <em>"));
        assert!(out.contains("<em><span class=\"koboSpan\" id=\"kobo.2.4\">Yes</span></em>"));
        assert!(out.contains("id=\"kobo.2.5\">, it was.</span></p>"));
    }

    #[test]
    fn the_body_is_in_kobos_wrapper_divs() {
        let out = spans(PAGE);
        assert!(
            out.contains(
                "<body class=\"x\"><div id=\"book-columns\"><div id=\"book-inner\">\n<h1>"
            )
        );
        assert!(out.contains("</div></div></body></html>"));
    }

    #[test]
    fn head_comments_pre_and_tables_are_left_alone() {
        let out = spans(PAGE);
        assert!(out.contains("<title>T</title>"));
        assert!(out.contains("<style>p { margin: 0; }</style>"));
        assert!(out.contains("<!-- a comment. With sentences. -->"));
        assert!(out.contains("<pre>code. stays.</pre>"));
        assert!(out.contains("<td>Mon. Tue.</td>"));
    }

    #[test]
    fn a_picture_gets_its_own_span_and_quoted_brackets_dont_end_a_tag() {
        let out = spans(PAGE);
        assert!(out.contains(
            "<span class=\"koboSpan\" id=\"kobo.4.1\"><img src=\"a.jpg\" alt=\"A > B\"/></span>"
        ));
    }

    #[test]
    fn ids_are_unique_and_the_result_still_parses() {
        let out = spans(PAGE);
        let doc = roxmltree::Document::parse_with_options(
            &out,
            roxmltree::ParsingOptions {
                allow_dtd: true,
                ..Default::default()
            },
        )
        .unwrap();
        let ids: Vec<&str> = doc
            .descendants()
            .filter_map(|n| n.attribute("id"))
            .filter(|id| id.starts_with("kobo."))
            .collect();
        let mut unique = ids.clone();
        unique.sort();
        unique.dedup();
        assert_eq!(ids.len(), unique.len());
        // the heading, five pieces of the paragraph, the picture
        assert_eq!(ids.len(), 7);
    }

    #[test]
    fn sentences_keep_abbreviations_without_a_following_space_together() {
        assert_eq!(sentences("3.14 is pi. Next"), ["3.14 is pi. ", "Next"]);
        assert_eq!(sentences("“Go.” She went."), ["“Go.” ", "She went."]);
        assert_eq!(sentences("Wait..."), ["Wait..."]);
    }

    fn epub(pages: &[(&str, &str)]) -> Vec<u8> {
        let mut files: Vec<(String, Vec<u8>)> = vec![
            ("mimetype".into(), b"application/epub+zip".to_vec()),
            (
                "META-INF/container.xml".into(),
                b"<?xml version=\"1.0\"?><container version=\"1.0\" \
                  xmlns=\"urn:oasis:names:tc:opendocument:xmlns:container\"><rootfiles>\
                  <rootfile full-path=\"OEBPS/content.opf\" \
                  media-type=\"application/oebps-package+xml\"/></rootfiles></container>"
                    .to_vec(),
            ),
        ];
        let items: String = pages
            .iter()
            .enumerate()
            .map(|(i, (name, _))| {
                format!("<item id=\"p{i}\" href=\"{name}\" media-type=\"application/xhtml+xml\"/>")
            })
            .collect();
        files.push((
            "OEBPS/content.opf".into(),
            format!(
                "<?xml version=\"1.0\"?><package xmlns=\"http://www.idpf.org/2007/opf\" \
                 version=\"3.0\"><manifest>{items}\
                 <item id=\"css\" href=\"style.css\" media-type=\"text/css\"/></manifest>\
                 </package>"
            )
            .into_bytes(),
        ));
        files.push(("OEBPS/style.css".into(), b"p. { }".to_vec()));
        for (name, body) in pages {
            let path = crate::epub::percent_decode(name);
            files.push((format!("OEBPS/{path}"), body.as_bytes().to_vec()));
        }
        write(&files).unwrap()
    }

    fn read(bytes: &[u8]) -> Vec<(String, Vec<u8>)> {
        let mut zip = zip::ZipArchive::new(std::io::Cursor::new(bytes)).unwrap();
        (0..zip.len())
            .map(|i| {
                let mut f = zip.by_index(i).unwrap();
                let mut d = Vec::new();
                f.read_to_end(&mut d).unwrap();
                (f.name().unwrap().into_owned(), d)
            })
            .collect()
    }

    #[test]
    fn a_book_converts_its_xhtml_and_nothing_else() {
        let book = epub(&[(
            "text%201.xhtml",
            "<html><body><p>One. Two.</p></body></html>",
        )]);
        let out = convert(&book).unwrap();
        let files = read(&out);
        assert_eq!(files[0].0, "mimetype");
        let page = &files
            .iter()
            .find(|(n, _)| n == "OEBPS/text 1.xhtml")
            .unwrap()
            .1;
        assert!(String::from_utf8_lossy(page).contains("id=\"kobo.1.2\">Two.</span>"));
        let css = &files
            .iter()
            .find(|(n, _)| n == "OEBPS/style.css")
            .unwrap()
            .1;
        assert_eq!(css, b"p. { }");
        // Mimetype stays stored, so the EPUB is still valid.
        let mut zip = zip::ZipArchive::new(std::io::Cursor::new(&out)).unwrap();
        assert_eq!(
            zip.by_index(0).unwrap().compression(),
            zip::CompressionMethod::Stored
        );
    }

    #[test]
    fn a_kepub_comes_back_unchanged() {
        let book = epub(&[("a.xhtml", "<html><body><p>One.</p></body></html>")]);
        let once = convert(&book).unwrap();
        assert_eq!(convert(&once).unwrap(), once);
    }

    #[test]
    fn not_an_epub_is_an_error() {
        assert!(convert(b"not a zip").is_err());
    }
}
