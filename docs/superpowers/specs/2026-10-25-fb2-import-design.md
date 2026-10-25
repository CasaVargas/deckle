# FictionBook (FB2) in — design

**Status:** written 2026-10-25 for the FB2 slice of [#24](https://github.com/CasaVargas/deckle/issues/24)
("More source formats"), the next format after TXT (#86) in that issue's order ·
**Owner:** Jonathan (CasaVargas LLC)
**Builds on:** `2026-10-21-docx-import-design.md` (the `import` stage), `2026-10-21-txt-import-design.md`

Drop a `.fb2` or `.fb2.zip` (the format most Russian and Eastern European ebook libraries ship) and it
reaches the Kindle as a real book. It keeps its chapters, footnotes, poems, embedded cover and
illustrations, and the author, series and language the file declares.

## 1. Decision

- **A new crate, `crates/deckle-fb2`**, shaped like `deckle-txt`: `is_fb2(path)`,
  `convert(fb2, epub) -> Result<ImportReport, Fb2Error>`, no dependency on the rest of Deckle.
  The pipeline's `import` stage dispatches to it, so preflight, bind, the library and the desktop
  all get it.
- **FB2 is XML with a fixed schema, so the importer maps the structure rather than guessing it.**
  It uses no heuristics. Elements it doesn't know keep their text and produce one warning per
  element name.
- **Parsing with `roxmltree`** (already a workspace dependency). Non-UTF-8 files are decoded first
  with `encoding_rs` (already a dependency), because roxmltree only reads UTF-8.
- **`.fb2.zip` is accepted**, because that's how most FB2 libraries distribute books. A zip
  containing exactly one `.fb2` entry is unpacked in memory. Any other shape of zip is refused.
- **Out of scope:** FB3, FB2 → anything except EPUB, `<stylesheet>` (FB2's rarely used embedded
  CSS), `<output>` and `<custom-info>`, and the Daily Inbox (its book list stays EPUB/DOCX/KFX).

## 2. Rules

**Input.** `.fb2`, or `.fb2.zip`, in any case. At most 50 MB on disk, and at most 50 MB once
unzipped.

**Encoding.**
1. A BOM (UTF-8 or UTF-16) wins.
2. Otherwise, the XML declaration's `encoding="…"` label is decoded via `encoding_rs` (`windows-1251`,
   `koi8-r`, `utf-8`, …).
3. Otherwise the file is read as UTF-8.

An unknown label falls back to UTF-8 with a warning. Undecodable bytes become U+FFFD, with a warning.
The declaration is removed before parsing.

**Metadata** (from `description/title-info`):
- **Title:** `book-title`, else the file stem.
- **Authors:** each `author` is `first-name middle-name last-name`, or else `nickname`, and each
  becomes its own `dc:creator`.
- **Language:** `lang`, if it looks like a BCP 47 tag (letters, digits, `-`). Otherwise `en`, with a
  warning.
- **Series:** the first `sequence` gives its `name`, and its `number` when that's a positive integer.
  These are written as `belongs-to-collection` + `group-position` and as `calibre:series` /
  `calibre:series_index`, the two forms `inspect` already reads.
- **Description:** `annotation`, as plain text, goes in `dc:description`.
- **Subjects:** each `genre` becomes a `dc:subject`, using its raw code (`sf_fantasy`).
- **Identifier:** `urn:deckle:fb2:` plus a hash of the title and authors, the same scheme as TXT and
  DOCX, so a re-sent file stays the same book.
- **Cover:** a `coverpage/image` pointing at a usable binary becomes the EPUB's `cover-image`.
  Otherwise the typographic cover generator makes one, as for TXT.

**Bodies.** The first `<body>` without a `name` attribute, or else the first `<body>`, is the
book. A body named `notes` or `comments` is the notes. Any other extra body is appended after the
book, with a warning.

**Structure.** Mapping into the book body:

| FB2 | EPUB |
| --- | --- |
| `body/title` | the book's title page, the first spine file |
| `section` with a `title` | a heading, `<h1>`–`<h6>` by nesting depth (capped at 6), and a contents entry |
| `section` without a `title` | its content, with no contents entry |
| `epigraph`, `text-author` | `<blockquote class="epigraph">`, `<p class="text-author">` |
| `poem` / `stanza` / `v` | `<div class="poem">` / `<p class="stanza">` with `<br/>` between lines |
| `cite` | `<blockquote>` |
| `subtitle` | `<p class="subtitle">` |
| `empty-line` | `<p class="empty-line">&#160;</p>` |
| `p` | `<p>` |
| `strong`, `emphasis`, `strikethrough`, `sub`, `sup`, `code` | `<strong>`, `<em>`, `<s>`, `<sub>`, `<sup>`, `<code>` |
| `table` / `tr` / `th` / `td` | the same HTML elements (`colspan`/`rowspan`/`align` kept) |
| `image` (block or inline) | `<img>` with the binary, its `alt` or `title`, else `""` |
| `a l:href="#id"` | a link to wherever `id` lands; `type="note"` makes it a noteref |
| `a l:href="http(s):…"` / `mailto:` | an ordinary link |

**Files.** The title page is one file. A new file starts at each top-level section, and at each
second-level section that has a title, so a book with one wrapper section around its chapters still
splits per chapter. Untitled content between files stays with the file before it.

**Notes.** Each `section` with an `id` in the notes body becomes an
`<aside epub:type="footnote" id="…">` in `notes.xhtml`, the last spine file, under a "Notes" heading.
It appears as one contents entry. A link with `type="note"`, or any link into the notes body, gets
`epub:type="noteref"`, which gives a Kindle pop-up footnote. Each note ends with a back-link to its
first reference.

**Links.** IDs from FB2 elements are kept, after sanitising to valid XML IDs, with numeric suffixes
for duplicates. A `#id` link points to the file that holds that element. A link to a missing ID, or
to another scheme, keeps its text and drops the link, with one warning per target.

**Images.** `<binary id content-type>` entries are base64-decoded. PNG, JPEG and GIF are
packaged under `OEBPS/images/` in first-use order, and each is packaged once. The type is sniffed
from the bytes, and the `content-type` is ignored, because real files mislabel it. A missing,
undecodable, unsupported or over-10 MB binary drops the `<img>` with a warning. Unused binaries are
skipped.

**Errors.** There are specific errors for:
- an empty file;
- XML that doesn't parse (the message includes the parser's line);
- a root that isn't `FictionBook`;
- no `<body>`, or a body with no text;
- a file over 50 MB;
- a `.fb2.zip` that doesn't hold exactly one `.fb2`.

## 3. Where it sits

```
book.fb2 ──import (deckle-fb2)──▶ imported.epub ──inspect──▶ normalize ──▶ …
```

The changes outside the crate:
- `deckle_core::import` adds `deckle_fb2::is_fb2` to `needs_import` and `import`.
- `deckle_library::is_source` and `book_info` accept FB2. The library keeps the original `.fb2`,
  which is self-contained, as it does for DOCX.
- In the desktop:
  - `is_imported` and the folder walk's `is_book` include FB2. Unlike TXT, an `.fb2` in a folder
    is always a book.
  - The file picker lists `fb2` and `zip`. Its zip entry only matches `.fb2.zip`, which
    `expand_book_paths` checks.
  - The drop-zone text names FB2.
- The CLI help text, the README and the v1 spec's non-goals note are updated.

## 4. Verification

`fixtures/fb2/` is made by `tools/ref/make_fb2_fixtures.py`, which uses only the standard library:
- `mini.fb2` (UTF-8) has:
  - title-info with two authors, a series, a genre, an annotation and a cover;
  - a body title and an epigraph;
  - two parts with two chapters each, a poem and a table;
  - inline emphasis, an inline image, an internal link, two footnotes and an external link.
- `cp1251.fb2` is a short Russian book declared `windows-1251`.
- `mini.fb2.zip` is `mini.fb2` zipped.
- `broken.fb2` is truncated XML.

Crate tests cover:
- XML well-formedness of every file, and that every manifest/spine/link target exists;
- metadata, including series, authors and cover;
- heading levels and contents nesting, and the file splits;
- the poem, epigraph and table markup;
- noterefs, asides and back-links;
- image packaging and dedup, and every image warning;
- the encoding fallback, `.fb2.zip` handling, determinism, and each refusal.

The pipeline and CLI tests run `mini.fb2` and `mini.fb2.zip` end to end through the fake
Previewer. The library test adds an FB2 and reads its metadata back. An opt-in
`fb2_real_previewer_smoke` test, like the Markdown one, runs the fixtures through real Kindle
Previewer.
