// SPDX-License-Identifier: GPL-3.0-or-later
//! Screen sizes per device family, for fitting comic pages (fixed-layout spec §7, §7.1).
//!
//! A Kindle says little about its model: over mass storage only "Kindle" and its firmware, over
//! MTP a product name ("Kindle Scribe"). So the table goes by family and holds the largest panel
//! in each. A page fitted to a panel a little larger than the real one costs nothing: pages are
//! never scaled up, and the Kindle scales the rest of the way.

use crate::deliver::Target;
pub use deckle_comic::pages::Screen;

/// The most common current Kindle panel, grey: for a Kindle the table doesn't know.
pub const DEFAULT: Screen = Screen {
    width: 1264,
    height: 1680,
    color: false,
};

/// Kindle families by a word in their name, most specific first.
pub const KINDLES: [(&str, Screen); 4] = [
    (
        "scribe",
        Screen {
            width: 1860,
            height: 2480,
            color: false,
        },
    ),
    (
        "colorsoft",
        Screen {
            width: 1264,
            height: 1680,
            color: true,
        },
    ),
    ("paperwhite", DEFAULT),
    ("oasis", DEFAULT),
];

/// The screen of a Kindle called `name` (a model hint or an MTP product name).
pub fn for_kindle(name: &str) -> Screen {
    let name = name.to_lowercase();
    KINDLES
        .iter()
        .find(|(word, _)| name.contains(word))
        .map_or(DEFAULT, |(_, s)| *s)
}

/// The screen to fit pages to on `target`; `None` for a folder, which has no device.
pub fn for_target(target: &Target) -> Option<Screen> {
    match target {
        Target::Folder(_) => None,
        Target::MassStorage(d) => Some(for_kindle(&d.model_hint)),
        Target::Mtp(m) => Some(for_kindle(m.product.as_deref().unwrap_or(""))),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_row_is_a_real_portrait_screen() {
        for (word, s) in KINDLES.iter().chain([("default", DEFAULT)].iter()) {
            assert!(s.width > 0 && s.height > s.width, "{word}");
        }
    }

    #[test]
    fn kindles_are_found_by_family_and_anything_else_is_the_default() {
        assert_eq!(for_kindle("Kindle Scribe").width, 1860);
        assert!(for_kindle("KINDLE COLORSOFT").color);
        assert_eq!(for_kindle("Kindle Paperwhite"), DEFAULT);
        // What a mass-storage Kindle's version.txt says: no model at all.
        assert_eq!(for_kindle("Kindle 5.16.2.1.1 (4184940002)"), DEFAULT);
        assert_eq!(for_kindle(""), DEFAULT);
        assert_eq!(for_target(&Target::Folder("/tmp".into())), None);
    }
}
