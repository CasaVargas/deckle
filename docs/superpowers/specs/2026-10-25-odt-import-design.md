# OpenDocument Text (ODT) in — design

**Status:** written 2026-10-25 for the ODT slice of [#24](https://github.com/CasaVargas/deckle/issues/24)
("More source formats"), next after FB2 (#166) in that issue's order ·
**Owner:** Jonathan (CasaVargas LLC)
**Builds on:** `2026-10-21-docx-import-design.md`, whose shape and output this mirrors

Drop a `.odt` (what LibreOffice, Google Docs' "OpenDocument" export and Pages' ODF export write)
and it reaches the Kindle the way a Word document does:
- headings become the contents;
- bold, italics, lists and tables come through;
- pictures stay where they were;
- footnotes become pop-up notes.

## 1. Decision

- **A new crate, `crates/deckle-odt`**, shaped like `deckle-docx`: `is_odt(path)`,
  `convert(odt, epub) -> Result<ImportReport, OdtError>`, `zip` + `roxmltree` + `sha2`, and no
  dependency on the rest of Deckle. The pipeline's `import` stage dispatches to it.
- **The same output as DOCX.** The heading → contents rules, file splitting, note asides,
  stylesheet classes and identifier scheme all match, so later stages see the same kind of EPUB
  whichever word processor the book came from.
- **Formats:**
  - `.odt` and `.ott` (template), both zipped ODF.
  - `.fodt` (flat ODF, one XML file), because it is the same markup with images inlined as
    base64, and it costs one extra reader.
  - Other ODF types (`.ods`, `.odp`, `.odg`) are refused, with a message naming what they are.
- **Out of scope:**
  - text boxes, drawings and shapes, charts, formulas, comments (`office:annotation`), tracked
    changes (deleted text is dropped, inserted text kept);
  - fields beyond their displayed text;
  - generated indexes: the stored table of contents is skipped, because Deckle builds its own;
  - headers and footers, and master pages;
  - continued list numbering across interruptions;
  - SVG/EMF/WMF images, which are skipped with a warning.

## 2. Rules

**Reading the package.**
1. The `mimetype` entry, or the flat file's `office:mimetype`, must be
   `application/vnd.oasis.opendocument.text` or `…text-template`.
2. Main content is `content.xml`, styles are `styles.xml`, and metadata is `meta.xml`.
3. If `META-INF/manifest.xml` has `manifest:encryption-data` on `content.xml`, the document is
   password-protected and is refused.

A flat `.fodt` holds all of these in one `office:document`.

**Styles.** A paragraph or span's formatting is found by walking its `style:name` →
`style:parent-style-name` chain. The walk covers automatic styles first, then the common styles
in `styles.xml`. The first value found for each property wins:
- `fo:font-weight="bold"` (or 600 and above) → `<b>`
- `fo:font-style="italic"` / `oblique` → `<i>`
- `style:text-underline-style` other than `none` → `<u>`
- `style:text-line-through-style` other than `none` → `<s>`
- `style:text-position` starting `super` / `sub`, or a positive / negative percentage →
  `<sup>` / `<sub>`
- `fo:font-variant="small-caps"` → `<span class="sc">`
- paragraph `fo:text-align` of `center`, or `end`/`right` → `class="center"` / `"right"`

A heading's own bold is not repeated inside the heading. The style chain's display name is used
to detect `Title`, `Subtitle` and `Quotations`, which get the same classes as DOCX's Title,
Subtitle and Quote.

**Headings.**
- `text:h` with `text:outline-level` N (default 1) becomes `<hN>`, capped at 6. A paragraph
  whose style chain has a `style:default-outline-level` counts as a heading too.
- Levels 1–3 are nested contents entries. Each level 1 starts a new XHTML file.
- The first `Title` paragraph is the title when `meta.xml` has none.

**Body.** Mapping from ODF to EPUB:

| ODF | EPUB |
| --- | --- |
| `text:p` | `<p>` (style classes as above) |
| `text:span` | its formatting |
| `text:s` | `text:c` spaces (one, by default) as non-breaking spaces after the first |
| `text:tab` | a space |
| `text:line-break` | `<br/>` |
| `text:soft-page-break` | dropped |
| `text:a` | an `http(s)`/`mailto` link; `#bookmark` links resolve to the bookmark's file; anything else keeps its text |
| `text:bookmark`, `text:bookmark-start` | an anchor id |
| `text:list` / `text:list-item` | nested `<ul>`/`<ol>` (see Lists) |
| `table:table` | `<table>` (see Tables) |
| `text:section` | its contents |
| `text:note` | a noteref and aside (see Notes) |
| `draw:frame` › `draw:image` | `<img>` (see Images) |
| `text:table-of-content`, other indexes | dropped, because Deckle builds the contents |
| `office:annotation`, `text:tracked-changes`, `text:deletion` | dropped |

Unknown elements keep their text, with one warning per element name.

**Lists.** A list's style is `text:style-name` on the outermost `text:list`, inherited by
nested lists. At each nesting level, `text:list-level-style-number` gives `<ol>`, and a bullet,
image or missing style gives `<ul>`. `text:start-value` on the first item becomes `start`.

**Tables.**
- `table:table-header-rows` rows become `<th>` cells.
- `table:number-columns-spanned` and `table:number-rows-spanned` become `colspan`/`rowspan`.
- `table:covered-table-cell` is dropped.
- Cell content is block content.

**Notes.** A `text:note` with `text:note-class` `footnote` or `endnote` becomes
`<a epub:type="noteref">` with its `text:note-citation` text, in `<sup>`. The note body becomes
`<aside epub:type="footnote|endnote">` at the end of the chapter that references it, with a
back-link: the same placement as DOCX.

**Images.**
- In a zipped document, `draw:image xlink:href="Pictures/…"` is read from the package. In a
  `.fodt`, the image comes from `office:binary-data` (base64).
- The type is sniffed from the bytes: PNG, JPEG or GIF. Anything else, or a missing image, is
  skipped with a warning. An external `http` image is never fetched.
- Alt text is the frame's `svg:title`, else `svg:desc`, else `draw:name`.
- An image anchored as a character stays inline; any other image becomes a centred block.
- Each image is packaged once, up to 10 MB.

**Metadata** (from `meta.xml` / `office:meta`):
- **Title:** `dc:title`, else the first Title paragraph, else the file stem.
- **Author:** `dc:creator`, else `meta:initial-creator`.
- **Language:** `dc:language`, else the default paragraph style's `fo:language`-`fo:country`,
  else `en`.
- **Description:** `dc:description`.
- **Subjects:** `meta:keyword` entries become `dc:subject`s.
- **Modified:** `dc:date`.
- **Identifier:** `urn:deckle:odt:` plus a hash of the title and author.
- **Cover:** there is none; the typographic cover generator makes one.

**Errors.** There are specific errors for:
- not a zip, and not flat ODF XML;
- an ODF type other than text;
- password protection;
- a missing `content.xml`;
- a corrupt part (the message names the part);
- a file over 50 MB.

## 3. Where it sits

```
book.odt ──import (deckle-odt)──▶ imported.epub ──inspect──▶ normalize ──▶ …
```

The changes outside the crate:
- `deckle_core::import` adds `deckle_odt::is_odt` to `needs_import` and `import`.
- `deckle_library::is_source` and `book_info` accept ODT. The library keeps the original file,
  which is self-contained, as it does for DOCX.
- In the desktop:
  - `is_imported` and the folder walk's `is_book` include ODT.
  - The file picker lists `odt`, `ott` and `fodt`.
  - The drop-zone text names OpenDocument.
- The CLI help, the README and the v1 spec's non-goals note are updated.

## 4. Verification

`fixtures/odt/` is made by `tools/ref/make_odt_fixtures.py`, which uses only the standard
library:
- `mini.odt` has:
  - `meta.xml` with a title, creator, language, description, keywords and date;
  - a Title paragraph, and H1–H4 with H1 split into two chapters;
  - formatting in both automatic styles and inherited common styles (bold, italic, underline,
    strike, super/sub, small caps, centred);
  - a nested bullet list inside a numbered list, with a start value;
  - a table with a header row and a spanned cell;
  - an inline image and a block image with alt text;
  - two footnotes and an endnote, a bookmark link and an external link;
  - a stored table of contents, an annotation and a tracked deletion, all of which must not
    appear.
- `mini.fodt` is the same document as flat ODF, with base64 images.
- `sheet.ods` has a spreadsheet mimetype.
- `encrypted.odt` has a manifest with `encryption-data`.

Crate tests cover:
- XML well-formedness, and that every manifest/link/src target exists;
- contents order and nesting, and file splits;
- every mapped element and style property, including inheritance;
- lists, tables, notes in both directions, and images (sniffed, placed, alt text);
- metadata, `.odt` and `.fodt` producing the same EPUB, determinism, and each refusal.

The pipeline and CLI tests run both fixtures through the fake Previewer. A library test adds an
ODT and reads its metadata back. An opt-in `odt_real_previewer_smoke` test runs them through real
Kindle Previewer.

## 5. Implementation notes (2026-10-25, #168)

These are the calls made where §2 left room, so the code and this document agree:

- **Other ODF types.** `is_odt` takes only `.odt`, `.ott` and `.fodt`, so a folder walk never
  picks up a spreadsheet. The "this is an OpenDocument spreadsheet" error therefore fires only
  for a text-named file whose mimetype says otherwise.
- **Image placement.** Every picture is an `<img>` where its frame sits. A paragraph holding
  only pictures gets the `figure` class, which centres it, as in DOCX. The anchor type isn't
  consulted.
- **Text boxes keep their text.** LibreOffice puts captioned pictures inside a frame's
  `draw:text-box`. Its paragraphs are rendered inline with `<br/>` between them, so the picture
  and caption survive. Embedded objects (charts, formulas) warn and are left out.
- **Title and Subtitle.** These paragraphs don't repeat their style chain's bold, because the
  class already styles them.
- **List items** with several paragraphs join them with `<br/>`. A heading inside a list item is
  a heading, not an item.
- **Checked:** `odt_real_previewer_smoke` passed with Kindle Previewer 4 on macOS on 2026-10-25,
  for `mini.odt` and `mini.fodt`. Reading on physical hardware remains untested.
