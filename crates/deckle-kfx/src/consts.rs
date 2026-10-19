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

/// Per fragment type, the struct keys at top level that hold the fragment's own id.
pub const FRAGMENT_ID_KEYS: [(&str, &[&str]); 18] = [
    ("$266", &["$180"]),
    ("$597", &["$174", "$598"]),
    ("$418", &["$165"]),
    ("$417", &["$165"]),
    ("$394", &["$240"]),
    ("$145", &["name"]),
    ("$164", &["$175"]),
    ("$391", &["$239"]),
    ("$692", &["name"]),
    ("$387", &["$174"]),
    ("$756", &["$757"]),
    ("$260", &["$174"]),
    ("$267", &["$174"]),
    ("$609", &["$174"]),
    ("$259", &["$176"]),
    ("$608", &["$598"]),
    ("$157", &["$173"]),
    ("$610", &["$602"]),
];

/// Struct key -> type of the fragment a symbol value references.
pub const COMMON_FRAGMENT_REFERENCES: [(&str, &str); 24] = [
    ("$749", "$259"),
    ("$266", "$266"),
    ("$597", "$597"),
    ("$429", "$157"),
    ("$479", "$164"),
    ("$145", "$145"),
    ("$146", "$608"),
    ("$245", "$164"),
    ("$179", "$266"),
    ("$165", "$417"),
    ("$392", "$391"),
    ("name", "$145"),
    ("$167", "$164"),
    ("$175", "$164"),
    ("$757", "$756"),
    ("$174", "$260"),
    ("$170", "$260"),
    ("$176", "$259"),
    ("$157", "$157"),
    ("$173", "$157"),
    ("$528", "$164"),
    ("$214", "$164"),
    ("$636", "$417"),
    ("$635", "$164"),
];

/// (parent key, key) -> referenced type.
pub const NESTED_FRAGMENT_REFERENCES: [((&str, &str), &str); 5] = [
    (("$597", "$351"), "$597"),
    (("$597", "$538"), "$597"),
    (("$597", "$613"), "$597"),
    (("$597", "$614"), "$597"),
    (("$597", "$697"), "$597"),
];

/// (fragment type, key) -> referenced type.
pub const SPECIAL_FRAGMENT_REFERENCES: [((&str, &str), &str); 4] = [
    (("$391", "$247"), "$394"),
    (("$387", "$213"), "$164"),
    (("$387", "$214"), "$164"),
    (("$387", "$212"), "$164"),
];

/// (fragment type, parent key) whose symbol values are not references.
pub const SPECIAL_PARENT_NON_REFERENCES: [(&str, &str); 1] = [("$538", "yj.print.style")];
