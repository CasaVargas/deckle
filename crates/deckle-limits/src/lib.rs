//! The limits Deckle holds every file it opens to, in one place.
//!
//! - Size limits (`docs/superpowers/specs/2026-11-12-input-size-limits-design.md`): the constants
//!   below, [`read_capped`] and [`Budget`]. A file over a limit gives a [`TooLarge`] error that
//!   names the limit, not a crash.
//! - Nesting depth (`2026-11-12-nesting-depth-limits-design.md`): [`MAX_DEPTH`],
//!   [`MAX_ION_DEPTH`], and with the `xml` feature `parse_xml`.
//! - State files (`2026-11-12-state-file-safety-design.md`): with the `state` feature, the
//!   `state_file` module.
//!
//! With default features this crate depends on nothing.

use std::fmt;
use std::io::{self, Read};

#[cfg(feature = "xml")]
mod xml;
/// The `roxmltree` that [`parse_xml`] parses with, so callers name the same version.
#[cfg(feature = "xml")]
pub use roxmltree;
#[cfg(feature = "xml")]
pub use xml::{XmlError, parse_xml};

#[cfg(feature = "state")]
pub mod state_file;

const MB: u64 = 1024 * 1024;
const GB: u64 = 1024 * MB;

// --- Size limits. The numbers and the reasons for them are in the size-limits design. ---

/// EPUB, KPF, CBZ: one archive entry, unpacked.
pub const MAX_ENTRY_BYTES: u64 = 256 * MB;
/// EPUB, KPF: the whole archive, unpacked. The EPUB inside a KPF draws on the KPF's budget.
pub const MAX_BOOK_BYTES: u64 = GB;
/// CBZ, CBR: the whole archive, unpacked.
pub const MAX_COMIC_BYTES: u64 = 4 * GB;
/// CBZ, CBR: one page, as a compressed image file.
pub const MAX_COMIC_PAGE_BYTES: u64 = 256 * MB;
/// DOCX: the whole package unpacked, all parts and images together.
pub const MAX_DOCX_BYTES: u64 = 50 * MB;
/// MOBI/AZW3: one inflated `FONT` record.
pub const MAX_FONT_BYTES: u64 = 32 * MB;
/// MOBI/AZW3: all inflated `FONT` records of one book together.
pub const MAX_FONTS_BYTES: u64 = 256 * MB;
/// MOBI: decompressed text. The header's text length is used, capped at this.
pub const MAX_MOBI_TEXT_BYTES: u64 = 256 * MB;
/// Image decode, width × height, except comic pages.
pub const MAX_IMAGE_PIXELS: u64 = 100_000_000;
/// PDF: a rendered page, width × height in pixels. A page over it renders at a scale that fits.
pub const MAX_PDF_RENDER_PIXELS: u64 = 100_000_000;
/// PDF: page width or height in points, the PDF specification's own maximum.
pub const MAX_PDF_PAGE_POINTS: f32 = 14_400.0;
/// TXT: chapters (and table-of-contents entries). Past it the rest stays in the last chapter.
pub const MAX_TXT_CHAPTERS: usize = 10_000;
/// KFX: the symbol table's `max_id`.
pub const MAX_KFX_SYMBOLS: u32 = 1_000_000;
/// KFX, KPF: Ion values (and annotations, and a unit for every 64 bytes of symbol text copied)
/// decoded from one file. About 2.5 times the most seen in a real file: up to 466,499 in a real
/// library, 1,971,808 in a 17 MB KFX generated from a long test book. A decoded value measured
/// about 320 bytes in memory, so the tree can reach about 1.6 GB at the limit. This bounds what
/// the decoder builds only: the fix-ups afterwards copy parts of the tree and are not counted
/// (a `$608` structure cloned per `$146` reference, the whole fragment list cloned for writing,
/// the text of each eid as a `Vec<char>` for page numbers); jloutsch/deckle#79 tracks those.
pub const MAX_ION_VALUES: u64 = 5_000_000;
/// ODT, FB2, RTF, TXT, MOBI and Markdown: the file (ODT: unpacked), read whole.
pub const MAX_IMPORT_BYTES: u64 = 50 * MB;
/// PDF: the file.
pub const MAX_PDF_BYTES: u64 = 200 * MB;
/// ODT, FB2, RTF: one embedded picture.
pub const MAX_EMBEDDED_IMAGE_BYTES: u64 = 10 * MB;
/// A KOReader plugin archive, unpacked.
pub const MAX_PLUGIN_BYTES: u64 = 20 * MB;

/// Largest reservation made from a size a file declares (rule 1 of the size-limits design):
/// `Vec::with_capacity(declared.min(MAX_RESERVE))`.
pub const MAX_RESERVE: usize = 64 * 1024;

// --- Nesting depth. ---

/// Deepest element or group nesting any reader accepts.
pub const MAX_DEPTH: usize = 256;
/// Deepest Ion value nesting (KFX, KPF) the Ion decoder accepts.
pub const MAX_ION_DEPTH: usize = 128;

/// A read or a running total went past its limit.
///
/// Its message names the limit in plain words ("more than 256 MB"), for a reader's error to
/// wrap: "this book unpacks to more than 1 GB".
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TooLarge {
    /// The limit in bytes that was passed.
    pub limit: u64,
}

impl fmt::Display for TooLarge {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "more than {}", plain_size(self.limit))
    }
}

impl std::error::Error for TooLarge {}

impl From<TooLarge> for io::Error {
    fn from(e: TooLarge) -> Self {
        io::Error::new(io::ErrorKind::InvalidData, e)
    }
}

impl TooLarge {
    /// The `TooLarge` inside an `io::Error` from [`read_capped`] or [`Budget::read`], if that
    /// is why it failed.
    pub fn in_io(e: &io::Error) -> Option<TooLarge> {
        e.get_ref()?.downcast_ref::<TooLarge>().copied()
    }
}

/// `1073741824` as "1 GB", `268435456` as "256 MB", anything else in bytes.
fn plain_size(bytes: u64) -> String {
    if bytes >= GB && bytes.is_multiple_of(GB) {
        format!("{} GB", bytes / GB)
    } else if bytes >= MB && bytes.is_multiple_of(MB) {
        format!("{} MB", bytes / MB)
    } else {
        format!("{bytes} bytes")
    }
}

/// Reads `r` to the end, failing with [`TooLarge`] past `limit` bytes.
///
/// At most `limit + 1` bytes are read. The error is an `io::Error` so read errors pass
/// through too; [`TooLarge::in_io`] tells the two apart, and its message names the limit.
pub fn read_capped(r: impl Read, limit: u64) -> io::Result<Vec<u8>> {
    let mut out = Vec::new();
    r.take(limit.saturating_add(1)).read_to_end(&mut out)?;
    if out.len() as u64 > limit {
        return Err(TooLarge { limit }.into());
    }
    Ok(out)
}

/// A running total for one file: [`add`](Budget::add) fails once the total passes `limit`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Budget {
    limit: u64,
    used: u64,
}

impl Budget {
    /// A budget of `limit` bytes, none used.
    pub fn new(limit: u64) -> Self {
        Budget { limit, used: 0 }
    }

    /// Counts `n` more bytes. Fails, and counts nothing, when the total would pass the limit.
    pub fn add(&mut self, n: u64) -> Result<(), TooLarge> {
        if n > self.remaining() {
            return Err(TooLarge { limit: self.limit });
        }
        self.used += n;
        Ok(())
    }

    /// Reads `r` to the end through both this budget and a per-entry limit, and counts what
    /// was read. Fails with the limit that was passed: `entry_limit`, or this budget's limit
    /// when what is left of it is smaller.
    pub fn read(&mut self, r: impl Read, entry_limit: u64) -> io::Result<Vec<u8>> {
        let (cap, limit) = if self.remaining() < entry_limit {
            (self.remaining(), self.limit)
        } else {
            (entry_limit, entry_limit)
        };
        let out = read_capped(r, cap).map_err(|e| match TooLarge::in_io(&e) {
            Some(_) => TooLarge { limit }.into(),
            None => e,
        })?;
        self.used += out.len() as u64;
        Ok(out)
    }

    /// The limit this budget was made with.
    pub fn limit(&self) -> u64 {
        self.limit
    }

    /// Bytes counted so far.
    pub fn used(&self) -> u64 {
        self.used
    }

    /// Bytes still allowed.
    pub fn remaining(&self) -> u64 {
        self.limit - self.used
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A reader that yields `n` zero bytes and counts what was taken from it.
    struct Zeros {
        left: u64,
        taken: u64,
    }

    impl Read for Zeros {
        fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
            let n = (buf.len() as u64).min(self.left) as usize;
            buf[..n].fill(0);
            self.left -= n as u64;
            self.taken += n as u64;
            Ok(n)
        }
    }

    fn zeros(n: u64) -> Zeros {
        Zeros { left: n, taken: 0 }
    }

    #[test]
    fn read_capped_reads_up_to_the_limit() {
        assert_eq!(read_capped(&b"hello"[..], 5).unwrap(), b"hello");
        assert_eq!(read_capped(&b""[..], 0).unwrap(), b"");
    }

    #[test]
    fn read_capped_refuses_one_byte_over() {
        let e = read_capped(&b"hello!"[..], 5).unwrap_err();
        assert_eq!(TooLarge::in_io(&e), Some(TooLarge { limit: 5 }));
        assert_eq!(e.to_string(), "more than 5 bytes");
    }

    #[test]
    fn read_capped_stops_reading_at_limit_plus_one() {
        let mut r = zeros(u64::MAX);
        let e = read_capped(&mut r, 1000).unwrap_err();
        assert!(TooLarge::in_io(&e).is_some());
        assert_eq!(r.taken, 1001);
    }

    #[test]
    fn read_capped_passes_read_errors_through() {
        struct Broken;
        impl Read for Broken {
            fn read(&mut self, _: &mut [u8]) -> io::Result<usize> {
                Err(io::Error::other("disk gone"))
            }
        }
        let e = read_capped(Broken, 10).unwrap_err();
        assert_eq!(TooLarge::in_io(&e), None);
        assert_eq!(e.to_string(), "disk gone");
    }

    #[test]
    fn the_message_names_the_limit_in_plain_words() {
        assert_eq!(
            TooLarge {
                limit: MAX_BOOK_BYTES
            }
            .to_string(),
            "more than 1 GB"
        );
        assert_eq!(
            TooLarge {
                limit: MAX_ENTRY_BYTES
            }
            .to_string(),
            "more than 256 MB"
        );
        assert_eq!(
            TooLarge {
                limit: MAX_COMIC_BYTES
            }
            .to_string(),
            "more than 4 GB"
        );
    }

    #[test]
    fn budget_allows_up_to_the_limit_and_refuses_past_it() {
        let mut b = Budget::new(10);
        b.add(4).unwrap();
        b.add(6).unwrap();
        assert_eq!(b.remaining(), 0);
        assert_eq!(b.add(1), Err(TooLarge { limit: 10 }));
        assert_eq!(b.used(), 10, "a refused add counts nothing");
    }

    #[test]
    fn budget_add_does_not_overflow() {
        let mut b = Budget::new(10);
        b.add(5).unwrap();
        assert_eq!(b.add(u64::MAX), Err(TooLarge { limit: 10 }));
        assert_eq!(b.used(), 5);
    }

    #[test]
    fn budget_read_applies_both_limits() {
        let mut b = Budget::new(10);
        assert_eq!(b.read(&b"abcd"[..], 5).unwrap(), b"abcd");
        assert_eq!(b.used(), 4);

        // Over the entry limit, with room left in the budget.
        let e = b.read(&b"abcdef"[..], 5).unwrap_err();
        assert_eq!(TooLarge::in_io(&e), Some(TooLarge { limit: 5 }));
        assert_eq!(b.used(), 4);

        // Within the entry limit, over what is left of the budget (6 bytes).
        let mut r = zeros(u64::MAX);
        let e = b.read(&mut r, 100).unwrap_err();
        assert_eq!(TooLarge::in_io(&e), Some(TooLarge { limit: 10 }));
        assert_eq!(r.taken, 7, "stops one byte past what is left");
        assert_eq!(b.used(), 4);

        assert_eq!(b.read(&b"123456"[..], 100).unwrap(), b"123456");
        assert_eq!(b.remaining(), 0);
    }
}
