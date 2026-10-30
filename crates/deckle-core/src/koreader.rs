// SPDX-License-Identifier: GPL-3.0-or-later
//! KOReader's sidecars (docs/superpowers/specs/2026-10-30-koreader-reads-design.md): each
//! book's `<book>.sdr/metadata.<ext>.lua`, read (never written) for its highlights, notes and
//! progress, in the shapes a Kobo read gives: `My Clippings.txt` entries and [`kobo::Book`]s.

use std::path::{Path, PathBuf};
use std::time::SystemTime;

use crate::kobo::{self, Book};

/// Folders deep the walk goes below the root.
const MAX_DEPTH: usize = 8;
/// Entries the walk looks at before it stops.
const MAX_ENTRIES: usize = 20_000;
/// Folders never walked into.
const SKIP: &[&str] = &[
    ".kobo",
    ".Trashes",
    ".Spotlight-V100",
    ".fseventsd",
    "System Volume Information",
];
/// The device's own mount point at the front of a `docsettings/` path, per kind of reader.
const DEVICE_PREFIXES: &[&str] = &[
    "mnt/onboard/",
    "mnt/us/",
    "mnt/ext1/",
    "sdcard/",
    "storage/emulated/0/",
];

/// One sidecar found on a drive or in a folder.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Sidecar {
    /// The `metadata.<ext>.lua` file.
    pub file: PathBuf,
    /// The book's path relative to the root, `/`-separated, or `koreader:<folder>` when the
    /// sidecar is kept by hash and doesn't say.
    pub book: String,
}

/// What a KOReader read found, as a Kobo read gives it.
#[derive(Clone, Debug, Default)]
pub struct Read {
    /// Books in progress, most recently read first.
    pub books: Vec<Book>,
    /// Highlights and notes as `My Clippings.txt` entries, oldest first; `None` when there are
    /// none.
    pub clippings: Option<Vec<u8>>,
    /// `<file>: <why>` for sidecars that couldn't be read.
    pub warnings: Vec<String>,
    /// Where KOReader is in each book beside its sidecar, for readership (KOReader hub design
    /// §C). A sidecar kept by hash doesn't say which book it is: left out.
    pub positions: Vec<Position>,
}

/// Where KOReader is in one book.
#[derive(Clone, Debug, PartialEq)]
pub struct Position {
    /// The book's path relative to the root, as [`Sidecar::book`].
    pub book: String,
    /// `last_xpointer`: `/body/DocFragment[N]/…` in an EPUB.
    pub xpointer: Option<String>,
    /// `percent_finished`, 0–1.
    pub percent: Option<f64>,
    /// When the sidecar was last written, ISO 8601 UTC: when KOReader was last there.
    pub when: String,
}

/// Every KOReader sidecar under `root` (spec §3).
pub fn find(root: &Path) -> Vec<Sidecar> {
    let mut out = Vec::new();
    let mut seen = 0usize;
    let mut stack = vec![(root.to_path_buf(), 0usize)];
    while let Some((dir, depth)) = stack.pop() {
        let Ok(rd) = std::fs::read_dir(&dir) else {
            continue;
        };
        let mut entries: Vec<_> = rd.flatten().collect();
        entries.sort_by_key(|e| e.file_name());
        for entry in entries {
            seen += 1;
            if seen > MAX_ENTRIES {
                return out;
            }
            let name = entry.file_name().to_string_lossy().into_owned();
            let path = entry.path();
            if !entry.file_type().is_ok_and(|t| t.is_dir()) || SKIP.contains(&name.as_str()) {
                continue;
            }
            if let Some(stem) = name.strip_suffix(".sdr") {
                if let Some((file, ext)) = metadata_file(&path) {
                    let book = book_path(root, &dir, stem, &ext);
                    out.push(Sidecar { file, book });
                }
                continue;
            }
            if depth < MAX_DEPTH {
                stack.push((path, depth + 1));
            }
        }
    }
    out.sort_by(|a, b| a.file.cmp(&b.file));
    out
}

/// `metadata.<ext>.lua` in an `.sdr` folder, and its `<ext>`.
fn metadata_file(sdr: &Path) -> Option<(PathBuf, String)> {
    let mut found: Vec<(PathBuf, String)> = std::fs::read_dir(sdr)
        .ok()?
        .flatten()
        .filter_map(|e| {
            let n = e.file_name().to_string_lossy().into_owned();
            let ext = n
                .strip_prefix("metadata.")?
                .strip_suffix(".lua")?
                .to_owned();
            (!ext.is_empty()).then(|| (e.path(), ext))
        })
        .collect();
    found.sort();
    found.into_iter().next()
}

/// The book a sidecar in `parent/<stem>.sdr` belongs to, relative to `root` (spec §3).
fn book_path(root: &Path, parent: &Path, stem: &str, ext: &str) -> String {
    let rel: Vec<String> = parent
        .strip_prefix(root)
        .unwrap_or(parent)
        .components()
        .map(|c| c.as_os_str().to_string_lossy().into_owned())
        .collect();
    if rel.iter().any(|c| c == "hashdocsettings") {
        return format!("koreader:{stem}");
    }
    let mut rel = rel.join("/");
    // `…/docsettings/<the book's folder on the device>`
    if let Some(i) = rel.find("docsettings/") {
        rel = rel[i + "docsettings/".len()..].to_owned();
        if let Some(p) = DEVICE_PREFIXES.iter().find(|p| rel.starts_with(**p)) {
            rel = rel[p.len()..].to_owned();
        }
    }
    let file = format!("{stem}.{ext}");
    if rel.is_empty() {
        file
    } else {
        format!("{rel}/{file}")
    }
}

/// KOReader's name for a book: the MD5 of 1 KiB samples at offsets `1024 << 2i` for i = -1 to
/// 10, as `util.partialMD5` (`frontend/util.lua`) takes them. LuaJIT masks the shift count to 5
/// bits, so i = -1 is offset 0. A sample past the end stops it; a short last one still counts.
/// Lowercase hex; `None` when the file can't be read.
pub fn partial_md5(path: &Path) -> Option<String> {
    use md5::{Digest, Md5};
    use std::io::{Read as _, Seek, SeekFrom};
    let mut f = std::fs::File::open(path).ok()?;
    let len = f.metadata().ok()?.len();
    let mut md5 = Md5::new();
    let mut buf = [0u8; 1024];
    for i in -1i32..=10 {
        let offset: u64 = if i < 0 { 0 } else { 1024 << (2 * i) };
        if offset >= len {
            break;
        }
        f.seek(SeekFrom::Start(offset)).ok()?;
        let mut n = 0;
        while n < buf.len() {
            match f.read(&mut buf[n..]).ok()? {
                0 => break,
                k => n += k,
            }
        }
        md5.update(&buf[..n]);
    }
    Some(md5.finalize().iter().map(|b| format!("{b:02x}")).collect())
}

/// Reads every sidecar under `root`.
pub fn read(root: &Path) -> Read {
    let mut out = Read::default();
    let mut notes: Vec<(String, String)> = Vec::new(); // (datetime, entry), for oldest first
    let mut books: Vec<(SystemTime, Book)> = Vec::new();
    for s in find(root) {
        let text = match std::fs::read(&s.file) {
            Ok(b) => String::from_utf8_lossy(&b).into_owned(),
            Err(e) => {
                out.warnings.push(format!("{}: {e}", s.file.display()));
                continue;
            }
        };
        let parsed = match parse(&text) {
            Ok(p) => p,
            Err(e) => {
                out.warnings
                    .push(format!("{}: not read: {e}", s.file.display()));
                continue;
            }
        };
        let fallback = s
            .book
            .rsplit('/')
            .next()
            .and_then(|f| f.rsplit_once('.').map(|(a, _)| a))
            .unwrap_or(&s.book)
            .trim_start_matches("koreader:")
            .to_owned();
        let title = parsed.title.clone().unwrap_or(fallback);
        let author = parsed.authors.join(", ");
        for h in &parsed.highlights {
            let page = h.page.as_deref();
            let mut entry =
                kobo::clipping(&title, &author, "Highlight", page, &h.text, &h.datetime);
            entry.push_str(&kobo::clipping(
                &title,
                &author,
                "Note",
                page,
                &h.note,
                &h.datetime,
            ));
            if !entry.is_empty() {
                notes.push((h.datetime.clone(), entry));
            }
        }
        let modified = std::fs::metadata(&s.file)
            .and_then(|m| m.modified())
            .unwrap_or(SystemTime::UNIX_EPOCH);
        if !s.book.starts_with("koreader:")
            && (parsed.xpointer.is_some() || parsed.percent.is_some())
        {
            out.positions.push(Position {
                book: s.book.clone(),
                xpointer: parsed.xpointer.clone(),
                percent: parsed.percent,
                when: crate::devices::iso(modified),
            });
        }
        if parsed.in_progress() {
            books.push((
                modified,
                Book {
                    content_id: s.book.clone(),
                    title,
                    authors: parsed.authors.clone(),
                    percent: (parsed.percent.unwrap_or(0.0) * 100.0).clamp(0.0, 100.0),
                    last_read: Some(crate::devices::iso(modified)),
                },
            ));
        }
    }
    notes.sort_by(|a, b| a.0.cmp(&b.0));
    let clippings: String = notes.into_iter().map(|(_, e)| e).collect();
    out.clippings = (!clippings.is_empty()).then(|| clippings.into_bytes());
    books.sort_by_key(|b| std::cmp::Reverse(b.0));
    out.books = books.into_iter().map(|(_, b)| b).collect();
    out
}

/// A Kobo's read and a KOReader read of the same drive as one: clippings joined, and a book
/// both know kept once, from whichever read it last.
pub fn merge(kobo: kobo::Read, ko: Read) -> kobo::Read {
    let clippings = match (kobo.clippings, ko.clippings) {
        (Some(mut a), Some(b)) => {
            a.extend(b);
            Some(a)
        }
        (a, b) => a.or(b),
    };
    let mut books = kobo.books;
    for b in ko.books {
        let key = |id: &str| {
            id.strip_prefix("file:///mnt/onboard/")
                .unwrap_or(id)
                .to_owned()
        };
        match books
            .iter_mut()
            .find(|k| key(&k.content_id) == key(&b.content_id))
        {
            Some(k) if b.last_read > k.last_read => *k = b,
            Some(_) => {}
            None => books.push(b),
        }
    }
    books.sort_by(|a, b| b.last_read.cmp(&a.last_read));
    kobo::Read { books, clippings }
}

/// What one sidecar says.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Parsed {
    pub title: Option<String>,
    pub authors: Vec<String>,
    /// 0–1.
    pub percent: Option<f64>,
    /// `summary.status`: `reading`, `complete`, `abandoned`.
    pub status: Option<String>,
    /// `last_xpointer`: where the reader is, `/body/DocFragment[N]/…` in an EPUB.
    pub xpointer: Option<String>,
    pub highlights: Vec<Highlight>,
}

impl Parsed {
    /// Being read: not finished or set aside, and somewhere past the start (spec §4).
    pub fn in_progress(&self) -> bool {
        let p = self.percent.unwrap_or(0.0);
        matches!(self.status.as_deref(), None | Some("reading")) && p > 0.0 && p < 1.0
    }
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Highlight {
    pub text: String,
    /// The note on it; empty for none.
    pub note: String,
    pub page: Option<String>,
    /// `YYYY-MM-DD HH:MM:SS`.
    pub datetime: String,
}

/// Reads a sidecar's text (spec §4).
pub fn parse(text: &str) -> Result<Parsed, String> {
    use lua::Value;
    let root = lua::parse(text)?;
    let s = |v: Option<&Value>| v.and_then(Value::as_str).map(str::to_owned);
    let props = root.get("doc_props");
    let mut out = Parsed {
        title: s(props.and_then(|p| p.get("title"))).filter(|t| !t.trim().is_empty()),
        authors: s(props.and_then(|p| p.get("authors")))
            .map(|a| {
                a.split('\n')
                    .map(str::trim)
                    .filter(|a| !a.is_empty())
                    .map(str::to_owned)
                    .collect()
            })
            .unwrap_or_default(),
        percent: root.get("percent_finished").and_then(Value::as_f64),
        status: s(root.get("summary").and_then(|v| v.get("status"))),
        xpointer: s(root.get("last_xpointer")).filter(|x| !x.is_empty()),
        highlights: Vec::new(),
    };
    let page_of = |v: Option<&Value>| -> Option<String> {
        match v? {
            Value::Num(n) if n.fract() == 0.0 && *n > 0.0 => Some(format!("{}", *n as i64)),
            _ => None,
        }
    };
    if let Some(Value::Table(items)) = root.get("annotations") {
        for (_, a) in items {
            if a.get("pos0").is_none() && a.get("drawer").is_none() {
                continue; // a bookmark
            }
            out.highlights.push(Highlight {
                text: s(a.get("text")).unwrap_or_default(),
                note: s(a.get("note")).unwrap_or_default(),
                page: page_of(a.get("pageno")).or_else(|| page_of(a.get("page"))),
                datetime: s(a.get("datetime")).unwrap_or_default(),
            });
        }
    } else if let Some(Value::Table(pages)) = root.get("highlight") {
        for (key, list) in pages {
            let Value::Table(list) = list else {
                continue;
            };
            let page = match key {
                lua::Key::Num(n) if *n > 0.0 => Some(format!("{}", *n as i64)),
                _ => None,
            };
            for (_, h) in list {
                out.highlights.push(Highlight {
                    text: s(h.get("text")).unwrap_or_default(),
                    note: String::new(),
                    page: page.clone(),
                    datetime: s(h.get("datetime")).unwrap_or_default(),
                });
            }
        }
    }
    out.highlights.sort_by(|a, b| a.datetime.cmp(&b.datetime));
    Ok(out)
}

/// A reader for the Lua table literals KOReader writes (`return { … }`): nothing is evaluated.
pub mod lua {
    #[derive(Clone, Debug, PartialEq)]
    pub enum Key {
        Str(String),
        Num(f64),
    }

    #[derive(Clone, Debug, PartialEq)]
    pub enum Value {
        Nil,
        Bool(bool),
        Num(f64),
        Str(String),
        /// Entries in the order written.
        Table(Vec<(Key, Value)>),
    }

    impl Value {
        /// The entry `key` of a table.
        pub fn get(&self, key: &str) -> Option<&Value> {
            match self {
                Value::Table(t) => t.iter().find_map(|(k, v)| match k {
                    Key::Str(s) if s == key => Some(v),
                    _ => None,
                }),
                _ => None,
            }
        }

        pub fn as_str(&self) -> Option<&str> {
            match self {
                Value::Str(s) => Some(s),
                _ => None,
            }
        }

        pub fn as_f64(&self) -> Option<f64> {
            match self {
                Value::Num(n) => Some(*n),
                _ => None,
            }
        }
    }

    /// The value after `return`, past any comments before it.
    pub fn parse(text: &str) -> Result<Value, String> {
        let mut p = Parser {
            s: text.as_bytes(),
            i: 0,
        };
        p.space();
        if p.s[p.i..].starts_with(b"return") {
            p.i += 6;
        }
        let v = p.value(0)?;
        p.space();
        if p.i < p.s.len() {
            return Err(format!("unexpected text at byte {}", p.i));
        }
        Ok(v)
    }

    struct Parser<'a> {
        s: &'a [u8],
        i: usize,
    }

    impl Parser<'_> {
        fn err<T>(&self, what: &str) -> Result<T, String> {
            Err(format!("{what} at byte {}", self.i))
        }

        fn peek(&self) -> Option<u8> {
            self.s.get(self.i).copied()
        }

        /// Whitespace and `--` comments, short or `--[[ long ]]`.
        fn space(&mut self) {
            loop {
                while self.peek().is_some_and(|c| c.is_ascii_whitespace()) {
                    self.i += 1;
                }
                if self.s[self.i..].starts_with(b"--[[") {
                    match find(&self.s[self.i..], b"]]") {
                        Some(e) => self.i += e + 2,
                        None => self.i = self.s.len(),
                    }
                } else if self.s[self.i..].starts_with(b"--") {
                    while self.peek().is_some_and(|c| c != b'\n') {
                        self.i += 1;
                    }
                } else {
                    return;
                }
            }
        }

        fn value(&mut self, depth: usize) -> Result<Value, String> {
            if depth > 64 {
                return self.err("tables nested too deep");
            }
            self.space();
            match self.peek() {
                Some(b'{') => self.table(depth),
                Some(b'"' | b'\'') => self.string().map(Value::Str),
                Some(b'[') if matches!(self.s.get(self.i + 1), Some(b'[' | b'=')) => {
                    self.long_string().map(Value::Str)
                }
                Some(c) if c == b'-' || c == b'.' || c.is_ascii_digit() => {
                    self.number().map(Value::Num)
                }
                Some(_) => {
                    let w = self.word();
                    match w.as_str() {
                        "true" => Ok(Value::Bool(true)),
                        "false" => Ok(Value::Bool(false)),
                        "nil" => Ok(Value::Nil),
                        _ => self.err("expected a value"),
                    }
                }
                None => self.err("unexpected end"),
            }
        }

        fn word(&mut self) -> String {
            let start = self.i;
            while self
                .peek()
                .is_some_and(|c| c.is_ascii_alphanumeric() || c == b'_')
            {
                self.i += 1;
            }
            String::from_utf8_lossy(&self.s[start..self.i]).into_owned()
        }

        fn table(&mut self, depth: usize) -> Result<Value, String> {
            self.i += 1; // {
            let mut out = Vec::new();
            let mut next = 1.0;
            loop {
                self.space();
                match self.peek() {
                    Some(b'}') => {
                        self.i += 1;
                        return Ok(Value::Table(out));
                    }
                    None => return self.err("unclosed table"),
                    _ => {}
                }
                let key = if self.peek() == Some(b'[')
                    && !matches!(self.s.get(self.i + 1), Some(b'[' | b'='))
                {
                    self.i += 1;
                    let k = match self.value(depth + 1)? {
                        Value::Str(s) => Key::Str(s),
                        Value::Num(n) => Key::Num(n),
                        _ => return self.err("unsupported key"),
                    };
                    self.space();
                    if self.peek() != Some(b']') {
                        return self.err("expected ]");
                    }
                    self.i += 1;
                    self.expect_eq()?;
                    Some(k)
                } else if self
                    .peek()
                    .is_some_and(|c| c.is_ascii_alphabetic() || c == b'_')
                {
                    let save = self.i;
                    let w = self.word();
                    self.space();
                    if self.peek() == Some(b'=') && self.s.get(self.i + 1) != Some(&b'=') {
                        self.i += 1;
                        Some(Key::Str(w))
                    } else {
                        self.i = save;
                        None
                    }
                } else {
                    None
                };
                let v = self.value(depth + 1)?;
                let k = key.unwrap_or_else(|| {
                    let k = Key::Num(next);
                    next += 1.0;
                    k
                });
                out.push((k, v));
                self.space();
                match self.peek() {
                    Some(b',' | b';') => self.i += 1,
                    Some(b'}') => {}
                    _ => return self.err("expected , or }"),
                }
            }
        }

        fn expect_eq(&mut self) -> Result<(), String> {
            self.space();
            if self.peek() != Some(b'=') {
                return self.err("expected =");
            }
            self.i += 1;
            Ok(())
        }

        fn number(&mut self) -> Result<f64, String> {
            let start = self.i;
            if self.peek() == Some(b'-') {
                self.i += 1;
            }
            if self.s[self.i..].starts_with(b"0x") || self.s[self.i..].starts_with(b"0X") {
                self.i += 2;
                let h = self.i;
                while self.peek().is_some_and(|c| c.is_ascii_hexdigit()) {
                    self.i += 1;
                }
                let v =
                    i64::from_str_radix(std::str::from_utf8(&self.s[h..self.i]).unwrap_or(""), 16)
                        .map_err(|e| e.to_string())? as f64;
                return Ok(if self.s[start] == b'-' { -v } else { v });
            }
            while self.peek().is_some_and(|c| {
                c.is_ascii_digit() || matches!(c, b'.' | b'e' | b'E' | b'+' | b'-')
            }) {
                // a sign only right after an exponent
                if matches!(self.peek(), Some(b'+' | b'-'))
                    && !matches!(self.s.get(self.i - 1), Some(b'e' | b'E'))
                {
                    break;
                }
                self.i += 1;
            }
            let t = std::str::from_utf8(&self.s[start..self.i]).unwrap_or("");
            t.parse::<f64>()
                .or_else(|e| self.err(&format!("bad number {t:?} ({e})")))
        }

        /// `"…"` or `'…'` with Lua's escapes, including `%q`'s backslash-newline.
        fn string(&mut self) -> Result<String, String> {
            let quote = self.s[self.i];
            self.i += 1;
            let mut out: Vec<u8> = Vec::new();
            loop {
                let Some(c) = self.peek() else {
                    return self.err("unclosed string");
                };
                self.i += 1;
                if c == quote {
                    break;
                }
                if c != b'\\' {
                    out.push(c);
                    continue;
                }
                let Some(e) = self.peek() else {
                    return self.err("unclosed string");
                };
                self.i += 1;
                match e {
                    b'n' | b'\n' => out.push(b'\n'),
                    b't' => out.push(b'\t'),
                    b'r' => out.push(b'\r'),
                    b'a' => out.push(7),
                    b'b' => out.push(8),
                    b'f' => out.push(12),
                    b'v' => out.push(11),
                    b'\r' => {
                        out.push(b'\n');
                        if self.peek() == Some(b'\n') {
                            self.i += 1;
                        }
                    }
                    b'x' => {
                        let h = std::str::from_utf8(self.s.get(self.i..self.i + 2).unwrap_or(b""))
                            .unwrap_or("");
                        let v =
                            u8::from_str_radix(h, 16).or_else(|_| self.err("bad \\x escape"))?;
                        out.push(v);
                        self.i += 2;
                    }
                    b'z' => {
                        while self.peek().is_some_and(|c| c.is_ascii_whitespace()) {
                            self.i += 1;
                        }
                    }
                    b'0'..=b'9' => {
                        let mut v: u32 = (e - b'0') as u32;
                        for _ in 0..2 {
                            match self.peek() {
                                Some(d @ b'0'..=b'9') => {
                                    v = v * 10 + (d - b'0') as u32;
                                    self.i += 1;
                                }
                                _ => break,
                            }
                        }
                        out.push(u8::try_from(v).or_else(|_| self.err("bad \\ddd escape"))?);
                    }
                    other => out.push(other), // \\ \" \' and anything else as itself
                }
            }
            Ok(String::from_utf8_lossy(&out).into_owned())
        }

        /// `[[…]]` or `[==[…]==]`.
        fn long_string(&mut self) -> Result<String, String> {
            self.i += 1;
            let mut level = 0;
            while self.peek() == Some(b'=') {
                level += 1;
                self.i += 1;
            }
            if self.peek() != Some(b'[') {
                return self.err("bad long string");
            }
            self.i += 1;
            if self.peek() == Some(b'\n') {
                self.i += 1;
            }
            let close: Vec<u8> = [b"]".as_slice(), &vec![b'='; level], b"]"].concat();
            let Some(e) = find(&self.s[self.i..], &close) else {
                return self.err("unclosed long string");
            };
            let out = String::from_utf8_lossy(&self.s[self.i..self.i + e]).into_owned();
            self.i += e + close.len();
            Ok(out)
        }
    }

    fn find(hay: &[u8], needle: &[u8]) -> Option<usize> {
        hay.windows(needle.len()).position(|w| w == needle)
    }
}

#[cfg(test)]
mod tests {
    use super::lua::{self, Key, Value};
    use super::*;

    #[test]
    fn lua_reads_koreaders_dump() {
        let v = lua::parse(
            "-- we can read Lua syntax here!\nreturn {\n    [\"a\"] = \"say \\\"hi\\\"\\\nback\\\\slash \\065\",\n    [\"n\"] = -1.5e2,\n    [\"t\"] = {\n        [1] = true,\n        [2] = nil,\n    },\n    plain = 'x',\n    [3] = 0x1F,\n}\n",
        )
        .unwrap();
        assert_eq!(
            v.get("a").and_then(Value::as_str),
            Some("say \"hi\"\nback\\slash A")
        );
        assert_eq!(v.get("n").and_then(Value::as_f64), Some(-150.0));
        assert_eq!(
            v.get("t"),
            Some(&Value::Table(vec![
                (Key::Num(1.0), Value::Bool(true)),
                (Key::Num(2.0), Value::Nil)
            ]))
        );
        assert_eq!(v.get("plain").and_then(Value::as_str), Some("x"));
        let Value::Table(t) = &v else { panic!() };
        assert!(t.contains(&(Key::Num(3.0), Value::Num(31.0))));
    }

    #[test]
    fn lua_rejects_code() {
        assert!(lua::parse("return os.execute('rm -rf /')").is_err());
        assert!(lua::parse("return { [\"a\"] = 1 } extra").is_err());
    }

    #[test]
    fn docsettings_and_hash_paths_map_to_the_book() {
        let root = Path::new("/V");
        assert_eq!(
            book_path(root, Path::new("/V/Deckle"), "Walden", "epub"),
            "Deckle/Walden.epub"
        );
        assert_eq!(
            book_path(
                root,
                Path::new("/V/.adds/koreader/docsettings/mnt/onboard/Deckle"),
                "Walden",
                "epub"
            ),
            "Deckle/Walden.epub"
        );
        assert_eq!(
            book_path(
                root,
                Path::new("/V/koreader/hashdocsettings/ab"),
                "abcdef",
                "epub"
            ),
            "koreader:abcdef"
        );
        assert_eq!(book_path(root, root, "Walden", "pdf"), "Walden.pdf");
    }

    #[test]
    fn partial_md5_samples_as_koreader_does() {
        // Bytes `i % 251`. Expected values from a one-off Python run of util.partialMD5's
        // offsets: 17,000 bytes take samples at 0, 1024, 4096 and a short one at 16384.
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("book.epub");
        let bytes: Vec<u8> = (0..17_000u32).map(|i| (i % 251) as u8).collect();
        std::fs::write(&file, &bytes).unwrap();
        assert_eq!(
            partial_md5(&file).as_deref(),
            Some("5f5620a36f2710bff4bfae9324f07628")
        );
        std::fs::write(&file, &bytes[..100]).unwrap();
        assert_eq!(
            partial_md5(&file).as_deref(),
            Some("7acedd1a84a4cfcb6e7a16003242945e")
        );
        assert_eq!(partial_md5(&dir.path().join("missing")), None);
    }
}
