# Word documents in — design

**Status:** plan settled on the issue (#17, "Native Rust DOCX to EPUB crate"); built in #17's PR ·
**Date:** 2026-10-21 · **Owner:** Jonathan (CasaVargas LLC)
**Builds on:** `2026-10-18-deckle-design.md` (the pipeline, §2 non-goals)

Drop a `.docx` and it goes to the Kindle like an EPUB: Word headings become the contents,
bold, italics, lists and tables come through, pictures stay where they were, and footnotes
become Kindle pop-up notes. There's no manual "save as EPUB" step.

## 1. Intent and why

The Word document is the most common "book" people own that isn't an EPUB: drafts, manuscripts,
long reports, a friend's novel. DOCX is a zip of XML with explicit structure (styles, numbering,
footnotes as separate parts), so a faithful conversion is cheap compared with PDF (#18), and it
reuses everything Deckle already does once there's an EPUB.

Success: `deckle run book.docx` puts the book on the Kindle with a contents list that matches the
document's Heading 1–3, and tapping a footnote number opens the note in a pop-up.

## 2. Goals, non-goals

**Goals**

1. `.docx` (and `.docm`, `.dotx`, `.dotm`) accepted anywhere `deckle run` takes an EPUB.
2. Heading 1–3 → nested contents entries, in document order. Heading 4–6 stay headings in the
   text. Each Heading 1 starts a new XHTML file (a fresh page on the Kindle).
3. Bold, italic, underline, strike, small caps, super/subscript → `b`, `i`, `u`, `s`,
   `span.sc`, `sup`, `sub`. Bullet and numbered lists → nested `ul`/`ol`. Tables → `table`
   with header rows as `th` and merged columns as `colspan`. Title, Subtitle, Quote and
   centred/right-aligned paragraphs keep their look through a small stylesheet.
4. Every PNG, JPEG, GIF or SVG in `word/media` is packaged; placed images appear at their
   original position with Word's alt text.
5. Footnotes and endnotes → `<aside epub:type="footnote|endnote">` at the end of the chapter that
   references them, linked from `<a epub:type="noteref">` (Kindle pop-up notes), with a link back.
6. Metadata from `docProps/core.xml`: title (else the first Title paragraph, else the file name),
   author, language (else the document's default language, else `en`).
7. Specific errors for what can't be converted: not a zip, a zip without `word/document.xml`, a
   password-protected document, a Word 97-2003 `.doc`, a corrupt part.

**Non-goals (this increment)**

- The desktop app's drop zone and file picker (still EPUB/KFX). This is a CLI and pipeline
  feature first; the app follows.
- Text boxes, shapes, charts, equations, comments, tracked-change deletions, EMF/WMF pictures
  (warned and skipped), floating-image layout, list numbering that continues across interruptions.
- A generated cover. A DOCX rarely has one; the typographic cover generator (#27) will give
  imported documents one.
- Old binary `.doc` (refused with a "save as .docx" message).

## 3. Where it sits

```
book.docx ──import──▶ imported.epub ──inspect──▶ normalize ──▶ Previewer ──▶ repackage ──▶ deliver
```

A new crate, `crates/deckle-docx`, with no dependency on the rest of Deckle (`zip`, `roxmltree`,
`sha2`). `deckle_core::pipeline::run` detects a Word extension and runs `deckle_docx::convert`
into the work folder as the `import` stage; every later stage reads the imported EPUB and is
unchanged. `--keep-work-dir` keeps `imported.epub` for inspection.

The conversion reads the main part from `_rels/.rels`, then its relationships for `styles`,
`numbering`, `footnotes`, `endnotes`, images and hyperlinks. A paragraph's heading level comes
from a `heading N` style or an outline level anywhere on its style chain. Inline formatting is
the paragraph style, then the character style, then direct formatting. A heading's own
formatting is not repeated as `<b>` inside the heading.

The EPUB identifier is `urn:deckle:docx:` plus a hash of title and author, so a re-sent, edited
document stays the same book (and keeps the same generated ASIN).

## 4. Verification

`fixtures/docx/deckle-mini.docx` (made by `tools/ref/make_mini_docx.py`, standard library only)
has a Title, Heading 1–4, inline styles, a nested bullet list, a numbered list, a header-row
table, two images and three footnotes. The crate's tests check the contents order and nesting,
the XHTML elements, every media file packaged and placed, notes linked both ways, metadata, and
each refusal. The pipeline and CLI tests run the fixture end to end through the fake Previewer.
Real Previewer conversion of an imported document is checked by hand, like every other stage.
