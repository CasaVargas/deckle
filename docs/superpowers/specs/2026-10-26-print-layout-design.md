# Print layout: the paper laid out like a paper, on e-ink too — design

**Status:** draft for owner review · **Date:** 2026-10-26 · **Owner:** Jonathan (CasaVargas LLC)
· **Builds on:** `2026-10-22-newspaper-sections-design.md` (front page, section fronts, back
links), `2026-10-23-pdf-edition-design.md` (the PDF engine), `2026-10-23-other-readers-design.md`
(who gets which format), `2026-10-22-tomorrows-paper-design.md` (page plan, lead rule) ·
**Amends:** the PDF edition design, §3 ("No columns, justification or hyphenation") and §5
(the footer) · **Mockups:** the "Deckle Daily — Turning Pages" design canvas, e-ink row.

The Daily has a paper's parts (masthead, ears, sections, a lead), but it doesn't use the
language that makes a paper feel like one: page numbers like A1 and B3, a folio at the top of
every page, a lead that jumps inside ("See MILL, B1"), teasers above the masthead, and stories
set in justified columns. This spec adds that language where it can live: all of it in the PDF
edition, which has fixed pages, and the part that survives reflow in the EPUB every other
e-reader gets.

## 1. Intent

- **Why:** on the reMarkable, the PDF edition is one ragged column with a footer page count, and
  the Kindle, Kobo, PocketBook and Boox get a reflowed EPUB with no sense of where you are in the
  paper. Both read like documents, not like a newspaper.
- **Goals**
  - **A shared print plan** (§2): one place in `deckle-digest` decides each section's letter,
    the skybox, the jump and the pull quotes, so the PDF and the EPUB agree.
  - **Page numbers in the paper's own style** (§3): A1 is the front page, letters change between
    books of sections, and every output prints them.
  - **The PDF edition laid out like a paper** (§4): folios, PDF page labels, a skybox, a lead
    that jumps, refers with page numbers, and bodies in hyphenated, justified columns where the
    page is wide enough.
  - **The EPUB speaks the same language** (§5): a skybox line, a folio line at the head of each
    section, the jump as a link with "Continued from", page labels in the index and on the tiles,
    and pull quotes.
- **Non-goals**
  - Spreads, page edges, motion, grain or show-through. They belong to the iPad reader's paged
    Edition mode, a spec of its own.
  - Halftone photos dithered at the screen's pixels. It's worth doing, but separately.
  - Columns in the EPUB. Reflowing readers set the size and the font, and multi-column CSS is
    unreliable on Kindle (newspaper sections design §1, non-goals).
  - Knuth–Plass line breaking. Greedy breaking with hyphenation is the first step (§4.5).
  - Corrections boxes. Deckle has nothing to correct; a box with no real corrections in it
    would be decoration.
- **Anti-goals**
  - A thicker paper. The jump moves text off the front page; it doesn't print it twice.
  - New settings. Column count follows the page's width, letters follow the page plan.
  - Losing anything the PDF or the EPUB does today: outline, links, searchable text, Kindle
    reading features, the large-print stylesheet's floor (`large_print_has_nothing_under_body_size`).

## 2. The print plan

A new module, `deckle-digest::print`, computes a `PrintPlan` from a `Gathered` once per edition,
after the front plan (`front::front_plan`) and before any output renders:

```rust
pub struct PrintPlan {
    /// The book (A, B, C…) each section is in, by section index. Nudge sections have none.
    pub book: Vec<Option<char>>,
    /// Up to three teasers above the masthead.
    pub skybox: Vec<Teaser>,
    /// The front-page lead's jump, when its story is long enough to continue inside.
    pub jump: Option<Jump>,
    /// A pull quote per article, keyed by (section, article).
    pub pulls: BTreeMap<(usize, usize), String>,
}

pub struct Teaser { pub section: usize, pub article: usize, pub label: String, pub text: String }

pub struct Jump {
    pub section: usize,
    pub article: usize,
    /// "MILL": the word the jump line and the jump head share.
    pub slug: String,
    /// The story's opening paragraphs, for the front page; `rest` is everything after them.
    pub opening: String,
    pub rest: String,
}
```

Page numbers aren't in the plan. They depend on the output (§3), so each renderer numbers its
own pages from `book`.

### 2.1 Books

A paper comes in books (sections in the print sense), each with a letter. Deckle's page plan
can put sections in any order, and the order must never change, so books are runs in page-plan
order:

- A section is **news** when its kind is Local (any place), Feeds, Interests or Subscribed;
  everything else (Your Reading, Weather, From Your Shelf, the lottery, the horoscope, the
  puzzle, On This Day) is the **back of the book**.
- The front page opens book **A**. The first section joins A whatever its kind.
- A new letter starts whenever the kind changes from one section to the next.
- Nudge sections (Kindle only, after everything) get no letter and no folio.

With the default plan (`reading, weather, local, interests, feeds, shelf, lottery, horoscope,
puzzle, onthisday`) that gives **A**: front, Your Reading, Weather · **B**: Local, interests,
feeds · **C**: the rest. B is the local and news book, as it is in most papers.

An empty section keeps its letter and its place, so the letters don't shift when a feed fails.

### 2.2 The jump

The lead (from `front_plan`) jumps when its story has more after its opening:

- **Opening:** the story body's top-level `<p>` elements, in order, until they reach 110 words
  (at least one). A leading figure (`div.figure.lead`) isn't counted as part of the opening.
  The PDF moves it to the front page (§4.3); the EPUB leaves it in `rest`.
- **Rest:** everything after the opening. When it holds no paragraph text, there's no jump: the
  story prints whole on the front page, and its section lists it as a refer (§5.4).
- **Slug:** the headline's first word that isn't a stopword (a short fixed list: a, an, and,
  the, of, to, in, on, for, at, by, with, is, are, was, will, be, as, from, after, how, why,
  what, new), upper-cased, letters only. "Mill Yard Will Be a Market Again" gives MILL,
  "Council Weighs a Night Bus to the Hospital" gives COUNCIL. When the optional editor runs
  (daily LLM editor design), it may suggest a slug. A suggestion is used only if it's one word
  from the headline, so the slug always matches what the reader sees.
- Only the lead jumps. Tiles stay headlines with a page number (a refer).

### 2.3 Skybox

Up to three teasers, from sections in page-plan order:

- Weather and nudges are skipped (weather is already in the ear).
- So is any section whose first article is already the lead or a tile.
- Each teaser is the section's title in capitals plus its first article's title, shortened at
  a word to 48 characters with an ellipsis. Your Reading's teaser says "whole" after the title
  when the piece prints whole.
- Fewer than two candidates means no skybox: a lone teaser looks like a mistake.

### 2.4 Pull quotes

At most one per article, only in stories whose `rest` (or whole body, when they don't jump) has
at least six paragraphs:

- Candidates are sentences, from the third paragraph on, that contain a passage in quotation
  marks (“…” or "…") of 8 to 30 words.
- The pull quote is that quoted passage, the first one found, with its marks.
- It's printed after the paragraph two after the one it came from, so it never sits beside its
  own sentence.
- Nothing is made up or reworded. A story with no candidate gets no pull quote.

## 3. Page numbers

- **PDF:** real pages. A book's first page is number 1, and each book starts on a new page (it
  does already, since every section does). The front page is A1. The labels come from layout
  (§4.2).
- **EPUB:** a reflowed book has no pages, so **a section is a page**: the front page is A1, and
  each section takes the next number in its book. With the default plan, Local is B1, and the
  first interest is B2 when there is one.
- The two outputs can number the same edition differently: Local can be B1 in both while
  Puzzles is C5 in the EPUB and C9 in the PDF. Each output is internally consistent, and nobody
  reads both editions of one day.

## 4. The PDF edition

### 4.1 Folios

- **Every page but the front** carries a folio at the top: its label in bold at the left, the
  paper's name in the middle, the long date at the right, at the meta size, over a hairline.
- The current footer (`<paper> · <date> · <n>`, PDF edition design §5) goes. The page's top
  margin grows by one meta line, and the bottom margin loses the footer's room.
- **A section's first page** starts with a section flag: its title at the section size on the
  left, its kicker in italics on the right when there is one, over a double rule. This replaces
  today's title over a single rule.

### 4.2 Page labels and two passes

- The PDF gets a `/PageLabels` number tree (`Catalog::page_labels` in `pdf-writer` 0.15): one
  entry at each book's first page, prefix the letter, decimal style, starting at 1. A reader that shows page labels then says "B3" where it
  said "7", and "Go to page" can take the paper's number (spike 2).
- The front page has to print labels of later pages (the jump line, the refers, the index), and
  those are only known after layout. So layout runs **twice**:
  1. a first pass with every label set to a placeholder;
  2. a second pass with the real labels from the first.
- Every label on the front page sits in a **slot sized for the widest label** the paper could
  print, measured in the label face, and is right-aligned in it. Pass 2 then can't move a line
  break or a page break, so its labels are the labels pass 1 found. A debug assertion checks
  that both passes produce the same page count and anchors.

### 4.3 The front page

Top to bottom:

1. **Skybox:** the teasers (§2.3) in a row of equal cells divided by hairlines, each ending with
   its page label in italics. The whole cell is a link to the section.
2. **Ears and masthead,** as today.
3. **Dateline band:** a heavy rule above, the long date at the left, "Final edition" in the
   middle, "<N> pages" at the right (the page count from pass 1), and a double rule below.
4. **The lead:**
   - Its section in small capitals as a kicker, the headline at 1.6× the headline size, and its
     meta line.
   - The lead photo when it has one: full width, at most 30% of the page's height.
   - The **opening** in columns (§4.5), with the drop cap the reader uses.
   - The **jump line**, right-aligned and bold italic: "See MILL, B1 ›". It's a link to the
     jump head.
   - With no jump, the whole story is set here instead.
5. **Refers:** the tiles in columns, each its headline in bold, its section in small capitals
   and its page label, linked to the story's anchor (today they link to the section).
6. **Inside:** the index in a box: a heavy rule above, a hairline below, each section's title,
   dot leaders, its page label, and its item count in italics. Each row is a link.

The lead and its photo should finish above the middle of the page (the fold). That's a target
for the sizes above, checked by a test on the sample edition; it isn't enforced at layout.

### 4.4 The jump head

The lead's story, in its section, starts with a jump head instead of its headline:

- The slug in bold capitals, then "Continued from A1" in italics, on one line. The whole line
  links back to the jump line.
- The headline below it, in italics at body size, so someone reading the section from its top
  still knows the story.
- Then `rest`.

The outline entry for the story keeps the full headline and points at the jump head.

### 4.5 Columns, justification and hyphenation

This reverses PDF edition design §3. A 6- to 7-inch page holds two newspaper columns: about 19
ems each, close to a real paper's column.

- **How many:** `n = clamp(floor((W + g) / (17·body + g)), 1, 3)`, where `W` is the text width
  and `g` the gutter (1.4 × body).
  - Two columns on `remarkable-2`, `remarkable-paper-pro`, `ipad` and `a5`.
  - One on `remarkable-paper-pro-move`.
  - A large-print paper always gets one column.
- **What goes in columns:** the paragraphs, lists and block quotes of story bodies in news
  sections, Your Reading, From Your Shelf, the horoscope and On This Day. Weather, the puzzle
  and the lottery stay in one column; their tables are the point.
- **What spans the page:** section flags, headlines with their meta, jump heads, figures,
  tables and pull quotes.
- **Column runs:** the column items between two spanning items form a run.
  - A run fills column 1 to the page's foot, then column 2, and so on, then a new page.
  - On its last page the run is **balanced**: the shortest column height that fits it, found
    by bisection over the run's line heights.
  - A headline is never the last line of a column (the `keep` rule, per column). A paragraph
    leaves at least two lines at the foot of a column and takes at least two to the next.
- **Rules:** a hairline between columns, from the run's top to its balanced bottom.
- **Justification:** every line in a column is justified by stretching word spaces, except:
  - the last line of a paragraph and a line ending at a forced break;
  - a line whose spaces would stretch past 2.5× normal, which is set ragged-right instead.
  
  Lines outside columns stay as they are today.
- **Hyphenation**, in the greedy breaker, only for column lines:
  - When a word doesn't fit, try its hyphenation points from the right, and take the longest
    prefix that fits with its hyphen.
  - At least 2 letters before the break and 3 after. Only words of 5 or more letters that
    contain no digit, `@`, `/` or `.`.
  - A word with a hyphen of its own breaks there first.
  - At most two hyphenated lines in a row.
  - Patterns come from **`hypher`** (MIT/Apache-2.0, from the Typst project like `pdf-writer`),
    built with its English patterns only (spike 1).
- **Searchable text:** a hyphenated word is two glyph runs with a hyphen glyph. Each half is
  wrapped in marked content: the first half's `/ActualText` holds the whole word, the second
  half's is empty, so search and read-back see one word. `pdf-writer` 0.15 writes this through
  `MarkedContent::actual_text`.
- **Order:** columns are written in reading order (column 1's lines, then column 2's), so text
  extraction and read-back stay in order.

### 4.6 Pull quotes

A pull quote spans the page between hairlines above and below. It's centered, in italics at 1.5×
body, with no attribution (the sentence it came from is two paragraphs above).

## 5. The EPUB

The EPUB keeps one reflowing column and its current files (`front.xhtml`, `s<n>.xhtml`). It
gains the print language, in markup and CSS that Kindle (KFX through Previewer, and PDOC), Kobo
(KEPUB), PocketBook and Boox all render (spike 3). Every new class gets a large-print rule, so
`large_print_has_nothing_under_body_size` still holds.

### 5.1 Front page

- **Skybox:** a `p.skybox` line above the masthead. Each teaser is shortened to the section
  title and its label ("PUZZLES C5 · YOUR READING A2"), and each is a link.
- **The lead:**
  - The headline links to the story as today (`s<n>.xhtml#a<j>`), which is now its jump head.
  - Under it, the opening's paragraphs replace today's one-paragraph teaser.
  - Then a `p.jump` line: "Continued on B1: MILL ›", as a link.
  - Without a jump, the whole story prints here.
- **Tiles:** each tile's meta line gains its page label ("B1 · 3 hours ago").
- **Inside today:** each section's line shows its label before its item count ("B1 · 4 items").

### 5.2 Folio line

Each section file starts with a `p.folio` line above the section head: "**B1** · Local · The
Deckle Daily · Mon., Oct. 26", in small capitals, over a hairline. A section already starts on
a fresh screen, so the folio sits where a page's folio would.

### 5.3 Jump head

The lead's story in its section starts with a jump head instead of its `h2.headline`:

- `p.jumphead` with the slug in bold, then "Continued from A1" as a link to the jump line;
- the headline in italics, as `p.jumptitle`, carrying the article's `id="a<j>"`, so every
  existing link to the story still lands;
- then `rest`.

The back links at the end ("↩ Local · ↩ Front page") stay as they are.

### 5.4 Refers for stories printed whole on the front

When the lead has no jump, its section doesn't print it again. The section's "In this section"
list, and the story's place in the section, show a refer instead: "Mill Yard Will Be a Market
Again — on A1", linked to the front page's lead. Its `id="a<j>"` stays on the refer, so links
still land.

### 5.5 Pull quotes

`p.pull`: centered, italic, about 1.25em, with a hairline above and below, at the place §2.4
picks. It's plain text in a paragraph, so a device reading aloud reads it where it stands.

## 6. Where the code goes

- `print.rs` (new): `PrintPlan`, `print_plan(g, &front_plan)`, books, slug, opening and rest,
  skybox, pull quotes.
- `render.rs`:
  - `front_page` and `section_page` take the plan;
  - the EPUB's section numbers are computed here from `book`;
  - `STYLE` and `LARGE_STYLE` gain `skybox`, `folio`, `jump`, `jumphead`, `jumptitle` and
    `pull`.
- `pdf.rs`:
  - `document` takes the plan and emits spanning blocks and column runs (a new `Block::Columns`
    holding its paragraphs);
  - `Layout` gains column state, balancing, justification and the hyphenating breaker;
  - `render` runs layout twice;
  - `write` drops the footer, draws folios, and writes `/PageLabels` and the `/ActualText`
    spans.
- `Cargo.toml`: `hypher` (0.1.8, MIT OR Apache-2.0). It enables every language by default;
  build it with default features off and English only (spike 1 confirms the feature name).
- The desktop's "Tomorrow's paper, so far" and the edition bundle are unchanged. The iPad
  Edition-mode spec decides whether the bundle carries the plan's books, so the reader can
  print the same letters.

## 7. Spikes before code

1. **`hypher`:** the license of its English patterns (the crate itself is MIT OR Apache-2.0),
   the feature that builds English alone, the size it adds to the binary, and its API for the
   minimum letters on each side of a break. Already checked: `pdf-writer` 0.15 has page labels
   and marked-content `/ActualText`.
2. **Page labels on devices:**
   - **reMarkable 2:** does it show the label or the index?
   - **KOReader:** it should show the label; confirm.
   - **Boox's NeoReader, Apple Books and Preview through the folder target:** check each.
   
   A device that ignores labels still works; the folio carries the number.
3. **EPUB rendering:** the new classes on the following, at default and largest type:
   - a Kindle Paperwhite (KFX and PDOC);
   - a Kobo (after `kepub::convert`);
   - PocketBook;
   - Boox.
   
   In particular, check that links into `p.jumptitle` ids land, and that a centered `p.pull`
   with borders survives KEPUB spans.
4. **Front page fit:** on the 2026-10-25 sample edition at `remarkable-2`, does the lead with
   its photo end above the middle of the page? If not, lower the photo cap or the opening's
   110 words before shipping.

## 8. Tests the build must add

- **Books:**
  - the default plan gives A (front, reading, weather), B (news), C (the rest);
  - a plan that alternates kinds gets a new letter at each change;
  - an empty section keeps its letter;
  - nudges get none.
- **Slug:** the two examples in §2.2; a headline made only of stopwords falls back to its first
  word; an editor suggestion that isn't a headline word is ignored.
- **Opening and rest:**
  - the split is at a paragraph boundary at or past 110 words;
  - a leading figure is excluded from the count;
  - a story that's all opening has no jump.
- **Skybox:** skips weather, nudges and anything already on the front; shortens at a word;
  fewer than two candidates gives none.
- **Pull quotes:** only from paragraph three on, only 8–30 words inside quotation marks, printed
  two paragraphs after their source, never in a story under six paragraphs.
- **EPUB:**
  - section numbers follow the books;
  - the front's jump link and the jump head's back link resolve inside the EPUB;
  - every existing `#a<j>` link still resolves;
  - the large-print floor holds.
- **PDF:**
  - the column count per screen, as in §4.5;
  - justified lines fill the column width to within 0.5 pt, apart from the exceptions;
  - no hyphen with fewer than 2 or 3 letters on either side;
  - never three hyphenated lines in a row;
  - a balanced run's columns differ by at most one line;
  - `/PageLabels` starts a new prefix at each book;
  - the two layout passes produce the same page count and anchors;
  - the text extracted from a page with columns comes out in reading order.

## 9. Phasing

1. **The print plan and the EPUB** (§2, §3, §5). Small, and every e-reader gets it at once.
2. **PDF furniture** (§4.1–§4.4, §4.6): folios, page labels, the front page, the jump, pull
   quotes, and the two passes, still in one column.
3. **PDF columns** (§4.5): columns, balancing, justification and hyphenation. This is the
   largest piece of work and changes how every PDF page looks, so it ships last and alone.

## 10. Decisions made in this design (owner may overrule)

- **Books are runs in page-plan order,** not fixed letters per kind, so the reading order never
  changes. Alternative: one letter (A1–A18) for the whole paper, which is simpler and less like
  a paper.
- **A section is a page in the EPUB.** It's the only stable unit a reflowing reader has.
- **Only the lead jumps,** and the opening is word-counted, not fitted to space, so both
  outputs split the story in the same place.
- **Columns come from the page's width, with no setting.** A Move gets one column, a
  reMarkable 2 two.
- **`hypher` over the `hyphenation` crate:** no build step, embedded patterns, the same
  family as `pdf-writer`.
- **No corrections box and no invented datelines:** Deckle doesn't have the data, and the paper
  never prints what it doesn't know.
