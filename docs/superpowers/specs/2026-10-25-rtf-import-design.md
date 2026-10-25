# Rich Text Format (RTF) in — design

**Status:** written 2026-10-25 for the RTF slice of [#24](https://github.com/CasaVargas/deckle/issues/24)
("More source formats"), next after ODT (#168) in that issue's order ·
**Owner:** Jonathan (CasaVargas LLC)
**Builds on:** `2026-10-21-docx-import-design.md` and `2026-10-25-odt-import-design.md`, whose
output this matches

Drop a `.rtf` (what TextEdit, WordPad, Scrivener's compile, older Word and countless exports
write) and it reaches the Kindle the way a Word document does:
- headings become the contents;
- bold, italics, lists and tables come through;
- pictures stay where they were;
- footnotes become pop-up notes.

## 1. Decision

- **A new crate, `crates/deckle-rtf`**, shaped like `deckle-odt`: `is_rtf(path)`,
  `convert(rtf, epub) -> Result<ImportReport, RtfError>`. It depends on `zip`, `sha2` and
  `encoding_rs`, and on nothing else from Deckle.
- **The parser is hand-written.** RTF is a stream of control words inside `{…}` groups, not XML,
  and a small tokenizer plus a group-state stack is all it needs. The Rust RTF crates either
  drop formatting and destinations this needs (footnotes, fields, pictures, tables) or are
  unmaintained.
- **The same output as DOCX and ODT.** The heading → contents rules, file splitting, note asides,
  stylesheet classes and identifier scheme all match. Later stages see the same kind of EPUB.
- **Out of scope:**
  - drawing objects (`\shp`) other than their pictures;
  - embedded OLE objects (their `\result` text or picture is kept);
  - comments (`\annotation`), revision marks (deleted text is dropped, inserted text kept);
  - headers and footers, and section and page layout;
  - continued numbering across separate lists;
  - nested tables, which are flattened, with a warning;
  - WMF/EMF/PICT pictures, which are skipped with a warning.

## 2. Rules

**Input.**
- `.rtf`, in any case, at most 50 MB.
- The file must begin with `{\rtf` (after optional whitespace). Anything else is refused as
  not RTF.
- A missing final `}` is tolerated, with a warning.

**Text and encoding.**
- `\uN` is a Unicode character: a negative N is taken plus 65536. The `\ucN` replacement
  characters after it are skipped. `\uc` is scoped by group and defaults to 1.
- `\'hh` bytes are decoded with the current font's code page, else the document's
  `\ansicpgN`, else Windows-1252:
  - the font's code page comes from `\fcharsetN` in `\fonttbl`, or from `\cpgN`;
  - consecutive `\'hh` bytes are decoded together, so double-byte code pages (932, 936, 949,
    950) work.
- `\mac` and `\pc` select their code pages (10000 and 437).
- Control symbols:

  | Symbol | Becomes |
  | --- | --- |
  | `\~` | a non-breaking space |
  | `\_` | a non-breaking hyphen |
  | `\-` | dropped |
  | `\{`, `\}`, `\\` | the literal character |

- Control words for characters:

  | Control word | Becomes |
  | --- | --- |
  | `\tab` | a space |
  | `\emdash`, `\endash` | — and – |
  | `\lquote`, `\rquote`, `\ldblquote`, `\rdblquote` | curly quotes |
  | `\bullet` | • |
  | `\emspace`, `\enspace`, `\qmspace` | spaces |

- Line breaks in the file itself are not text.

**Destinations.**
- **Read:** `\fonttbl` (code pages), `\stylesheet` (style names), `\listtable` and
  `\listoverridetable`, `\info`, `\pict`, `\footnote`, `\field`.
- **Skipped entirely**, along with any `{\*\…}` destination not listed above:
  - `\colortbl`, `\header…`, `\footer…`, `\annotation`, `\atnid`, `\atnauthor`;
  - `\pntext`, `\listtext` (the rendered number; lists are rebuilt);
  - `\nonshppict` (the fallback copy of a picture);
  - `\deleted` runs, `\xe`, `\tc`, `\bkmkend`, `\themedata`, `\datastore`, `\latentstyles`.

**Character formatting.** Group-scoped. `\plain` resets it.

| RTF | XHTML |
| --- | --- |
| `\b` / `\b0` | `<b>` |
| `\i` / `\i0` | `<i>` |
| `\ul…` (any underline kind) / `\ulnone`, `\ul0` | `<u>` |
| `\strike`, `\striked1` / `\strike0` | `<s>` |
| `\super`, `\up N` / `\sub`, `\dn N` / `\nosupersub` | `<sup>` / `<sub>` |
| `\scaps` | `<span class="sc">` |
| `\v` (hidden) | dropped |

**Paragraphs.**
- `\par` ends a paragraph, and `\pard` resets paragraph properties.
- `\line` is `<br/>`. `\page` and `\sect` are paragraph breaks only.
- `\qc` → `class="center"`, and `\qr` → `class="right"`.
- A style (`\sN`) is a heading when its stylesheet name is `heading N` (any case), or the
  paragraph has `\outlinelevelN` (0-based, so 0 is level 1).
- Headings 1–3 are nested contents entries, and each level 1 starts a new XHTML file. A heading's
  own bold is not repeated inside it.
- Styles named `Title`, `Subtitle` and `Quote`/`Quotations` get the same classes as DOCX. The
  first Title paragraph is the title when `\info` has none.
- Empty paragraphs are dropped.

**Lists.**
- A paragraph with `\lsN` (and `\ilvlN`, default 0) is a list item.
- `\lsN` maps through `\listoverridetable` to a `\list`. At each level, `\levelnfcN` decides the
  list type: 23 (bullet) or 255 (none) gives `<ul>`, anything else `<ol>`. `\levelstartatN` gives
  `start`.
- Old-style `{\*\pn …}` paragraphs are items too: `\pnlvlblt` gives `<ul>`, and anything else
  `<ol>`.

**Tables.**
- `\trowd` starts a row definition. `\intbl` paragraphs are cell content, `\cell` ends a cell, and
  `\row` ends the row.
- `\trhdr` rows use `<th>`.
- `\clmgf` / `\clmrg` horizontal merges become `colspan`, and vertical merges (`\clvmgf` /
  `\clvmrg`) become `rowspan`.
- Content at a nesting depth above 1 (`\itapN`, `\nestcell`) is flattened into its outer cell,
  with a warning.

**Fields.** `{\field{\*\fldinst HYPERLINK "url" …}{\fldrslt text}}`:
- An `http(s)` or `mailto` URL becomes a link.
- A `\l "bookmark"` switch becomes a link to that bookmark, resolved across chapters.

Any other field keeps its `\fldrslt` text. `{\*\bkmkstart name}` is an anchor.

**Notes.** A `{\footnote …}` group becomes a noteref at that point. The `\chftn` marks inside and
before it are ignored, and the reference gets the next number. `\ftnalt` makes an endnote. The
body becomes an `<aside epub:type="footnote|endnote">` at the end of the chapter, with a
back-link, as in DOCX.

**Pictures.**
- A `\pict` with `\pngblip` or `\jpegblip` is decoded, from hex or from `\binN` raw bytes, and
  sniffed. Each is packaged once, up to 10 MB.
- Alt text is the `wzDescription` shape property in `\picprop`, else `""`.
- Other blip types are skipped with a warning. `\shppict` is read and `\nonshppict` ignored, so
  each picture appears once.

**Metadata** (from `\info`):
- **Title:** `\title`, else the first Title paragraph, else the file stem.
- **Author:** `\author`.
- **Description:** `\subject`, else `\doccomm`.
- **Subjects:** `\keywords`, split on commas and semicolons.
- **Modified:** `\revtim` (`\yr \mo \dy \hr \min`).
- **Language:** the `\deflangN` LCID mapped to a BCP 47 tag. A table covers about 30 common
  LCIDs; an unknown one falls back to `en` with a warning.
- **Identifier:** `urn:deckle:rtf:` plus a hash of the title and author.
- **Cover:** there is none; the typographic cover generator makes one.

**Errors.** There are specific errors for:
- not RTF;
- an empty document (no text after parsing);
- a file over 50 MB.

Malformed control words keep their text, with a warning, rather than failing.

## 3. Where it sits

```
book.rtf ──import (deckle-rtf)──▶ imported.epub ──inspect──▶ normalize ──▶ …
```

The changes outside the crate:
- `deckle_core::import` adds `deckle_rtf::is_rtf` to `needs_import` and `import`.
- `deckle_library::is_source` and `book_info` accept RTF. The library keeps the original file.
- In the desktop:
  - `is_imported` and the folder walk's `is_book` include RTF.
  - The file picker lists `rtf`.
  - The drop-zone text names RTF.
- The CLI help, the README, the CHANGELOG and the v1 spec's non-goals note are updated.

## 4. Verification

`fixtures/rtf/` is made by `tools/ref/make_rtf_fixtures.py`, which uses only the standard
library:
- `mini.rtf` has:
  - `\info` with a title, author, subject, keywords and revision time;
  - a stylesheet with Title and heading 1–4;
  - every character format listed above, alignment, `\line` and `\tab`;
  - curly quotes and dashes as control words, `\'hh` in Windows-1252, and `\u` with a
    replacement;
  - a bulleted list nested in a numbered list starting at 3, via `\listtable`;
  - a header-row table with a merged cell;
  - a PNG in `\shppict` with a `\nonshppict` copy and alt text, and a JPEG as raw `\bin` bytes;
  - a hyperlink field, a bookmark and a link to it, two footnotes and an endnote;
  - a hidden run, a deleted revision, and an annotation, none of which must appear;
  - the final `}` present.
- `cyrillic.rtf` has `\ansicpg1251` with Cyrillic `\'hh` text and `\deflang1049`.
- `japanese.rtf` uses a `\fcharset128` font with double-byte `\'hh` pairs.
- `unclosed.rtf` is missing its final brace.

Crate tests cover:
- XML well-formedness, and that every manifest/link/src target exists;
- contents and file splits;
- each formatting and paragraph rule, lists, tables and merges;
- notes in both directions, pictures (dedup, `\bin`, alt text, nonshppict), and fields and
  bookmarks;
- every encoding path, metadata, determinism, each refusal and each warning.

The pipeline and CLI tests run `mini.rtf` through the fake Previewer. A library test adds an RTF
and reads its metadata back. An opt-in `rtf_real_previewer_smoke` test runs the fixtures through
real Kindle Previewer.
