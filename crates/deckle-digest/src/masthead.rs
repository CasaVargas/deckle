// SPDX-License-Identifier: GPL-3.0-or-later
//! The masthead font: the paper's name on the cover and the PDF edition (masthead design, Daily
//! tab redesign §7). Ten bundled faces (SIL OFL, each with its licence in `fonts/`) or a
//! TrueType file of the reader's own. A bad choice never fails a paper: it falls back to the
//! default face, then to Old Standard, and says why.

use std::collections::HashMap;
use std::sync::{Mutex, OnceLock};

use ab_glyph::{Font, FontRef, GlyphId};
use serde::{Deserialize, Deserializer, Serialize, Serializer};

const CONSENT: &[u8] = include_bytes!("../fonts/ManufacturingConsent-Regular.ttf");
const GRENZE: &[u8] = include_bytes!("../fonts/GrenzeGotisch-SemiBold.ttf");
const PIRATA: &[u8] = include_bytes!("../fonts/PirataOne-Regular.ttf");
const TEXTURINA: &[u8] = include_bytes!("../fonts/Texturina-Bold.ttf");
const UNIFRAKTUR: &[u8] = include_bytes!("../fonts/UnifrakturMaguntia-Book.ttf");
const PLAYFAIR: &[u8] = include_bytes!("../fonts/PlayfairDisplay-Bold.ttf");
const BODONI: &[u8] = include_bytes!("../fonts/BodoniModa-Bold.ttf");
const CASLON: &[u8] = include_bytes!("../fonts/LibreCaslonDisplay-Regular.ttf");
const SERIF: &[u8] = include_bytes!("../fonts/OldStandard-Bold.ttf");
const SANS: &[u8] = include_bytes!("../fonts/Inter-Bold.ttf");

/// `masthead` in `digest.toml`.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub enum MastheadFont {
    /// Manufacturing Consent, the default.
    #[default]
    Consent,
    Grenze,
    Pirata,
    Texturina,
    /// UnifrakturMaguntia, the default before; `blackletter` in older files.
    Unifraktur,
    Playfair,
    Bodoni,
    Caslon,
    /// Old Standard Bold.
    Serif,
    /// Inter Bold.
    Sans,
    /// A `.ttf` file; `~` is the home folder.
    File(String),
}

/// The bundled faces, in the order the app shows them: blackletters, then the rest.
pub const BUNDLED: [MastheadFont; 10] = [
    MastheadFont::Consent,
    MastheadFont::Grenze,
    MastheadFont::Pirata,
    MastheadFont::Texturina,
    MastheadFont::Unifraktur,
    MastheadFont::Playfair,
    MastheadFont::Bodoni,
    MastheadFont::Caslon,
    MastheadFont::Serif,
    MastheadFont::Sans,
];

impl MastheadFont {
    pub fn is_default(&self) -> bool {
        *self == MastheadFont::Consent
    }

    /// The setting as written in `digest.toml`.
    pub fn as_str(&self) -> &str {
        match self {
            MastheadFont::Consent => "consent",
            MastheadFont::Grenze => "grenze",
            MastheadFont::Pirata => "pirata",
            MastheadFont::Texturina => "texturina",
            MastheadFont::Unifraktur => "unifraktur",
            MastheadFont::Playfair => "playfair",
            MastheadFont::Bodoni => "bodoni",
            MastheadFont::Caslon => "caslon",
            MastheadFont::Serif => "serif",
            MastheadFont::Sans => "sans",
            MastheadFont::File(p) => p,
        }
    }

    /// A bundled face's font and the name the PDF embeds it under; None for a file.
    fn bundled(&self) -> Option<(&'static [u8], &'static str)> {
        Some(match self {
            MastheadFont::Consent => (CONSENT, "ManufacturingConsent-Regular"),
            MastheadFont::Grenze => (GRENZE, "GrenzeGotisch-SemiBold"),
            MastheadFont::Pirata => (PIRATA, "PirataOne-Regular"),
            MastheadFont::Texturina => (TEXTURINA, "Texturina-Bold"),
            MastheadFont::Unifraktur => (UNIFRAKTUR, "UnifrakturMaguntia-Book"),
            MastheadFont::Playfair => (PLAYFAIR, "PlayfairDisplay-Bold"),
            MastheadFont::Bodoni => (BODONI, "BodoniModa-Bold"),
            MastheadFont::Caslon => (CASLON, "LibreCaslonDisplay-Regular"),
            MastheadFont::Serif => (SERIF, "OldStandard-Bold"),
            MastheadFont::Sans => (SANS, "Inter-Bold"),
            MastheadFont::File(_) => return None,
        })
    }

    /// The face for a paper called `title`. Err says why it can't be used.
    pub fn resolve(&self, title: &str) -> Result<Masthead, String> {
        let (bytes, name) = match self.bundled() {
            Some(b) => b,
            None => match self {
                MastheadFont::File(path) => (load(path)?, "Masthead"),
                _ => unreachable!("every other face is bundled"),
            },
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
        Ok(Masthead { bytes, name })
    }

    /// The face to use, falling back to the default and then Old Standard; the note says why.
    pub fn resolve_or_fall_back(&self, title: &str) -> (Masthead, Option<String>) {
        match self.resolve(title) {
            Ok(m) => (m, None),
            Err(why) => match MastheadFont::default().resolve(title) {
                Ok(m) if !self.is_default() => {
                    (m, Some(format!("{why}; using Manufacturing Consent")))
                }
                _ => (
                    Masthead {
                        bytes: SERIF,
                        name: "OldStandard-Bold",
                    },
                    Some(format!("{why}; using Old Standard")),
                ),
            },
        }
    }
}

/// A resolved masthead face: font bytes that live for the whole run, and the name the PDF
/// embeds them under.
#[derive(Clone, Copy)]
pub struct Masthead {
    bytes: &'static [u8],
    name: &'static str,
}

impl Masthead {
    pub fn bytes(self) -> &'static [u8] {
        self.bytes
    }

    /// The font's name in the PDF: the face's own, or "Masthead" for a file.
    pub fn name(self) -> &'static str {
        self.name
    }
}

impl Default for Masthead {
    fn default() -> Self {
        Masthead {
            bytes: CONSENT,
            name: "ManufacturingConsent-Regular",
        }
    }
}

impl std::fmt::Debug for Masthead {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "Masthead({}, {} bytes)", self.name, self.bytes.len())
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
        let key = s.trim().to_lowercase();
        if key.is_empty() {
            return Ok(MastheadFont::Consent);
        }
        if key == "blackletter" {
            return Ok(MastheadFont::Unifraktur);
        }
        Ok(BUNDLED
            .into_iter()
            .find(|f| f.as_str() == key)
            .unwrap_or_else(|| MastheadFont::File(s.trim().to_string())))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fonts_dir(name: &str) -> String {
        format!("{}/fonts/{name}", env!("CARGO_MANIFEST_DIR"))
    }

    #[test]
    fn the_default_is_manufacturing_consent() {
        assert!(MastheadFont::default().is_default());
        let m = MastheadFont::default().resolve("The Deckle Daily").unwrap();
        assert_eq!(m.name(), "ManufacturingConsent-Regular");
    }

    #[test]
    fn every_bundled_face_draws_the_deckle_daily() {
        let mut names = std::collections::HashSet::new();
        for face in BUNDLED {
            let m = face
                .resolve("The Deckle Daily")
                .unwrap_or_else(|e| panic!("{e}"));
            assert!(names.insert(m.name()), "{} named twice", m.name());
        }
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
        assert_eq!(parse(""), MastheadFont::Consent);
        assert_eq!(parse("blackletter"), MastheadFont::Unifraktur);
        assert_eq!(parse("Unifraktur"), MastheadFont::Unifraktur);
        assert_eq!(parse("bodoni"), MastheadFont::Bodoni);
        for face in BUNDLED {
            assert_eq!(parse(face.as_str()), face, "{} round-trips", face.as_str());
        }
        assert_eq!(parse("~/f.ttf"), MastheadFont::File("~/f.ttf".into()));
    }

    #[test]
    fn bundled_faces_resolve_and_differ() {
        let b = MastheadFont::Unifraktur.resolve("Gazette").unwrap();
        let s = MastheadFont::Serif.resolve("Gazette").unwrap();
        let n = MastheadFont::Sans.resolve("Gazette").unwrap();
        assert_ne!(b.bytes().len(), s.bytes().len());
        assert_ne!(s.bytes().len(), n.bytes().len());
    }

    #[test]
    fn a_ttf_path_resolves() {
        let path = fonts_dir("OldStandard-Italic.ttf");
        let m = MastheadFont::File(path).resolve("Gazette").unwrap();
        assert_eq!(m.name(), "Masthead");
    }

    #[test]
    fn a_bad_file_says_why() {
        let missing = MastheadFont::File("/no/such/font.ttf".into());
        assert!(missing.resolve("Gazette").unwrap_err().contains("font.ttf"));
        let not_ttf = MastheadFont::File(fonts_dir("Inter-OFL.txt"));
        assert!(not_ttf.resolve("Gazette").unwrap_err().contains("TrueType"));
    }

    #[test]
    fn a_missing_letter_falls_back_to_the_default_then_old_standard() {
        // Ж is in Old Standard but not in Pirata One or Manufacturing Consent
        let (m, why) = MastheadFont::Pirata.resolve_or_fall_back("Жизнь");
        assert_eq!(m.name(), "OldStandard-Bold");
        assert!(why.unwrap().contains("Old Standard"));
        let (m, why) =
            MastheadFont::File("/no/such/font.ttf".into()).resolve_or_fall_back("Gazette");
        assert_eq!(m.name(), "ManufacturingConsent-Regular");
        assert!(why.unwrap().contains("Manufacturing Consent"));
        let (m, why) = MastheadFont::Bodoni.resolve_or_fall_back("Gazette");
        assert_eq!(m.name(), "BodoniModa-Bold");
        assert!(why.is_none());
    }

    #[test]
    fn a_font_without_the_letters_is_refused() {
        let err = MastheadFont::Serif.resolve("日本新聞").unwrap_err();
        assert!(err.contains("no letter"), "{err}");
    }
}
