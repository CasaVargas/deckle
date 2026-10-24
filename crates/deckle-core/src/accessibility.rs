// SPDX-License-Identifier: GPL-3.0-or-later
//! Offline language-tag repair and an image-alt audit (#32).
use language_tags::LanguageTag;
use std::ops::Range;

const XML: &str = "http://www.w3.org/XML/1998/namespace";
const XHTML: &str = "http://www.w3.org/1999/xhtml";
const DC: &str = "http://purl.org/dc/elements/1.1/";

/// Normalize well-formed BCP 47 spelling, also accepting common locale underscores.
/// Do not guess languages from names or rewrite registered aliases/scripts.
pub fn language_tag(raw: &str) -> Option<String> {
    LanguageTag::parse(&raw.trim().replace('_', "-"))
        .ok()
        .map(LanguageTag::into_string)
}

pub fn book_language(raw: &str, warnings: &mut Vec<String>) -> String {
    if raw.trim().is_empty() {
        warnings.push("no dc:language; assuming en".into());
        "en".into()
    } else if let Some(tag) = language_tag(raw) {
        tag
    } else {
        warnings.push(format!(
            "invalid dc:language {raw:?}; using und (undetermined)"
        ));
        "und".into()
    }
}

fn apply(text: &str, mut edits: Vec<(Range<usize>, String)>) -> String {
    edits.sort_by_key(|(r, _)| r.start);
    let mut result = text.to_owned();
    for (range, replacement) in edits.into_iter().rev() {
        result.replace_range(range, &replacement);
    }
    result
}

/// Return the corrected OPF, primary language, and number of changed language elements.
/// Namespace-aware spans preserve every unrelated byte of the package.
pub fn normalize_opf(
    text: &str,
    warnings: &mut Vec<String>,
) -> Result<(String, String, u32), String> {
    let doc = roxmltree::Document::parse(text).map_err(|e| e.to_string())?;
    let metadata = doc
        .root_element()
        .children()
        .find(|n| n.has_tag_name("metadata"))
        .ok_or("OPF has no metadata element")?;
    let languages: Vec<_> = metadata
        .children()
        .filter(|n| {
            n.is_element()
                && n.tag_name().name() == "language"
                && matches!(n.tag_name().namespace(), None | Some(DC))
        })
        .collect();
    let raw_language = languages
        .iter()
        .filter_map(|n| n.text())
        .find(|s| !s.trim().is_empty())
        .unwrap_or("");
    let language = book_language(raw_language, warnings);
    let mut edits = Vec::new();
    for node in &languages {
        let raw: String = node
            .children()
            .filter(|n| n.is_text())
            .filter_map(|n| n.text())
            .collect();
        let lang = if raw.trim().is_empty() || raw == raw_language {
            language.clone()
        } else {
            book_language(&raw, warnings)
        };
        if raw != lang {
            let range = node.range();
            edits.push((range.clone(), element_content(&text[range], &lang)?));
        }
    }
    if languages.is_empty() {
        let element = &text[metadata.range()];
        let added = format!("<dc:language xmlns:dc=\"{DC}\">{language}</dc:language>");
        if let Some(closing) = element.rfind("</") {
            let at = metadata.range().start + closing;
            edits.push((at..at, added));
        } else {
            edits.push((metadata.range(), element_content(element, &added)?));
        }
    }
    let changed = edits.len() as u32;
    Ok((apply(text, edits), language, changed))
}

/// Replace element content while preserving its qualified name and attributes.
fn element_content(original: &str, content: &str) -> Result<String, String> {
    let end = tag_end(original).ok_or("element has no opening tag end")?;
    if original[..end].trim_end().ends_with('/') {
        let qname = original[1..]
            .split([' ', '\t', '\r', '\n', '/', '>'])
            .next()
            .ok_or("element has no name")?;
        Ok(format!(
            "{}>{content}</{qname}>",
            original[..end].trim_end().trim_end_matches('/').trim_end()
        ))
    } else {
        let closing = original.rfind("</").ok_or("element has no closing tag")?;
        Ok(format!(
            "{}{content}{}",
            &original[..end + 1],
            &original[closing..]
        ))
    }
}

/// Find an opening tag's `>` outside quoted attribute values.
fn tag_end(tag: &str) -> Option<usize> {
    let mut quote = None;
    for (i, c) in tag.char_indices() {
        match (quote, c) {
            (Some(q), c) if q == c => quote = None,
            (None, '\'' | '"') => quote = Some(c),
            (None, '>') => return Some(i),
            _ => {}
        }
    }
    None
}

/// Fill missing root language attributes without overriding chapter/inline language choices.
/// Invalid/conflicting explicit values are reported and retained for the owner to resolve.
pub fn normalize_xhtml(
    text: &str,
    book_language: &str,
    warnings: &mut Vec<String>,
) -> Result<(String, u32), String> {
    let doc = roxmltree::Document::parse(text).map_err(|e| e.to_string())?;
    let root = doc.root_element();
    if root.tag_name().name() != "html"
        || !matches!(root.tag_name().namespace(), None | Some(XHTML))
    {
        return Ok((text.into(), 0));
    }
    let mut edits = Vec::new();
    let attrs = [
        root.attribute_node("lang"),
        root.attribute_node((XML, "lang")),
    ];
    let mut normalized = [None, None];
    for (i, attr) in attrs.iter().enumerate() {
        let Some(attr) = attr else {
            continue;
        };
        let value = if attr.value().is_empty() {
            Some(String::new())
        } else {
            language_tag(attr.value())
        };
        if let Some(value) = &value {
            if value != attr.value() {
                edits.push((attr.range_value(), value.clone()));
            }
        } else {
            warnings.push(format!(
                "invalid root {}={:?}; preserved",
                if i == 0 { "lang" } else { "xml:lang" },
                attr.value()
            ));
        }
        normalized[i] = value;
    }
    if let (Some(a), Some(b)) = (&normalized[0], &normalized[1])
        && a != b
    {
        warnings
            .push("conflicting root lang and xml:lang; preserved both explicit languages".into());
    }
    let inherited = normalized[1]
        .as_ref()
        .or(normalized[0].as_ref())
        .cloned()
        .or_else(|| {
            attrs
                .iter()
                .all(Option::is_none)
                .then(|| book_language.into())
        });
    let mut additions = String::new();
    let mut changed = edits.len() as u32;
    if let Some(language) = inherited {
        for (i, name) in ["lang", "xml:lang"].iter().enumerate() {
            if attrs[i].is_none() {
                additions.push_str(&format!(" {name}=\"{language}\""));
                changed += 1;
            }
        }
    }
    if !additions.is_empty() {
        let start = root.range().start;
        let end = start + tag_end(&text[start..]).ok_or("html root has no opening tag end")?;
        let at = if text[..end].ends_with('/') {
            end - 1
        } else {
            end
        };
        edits.push((at..at, additions));
    }
    Ok((apply(text, edits), changed))
}

/// Missing `alt` is different from an intentionally empty `alt`. No descriptions are inferred.
pub fn missing_alt(text: &str) -> Result<u32, String> {
    let doc = roxmltree::Document::parse(text).map_err(|e| e.to_string())?;
    Ok(doc
        .descendants()
        .filter(|n| {
            n.is_element()
                && n.tag_name().name() == "img"
                && matches!(n.tag_name().namespace(), None | Some(XHTML))
                && n.attribute("alt").is_none()
        })
        .count() as u32)
}
