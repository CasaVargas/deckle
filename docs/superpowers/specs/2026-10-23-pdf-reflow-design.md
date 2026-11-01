# PDF in, reflowed — design

**Status:** for [#18](https://github.com/CasaVargas/deckle/issues/18) ("PDF → reflowable book"),
following the plan recorded on the issue (2026-10-20: PDFium through `pdfium-render`) ·
**Date:** 2026-10-23 · **Owner:** Jonathan (CasaVargas LLC)
**Builds on:** `2026-10-21-docx-import-design.md`, `2026-10-21-txt-import-design.md` (the
`import` stage) · **Amends:** `2026-10-18-deckle-design.md` §2 (PDFs were a non-goal)

Drop a text PDF (a paper, a report, a book someone typeset) and it reaches the Kindle as a
book that reflows: one column, real paragraphs, no running heads or page numbers in the middle
of a sentence, broken words joined again. Tables, displayed equations and figures come along as
pictures cut from the page, because reflowed table text is worse than useless.

A reflowed PDF is never as good as a book made for reading. Deckle says so every time.

## 1. Decision

- **PDFium, through `pdfium-render`.** It's the PDF engine in Chrome, BSD-licensed, reads
  nearly every PDF in the wild, gives each character with its box and font, and renders pages
  for the pictures. The pure-Rust readers don't render, and get text positions wrong on
  enough real files that the layout passes would be built on sand.
- **PDFium is found at run time, not linked.** `pdfium-render` binds a dynamic library, in this
  order: `DECKLE_PDFIUM` (the file or its folder), beside the executable, the app bundle's
  `Frameworks` (macOS), the workspace's `target/pdfium` in a development build, then the
  system's. Without it, a PDF fails at `import` with a message that says where to get it;
  nothing else in Deckle needs it.
- **A new crate, `crates/deckle-pdf`**, the same shape as `deckle-txt` (`is_pdf(path)`,
  `convert(pdf, epub) -> Result<ImportReport, PdfError>`), with no dependency on the rest of
  Deckle. The pipeline's `import` stage dispatches on the extension.
- **The layout is pure code over positioned glyphs** (`layout.rs`), separate from PDFium
  (`engine.rs`), so every pass is unit-tested on hand-made pages and only the end-to-end tests
  need the library.
- **Conservative, like TXT.** A column the passes can't find is read top to bottom; a word
  they can't decide about keeps its hyphen; text they can't place stays text.

## 2. Out of scope

- **Scans.** A PDF with no text layer (under 20 characters a page, with pictures) is refused:
  "this PDF has no text in it, only pictures of pages (a scan)…". OCR is #19.
- **Fixed layout.** Comics and picture books are #20.
- **The desktop app** was a follow-up (#133); see §8.
- **Bookmarks as the contents.** The contents come from the headings the layout finds. *(Since
  #25, a PDF's bookmarks are its chapters and contents when it has them; see
  `2026-11-01-structure-recovery-design.md` §4.)*
- **Language.** Like TXT, the book is tagged English.

## 3. PDFium

Pinned to `chromium/8111` from `bblanchon/pdfium-binaries`. `tools/fetch-pdfium.sh` downloads
the build for this machine (or a named platform), checks its SHA-256 and unpacks it into
`target/pdfium/`, where tests and development builds find it. CI runs it before
`cargo test`. PDFium is bound once per process (`OnceLock`): it's initialised once and its
bindings are global.

## 4. Layout passes (`layout.rs`)

Coordinates are points, origin top left. In order:

1. **Lines.** Glyphs go into rows by vertical overlap, then each row is cut into runs at gaps
   wider than 1.5 em (a gutter or a table cell). A word space is a gap over 0.18 em, or a space
   the PDF itself has (PDFium's generated spaces have no box, so they ride on the next glyph as
   `space_before`). Rotated glyphs are dropped.
2. **Running heads and folios.** In the top and bottom 10% of a page, a line is removed when
   it's a page number ("12", "- 12 -", "Page 12", "12 of 40", "xiv") or when the same line, with
   its digits blanked, appears in that margin on at least 40% of pages (and at least two).
3. **Pictures.** Images and vector drawing (paths, shadings, form objects) merge into boxes
   (within 4 pt). A box at least 40 × 30 pt and under 80% of the page is a figure, unless it
   frames over 400 characters of text and holds no image (a shaded sidebar stays text). Text
   inside a figure leaves the flow; it's in the picture.
4. **Tables without rules.** Three or more consecutive rows each with three or more runs, at
   least half of them 30 characters or shorter, aligned with the row above. A table is a
   picture.
5. **Equations.** Runs set mostly in math fonts (Computer Modern math, Symbol, STIX, Cambria
   Math and the like), merged with their neighbours, with at least three characters. Inline math
   in a sentence stays text; a lone footnote star stays text.
6. **Reading order.** A page has two columns when an empty vertical channel in its middle
   40% has text on both sides and at most 10% of lines cross it; the gutter is the middle of the
   widest such channel. Anything spanning the gutter (a title, a wide figure) ends a band; each
   band reads its left column, then its right. Otherwise the page reads top to bottom.
7. **Paragraphs.** Body size is the size most characters are set in. A paragraph ends at a
   gap over 1.4 times the usual line step, a list item (a bullet, "1.", "(a)"), an indented first
   line, a sentence that ends on a line stopping before three quarters of its column, a change of
   size, or a bold line followed by a plain one. Across a column or a page it runs on unless
   the sentence ended. A small line of three characters or fewer with no letters (a stray
   superscript) is dropped.
8. **Headings.** The three largest sizes at least 15% over the body size are h1–h3; a heading
   set over two lines that overlap side to side is one heading. A short, bold paragraph that
   doesn't end a sentence is an h3.
9. **Hyphens.** A line ending in a hyphen joins the next. The hyphen goes when the PDF marks it
   soft (U+00AD), or when the word continues in lower case and neither of these holds: the
   document uses the hyphenated form mid-line, the first half is a common compound head ("well",
   "self", "non", …), or the next word is itself hyphenated ("English-" + "to-German"). A word
   the document also uses whole is always joined. A URL broken after a slash joins without a
   space.

## 5. The EPUB (`epub.rs`)

The same package as TXT: `chapter-NNN.xhtml`, `nav.xhtml`, `style.css`, an `urn:deckle:pdf:`
identifier from the title and author. Pictures are rendered at 2× (144 dpi) and cut out as
`images/figure-NNN.png`, with alt text naming what they are and the page ("Table from page 4 of
the PDF").

- **Title and author.** The PDF's own, unless empty or tool-made ("Untitled",
  "Microsoft Word - x.docx", a file name); else the largest heading among the first blocks; else
  the file name.
- **Chapters.** A new file at each heading of the highest level that occurs at least twice,
  when every one of those headings starts in the top third of a page and leads into at least
  200 characters of text (a label in a figure isn't a chapter). Otherwise a new file about every
  200 KB, as TXT does. The contents list that level and the one under it.

## 6. Pipeline, library, CLI

- `pipeline::run` checks `is_word`, then `is_pdf`, then `is_text`. The import report's
  warnings, the quality note first, become the run's warnings, so `deckle run` prints
  "warnings: reflowed from a PDF: …" for every PDF.
- The library keeps PDFs as sources (`is_source`). Because reading a PDF's metadata means
  reflowing it, `refresh` leaves an unchanged PDF (same SHA-256) alone.
- `cargo run -p deckle-pdf --example reflow -- in.pdf out.epub` runs the importer alone and
  prints its report, for trying the passes on real documents.

## 7. Tests

`tools/pdf-fixtures/make.py` (reportlab) writes the fixtures in `fixtures/pdf/`, placing every
line by hand: `two-column.pdf` (a spanning title, two columns over two pages, a running head
and footer, a word broken across lines and a compound broken at its hyphen), `table.pdf` (an
unruled table, a Symbol-font equation, an image), `book.pdf` (two chapters with page numbers)
and `scan.pdf` (a page that's only a picture).

- Unit tests in `layout.rs` cover each pass on synthetic glyphs.
- `crates/deckle-pdf/tests/convert.rs`: column order; running heads gone; "informa-" + "tion"
  rejoined and "well-" + "known" kept; table, equation and image as pictures in place with no
  table text left; chapters and contents from headings; the PDF's title and author; a scan
  refused; a non-PDF unreadable.
- `deckle-core`'s pipeline test runs `book.pdf` through every stage with the fake Previewer and
  checks the inspected metadata and the quality note; the library test adds a PDF.

Checked by hand on two real papers (a one-column and a two-column one): reading order, sections
and display equations came out right. Figure-heavy pages (text drawn inside charts) remain the
weakest part.

## 8. In the desktop app (#133)

- **PDFium ships inside the app.** The release workflow fetches the pinned build for each target
  (`tools/fetch-pdfium.sh mac-arm64 | mac-x64 | win-x64`) and adds it through a per-OS config
  overlay: `pdfium.macos.conf.json` puts `libpdfium.dylib` in `Contents/Frameworks`, where the
  bundler signs it with the app before notarization; `pdfium.windows.conf.json` installs
  `pdfium.dll` beside `Deckle.exe`. `engine::candidates()` already looks in both places. The
  overlays are passed only by the release workflow, so `pnpm tauri dev` needs no PDFium.
- **Checked on every release and dry run.** Each built app runs `--json inspect
  fixtures/pdf/book.pdf` with no `DECKLE_PDFIUM` and the fetched copy deleted, so the PDF can
  only be read through the bundled library. The `.app` must pass `codesign --verify --deep
  --strict` (and `spctl` when signed with a Developer ID). The x86_64 macOS build's read is
  skipped when the runner has no Rosetta.
- **The app takes PDFs** on drop, from **Choose files…** and in folders. A row shows the
  reflowed title and author (`deckle_library::book_info`, off the main thread); a scan or a
  missing PDFium is an error row with the importer's reason; the quality note arrives with the
  run's warnings. `deckle inspect` (and the app binary's `inspect`) read PDFs, Word documents
  and text files through their importers.
