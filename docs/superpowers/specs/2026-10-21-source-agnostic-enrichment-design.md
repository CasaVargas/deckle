# X-Ray and Word Wise for every source — design

**Status:** plan settled on the issue (#26, "X-Ray and Word Wise for non-EPUB sources"); built in
#26's PR · **Date:** 2026-10-21 · **Owner:** Jonathan (CasaVargas LLC)
**Builds on:** `2026-10-21-docx-import-design.md` (import before the pipeline),
`2026-10-21-web-article-design.md` (articles become EPUBs), `docs/formats/xray.md`,
`docs/formats/wordwise.md`

## 1. Decision

Enrichment is **source-agnostic**. X-Ray and Word Wise need no per-format work: every source is
turned into an EPUB before the shared stages, and the `enrich` stage reads its text from the
**KFX** Kindle Previewer produced (`deckle_kfx::text::content_runs`), never from the source file.
So whatever reaches `convert` gets the same X-Ray and Word Wise an EPUB gets.

```
.epub ─────────────────────────────┐
.docx ──import (deckle-docx)──▶ EPUB ┤
URL / .webloc / .url / .html ──▶ EPUB ┼─▶ inspect ▶ normalize ▶ convert ▶ repackage ▶ enrich ▶ deliver
long reads / Inbox (digest) ──▶ EPUB ┘
```

The rules are the ones that already hold for EPUBs, unchanged:

- Only **EBOK** books get sidecars (the cover rule or `--cde ebok`); PDOC books never do.
- X-Ray runs when `xray` is `auto`; Word Wise when it is on. A failed or offline build is a
  warning on the run, never a failed run.
- Long reads and Inbox books are built for a folder (PDOC, no sidecars) and finished at push for
  the Kindle that takes them (#72), so they get X-Ray and Word Wise there when that Kindle is EBOK.

## 2. Not in this change

- **PDF** is carried by #18 and **Markdown** by #23. Each only needs an importer that writes an
  EPUB before `inspect`, as `deckle-docx` does; X-Ray and Word Wise then come for free, with no
  change to `enrich`.
- Tuning X-Ray or Word Wise for short articles or documents.

## 3. Tests

- `crates/deckle-core/tests/pipeline.rs`
  `a_word_document_built_as_ebok_ships_xray_and_word_wise_sidecars`: the DOCX fixture with an
  EBOK target, X-Ray `auto` and Word Wise on runs `import … enrich … deliver`, reports both
  summaries and delivers `XRAY.entities.<ASIN>.asc` and `LanguageLayer.en.<ASIN>.kll` in the
  `.sdr` folder.
