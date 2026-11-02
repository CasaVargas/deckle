// SPDX-License-Identifier: GPL-3.0-or-later
//! Screen sizes per device model, for fitting comic pages (fixed-layout spec §7, §7.1; #281).
//!
//! A Kindle's model is in its serial number, which Deckle reads from USB: the device code at
//! characters 4–6 (3–4 on the oldest). A Kobo's is the product id at the end of `.kobo/version`.
//! The tables follow KOReader's (`frontend/device/{kindle,kobo}/device.lua`); FBInk's `fbink.h`
//! agrees on the Kobo ids. A model the tables don't know falls back to its family, by the largest
//! panel in it: a page fitted to a panel a little larger than the real one costs nothing, since
//! pages are never scaled up and the reader scales the rest of the way.
//!
//! The 2025 Scribe and Scribe Colorsoft are left out until their pixel size is confirmed: they
//! fall back to the Scribe by name, the Colorsoft keeping its colour.

use crate::deliver::Target;
pub use deckle_comic::pages::Screen;

const fn grey(width: u32, height: u32) -> Screen {
    Screen {
        width,
        height,
        color: false,
    }
}

const fn colour(width: u32, height: u32) -> Screen {
    Screen {
        width,
        height,
        color: true,
    }
}

/// The most common current Kindle panel, grey: for a Kindle the tables don't know.
pub const DEFAULT: Screen = grey(1264, 1680);

const SCRIBE: Screen = grey(1860, 2480);

/// Kindle families by a word in their name, most specific first: the fallback when the serial
/// isn't known. A name with "Colorsoft" in it also keeps colour (a Scribe Colorsoft).
pub const KINDLES: [(&str, Screen); 4] = [
    ("scribe", SCRIBE),
    ("colorsoft", colour(1264, 1680)),
    ("paperwhite", DEFAULT),
    ("oasis", DEFAULT),
];

/// Kindles by the device code at characters 4–6 of the serial: `(model, codes, panel)`.
pub const KINDLE_CODES: &[(&str, &[&str], Screen)] = &[
    (
        "Paperwhite 3",
        &[
            "0G1", "0G2", "0G4", "0G5", "0G6", "0G7", "0KB", "0KC", "0KD", "0KE", "0KF", "0KG",
            "0LK", "0LL",
        ],
        grey(1072, 1448),
    ),
    (
        "Oasis",
        &["0GC", "0GD", "0GR", "0GS", "0GT", "0GU"],
        grey(1072, 1448),
    ),
    (
        "Oasis 2",
        &[
            "0LM", "0LN", "0LP", "0LQ", "0P1", "0P2", "0P6", "0P7", "0P8", "0S1", "0S2", "0S3",
            "0S4", "0S7", "0SA",
        ],
        grey(1264, 1680),
    ),
    ("Basic 2", &["0DU", "0K9", "0KA"], grey(600, 800)),
    (
        "Paperwhite 4",
        &[
            "0PP", "0T1", "0T2", "0T3", "0T4", "0T5", "0T6", "0T7", "0TJ", "0TK", "0TL", "0TM",
            "0TN", "102", "103", "16Q", "16R", "16S", "16T", "16U", "16V",
        ],
        grey(1072, 1448),
    ),
    (
        "Basic 3",
        &["10L", "0WF", "0WG", "0WH", "0WJ", "0VB"],
        grey(600, 800),
    ),
    (
        "Oasis 3",
        &["11L", "0WQ", "0WP", "0WN", "0WM", "0WL"],
        grey(1264, 1680),
    ),
    (
        "Paperwhite 5",
        &["1Q0", "1PX", "1VD", "21A", "2BJ", "2DK"],
        grey(1236, 1648),
    ),
    (
        "Paperwhite 5 Signature",
        &["1LG", "219", "2BH"],
        grey(1236, 1648),
    ),
    (
        "Basic 4",
        &["22D", "25T", "23A", "2AQ", "2AP", "1XH", "22C"],
        grey(1072, 1448),
    ),
    (
        "Scribe",
        &["27J", "2BL", "263", "227", "2BM", "23L", "23M", "270"],
        SCRIBE,
    ),
    (
        "Colorsoft",
        &[
            "3H2", "3H4", "3H6", "3H7", "3H9", "3JT", "3J6", "455", "456", "4EP", "34X", "3HB",
        ],
        colour(1264, 1680),
    ),
    (
        "Basic 5",
        &["A89", "3L2", "3L3", "3L4", "3L5", "3L6", "3KM"],
        grey(1072, 1448),
    ),
    (
        "Paperwhite 6",
        &[
            "33W", "33X", "346", "349", "3H3", "3H5", "3H8", "3HA", "3J5", "3JS",
        ],
        grey(1264, 1680),
    ),
    (
        "Scribe 2",
        &["3V0", "3V1", "3X5", "3UV", "3X4", "3X3", "41E", "410"],
        SCRIBE,
    ),
];

/// The oldest Kindles, whose serial starts `B` or `9`, by the code at characters 3–4.
pub const KINDLE_OLD_CODES: &[(&str, &[&str], Screen)] = &[
    (
        "Paperwhite",
        &["24", "1B", "1D", "1F", "1C", "20"],
        grey(758, 1024),
    ),
    (
        "Paperwhite 2",
        &[
            "D4", "5A", "D5", "D6", "D7", "D8", "F2", "17", "60", "F4", "F9", "62", "61", "5F",
        ],
        grey(758, 1024),
    ),
    ("Basic", &["C6", "DD"], grey(600, 800)),
    (
        "Voyage",
        &["13", "54", "2A", "4F", "52", "53"],
        grey(1072, 1448),
    ),
];

/// A Kobo the tables don't know, by its largest current panel (Sage), in colour: a colour Kobo
/// must keep its colour.
pub const KOBO: Screen = colour(1440, 1920);

/// Kobos by the product id ending `.kobo/version`: `(model, ids, panel)`. A colour Kobo shows
/// colour at half resolution; the panel is its grey one, the one to fit pages to.
pub const KOBO_MODELS: &[(&str, &[&str], Screen)] = &[
    ("Touch", &["300", "310", "320"], grey(600, 800)),
    ("Glo", &["330"], grey(758, 1024)),
    ("Mini", &["340"], grey(600, 800)),
    ("Aura HD", &["350"], grey(1080, 1440)),
    ("Aura", &["360"], grey(758, 1024)),
    ("Aura H2O", &["370"], grey(1080, 1430)),
    ("Glo HD", &["371"], grey(1072, 1448)),
    ("Touch 2.0", &["372"], grey(600, 800)),
    ("Aura One", &["373", "381"], grey(1404, 1872)),
    ("Aura H2O Edition 2", &["374", "378"], grey(1080, 1440)),
    ("Aura Edition 2", &["375", "379"], grey(758, 1024)),
    ("Clara HD", &["376"], grey(1072, 1448)),
    ("Forma", &["377", "380"], grey(1440, 1920)),
    ("Nia", &["382"], grey(758, 1024)),
    ("Sage", &["383"], grey(1440, 1920)),
    ("Libra H2O", &["384"], grey(1264, 1680)),
    ("Clara 2E", &["386"], grey(1072, 1448)),
    ("Elipsa", &["387"], grey(1404, 1872)),
    ("Libra 2", &["388"], grey(1264, 1680)),
    ("Elipsa 2E", &["389"], grey(1404, 1872)),
    ("Libra Colour", &["390"], colour(1264, 1680)),
    ("Clara BW", &["391", "395"], grey(1072, 1448)),
    ("Clara Colour", &["393"], colour(1072, 1448)),
];

/// A PocketBook, by its largest panel (InkPad), in colour: its model isn't read.
pub const POCKETBOOK: Screen = colour(1404, 1872);

fn find(table: &[(&str, &[&str], Screen)], code: &str) -> Option<Screen> {
    table
        .iter()
        .find(|(_, codes, _)| codes.contains(&code))
        .map(|&(_, _, s)| s)
}

/// The screen of the Kindle with serial `serial`, when the tables know its model.
pub fn for_kindle_serial(serial: &str) -> Option<Screen> {
    let serial = serial.trim().to_ascii_uppercase();
    if serial.starts_with('B') || serial.starts_with('9') {
        find(KINDLE_OLD_CODES, serial.get(2..4)?)
    } else {
        find(KINDLE_CODES, serial.get(3..6)?)
    }
}

/// The screen of the Kobo whose `.kobo/version` ends in `model_id`, when the table knows it.
pub fn for_kobo_model(model_id: &str) -> Option<Screen> {
    let id = model_id.trim();
    find(KOBO_MODELS, id.get(id.len().checked_sub(3)?..)?)
}

/// The screen to fit pages to on an EPUB reader.
pub fn for_reader(reader: &crate::reader::Reader) -> Screen {
    match reader {
        crate::reader::Reader::Kobo(k) => k
            .model_id
            .as_deref()
            .and_then(for_kobo_model)
            .unwrap_or(KOBO),
        crate::reader::Reader::PocketBook(_) => POCKETBOOK,
    }
}

/// The screen of a Kindle called `name` (a model hint or an MTP product name), by family.
pub fn for_kindle(name: &str) -> Screen {
    let name = name.to_lowercase();
    let mut screen = KINDLES
        .iter()
        .find(|(word, _)| name.contains(word))
        .map_or(DEFAULT, |(_, s)| *s);
    screen.color |= name.contains("colorsoft");
    screen
}

/// The screen to fit pages to on `target`; `None` for a folder, which has no device. The
/// serial decides when the tables know it, else the name.
pub fn for_target(target: &Target) -> Option<Screen> {
    match target {
        Target::Folder(_) => None,
        Target::MassStorage(d) => Some(
            d.serial
                .as_deref()
                .and_then(for_kindle_serial)
                .unwrap_or_else(|| for_kindle(&d.model_hint)),
        ),
        Target::Mtp(m) => {
            let product = m.product.as_deref().unwrap_or("");
            // An MTP product name ends in the serial ("Kindle Scribe G6G3X…").
            let named = product.split_whitespace().last();
            Some(
                m.serial
                    .as_deref()
                    .into_iter()
                    .chain(named)
                    .find_map(for_kindle_serial)
                    .unwrap_or_else(|| for_kindle(product)),
            )
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rows() -> impl Iterator<Item = (&'static str, &'static [&'static str], Screen)> {
        KINDLE_CODES
            .iter()
            .chain(KINDLE_OLD_CODES)
            .chain(KOBO_MODELS)
            .copied()
    }

    #[test]
    fn every_row_is_a_real_portrait_screen() {
        let others = [
            ("default", DEFAULT),
            ("kobo", KOBO),
            ("pocketbook", POCKETBOOK),
        ];
        let named = KINDLES.iter().chain(others.iter()).copied();
        for (model, s) in named.chain(rows().map(|(m, _, s)| (m, s))) {
            assert!(s.width >= 600 && s.height > s.width, "{model}");
        }
    }

    #[test]
    fn every_code_is_the_right_length_and_in_one_row_only() {
        for (table, len) in [(KINDLE_CODES, 3), (KINDLE_OLD_CODES, 2), (KOBO_MODELS, 3)] {
            let mut seen = std::collections::BTreeSet::new();
            for (model, codes, _) in table {
                for code in *codes {
                    assert_eq!(code.len(), len, "{model} {code}");
                    assert_eq!(*code, code.to_ascii_uppercase(), "{model} {code}");
                    assert!(seen.insert(*code), "{code} is in two rows");
                }
            }
        }
    }

    #[test]
    fn each_kindle_model_is_found_by_its_serial() {
        // One code per model, inside a made-up serial of the right shape.
        let cases: [(&str, (u32, u32, bool)); 19] = [
            ("G000G1AAAAAAAAAA", (1072, 1448, false)), // Paperwhite 3
            ("G000GCAAAAAAAAAA", (1072, 1448, false)), // Oasis
            ("G000LMAAAAAAAAAA", (1264, 1680, false)), // Oasis 2
            ("G000DUAAAAAAAAAA", (600, 800, false)),   // Basic 2
            ("G000T1AAAAAAAAAA", (1072, 1448, false)), // Paperwhite 4
            ("G0010LAAAAAAAAAA", (600, 800, false)),   // Basic 3
            ("G0011LAAAAAAAAAA", (1264, 1680, false)), // Oasis 3
            ("G001PXAAAAAAAAAA", (1236, 1648, false)), // Paperwhite 5
            ("G001LGAAAAAAAAAA", (1236, 1648, false)), // Paperwhite 5 Signature
            ("G0022DAAAAAAAAAA", (1072, 1448, false)), // Basic 4
            ("G0027JAAAAAAAAAA", (1860, 2480, false)), // Scribe
            ("G003H2AAAAAAAAAA", (1264, 1680, true)),  // Colorsoft
            ("G00A89AAAAAAAAAA", (1072, 1448, false)), // Basic 5
            ("G0033WAAAAAAAAAA", (1264, 1680, false)), // Paperwhite 6
            ("G6G3X5AAAAAAAAAA", (1860, 2480, false)), // Scribe 2, as the owner's 2024 Scribe
            ("B024AAAAAAAAAAAA", (758, 1024, false)),  // Paperwhite
            ("B0D4AAAAAAAAAAAA", (758, 1024, false)),  // Paperwhite 2
            ("90C6AAAAAAAAAAAA", (600, 800, false)),   // Basic
            ("B013AAAAAAAAAAAA", (1072, 1448, false)), // Voyage
        ];
        for (serial, (width, height, color)) in cases {
            let s = for_kindle_serial(serial).unwrap_or_else(|| panic!("{serial}"));
            assert_eq!(
                (s.width, s.height, s.color),
                (width, height, color),
                "{serial}"
            );
        }
        // Lower case and stray spaces read the same.
        assert_eq!(
            for_kindle_serial(" g6g3x5aaaa "),
            for_kindle_serial("G6G3X5AAAA")
        );
    }

    #[test]
    fn an_unknown_or_short_kindle_serial_is_none() {
        // The Scribe 2025 and Scribe Colorsoft wait for a confirmed pixel size.
        for serial in [
            "G0B0AAAA11112222",
            "G004PGAAAAAAAAAA",
            "G004VXAAAAAAAAAA",
            "",
            "G0",
            "B0",
        ] {
            assert_eq!(for_kindle_serial(serial), None, "{serial:?}");
        }
    }

    #[test]
    fn kindles_without_a_known_serial_are_found_by_family() {
        assert_eq!(for_kindle("Kindle Scribe").width, 1860);
        assert!(for_kindle("KINDLE COLORSOFT").color);
        assert_eq!(for_kindle("Kindle Paperwhite"), DEFAULT);
        // A Scribe Colorsoft is a Scribe that keeps its colour.
        let scs = for_kindle("Kindle Scribe Colorsoft");
        assert_eq!((scs.width, scs.height, scs.color), (1860, 2480, true));
        assert!(!for_kindle("Kindle Scribe").color);
        // What a mass-storage Kindle's version.txt says: no model at all.
        assert_eq!(for_kindle("Kindle 5.16.2.1.1 (4184940002)"), DEFAULT);
        assert_eq!(for_kindle(""), DEFAULT);
        assert_eq!(for_target(&Target::Folder("/tmp".into())), None);
    }

    fn mass_storage(serial: Option<&str>, model_hint: &str) -> Target {
        Target::MassStorage(crate::deliver::Device {
            root: "/Volumes/Kindle".into(),
            model_hint: model_hint.into(),
            thumbnails_writable: false,
            firmware: None,
            serial: serial.map(str::to_owned),
        })
    }

    fn mtp(serial: Option<&str>, product: &str) -> Target {
        Target::Mtp(deckle_transport::usb::MtpInfo {
            vendor_id: 0x1949,
            product_id: 0x9981,
            serial: serial.map(str::to_owned),
            product: Some(product.into()),
            bus: 1,
            address: 2,
        })
    }

    #[test]
    fn a_target_is_fitted_by_its_serial_before_its_name() {
        // A mass-storage Paperwhite 5 says only "Kindle" and its firmware.
        let pw5 = for_target(&mass_storage(Some("G001PX0000000000"), "Kindle 5.16.2.1.1")).unwrap();
        assert_eq!((pw5.width, pw5.height), (1236, 1648));
        // An unknown serial, or none, falls back to the name.
        assert_eq!(
            for_target(&mass_storage(Some("G0B0AAAA11112222"), "Kindle 5.16.2.1.1")),
            Some(DEFAULT)
        );
        assert_eq!(
            for_target(&mass_storage(None, "Kindle Paperwhite")),
            Some(DEFAULT)
        );

        // Over MTP the serial is the USB one, else the end of the product name.
        let colorsoft = for_target(&mtp(Some("G003H20000000000"), "Kindle")).unwrap();
        assert!(colorsoft.color);
        let scribe = for_target(&mtp(None, "Kindle Scribe G6G3X50000000000")).unwrap();
        assert_eq!(scribe.width, 1860);
        // A 2025 Scribe Colorsoft isn't in the table yet: the Scribe, in colour.
        let scs = for_target(&mtp(Some("G004VX0000000000"), "Kindle Scribe Colorsoft")).unwrap();
        assert_eq!((scs.width, scs.color), (1860, true));
    }

    #[test]
    fn a_kobo_is_found_by_its_product_id_else_the_family() {
        let id = |n: &str| format!("00000000-0000-0000-0000-000000000{n}");
        let libra2 = for_kobo_model(&id("388")).unwrap();
        assert_eq!(
            (libra2.width, libra2.height, libra2.color),
            (1264, 1680, false)
        );
        assert!(for_kobo_model(&id("390")).unwrap().color, "Libra Colour");
        assert!(for_kobo_model(&id("393")).unwrap().color, "Clara Colour");
        assert!(!for_kobo_model(&id("391")).unwrap().color, "Clara BW");
        assert_eq!(
            for_kobo_model("383").unwrap().width,
            1440,
            "a bare id reads too"
        );
        assert_eq!(for_kobo_model(&id("999")), None);
        assert_eq!(for_kobo_model(""), None);
        assert_eq!(for_kobo_model("39"), None);

        let kobo = |model_id: Option<String>| {
            crate::reader::Reader::Kobo(crate::kobo::Kobo {
                root: "/Volumes/KOBOeReader".into(),
                serial: None,
                firmware: None,
                model_id,
            })
        };
        assert_eq!(for_reader(&kobo(Some(id("388")))), libra2);
        assert_eq!(for_reader(&kobo(Some(id("999")))), KOBO);
        assert_eq!(for_reader(&kobo(None)), KOBO);
    }
}
