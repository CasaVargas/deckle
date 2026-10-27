# Print Layout, Phase 2: PDF furniture — Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** The PDF edition gets the paper's furniture, still in one column:
- folios in place of the footer, and `/PageLabels` (A1, B3…);
- a front page with a skybox, a dateline band with the page count, the lead's opening and photo, a jump line, refers with page labels, and a boxed Inside index with dot leaders;
- a section flag on each section's first page;
- a jump head where the lead continues, and pull quotes.

**Architecture:**
- `pdf.rs` gains `Block::Fields`: a row of cells. Each cell holds wrapped runs and can end in a right-aligned **slot**: a page label, the page count, or fixed text. A slot's width is fixed before layout (the widest label it could print), so the slot's text never changes a line or page break.
- `render` lays the document out twice:
  1. pass 1 with placeholder slots;
  2. labels computed from pass 1's anchors (`page_labels`);
  3. pass 2 fills the slots, and a debug assertion checks that both passes match.
- `write` draws folios on every page but the front, and writes the `/PageLabels` number tree.
- `document` takes the `PrintPlan` (`print::print_plan` on the PDF's own `Gathered`, without nudges). It reserves an anchor for every story, the lead and the jump line before laying out the front page.
- One column throughout; columns are phase 3.

**Spec:** `docs/superpowers/specs/2026-10-26-print-layout-design.md` §4.1–§4.4 and §4.6, phase 2 of §9.

## Global Constraints

- **Unchanged:** the EPUB, the outline (one entry per front page, section and story), the links, embedded fonts, ToUnicode, `Screen` sizes, and the photo modes.
- **Furniture text** is drawn in the paper's own fonts, so it's searchable like the rest.
- **A label wider than its slot** (a book of 100+ pages) is drawn right-aligned anyway. It may overlap leaders, but never changes layout.
- **The lead photo** moves from the continuation to the front page, capped at 30% of the page's height (other photos keep 45%). It's never printed twice.
- **Kickers:** a section kicker of more than 40 characters can't sit beside the flag's title, so it gets its own line under the flag.
- **Tests:**
  - Tests without PDFium inspect `Layout` ops.
  - PDFium read-back tests run with `DECKLE_PDFIUM=/Volumes/DATA/CasaVargas/deckle/target/pdfium/lib`.
  - Run focused tests per task (the cargo target is shared), and the whole suite at the end.

## Review Focus

- Both passes produce identical page counts and anchor positions, on every `Screen` and in a large-print paper. (Task 3)
- `/PageLabels` starts a new prefix only where the book changes, and a book's first page is its section's first page. (Task 2)
- A lead with no jump (printed whole), and a lead with no paragraphs (the teaser), both lay out without a jump line or a dangling anchor. (Task 4)
- Every link annotation still points at a laid-out anchor. (Task 4)

---

### Task 1: `print::take_lead_figure`

**Files:** `crates/deckle-digest/src/print.rs`

- [x] **Test first:** a rest that starts with `<div class="figure lead">…</div>` returns `(Some(figure), rest_without_it)`, and any other rest returns `(None, rest)`.
- [x] Implement with `top_level`.

### Task 2: Slots, fields, folios and page labels

**Files:** `crates/deckle-digest/src/pdf.rs`

- [x] **Types:**
  - `Align::Right`
  - `enum Slot { Label(usize), Pages, Text(String) }`
  - `struct Tail { prefix, slot, suffix, face, size }`
  - `struct Field { runs, align, tail: Option<Tail>, leader: bool, link: Option<usize> }`
  - `Block::Fields { fields: Vec<Field>, divided: bool, after: f32 }`
  - `Block::Image` gains `cap: f32`
- [x] **`Layout`:**
  - gains `labels: Vec<Option<String>>` (per anchor), `pages_total: Option<usize>` and `widest_label: String`;
  - `fn tail_width(&Tail)` and `fn tail_text(&Tail)`;
  - `fn fields(...)` lays the cells out side by side, with hairlines between divided cells, dot leaders, and a link box per cell.
- [x] **Margins:** `Layout::new` moves the footer's room to the top for the folio. Page 0 starts at the margin.
- [x] **`fn page_labels(book_of_section, section_pages, pages) -> (Vec<(usize, char)> /* range starts */, Vec<String> /* label per page */)`.**
- [x] **`write`** takes a `Furniture { paper, date, labels, ranges }` instead of the footer string. It draws folios on pages 1 and up (label bold left, paper centered, date right, a hairline below), and writes `/PageLabels`.
- [x] **Tests first:**
  - `page_labels` on [A, A, B, C] sections across pages;
  - `fields` puts a slot's text right-aligned at the cell's edge;
  - a field's line breaks don't depend on its slot's text (same ops with "A2" and with "B88");
  - leaders are drawn only when asked;
  - the written PDF contains `/PageLabels` and a prefix per book.

### Task 3: The document, and two passes

**Files:** `crates/deckle-digest/src/pdf.rs`

- [x] **`document(g, t, photos, print)`:**
  - reserves anchors for sections, stories, the front's lead and the jump line;
  - builds the front page (spec §4.3), the section flags (§4.1), and the jump head, refer and pull quotes (§4.4, §4.6).
- [x] **`render`:** pass 1, then labels, then pass 2. `debug_assert_eq!` on the page count and anchors.
- [x] **Tests first** (unit, without PDFium; `Layout` ops of a small `Gathered`):
  - the front page draws the skybox teasers with their labels;
  - "See LOCAL, A2 ›" is drawn, linked to the story's anchor;
  - refers carry labels;
  - Inside rows have leaders and labels;
  - the section page starts with the jump head ("Continued from A1") and doesn't repeat the opening;
  - a pull quote is drawn between rules;
  - a whole-printed lead gives a refer in its section;
  - pass 1 and pass 2 have the same page count on every `Screen`.

### Task 4: Read-back and a real edition

- [x] **PDFium tests in `tests/build.rs`:**
  - page 2's text starts with its folio label;
  - the front page's text has "See", the slug and a label;
  - the outline count is unchanged;
  - `/PageLabels` labels come back: PDFium's `label()` on a section's first page gives its letter.
- [x] Run the whole suite, clippy and fmt.
- [x] Build a real edition's PDF with the scratch-`HOME` recipe (Tolaria), render pages to PNG with PDFium, and look at the front page, a section's first page and an inside page.
- [x] Commit per task, push, and update PR #212.
