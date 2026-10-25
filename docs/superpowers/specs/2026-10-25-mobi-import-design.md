# MOBI, AZW and AZW3 in — design

**Status:** written 2026-10-25 for the last slice of [#24](https://github.com/CasaVargas/deckle/issues/24)
("More source formats"), after RTF (#170) ·
**Owner:** Jonathan (CasaVargas LLC)
**Builds on:** `2026-10-23-books-and-kindle-design.md` §5 (the KF8 rebuild in `deckle-mobi`)

Drop a DRM-free `.mobi`, `.azw`, `.azw3` or `.prc` (a Calibre or KindleGen output, an old
Mobipocket purchase that never had DRM, a book from a DRM-free store) and it goes through the
pipeline like an EPUB. It gets real KFX typesetting, page numbers, X-Ray and Word Wise, which a
sideloaded MOBI never has.

## 1. Decision

- **DRM is never touched.** A file whose record 0 says it is encrypted is refused before any
  text is read. The error says Deckle doesn't remove DRM. This is the v1 non-goal, unchanged.
  Fixed-layout books (EXTH 122) are refused too; they belong to #20.
- **The KF8 path already exists.** `deckle_mobi::kf8::rebuild` was built for the library's
  upgrade (Books and Kindle §5). It turns an AZW3, or the KF8 half of a joint MOBI, into a
  checked EPUB. The import stage calls it as it is.
- **New: older MOBI (version 6 and earlier, and PalmDOC).** These books have no KF8 half. Their
  text is one stream of loose "Mobipocket HTML": unquoted attributes, `<mbp:pagebreak/>`,
  `filepos` links and `recindex` images. A new `deckle_mobi::mobi6` module turns it into
  well-formed XHTML parts and writes them with the crate's existing EPUB writer.
  - The HTML is parsed with `dom_query` (html5ever), which `deckle-digest` already uses, so
    malformed markup parses the way a browser would.
  - The output is a small whitelist of XHTML elements, the same idea as web articles.
- **New: HUFF/CDIC decompression.** Some MOBI and AZW files compress their text this way, and
  both paths refuse them today. It is added to `text.rs` and used by both paths. It follows
  KindleUnpack's `HuffcdicReader` (GPL-3.0, the same source the KF8 path follows).
- **Out of scope:**
  - DRM of any kind; fixed layout;
  - KFX (`.kfx`, or an `.azw` that is really a KFX container), which is already sent as is;
  - Topaz (`.azw1`, `.tpz`);
  - `.pdb` (too many unrelated Palm formats share the extension);
  - audio and video records.

## 2. Rules

**Input.**
- Extensions `.mobi`, `.azw`, `.azw3` and `.prc`, in any case, at most 50 MB.
- The PalmDB type and creator must be `BOOKMOBI` or `TEXtREAd`. Otherwise the file is refused
  as not a MOBI. An `.azw` that is a KFX container gets "send it as a .kfx".

**Routing.**
1. Record 0 encrypted → refused (DRM).
2. EXTH 122 says `true` → refused (fixed layout).
3. The book has a KF8 part (MOBI version 8, or EXTH 121) → `kf8::rebuild`.
4. Otherwise → `mobi6::rebuild`.

**Text (both paths).**
- The trailing entries are trimmed as today.
- Compression 1 (none), 2 (PalmDOC) and 17480 (HUFF/CDIC) are read. The HUFF record and its
  CDIC records are found from record 0's `huffoff`/`huffnum` (0x70, 0x74).
- Any other compression is refused, naming the type.
- Record 0's text encoding 65001 is UTF-8, and 1252 (or anything else) is Windows-1252.

**MOBI 6 parts.**
- **Targets.** Every `filepos=N` in the text, and every NCX entry's position, is a target. An
  empty `<a id="fileposN"></a>` is inserted at byte N of the raw stream, before decoding. A
  position inside a tag moves to that tag's start.
- **Links.** `<a filepos=N>` becomes `href` to `fileposN` and is rewritten to the part that holds
  the anchor once the book is split.
- **Images.** `recindex="N"` is the N-th image record from record 0's first image index. Each
  becomes `../Images/imageNNNNN.ext`, the names the KF8 path uses. A missing record drops the
  `<img>` with a warning.
- **Parts.** The text after `<head>` is split at each `<mbp:pagebreak/>`, and each piece becomes
  one XHTML part. A book with no page breaks is one part, split again at about 200 KB at a
  paragraph boundary, so Previewer gets reasonably sized chapters. An empty piece is dropped.
- **Whitelist.** Each piece is parsed with `dom_query` and written as XHTML:

  | Mobipocket HTML | XHTML |
  | --- | --- |
  | `p`, `div`, `blockquote`, `h1`–`h6`, `ul`, `ol`, `li`, `table`, `tr`, `td`, `th`, `hr`, `br` | the same |
  | `b`/`strong`, `i`/`em`, `u`, `s`/`strike`, `sub`, `sup`, `small`, `big`, `code`, `pre`, `tt` | the same, or the nearest XHTML element |
  | `center`, or `align="center"` / `align="right"` on a block | `class="center"` / `class="right"` |
  | `a` with `href`, `id` or `name` | an `<a>` with `href` and `id` |
  | `img` | `<img>` with `alt` (or `""`) |
  | `font`, `span`, `mbp:nu`, unknown tags | their children |
  | `script`, `style`, `head`, `guide`, `mbp:frameset` | dropped |

  Attributes other than those listed are dropped. `id`s are sanitised to XML IDs and kept
  unique.
- **Contents.**
  - From the NCX index (rec0 0xF4) when there is one: each entry's position is its target, its
    label comes from the CNCX, and its depth from tag 4.
  - Otherwise from the parts' `h1`–`h3`, with ids added where missing.
  - Otherwise one entry per part, labelled with the book title.
- **Metadata.** It comes from EXTH, as the KF8 path does it:

  | EXTH | Metadata |
  | --- | --- |
  | 503, else the full name | title |
  | 100 | authors |
  | 101 | publisher |
  | 103 | description |
  | 104 | ISBN |
  | 105 | subjects |
  | 106 | date |
  | 524 | language (else `en`) |
  | 113 or 504 | identifier `urn:asin:…`, else a content hash |
  | 201 | cover |

- **Quality gate.** It works like the KF8 path's:
  - every part must parse as XML;
  - the decoded text must not come back short;
  - the book must have some text.

  Otherwise nothing is written and the error says why.

**PalmDOC (`TEXtREAd`).** The stream is plain text. Blank-line-separated blocks become
paragraphs, single line breaks become `<br/>`, and the result is split at about 200 KB.

## 3. Where it sits

```
book.azw3 ──import (deckle-mobi: kf8 or mobi6)──▶ imported.epub ──inspect──▶ normalize ──▶ …
```

The changes outside the crate:
- New in `deckle-mobi`: `is_mobi_file(path)` (the extension test) and `convert(path, epub) ->
  Result<ImportReport, MobiError>`, which routes as above. `ImportReport` carries title,
  parts, images and warnings.
- `deckle_core::import` adds both to `needs_import` and `import`.
- The library accepts these files as sources. It keeps the original, which is self-contained,
  as it does for DOCX. `book_info` imports it to read the metadata, like the other importers.
- In the desktop:
  - `is_imported` and the folder walk's `is_book` include these files.
  - The file picker lists `mobi`, `azw`, `azw3` and `prc`.
  - The drop-zone text names MOBI and AZW3.
- The CLI help, the README, the CHANGELOG and the v1 spec's non-goals note are updated. The note
  makes clear that DRM stays out.

## 4. Verification

The crate's own `fixture` module builds PalmDB books in tests: `build` for MOBI 6 with
uncompressed text, and `build_kf8`. It gains:
- a MOBI 6 body, given as raw Mobipocket HTML;
- PalmDOC and HUFF/CDIC compression of its text records;
- image records;
- an NCX index.

`fixtures/mobi/` holds three books written by a small test-only tool in the crate. They are
committed so the pipeline, CLI and library tests read real files:
- `mini6.mobi` is MOBI 6 Mobipocket HTML with:
  - page breaks, `filepos` links in both directions, and a `recindex` picture;
  - a cover, an NCX index and EXTH metadata;
  - loose markup (unquoted attributes, `<font>`, `<center>`, an unclosed `<p>`).
- `mini.azw3` is a KF8 book.
- `huff.mobi` is the MOBI 6 book with HUFF/CDIC text records, including a recursive phrase.

Crate tests cover:
- routing: KF8, MOBI 6 and PalmDOC; refusals for DRM, fixed layout and KFX;
- the whitelist, `filepos` anchors and links across parts, `recindex` images;
- contents from the NCX, from headings, and from neither;
- metadata, HUFF/CDIC decoding, the 200 KB split, and the quality gate;
- determinism.

The pipeline and CLI tests run `mini6.mobi` and `mini.azw3` through the fake Previewer. The
library test adds both. An opt-in `mobi_real_previewer_smoke` test runs them through real Kindle
Previewer.

## 5. Implementation notes (2026-10-25, #172)

These are the calls made where §2 left room, so the code and this document agree:

- **Fixtures** are built by the crate's own `fixture` module, not a Python script. The PalmDB,
  MOBI header, index and HUFF layouts already live there for the KF8 tests. An ignored test
  writes `fixtures/mobi/`, and a guard test fails if the committed files drift from the
  builders.
- **The HUFF fixture tables** are 8-bit codes in which each byte decodes to itself, plus one
  overridden phrase that is itself compressed. That exercises the code tables, the CDIC
  dictionary and recursion. Variable-length codes follow KindleUnpack line for line but have
  no fixture, and a real HUFF book is still to be checked by hand.
- **A piece with nothing to read** (only anchors, say, between two page breaks) carries its
  markup into the next part, so a link aimed at it still lands.
- **Part `<title>`s** are the book title. The contents come from the NCX or headings, not from
  file titles.
- **The DRM check** happens in `convert` before routing, and again in each rebuild. A DRM file
  gets `MobiError::Drm` ("Deckle never removes DRM"), never a decode attempt.
- **The desktop app** now depends on `deckle-mobi` normally, not only in tests.
- **Checked:** `mobi_real_previewer_smoke` passed with Kindle Previewer 4 on macOS on 2026-10-25,
  for `mini6.mobi`, `huff.mobi` and `mini.azw3`. Reading on physical hardware, and a real-world
  HUFF/CDIC book, remain unchecked.
