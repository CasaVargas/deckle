// SPDX-License-Identifier: GPL-3.0-or-later
//! Built-in, offline book styles. Explicit run flags can override their stage defaults.
use crate::pipeline::{RunOptions, XRayMode};
use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Preset {
    Academic,
    Novel,
    Manual,
}

impl std::str::FromStr for Preset {
    type Err = String;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "academic" => Ok(Self::Academic),
            "novel" => Ok(Self::Novel),
            "manual" => Ok(Self::Manual),
            _ => Err(format!(
                "unknown preset {s:?}; choose academic, novel, or manual"
            )),
        }
    }
}
impl Preset {
    pub fn name(self) -> &'static str {
        match self {
            Self::Academic => "academic",
            Self::Novel => "novel",
            Self::Manual => "manual",
        }
    }
    pub fn typography(self) -> bool {
        self != Self::Manual
    }
    pub fn popup_notes(self) -> bool {
        self != Self::Manual
    }
    pub fn xray(self) -> XRayMode {
        if self == Self::Novel {
            XRayMode::Auto
        } else {
            XRayMode::Off
        }
    }
    /// Does not change cover/CDE, metadata lookups, LLM or Wikipedia permissions.
    /// Word Wise defaults off so selecting a style never initiates a dictionary download.
    pub fn apply(self, opts: &mut RunOptions) {
        opts.preset = Some(self);
        opts.typography = self.typography();
        opts.popup_notes = self.popup_notes();
        opts.pages = Some(None);
        opts.xray = self.xray();
        opts.word_wise = None;
    }
    pub fn css(self) -> String {
        let (font, leading, margin, indent) = match self {
            Self::Academic => ("serif", "1.45", "0.6em", "0"),
            Self::Novel => ("serif", "1.35", "0", "1.2em"),
            Self::Manual => ("sans-serif", "1.4", "0.7em", "0"),
        };
        format!(
            "body {{ font-family: {font}; line-height: {leading}; }}\nbody p, body li, body blockquote {{ font-family: {font} !important; line-height: {leading} !important; }}\nbody p {{ margin-top: 0 !important; margin-bottom: {margin} !important; text-indent: {indent} !important; }}\nbody pre, body pre *, body code, body kbd, body samp {{ font-family: monospace !important; }}\nbody pre p, body blockquote p, body li p, body td p, body th p, body h1 + p, body h2 + p, body h3 + p {{ text-indent: 0 !important; }}\n"
        )
    }
}

/// Append the override after publisher styles. Replace only our marked style on reprocessing.
/// XML ranges preserve all book markup, including inline language and note semantics.
pub fn style_xhtml(text: &str, preset: Preset) -> Result<String, String> {
    let doc = roxmltree::Document::parse(text).map_err(|e| e.to_string())?;
    let head = doc
        .root_element()
        .children()
        .find(|n| {
            n.is_element()
                && n.tag_name().name() == "head"
                && matches!(
                    n.tag_name().namespace(),
                    None | Some("http://www.w3.org/1999/xhtml")
                )
        })
        .ok_or("XHTML has no head")?;
    let prefix = head
        .lookup_prefix("http://www.w3.org/1999/xhtml")
        .map(|p| format!("{p}:"))
        .unwrap_or_default();
    let style = format!(
        "<{prefix}style type=\"text/css\" data-deckle-preset=\"{}\">{}</{prefix}style>",
        preset.name(),
        preset.css()
    );
    let mut edits = Vec::new();
    for n in head.children().filter(|n| {
        n.is_element()
            && n.tag_name().name() == "style"
            && n.attribute("data-deckle-preset").is_some()
    }) {
        edits.push((n.range(), String::new()));
    }
    let range = head.range();
    let raw = &text[range.clone()];
    if raw.ends_with("/>") {
        edits.push((range.end - 2..range.end, format!(">{style}</{prefix}head>")));
    } else {
        let pos = range.start + raw.rfind("</").ok_or("head has no closing tag")?;
        edits.push((pos..pos, style));
    }
    edits.sort_by_key(|(r, _)| r.start);
    let mut out = text.to_owned();
    for (range, value) in edits.into_iter().rev() {
        out.replace_range(range, &value);
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn replacing_a_style_preserves_markup_and_is_idempotent() {
        for text in [
            r#"<html xmlns="http://www.w3.org/1999/xhtml"><head><style>p {color:red}</style></head><body><p lang="fr">Text <code>x</code></p></body></html>"#,
            r#"<h:html xmlns:h="http://www.w3.org/1999/xhtml"><h:head/><h:body/></h:html>"#,
        ] {
            let novel = style_xhtml(text, Preset::Novel).unwrap();
            assert_eq!(style_xhtml(&novel, Preset::Novel).unwrap(), novel);
            let manual = style_xhtml(&novel, Preset::Manual).unwrap();
            assert_eq!(manual.matches("data-deckle-preset=").count(), 1);
            assert!(manual.contains("sans-serif"));
            let a = roxmltree::Document::parse(text).unwrap();
            let b = roxmltree::Document::parse(&manual).unwrap();
            let body = |doc: &roxmltree::Document<'_>| {
                doc.descendants()
                    .find(|n| n.is_element() && n.tag_name().name() == "body")
                    .unwrap()
                    .range()
            };
            assert_eq!(&text[body(&a)], &manual[body(&b)]);
        }
    }
}
