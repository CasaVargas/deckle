# Footnotes → Kindle pop-up notes — design

**Status:** approved by owner (2026-10-21) for [#31](https://github.com/CasaVargas/deckle/issues/31) ("Footnotes → Kindle pop-up notes") ·
**Date:** 2026-10-21 · **Owner:** Jonathan (CasaVargas LLC)
**Builds on:** `2026-10-18-deckle-design.md` (stage 2, normalize) ·
`2026-10-21-typography-cleanup-design.md` (the opt-in normalize pass pattern) ·
`2026-10-21-docx-import-design.md` (DOCX already emits pop-up notes)

Tap a footnote number and the note opens in a pop-up, like it does in a store book, instead of
jumping to the back of the book and leaving you to find your way back.

## 1. Decision

- **An EPUB pass in normalize, opt-in, off by default:** `deckle run --popup-notes`, and the
  desktop setting *Pop-up footnotes* (off). This follows the typography precedent: it rewrites
  markup, so the passthrough promise holds unless you turn it on. Default-on can be revisited
  after use on real books.
- **Every source gets it, because it runs in normalize.** DOCX imports already emit the target
  form (§2), so for them the pass does nothing. Web articles and the Daily keep it off for now,
  like typography.
- **Two tiers of detection:**
  1. *Semantic notes:* `epub:type="noteref"`/`"footnote"`/`"endnote"`/`"rearnote"`, or the
     ARIA roles `doc-noteref`/`doc-footnote`/`doc-endnote`. These are always converted when the
     pass is on.
  2. *Heuristic notes:* the patterns Calibre, InDesign, Word "save as HTML" and Pandoc export.
     These are converted only when the pattern has a two-way link (§3). A one-way link to "the
     Notes chapter" is left as it is.
- **Notes stay where they are.** Moving every note to the end of its chapter is rejected: the
  back-of-book "Notes" chapter is part of the book, and moving it breaks other links into it
  and the publisher's layout. The Kindle shows an `aside` footnote in a pop-up whether it's in
  the same file or another one.
- Rejected: footnotes with no links at all (a superscript `1` and a "1." paragraph at the end
  of the chapter). Telling those apart from exponents and ordinary numbered lists can't be done
  reliably without guessing.

## 2. The target form

This is the form `deckle-docx` already writes, and Previewer turns it into a pop-up:

```html
… a claim<a epub:type="noteref" href="notes.xhtml#n12" id="r12">12</a> …

<aside epub:type="footnote" id="n12">
  <p><a href="ch03.xhtml#r12">12</a> The source for the claim.</p>
</aside>
```

- The noteref is an `<a>` with `epub:type="noteref"`. Any `<sup>` wrapper is kept, and so is
  the visible text.
- The note is an `<aside>` with `epub:type="footnote"`, or `endnote` when the source says so,
  and it carries the target `id`. If the target element is a `p`, `li` or `div`, it becomes the
  `aside`, keeping its attributes. If the target `id` sits on an inline element inside a block,
  the `id` moves to the new `aside` that replaces that block.
- A target `li` inside an `ol` loses its list numbering when it becomes an `aside`, so the
  number is written into the text when the list depended on it.
- The `xmlns:epub` namespace is added to `<html>` where it's missing.

## 3. Heuristic detection

A link becomes a noteref only when **all** of these are true:
1. Its href is a fragment, in the same file or another file in the spine.
2. Its visible text is 1–4 digits, a lowercase letter, a Roman numeral up to `xx`, or one of
   `* † ‡ § ¶ #`, optionally inside `[ ]` or `( )`. The link may be wrapped in or contain
   `<sup>`, or have a class matching `/note|fn|ref/i`.
3. The target block has a **link back** to the noteref's own `id`, or to the nearest ancestor
   with an `id`. Alternatively, the target block's class or its section's `id` or class matches
   `/foot.?note|end.?note|rearnote|^notes?$/i`.
4. The target block is under 2,000 characters. Longer than that isn't a note.

All of the target's noterefs must point at it. A target with two or more noterefs (a note cited
twice) is still converted. The backlink keeps pointing at the first one.

## 4. Where it sits

New module `crates/deckle-core/src/notes.rs`. It works across documents, because noterefs and
notes are often in different files. It runs inside `normalize` after typography, over every
spine document at once:

```
RunOptions.popup_notes ─▶ NormalizeOptions.popup_notes ─▶ notes::convert(&mut docs)
                                                       └▶ NormalizeReport.notes: Option<NotesReport>
```

`NotesReport { semantic, heuristic, skipped_one_way }`. The CLI shows `Notes: 84 pop-ups
(12 already marked, 72 detected), 3 one-way links left`. The edits are text splices on the
existing markup, like typography, so everything else in each file stays byte-identical.

## 5. Known limits

- Footnotes that are only images (a scanned note) are converted, but the pop-up shows the
  image, which may be large.
- A note made of several blocks (two paragraphs) is converted only when they're wrapped in one
  element that has the `id`. Otherwise just the first block becomes the `aside`.
- Kindle firmware decides the pop-up's look. Older mass-storage models show the note full
  screen. That's still better than a jump.

## 6. Acceptance

- `fixtures/notes/` is made by `tools/ref/make_notes_epubs.py`. It has a semantic EPUB 3, a
  Calibre-style EPUB 2 (`<a class="footnote-link" href="#fn1"><sup>1</sup></a>` with a `↩`
  backlink), an InDesign-style endnotes chapter, a Pandoc `section.footnotes` EPUB, and an
  EPUB with exponents and numbered lists that must not change.
- Flag off: every fixture's output is byte-identical to today's.
- Flag on:
  - the semantic and three heuristic fixtures convert every note to §2's form;
  - the noteref and backlink resolve;
  - other links into the Notes chapter still resolve;
  - the trap fixture is byte-identical;
  - running the pass twice gives the same result as once;
  - the DOCX fixture reports 0 conversions.
- By hand: a converted Calibre EPUB on a Scribe opens notes in a pop-up.

## 7. Implementation notes

Settled while building it (`crates/deckle-core/src/notes.rs`):

- **What counts.** A note is counted only when the pass changed its markup or one of its
  noterefs, so a book already in §2's form (every DOCX import) reports 0. `skipped_one_way`
  counts links, not notes, and only links whose target block passed the size limit.
- **§3.2's styling is required** for a heuristic noteref: a `<sup>` around or inside the link,
  or a `note|fn|ref` class. A bare digit link ("section 2") is never a noteref. A link with a
  note label to a semantically marked note needs no styling.
- **Anchors that link back with the number.** InDesign and Word note anchors repeat the digit
  and link back to the reference, so they look like noterefs to the body paragraph. When two
  heuristic targets each hold the other's link, the one that comes before its link (the body
  paragraph) is dropped: notes follow their references.
- **One note per block.** An inline target `id` moves to its block only when the block has no
  `id` of its own (both may be linked to). A second note claiming an already-converted block
  is left alone.
- **List numbers.** A converted `li` gets `n. ` at the start of its text (inside its first `p`
  when it opens with one) unless the text already starts with `n`; `start`, `value` and
  `reversed` are honoured.
- **Kind.** The note keeps its own `footnote`/`endnote`/`rearnote` type; otherwise it becomes
  an `endnote` when it or a section around it is marked endnotes (`epub:type` or ARIA), and a
  `footnote` otherwise.
