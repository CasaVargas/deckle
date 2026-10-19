// SPDX-License-Identifier: GPL-3.0-or-later
//! Constant tables copied from kfxlib (`yj_structure.py`, `resources.py`, `yj_versions.py`).

/// Struct keys whose symbol values are element ids and must become integers.
pub const EID_REFERENCES: [&str; 6] = ["$185", "$155", "$598", "$754", "$474", "$163"];

/// Image/resource format name → KFX symbol.
pub const FORMAT_SYMBOLS: [(&str, &str); 10] = [
    ("bmp", "$599"),
    ("gif", "$286"),
    ("jpg", "$285"),
    ("jxr", "$548"),
    ("pbm", "$420"),
    ("pdf", "$565"),
    ("png", "$284"),
    ("pobject", "$287"),
    ("tiff", "$600"),
    ("bpg", "$612"),
];

pub fn format_symbol(ext: &str) -> Option<&'static str> {
    FORMAT_SYMBOLS
        .iter()
        .find(|(e, _)| *e == ext)
        .map(|(_, s)| *s)
}

pub fn symbol_format(sym: &str) -> Option<&'static str> {
    FORMAT_SYMBOLS
        .iter()
        .find(|(_, s)| *s == sym)
        .map(|(e, _)| *e)
}

/// Metadata key → `$258` symbol.
pub const METADATA_SYMBOLS: [(&str, &str); 13] = [
    ("ASIN", "$224"),
    ("asset_id", "$466"),
    ("author", "$222"),
    ("cde_content_type", "$251"),
    ("cover_image", "$424"),
    ("description", "$154"),
    ("language", "$10"),
    ("orientation", "$215"),
    ("publisher", "$232"),
    ("reading_orders", "$169"),
    ("support_landscape", "$218"),
    ("support_portrait", "$217"),
    ("title", "$153"),
];

pub fn metadata_symbol(name: &str) -> Option<&'static str> {
    METADATA_SYMBOLS
        .iter()
        .find(|(n, _)| *n == name)
        .map(|(_, s)| *s)
}

/// (file_creator, creator_version) pairs that mean "unknown tool"; replaced from the KCB.
pub const GENERIC_CREATOR_VERSIONS: [(&str, &str); 3] = [
    ("YJConversionTools", "2.15.0"),
    ("KTC", "1.0.11.1"),
    ("", ""),
];

/// Maximum UTF-8 bytes per `$145` content fragment.
pub const MAX_CONTENT_FRAGMENT_SIZE: usize = 8192;
pub const KFX_COVER_RESOURCE: &str = "kfx_cover_image";
pub const APPROXIMATE_PAGE_LIST: &str = "APPROXIMATE_PAGE_LIST";
