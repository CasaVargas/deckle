# Batch bind — design

**Status:** for [#22](https://github.com/CasaVargas/deckle/issues/22) ("Batch bind: many files →
one book"), the CLI half; the app's action follows in its own PR ·
**Date:** 2026-10-23 · **Owner:** Jonathan (CasaVargas LLC)
**Builds on:** the import stage (`2026-10-21-docx-import-design.md`,
`2026-10-21-txt-import-design.md`, `2026-10-23-pdf-reflow-design.md`) and web articles
(`2026-10-21-web-article-design.md`)

Give Deckle several files — a course packet's readings, a week of saved long reads, a report
split across documents — and get one book with a contents page, in the order given.

## 1. Decision

- **Bind EPUBs, after importing.** Every input is first made an EPUB by the importer it would
  get in `deckle run` (Word, text, PDF, web article; an EPUB as it is), then the EPUBs are bound.
  One merge step, whatever the sources.
- **Parts keep their files.** Each part's files go under `OEBPS/pNN/` with the same paths they
  had relative to their own OPF. Nothing inside a part — links, styles, image references,
  fragment ids — needs rewriting, so a part looks exactly as it did alone.
- **The bound book writes an EPUB, not a KFX.** `deckle bind … --out pack.epub`, then
  `deckle run pack.epub`. It composes with every `run` option and keeps bind small.

## 2. Out of scope

- Reordering or retitling parts from the CLI beyond argument order (the app's list will).
- A generated cover from the parts: the bound book has no cover, so `run` generates the usual
  typographic one from its title.
- Merging styles or fonts across parts.

## 3. The merge (`deckle_core::bind`)

- **Package.** EPUB 3: `mimetype` first and stored, `META-INF/container.xml`,
  `OEBPS/content.opf`, `OEBPS/nav.xhtml`, then every part's files. The identifier is
  `urn:deckle:bind:` and a hash of the title and the part titles.
- **Manifest.** Every item of every part, its id prefixed `pNN-` and its href prefixed `pNN/`,
  except each part's own nav and NCX. `cover-image` and `nav` properties are dropped: no part's
  cover stands for the book.
- **Spine.** The parts' spines one after another, `linear="no"` kept.
- **Contents.** One entry per part, linking to its first page and titled with the part's
  title (its file name when it has none). Under it, the part's own contents, top two levels,
  unless it has just one entry (an article's single heading would only repeat the part).
- **Metadata.** The given title; the given author, else the author every part shares, else
  none; the first part's language.
- **Obfuscated fonts.** A part's fonts obfuscated against its own identifier would be garbage
  under the new one, so they're left out with a warning; that text falls back to the reader's
  font.
- **Errors.** Fewer than two parts, or a part that won't inspect or has no pages, fails the
  bind and names the file.

## 4. `deckle bind`

`deckle bind <inputs…> --title T [--author A] [--out FILE]`. Inputs are taken in order: web
articles (an address, `.webloc`/`.url`, a saved `.html`) are built like `run` builds them; Word,
text and PDF files are imported into a temporary folder; EPUBs are used as they are. A `.kfx` is
refused ("a finished Kindle book can't be bound"). The output defaults to `<title>.epub` in the
current folder. The importers' warnings are printed once each (a PDF's quality note appears
once however many PDFs are bound), then the parts in order and the output with a `deckle run`
hint. `--json` gives the file, title, author, parts and warnings.

## 5. Tests

- `deckle-core` `tests/bind.rs`: the mini EPUB, an imported Word document and an imported text
  file bind into a book that inspects with no warnings, has every part's spine in order, parts
  as top-level contents with the novel's chapters nested, no cover, one nav, and every manifest
  file in the zip; one file is refused; a shared author is kept.
- `deckle-core` `tests/pipeline.rs`: a bound book runs through every stage (fake Previewer).
- `deckle-cli` `tests/bind.rs`: EPUB + Word + text + PDF bind end to end with the PDF note once,
  and the result inspects with the given title and author; a `.kfx` and a single file are
  refused.
- The pipeline's import stage now goes through `deckle_core::import`, shared with bind; its
  existing tests pass unchanged.
