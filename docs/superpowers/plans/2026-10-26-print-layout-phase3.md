# Print Layout, Phase 3: PDF columns — Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Story text in the PDF edition is set in newspaper columns where the page is wide enough: two on a reMarkable 2, a Paper Pro, an iPad or A5, one on a Paper Pro Move. The columns are justified and hyphenated, with hairline rules between them, and a run's last page is balanced.

**Architecture** (all in `crates/deckle-digest/src/pdf.rs`):
- **`Layout::lines_with(runs, width, hyphenate)`** returns lines and how each ends: hyphenated (with the whole word), forced, or the paragraph's last. It's today's greedy breaker plus `hypher` hyphenation, which is used only when asked. `lines` stays as it is for everything outside columns.
- **`Block::Columns { items }`** holds a run of left-aligned paragraphs and spaces. `story_body` (and the front page's opening) gathers a story's paragraphs into runs. Figures, tables, rules and pull quotes stay spanning, so they end a run.
- **`Layout::columns`** cuts a run into line units, packs them into `n` columns page by page, and balances the last page by bisection over the column height. It draws justified lines column by column, in reading order, with hairline rules between columns.
- **`Op::BeginActual(String)` / `Op::EndActual`** wrap the two halves of a hyphenated word. `write` turns them into `/Span` marked content with `/ActualText`.

**Spec:** `docs/superpowers/specs/2026-10-26-print-layout-design.md` §4.5, phase 3 of §9.

**Branch:** `claude/print-layout-columns`, stacked on PR #212's branch. Its PR targets that branch until #212 merges.

## Global Constraints

- **Column count:** `n = clamp(floor((W + g) / (17·body + g)), 1, 3)` with `g = 1.4 · body`. A large-print paper always gets 1. With `n == 1`, a run lays out exactly as today: ragged, no hyphenation.
- **Sections with columns:** news sections (`print::is_news`), Your Reading, From Your Shelf, the horoscope and On This Day. Weather, the puzzle and the lottery stay in one column.
- **Hyphenation:**
  - at least 2 letters before the break and 3 after;
  - only words of 5 or more letters with no digit, `@`, `/` or `.`;
  - a word's own hyphen is preferred as the break;
  - at most two hyphenated lines in a row.
- **Justification:** stretch only the spaces. Never justify a paragraph's last line or a forced break, and set a line ragged when a space would stretch past 2.5× normal.
- **Packing:**
  - a paragraph leaves at least 2 lines at a column's foot and takes at least 2 to the next;
  - a `keep` paragraph (a subhead) is never a column's last line;
  - the spacing at a column's top is dropped.
- **Unchanged:** both layout passes still agree on page count and anchors (the existing debug assertion and tests), and the front-page fit still holds.
- **`hypher`:** `default-features = false, features = ["english", "alloc"]`. Its en-US pattern notice is kept beside the crate in `HYPHENATION-NOTICE.txt`.
- **Tests:** focused runs per task. PDFium tests need `DECKLE_PDFIUM=/Volumes/DATA/CasaVargas/deckle/target/pdfium/lib`.

## Review Focus

- A paragraph wider than one column's worth of a page, or a single word longer than the column: no infinite loop, and every word is placed once. (Task 2)
- A run that ends exactly at a page's foot, and a run of one line: no empty columns or blank pages. (Task 2)
- The two passes stay identical: columns depend only on the slot widths, never on the labels. (Task 3)

---

### Task 1: The hyphenating breaker and justification

- [x] **Tests first:**
  - `lines_with` breaks a long word at a syllable, with ≥2 letters before and ≥3 after;
  - "well-known" breaks after its own hyphen;
  - words with digits, URLs and short words never break;
  - never three hyphenated lines in a row;
  - without `hyphenate`, lines are identical to `lines`;
  - a justified line fills the column to within 0.5 pt, and the last line and a ragged fallback don't.
- [x] Implement: `lines_with`, `struct End { hyphen: Option<String>, continues: bool, forced: bool, last: bool }`, and `fn justify(line, width) -> Option<f32>` (the per-gap stretch, or `None` when the line stays ragged).

### Task 2: Column runs

- [x] **Tests first:**
  - the column count per screen (2, 2, 1, 2, 2), and 1 for large print;
  - a run's text comes out in reading order;
  - a balanced last page's columns differ by at most one line;
  - no column ends with a paragraph's first line alone, or starts with its last line alone;
  - a run longer than a page snakes to the next page;
  - hairlines are drawn between columns.
- [x] Implement: `Block::Columns`, `columnize` in `story_body` and the front-page opening, and `Layout::columns` (units, `pack`, balance, draw).

### Task 3: Searchable hyphenation

- [x] **Tests first:**
  - a hyphenated word's halves are wrapped in `BeginActual(word)` … `EndActual`, then `BeginActual("")` … `EndActual`;
  - the written PDF holds `/ActualText`.
- [x] Implement: the ops, and `write`'s marked content.

### Task 4: Verify and ship

- [x] Add `hypher` and `HYPHENATION-NOTICE.txt`. Amend the spec: refers stay full-width rows.
- [x] Run the whole `deckle-digest` suite (with PDFium), `deckle-morgue`, clippy and fmt.
- [x] Build a real edition's PDF (scratch-`HOME` recipe), render the front page, a section's first page and a long story with PDFKit, and look at them.
- [x] Push `claude/print-layout-columns`. Open a PR against #212's branch with "Stacked on #212" in its body.
