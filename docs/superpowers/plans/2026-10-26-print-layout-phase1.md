# Print Layout, Phase 1: the print plan and the EPUB — Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Every e-reader's EPUB gets the paper's print language:
- section letters with "a section is a page" labels (A1, B2…);
- a skybox line above the masthead;
- a folio line at the head of each section;
- the lead's jump, from the front page's opening to a "Continued from A1" jump head;
- labels in the index and on the tiles;
- pull quotes.

**Architecture:**
- A new `deckle-digest::print` module computes a `PrintPlan` from a `Gathered` and its `FrontPlan`. It holds books, the skybox, the jump (slug, opening, rest) and the pull quotes, all pure and unit-tested.
- `render.rs` builds the plan once in `render_as`, passes it to `front_page` and `section_page`, and derives the EPUB's labels with `print::epub_labels`.
- The PDF isn't touched in this phase.

**Tech Stack:** Rust (`crates/deckle-digest`), `roxmltree` in tests.

**Spec:** `docs/superpowers/specs/2026-10-26-print-layout-design.md` (§2, §3, §5; phase 1 of §9)

## Global Constraints

- `render::render` / `render_as` keep their signatures. Every output that isn't the EPUB is unchanged: the PDF, the bundle, the cover, the mail.
- Body XHTML is split by a top-level element scanner over the string, never by re-serializing a parse. Bytes outside the opening are copied as they are.
- **No jump when the opening is empty** (the body has no top-level `<p>`): the front page keeps today's teaser and the section prints the story as today. This keeps every paper with empty or paragraph-less bodies (most tests, the lottery, briefs) exactly as before.
- **The jump head carries the story's `id="a<j>"`,** not the jump title. A link lands at the top of the jump. (This refines spec §5.3; the spec is updated in Task 1.)
- **Nudge sections:** no letter, no folio, no label in the index, no skybox teaser. The nudge tests' "exactly one link" assertions must still hold.
- **Large print:** every new class has a rule that keeps it at body size or larger in `LARGE_STYLE`, so `large_print_has_nothing_under_body_size` stays green.
- **Golden pages** (weather, puzzle, shelf) change only by the folio line. Regenerate with `UPDATE_GOLDEN=1` and check that the diff is that line alone.
- **The editor's slug suggestion** (spec §2.2) is left out of this phase; the heuristic slug is always used.
- **Tests:** run focused per task (`cargo test -p deckle-digest <filter>`), and the whole `deckle-digest` suite once at the end. The cargo target is shared across worktrees (Tolaria: shared cargo target).

## Review Focus

- A lead whose body starts with `<div class="figure lead">`: the figure stays at the head of `rest`, and the opening counts only the paragraphs after it. (Task 2)
- A lead of one short paragraph: no jump. It prints whole on the front page, and its section shows a refer that still owns `id="a<j>"`. (Tasks 2, 5)
- Headlines and quotes with `&`, `<` and quotation marks: everything stays escaped and every page still parses. (Tasks 2, 3, 5)
- Every existing link (`#a<j>`, `front.xhtml#t<s>-<a>`, `#top`) still resolves after the change. (Task 5)

---

### Task 1: Spec refinements

**Files:** Modify `docs/superpowers/specs/2026-10-26-print-layout-design.md`

- [ ] §2.2: add the rule "no top-level paragraph means no jump, and the paper prints as today".
- [ ] §5.3: the jump head carries `id="a<j>"`.
- [ ] Commit with the plan: `docs(plan): print layout phase 1`.

### Task 2: `print.rs`, body scanning, slug, opening and rest

**Files:** Create `crates/deckle-digest/src/print.rs`; modify `src/lib.rs` (`pub mod print;`).

**Interfaces:**
- `fn top_level(body: &str) -> Vec<Block>`, where `Block { start, end, para: bool }` holds the byte ranges of top-level elements. Text between elements isn't a block.
- `pub fn plain(xhtml: &str) -> String`: strips the tags; decodes `&amp; &lt; &gt; &quot; &apos; &#N; &#xN;`; collapses whitespace.
- `pub fn slug(title: &str) -> String`
- `pub fn split_opening(body: &str, words: usize) -> Option<(String, String)>`: `(opening, rest)`, or `None` when the body has no top-level `<p>`.

- [ ] **Tests first:**
  - `slug("Mill Yard Will Be a Market Again") == "MILL"`
  - `slug("Council Weighs a Night Bus to the Hospital") == "COUNCIL"`
  - `slug("The Mill's Wheel") == "MILL"`
  - an all-stopword title falls back to its first word
  - `slug("2026")` is `"STORY"`
  - the split happens at the first paragraph boundary at or past 110 words
  - a leading figure stays at the head of `rest` and isn't counted
  - a subhead after the first paragraph stops the opening
  - no `<p>` gives `None`
  - `plain` decodes the entities and strips nested tags
- [ ] Implement; run `cargo test -p deckle-digest print::`.

### Task 3: Books, labels, skybox and pull quotes

**Files:** `src/print.rs`

**Interfaces:**
- `pub fn is_news(key: &str) -> bool`: `local`, `local:*`, `feeds:*`, `interests:*` and `subscribed:*`.
- `pub fn books(sections: &[Section]) -> Vec<Option<char>>` (spec §2.1).
- `pub fn epub_labels(book: &[Option<char>]) -> Vec<Option<String>>`: the front page is A1, then each section takes the next number in its letter.
- `pub struct Teaser { section, article, title: String /* the section's, upper-cased */, text: String /* the article's, shortened */ }`, and `fn skybox(g, front) -> Vec<Teaser>` (spec §2.3).
- `pub struct Pull { text: String /* plain */, after: usize /* the top-level <p> index it follows */ }`, and `pub fn pull_quote(body: &str) -> Option<Pull>` (spec §2.4).
- `pub struct Jump { section, article, slug, opening, rest }`.
- `pub struct PrintPlan { book, skybox, jump: Option<Jump>, whole: Option<(usize, usize)> /* the lead printed whole */, pulls: BTreeMap<(usize, usize), Pull> }`, and `pub fn print_plan(g: &Gathered, front: &FrontPlan) -> PrintPlan`.

- [ ] **Tests first:**
  - **books:**
    - the default plan's keys give A A A B B B C C C C;
    - a plan that alternates kinds gets a new letter at each change;
    - an empty section keeps its letter;
    - nudges get `None` and don't break a run.
  - **`epub_labels`:** A2, A3, B1, B2…
  - **skybox:**
    - skips weather, nudges, empty sections and stories already on the front;
    - shortens at a word to 48 characters with "…";
    - fewer than two candidates gives none.
  - **pull quotes:**
    - fewer than 6 paragraphs gives none;
    - quotes in paragraphs 0–1 are ignored;
    - passages of 7 and 31 words are rejected, and one of 8 is accepted;
    - `after == source + 2`;
    - a source too near the end to have a paragraph two after it gives none;
    - straight, curly and `&quot;` marks all work.
  - **`print_plan`:**
    - a long lead gets a jump;
    - a one-paragraph lead gets `whole`;
    - a lead without paragraphs gets neither;
    - pulls are taken from `rest` for the jumped story.
- [ ] Implement; run `cargo test -p deckle-digest print::`.

### Task 4: EPUB styles

**Files:** `src/render.rs` (`STYLE`, `LARGE_STYLE`)

- [ ] Add:
  - `.skybox` (centered, 0.75em, small caps, letter-spaced, a rule below)
  - `.folio` (0.75em, letter-spaced, uppercase, a rule below)
  - `p.jump` (bold italic, right-aligned)
  - `p.jumphead` (1.1em)
  - `p.jumptitle` (italic)
  - `p.pull` (centered, italic, 1.25em, rules above and below)
  - `p.refer` (italic)
- [ ] `LARGE_STYLE` lists `.skybox` and `.folio` among the classes forced to 1em.
- [ ] Run `cargo test -p deckle-digest large_print`.

### Task 5: The front page and the section pages

**Files:** `src/render.rs`; `tests/build.rs` (`mod front`)

- [ ] **Tests first** (in `tests/build.rs`, `mod front`, with a helper that gives articles paragraph bodies):
  - **front page:**
    - a skybox line with two links when two sections qualify;
    - the lead prints the opening and a `p.jump` link to `s1.xhtml#a1` that reads "Continued on A2: <SLUG> ›";
    - a one-paragraph lead prints whole, with no jump line;
    - tile metas read "<label> · meta";
    - the index meta reads "<label> · N items".
  - **section pages:**
    - a folio line "A2 · …" heads each section page, and nudge pages have none;
    - the jumped story has a jump head with `id="a1"`, a link back to `front.xhtml#jump`, its title in `p.jumptitle`, and no opening paragraphs;
    - the whole-printed lead's section shows a `p.refer` with `id="a1"` linking to `front.xhtml#t0-0`;
    - a pull quote appears after the paragraph two after its source;
    - every `href` in every file resolves to a file and an id in the EPUB.
- [ ] Update the existing assertions that change by design:
  - `tile_meta_leaves_out_the_section_name`: metas are "A3 · meta";
  - `inside_today_tells_empty_from_unavailable`: "· unavailable<".
- [ ] Implement:
  - `render_as` builds `front_plan` and `print_plan` once;
  - `front_page(g, &plan, &print, &labels)`;
  - `section_page(e, i, &plan, &print, &labels)`.
- [ ] Regenerate the golden pages with `UPDATE_GOLDEN=1`, and check with `git diff` that only folio lines were added.
- [ ] Run `cargo test -p deckle-digest --test build front`, then the golden tests and the nudge tests.

### Task 6: Verify on a real edition

- [ ] Run the whole suite: `cargo test -p deckle-digest --no-fail-fast` (PDFium-dependent failures in a fresh worktree are environmental; see the Tolaria gotcha).
- [ ] Build a sample edition offline from fixtures if one is available, else from a recent gathered edition. Unzip its EPUB, and check `front.xhtml` and one section page by eye (the markup, plus a render in the built-in browser).
- [ ] Commit per task; push; update PR #212's title and body to cover phase 1.
