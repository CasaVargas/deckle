# Structure recovery for messy Word and PDF files — design

**Status:** built in #268, all four increments, for [#25](https://github.com/CasaVargas/deckle/issues/25) ("Smart structure recovery
for messy DOCX/PDF"), following option 1 of the issue's triage (extend the importers) ·
**Date:** 2026-11-01 · **Owner:** Jonathan (CasaVargas LLC)
**Builds on:** `2026-10-21-docx-import-design.md`, `2026-10-23-pdf-reflow-design.md`

A manuscript whose chapter titles are just bold, bigger text, with no Word heading styles, now
reaches the Kindle as one long chapter with no contents. A PDF typeset with bookmarks ignores
them. This design closes those gaps inside the importers, so a messy document still gets
chapters and a contents list you can navigate.

## 1. Decisions

- **Heuristics only, in the importers.** Recovery reads the signals each format has (style,
  font size, weight, bookmarks, position) where the importer still has them. There's no model,
  no network and no new dependency. A pass over the finished EPUB was rejected because
  font size and weight are gone by then. Model-based extras are #29's business.
- **Automatic, never overriding the author.** Recovery runs only when the document gives no
  structure of its own: a Word document with no heading styles or outline levels, a PDF with
  no bookmarks. A document that has real headings converts exactly as before. There's no flag.
  If guesses misfire on real files, an opt-out can come later.
- **Always said out loud.** Every import that guessed headings adds a line to the import
  report's warnings, which `deckle run` prints and the app shows with the run.
- **The contents come for free.** The DOCX and PDF writers already build `nav.xhtml` from h1–h3
  and split chapters at the top level, so recovering headings is the whole job.

## 2. Scope

| Gap | Where | Increment |
|-----|-------|-----------|
| Word headings faked with bold or larger text | `deckle-docx` | 1 (this PR) |
| PDF bookmarks as the chapters and contents | `deckle-pdf` | 2 |
| PDF lone bold line ranked by the space above it (today always h3) | `deckle-pdf` | 3 |
| "Chapter 7", "Part II", "Prologue" set as plain text | `deckle-docx` | 4 |

**Not in scope:** footnotes in PDFs (they stay text at the page's foot), list markup in PDFs,
scans (OCR is #19), and changing how documents with real heading styles convert.

## 3. Word documents with fake headings (increment 1)

**When:** after the body is read, the document has no top-level heading (no `heading N` style
and no outline level on any body paragraph).

**Candidates:** a body paragraph, outside tables and lists, not Title, Subtitle or a quote
style, with letters or digits, under 100 characters, that doesn't end like a sentence (`.`, `!`,
`?`, `:`, `;` or `,`, ignoring closing quotes and brackets) and doesn't contain `". "`. Its
*look* is its smallest font size and whether every visible character is bold. Sizes come from
`w:sz`: direct formatting, then the character style, then the paragraph style chain, then
`w:docDefaults`, else Word's 10 pt.

**Body text:** the font size set on the most characters across the body, the same way the PDF
importer finds it. Bold counts as a cue only when less than half the body's characters are
bold.

**Promotion:** a candidate becomes a heading when its size is at least 15% over the body's, or
when it's all bold and bold is a cue. Its level comes from ranking the distinct looks among
the promoted paragraphs: largest size first, bold before plain at the same size. The first look
is h1, the second h2, and everything after that is h3. If more than a third of the body's
paragraphs would be promoted, the cue isn't telling headings apart (a poem in bold, a
document set in one big size), so nothing is promoted.

**Output:** a promoted paragraph becomes `<hN id="hK">` with its own text and inline markup,
minus the bold that made it a heading. h1 starts a new chapter file, as Heading 1 does today,
and h1–h3 go into the contents. The report gets a warning saying how many headings were
guessed (wording in §6).

**Tests** (`crates/deckle-docx/tests/structure.rs`, documents built in memory with `zip`):

- 16 pt bold chapter titles and 12 pt bold section titles over 12 pt body text give h1 and h2,
  one chapter per h1, and a nested contents list. The warning is reported.
- Titles that are bold at body size give h1s.
- A bold sentence ending in a period, a bold list item and a bold table cell stay as they are.
- A document that is mostly bold promotes nothing.
- A document with one real Heading 1 and other bold lines keeps only the real heading and has no
  warning.
- The existing `import.rs` tests pass unchanged (deckle-mini has real headings).

## 4. PDF bookmarks (increment 2)

`engine::Document::outline` reads the top two levels of the outline (`pdfium-render`'s
bookmarks). Each bookmark's page comes from its destination, else from its go-to action, and a
bookmark with neither is left out. `layout::apply_outline` places them as pure code:

- A bookmark *names* a block when a paragraph or heading on its page reads the same, comparing
  only letters and digits in lowercase ("THE SECOND PART" and "The Second Part!" match). That
  block becomes the h1 (top level) or h2 (second level) and keeps its own text.
- Otherwise the bookmark *opens* its page: a new heading with the bookmark's title goes before
  the first block on the page.
- Bookmarks are taken in order. One that points back before the previous one, or past the last
  page, is dropped.
- The layout's own headings drop to h3, under the bookmarks.
- If fewer than two top-level bookmarks land, the outline is ignored and the layout's headings
  stand as before.

With an outline, `epub::write` starts a new file at every h1, without the layout's "opens a
page, leads into 200 characters" test (the author put the bookmark there). The contents are
the h1s with their h2s nested under them. To support this, blocks now carry the page they start
on.

Fixture: `fixtures/pdf/outlined.pdf` (`tools/pdf-fixtures/make.py`, `outlined()`). It has a
title page, a part whose title is only in its bookmark, a part and its section set as plain
text that the bookmarks name, and a 22 pt heading the bookmarks outrank. Tested in
`tests/convert.rs` and in `layout.rs`.

## 5. PDF spacing cue (increment 3)

A lone bold line (`layout.rs`, `lone_bold`) is an h2 instead of an h3 when it opens its page's
text, or when the line above it (or the bottom of a picture above it) is at least three line
steps up. An ordinary section title has a line or two of space above it and stays an h3. The
point is the mix: bold chapter titles that open pages and bold section titles inside them
used to be all h3s, which failed the chapter rule (every heading at the split level opens a
page), so the book didn't split. Now the chapter titles are h2s and do split. Tested in
`layout.rs` on hand-made pages.

## 6. Chapter words (increment 4)

In a Word document with no headings, a candidate paragraph (§3) that reads as a chapter title
is a heading even without bold or size. A chapter title is "Chapter", "Part" or "Book" followed
by a number, a number word ("Twenty-One") or a Roman numeral, or "Prologue", "Epilogue",
"Interlude", "Afterword", "Foreword" or "Introduction" alone. Either can be followed by a
separator and a title ("Part II: The Return"), and case doesn't matter. A chapter title already
set apart by size or bold keeps that look's level. One set like the body text gets a look of
its own that ranks above all the others, so plain "Chapter One" lines become h1 over bold
section titles. Chapter titles count toward the one-third limit. The warning reads *"guessed N
headings from bold or large text and chapter titles (the document uses no Word heading
styles)"*.
