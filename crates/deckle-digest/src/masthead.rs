// SPDX-License-Identifier: GPL-3.0-or-later
//! The masthead font: the paper's name on the cover and the PDF edition (masthead design).
//! A choice of three bundled faces or a TrueType file of the reader's own. A bad choice never
//! fails a paper: it falls back to the blackletter and says why.

use std::collections::HashMap;
use std::sync::{Mutex, OnceLock};

use ab_glyph::{Font, FontRef, GlyphId};
use serde::{Deserialize, Deserializer, Serialize, Serializer};

const BLACKLETTER: &[u8] = include_bytes!("../fonts/UnifrakturMaguntia-Book.ttf");
const SERIF: &[u8] = include_bytes!("../fonts/OldStandard-Bold.ttf");
const SANS: &[u8] = include_bytes!("../fonts/Inter-Bold.ttf");

/// `masthead` in `digest.toml`.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub enum MastheadFont {
    #[default]
    Blackletter,
    Serif,
    Sans,
    /// A `.ttf` file; `~` is the home folder.
    File(String),
}

impl MastheadFont {
    pub fn is_default(&self) -> bool {
        *self == MastheadFont::Blackletter
    }

    /// The setting as written in `digest.toml`.
    pub fn as_str(&self) -> &str {
        match self {
            MastheadFont::Blackletter => "blackletter",
            MastheadFont::Serif => "serif",
            MastheadFont::Sans => "sans",
            MastheadFont::File(p) => p,
        }
    }

    /// The face for a paper called `title`. Err says why the blackletter is used instead.
    pub fn resolve(&self, title: &str) -> Result<Masthead, String> {
        let bytes = match self {
            MastheadFont::Blackletter => BLACKLETTER,
            MastheadFont::Serif => SERIF,
            MastheadFont::Sans => SANS,
            MastheadFont::File(path) => load(path)?,
        };
        let font = FontRef::try_from_slice(bytes).map_err(|e| format!("{}: {e}", self.as_str()))?;
        let missing: Vec<char> = title
            .chars()
            .filter(|c| !c.is_whitespace() && font.glyph_id(*c) == GlyphId(0))
            .collect();
        if !missing.is_empty() {
            let shown: String = missing.iter().take(3).collect();
            return Err(format!("{} has no letter for “{shown}”", self.as_str()));
        }
        Ok(Masthead(bytes))
    }
}

/// A resolved masthead face: font bytes that live for the whole run.
#[derive(Clone, Copy)]
pub struct Masthead(&'static [u8]);

impl Masthead {
    pub fn bytes(self) -> &'static [u8] {
        self.0
    }

    /// Whether this is the bundled blackletter (the default look).
    pub fn is_blackletter(self) -> bool {
        std::ptr::eq(self.0, BLACKLETTER)
    }
}

impl Default for Masthead {
    fn default() -> Self {
        Masthead(BLACKLETTER)
    }
}

impl std::fmt::Debug for Masthead {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "Masthead({} bytes)", self.0.len())
    }
}

/// A file's bytes, read once per path and kept: a font is `FontRef<'static>` everywhere else.
fn load(path: &str) -> Result<&'static [u8], String> {
    static CACHE: OnceLock<Mutex<HashMap<String, &'static [u8]>>> = OnceLock::new();
    let cache = CACHE.get_or_init(Default::default);
    let mut cache = cache.lock().unwrap_or_else(|e| e.into_inner());
    if let Some(bytes) = cache.get(path) {
        return Ok(bytes);
    }
    let full = crate::inbox::expand_home(path);
    let bytes = std::fs::read(&full).map_err(|e| format!("{}: {e}", full.display()))?;
    // TrueType outlines only: the PDF edition embeds them as they are.
    if !(bytes.starts_with(&[0, 1, 0, 0]) || bytes.starts_with(b"true")) {
        return Err(format!(
            "{}: not a TrueType (.ttf) font (.otf and .ttc aren't supported)",
            full.display()
        ));
    }
    let leaked: &'static [u8] = Box::leak(bytes.into_boxed_slice());
    cache.insert(path.to_string(), leaked);
    Ok(leaked)
}

impl Serialize for MastheadFont {
    fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        s.serialize_str(self.as_str())
    }
}

impl<'de> Deserialize<'de> for MastheadFont {
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        let s = String::deserialize(d)?;
        Ok(match s.trim().to_lowercase().as_str() {
            "" | "blackletter" => MastheadFont::Blackletter,
            "serif" => MastheadFont::Serif,
            "sans" => MastheadFont::Sans,
            _ => MastheadFont::File(s.trim().to_string()),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fonts_dir(name: &str) -> String {
        format!("{}/fonts/{name}", env!("CARGO_MANIFEST_DIR"))
    }

    #[test]
    fn the_default_is_the_blackletter() {
        assert!(MastheadFont::default().is_default());
        assert!(
            MastheadFont::default()
                .resolve("The Deckle Daily")
                .unwrap()
                .is_blackletter()
        );
    }

    #[test]
    fn names_parse_and_paths_are_files() {
        let parse =
            |s: &str| -> MastheadFont { toml::from_str::<Wrap>(&format!("m = {s:?}")).unwrap().m };
        #[derive(Deserialize)]
        struct Wrap {
            m: MastheadFont,
        }
        assert_eq!(parse("Serif"), MastheadFont::Serif);
        assert_eq!(parse("sans"), MastheadFont::Sans);
        assert_eq!(parse(""), MastheadFont::Blackletter);
        assert_eq!(parse("~/f.ttf"), MastheadFont::File("~/f.ttf".into()));
    }

    #[test]
    fn bundled_faces_resolve_and_differ() {
        let b = MastheadFont::Blackletter.resolve("Gazette").unwrap();
        let s = MastheadFont::Serif.resolve("Gazette").unwrap();
        let n = MastheadFont::Sans.resolve("Gazette").unwrap();
        assert_ne!(b.bytes().len(), s.bytes().len());
        assert_ne!(s.bytes().len(), n.bytes().len());
    }

    #[test]
    fn a_ttf_path_resolves() {
        let path = fonts_dir("OldStandard-Italic.ttf");
        let m = MastheadFont::File(path).resolve("Gazette").unwrap();
        assert!(!m.is_blackletter());
    }

    #[test]
    fn a_bad_file_says_why() {
        let missing = MastheadFont::File("/no/such/font.ttf".into());
        assert!(missing.resolve("Gazette").unwrap_err().contains("font.ttf"));
        let not_ttf = MastheadFont::File(fonts_dir("Inter-OFL.txt"));
        assert!(not_ttf.resolve("Gazette").unwrap_err().contains("TrueType"));
    }

    #[test]
    fn a_font_without_the_letters_is_refused() {
        let err = MastheadFont::Serif.resolve("日本新聞").unwrap_err();
        assert!(err.contains("no letter"), "{err}");
    }
}
