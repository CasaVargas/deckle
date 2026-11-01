# Write-on puzzle books — design

**Status:** draft for owner review · **Date:** 2026-11-01 · **Owner:** Jonathan (CasaVargas LLC)
· **Builds on:** `2026-10-22-puzzle-page-design.md` (the mini, word list, blocklist),
`2026-10-31-word-search-design.md` (the word search generator), `2026-10-23-pdf-edition-design.md`
(the PDF writer and fonts), #29's LLM runtime decision (one LLM setting, no bundled model).

Calls marked *(builder's call)* are mine, not the owner's; review them.

Deckle makes **puzzle books you solve in ink on a Kindle Scribe**: a book of word searches, a
book of crosswords, or a book of sudoku. Each book is a PDF with one puzzle per page and the
answers at the back. You can give a book a theme ("technology", "current events"). With an LLM
set up, the LLM supplies the theme words and clues; the grids are always made by Deckle's own
algorithms.

## 1. Intent and why

The Kindle Store sells "Write-on" puzzle books. The owner wants their own, made to order: any
kind, any size, any theme, delivered over USB with no store and no cloud.

**What a device test settled (2026-11-01, Kindle Scribe 2024, Tolaria
`deckle/deckle-scribe-write-on-research.md`):**

| Sideloaded over USB/MTP | Pen ink on the page? |
|---|---|
| Fixed-layout `.azw3` (EBOK, PDOC, comic path) | No |
| **PDF** | **Yes**. It stays after a page turn and after closing and reopening the book. |

So the output is a **PDF**, delivered with Deckle's own MTP/mass-storage transport. Not checked
yet: whether the eraser removes one letter at a time. Nothing in this design depends on it.

**Success:** the owner runs one command (`deckle puzzles search --count 10 --to device`), and a
minute later the Scribe's library has "Word Search" with a title page, ten puzzles and the
answers. They can circle words in ink, close the book, and find the ink still there when they
come back.

## 2. Goals, non-goals, anti-goals

**Goals (the owner's)**

1. **Several kinds of book**, one kind per book: word search, crossword, sudoku.
2. **Grid size is a setting** for word searches and crosswords.
3. **Themes you choose**, such as "technology" or "current events", written by the LLM when one
   is set up.
4. **Answers at the back.**

**Goals (mine)**

5. **Works offline with no LLM.** Without a theme, a book uses Deckle's built-in word list and
   clues. `--words <file>` gives a theme from your own list with no LLM at all *(builder's call)*.
6. **Reproducible.** The same options, `--seed` and `--date` make the same book, byte for
   byte, on any machine. That's needed for tests and for "send me that one again". LLM-themed
   books are the exception: the words come from a fresh request each time, so the output says
   such a book can't be remade exactly *(builder's call)*.
7. **Sized for the Scribe.** The default page is the Scribe's screen (§4.1). Other screens come
   from the Daily's list.

**Non-goals**

- **Recipe books.** The owner raised them. The Kitchen Book (`deckle cookbook`) already makes a
  cookbook from saved recipes. A write-on PDF edition of it, with room for notes, is a separate,
  later project. LLM-invented recipes are out: an untested recipe can be wrong in ways that
  matter more than a bad clue.
- **Reading the ink back** or scoring solves. The ink stays on the device (#99).
- **Fixed-layout AZW3 or EPUB output.** The test above shows it doesn't take ink.
- **Remembering earlier books** to avoid repeats across books. Each book is made fresh. The
  Daily's 60-day memory (`puzzles.json`) is not used or touched.
- **App UI.** The CLI comes first. A Puzzle Books screen in the app is a later increment and
  gets its own short design.
- **Sudoku variants** (6×6, 16×16, killer). Classic 9×9 only.

**Anti-goals (a book that does any of these has failed, even if it builds)**

- **A wrong puzzle.** That means a word-search word that isn't in the grid or reads twice, a
  crossword entry that isn't a word or has no clue, or a sudoku with zero or two solutions.
  Every generator checks its own output before it goes on a page.
- **Links or tappable areas on puzzle pages.** A pen stroke on a link would jump away
  mid-solve. Navigation uses the PDF outline (the Scribe's table of contents) only.
- **The LLM making grids or answers it can't back.** The LLM only proposes words and writes
  clues. Deckle checks every word (letters, length, blocklist) and places it itself.
- **Needing a network or an account.** The LLM is optional, so a book always builds offline.
- **Changing the Daily.** The Daily's puzzles, word list and output stay byte-identical.

## 3. The books

### 3.1 Shape

Every book has the same three parts:

1. **Title page:** the title (default: the kind, e.g. "Word Search", or "Technology Word
   Search" when themed), the theme if any, the size and difficulty, the puzzle count, the date
   made, and "Made with Deckle". No cover image in v1 *(builder's call)*.
2. **Puzzles:** one per page, numbered 1…N. Each page has the heading "No. 3", a subtitle (the
   LLM's sub-theme when there is one, e.g. "Programming languages"), the grid, then the word
   list or the clues, then "Answers on page 14" as plain text (not a link).
3. **Answers:** several solved grids per page, in puzzle order, each labelled "No. 3"
   (§4.4).

The PDF outline has "Title", "No. 1" … "No. N" and "Answers". The document title is the book
title, so the Scribe's library shows it.

Defaults: `--count 10`, at least 1 and at most 200 *(builder's call)*.

### 3.2 Word search

- **Sizes:** `--size` from 8 to 20, default 12 *(builder's call)*. The generator takes the size
  as a value; today's code has a fixed `SIZE = 9` const, so the Daily keeps passing 9 (§5.1).
- **Words per puzzle:** scales with the area, `clamp(size² / 9, 8, 40)` words (9×9 → 9,
  12×12 → 16, 15×15 → 25, 20×20 → 40) *(builder's call)*. A puzzle with fewer than 80% of that
  count placed is retried. After the attempts cap it has as many as placed, but never fewer
  than 8.
- **Word lengths:** 3 letters up to `size`. User and LLM words can be longer than the built-in
  list's (which stops at 5 letters), but no longer than the grid.
- **Difficulty** (`--difficulty`, default `medium`) *(builder's call)*:
  - `easy`: across and down only.
  - `medium`: across, down and the two left-to-right diagonals. This is the Daily's rule today.
  - `hard`: all eight directions, including backwards.
- **Word list:** below the grid in columns, alphabetical, upper case.
- **Rules kept from the Daily's generator:**
  - No word inside another word.
  - No hidden word reads twice (in any of the directions the difficulty allows).
  - The filler letters never spell a blocklisted word in any of the 8 directions.
- **Rule that changes for books:**
  - User-supplied words (`--words`) are taken as given, without the built-in list or blocklist
    check. You chose them. They're still reduced to the letters A–Z.
  - Built-in and LLM words still go through the blocklist.

### 3.3 Crossword (increment 2)

*Updated while building increment 2. The first draft's open grids, with up to 18% black squares
and long entries, almost never filled from a list of everyday words (measured: 0 of 8 patterns
at sizes 6–15). The settings below are what the measurements supported.*

- **Sizes:** `--size` from 5 to 15, default 11 *(builder's call)*.
  - A 5×5 uses the Daily's mini templates.
  - Larger sizes get generated block patterns:
    - 180° rotational symmetry.
    - Every white square is in an across entry and a down entry of 3 letters or more.
    - All white squares are connected.
    - **About 20% black squares, and no entry longer than 6 letters** *(builder's call)*.
      These fill 4 to 7 patterns in 8 at every size. With longer entries or fewer black
      squares, the fill rate falls toward zero.
- **Fill:** a constraint search over the word list.
  - It picks the most-constrained entry first, using a letter-at-position bitset index.
  - Words are tried in the order of how much room they leave their crossings, using the first
    48 that fit.
  - It counts search steps, never time, so a busy machine makes the same puzzle: up to 20k
    placements per pattern, and up to 24 patterns.
  - Theme words (increment 4) will be placed first.
- **Words and clues:** a new embedded list, `words-long.tsv.gz`, built with
  `deckle-wordwise-build crossword`. It has 45,271 entries of 3–15 letters, 826 KB gzipped, each
  with its zipf and a clue.
  - The cutoff is zipf 2.0 at every length. A big grid needs more short words than common
    ones provide.
  - **It includes plurals and verb forms** ("cards", "baked", "baking"). Each takes the clue of
    the sense it inflects, marked: "To cook in an oven (past tense)". A plural also drops the
    singular's article.
  - The Daily keeps `words.tsv.gz` as it is, so its puzzles don't move.
  - A clue never contains its answer.
  - **Known quirk:** a clue is Wiktionary's first everyday sense, which is sometimes an odd
    one. For example, PRIEST gets "a blunt tool… for stunning fish". The Daily's list shares
    this. LLM clues (increment 4) can do better.
- **Page:** the grid at the top with numbered squares, then Across and Down clues in two
  columns.
  - Clues that don't fit carry on over the page, under "No. N (continued)". A 15×15 takes
    three pages.
  - A clue is never split between columns, and a heading is never separated from its first
    clue.
  - Continuation pages have no outline entry.
- **Difficulty** sets how strongly the fill prefers everyday words *(builder's call)*:
  - Words are tried commonest first, each moved down the order by up to a random 0.5 zipf for
    `easy`, 1.5 for `medium` and 4.0 for `hard`.
  - Filtering the list instead (zipf ≥ 3.5 for `easy`) left too few words to fill.

### 3.4 Sudoku (increment 3)

- 9×9 classic, one per page, with cells big enough for small pencil-mark digits (§4.2).
- **Generation:**
  1. Make a random full grid from the seed.
  2. Remove givens in random order while the solution stays unique. Uniqueness is checked by
     a solver that counts solutions up to 2.
  3. Grade the result by the techniques a logical solver needs:
     - `easy`: naked and hidden singles only.
     - `medium`: adds naked/hidden pairs and pointing/claiming.
     - `hard`: needs more than that, but is still unique.
  4. Try again until the grade matches the one asked for.
- Answers: solved grids, 6 per page.
- `--size` is refused for sudoku.

### 3.5 Themes

Theme words come from these sources, in order:

| Source | Option | Needs |
|---|---|---|
| Your own list | `--words <file>` (one word per line; `#` comments) | nothing |
| The LLM | `--theme "<text>"` | Deckle's LLM setting (Anthropic, OpenAI, or an OpenAI-compatible server such as Ollama) |
| Recent headlines | `--theme current-events` | the Daily's stored headlines; the LLM is optional |

- **LLM (increment 4).**
  - Deckle makes one request per book, through `deckle_xray::llm::complete` with the user's
    existing provider, model and key (#29: no new key field).
  - It asks for JSON: for each puzzle, a sub-theme title and words, each with a short clue.
  - Deckle validates every word:
    - letters only after folding;
    - length within the kind's bounds;
    - not blocklisted;
    - no duplicates within a puzzle.

    It drops a clue that contains its answer.
  - For crosswords, the LLM's words are seeds and its clues are used for them. Other entries
    keep the list's clues. The LLM may also rewrite list clues to fit the theme
    *(builder's call: a later flag, not v1 of increment 4)*.
  - If the LLM is unavailable or returns too few good words, the book still builds with the
    words it got plus built-in words, and Deckle prints a warning. It never fails silently.
  - Cost: a few thousand tokens per book. On Haiku that's well under a cent, and on Ollama it's
    free.
- **Current events.** Words come from the headlines the Daily's puzzle store already keeps
  (`puzzles.json` `days[].headlines`), harvested with `puzzle::theme::harvest`/`pick` rules.
  - With an LLM, the LLM writes clues from the headline. Without one, the clue is the Daily's
    "In the news: <headline>" form (`theme::clue`).
- **Without an LLM, `--theme <text>`** (other than `current-events`) is an error that says how
  to set the LLM up or use `--words`.
- **Sudoku** has no words. A theme only names the book.

## 4. Rendering

### 4.1 Pages

- `--screen`, default `kindle-scribe`: 1860×2480 px at 300 ppi, so 446.4×595.2 pt. The same
  size works for the Scribe Colorsoft.
- The other values are the Daily's `pdf::Screen` list (`remarkable-2`, `a5`, …).
- The `kindle-scribe` value lives with the puzzle books; it is not added to the Daily's settings
  in this work *(builder's call)*.
- Margins are 28 pt. Nothing is drawn within 28 pt of the edges, where the Scribe's
  page-turn taps and toolbar sit.

### 4.2 Sizes for a pen

| Kind | Smallest cell | Why |
|---|---|---|
| Word search | 18 pt (6.4 mm) | circling a letter |
| Crossword | 22 pt (7.8 mm) | writing a letter |
| Sudoku | 40 pt (14 mm) | a digit plus pencil marks |

- The grid is as large as the page allows after the heading and the word list or clues. The
  grid shrinks first, down to the minimum. Then the word list goes to more columns.
- If the size still doesn't fit, the command refuses with the largest size that fits on that
  screen. It never prints a grid too small to write in.
- Letters in a word search grid are 55% of the cell in the regular face. Crossword numbers are
  25% of the cell, top-left.

### 4.3 Ink

- Black on white only, with no grays on puzzle pages. E-ink shows grays as dither, and gray
  lines compete with the ink.
- Line weights:
  - cells: 0.75 pt;
  - box borders (sudoku) and grid outline: 2 pt;
  - crossword black squares: solid fill.
- All text is real text in the embedded font (Type0/CID subset + ToUnicode, as the Daily's PDF
  does), so search and copy work.
- The PDF writes no link annotations and no form fields.

### 4.4 Answers

- **Word search:** the solved grid at reduced size. Each hidden word is outlined with a rounded
  capsule along its line (0.75 pt black stroke), and non-answer letters print in a lighter
  weight *(builder's call)*. Four per page (2×2).
- **Crossword:** the filled grid. Four per page.
- **Sudoku:** the solved grid with the givens in bold. Six per page (2×3).

### 4.5 Code

- **New module `crates/deckle-digest/src/puzzle/book/`:**
  - `mod.rs`: the book model (kind, options, puzzles, answers) and `make(options) -> Book`.
  - `pdf.rs`: a fixed-geometry renderer over `pdf-writer`.
  - The Daily's `pdf.rs` is a flowing-text layout engine, which is the wrong tool for exact
    grids.
- **Fonts:** the renderer reuses the Daily's Old Standard faces and its font embedding.
  - If the embedding code can't be called as it is, lift it into a shared function in
    `pdf.rs`. The Daily's PDF tests must pass unchanged.
- **Placement:** `deckle-digest` already owns the puzzle code, the fonts and `pdf-writer`.
  A new crate would only re-export them *(builder's call)*.

## 5. Interfaces

### 5.1 Generators

- **`puzzle::search::fill`** takes a size and a difficulty. The fixed `[[u8; 9]; 9]` grid
  becomes a `Vec<u8>` of `size × size`.
  - The Daily calls it with `(9, Medium)`, and its output for any date must not change. Test
    this with the existing fixtures and a seeded comparison.
  - It takes the words to place as an ordered list (themes first) and a pool to fill from, plus
    the word count wanted.
- **New modules:**
  - `puzzle::cross` for the crossword (increment 2).
  - `puzzle::sudoku` for sudoku (increment 3).

### 5.2 CLI

```
deckle puzzles <search|crossword|sudoku>
    [--count N] [--size N] [--difficulty easy|medium|hard]
    [--theme TEXT | --words FILE] [--title TEXT] [--seed N] [--date YYYY-MM-DD]
    [--screen kindle-scribe|remarkable-2|…]
    (-o DIR | --to device|<folder>) [--replace]
```

- **File name:** `<Title> <date> <seed4>.pdf`, e.g. `Word Search 2026-11-01 7f3a.pdf`. The
  name uses the same rules as Kindle documents (sanitized, 120 characters, `.pdf`), and
  `<seed4>` is the seed's first four hex digits *(builder's call)*.
  - **Why:** the Kindle keeps a PDF's ink in `<name>.sdr` beside it. If two books had the same
    name, the second would replace a half-solved first, and the first one's ink could land on
    the new puzzles.
  - **Never overwrite:** if a file with that name already exists at the target, the command
    refuses unless you pass `--replace`. With `--replace` it says the old ink may stay. Only
    remaking the same book on purpose (same seed and date) can collide.
  - **The library entry:** the PDF's Title metadata is the plain title, which is what the
    Kindle library shows when it reads it.
- **`-o DIR`** writes the PDF into `DIR`.
- **`--to`** builds the PDF and delivers it with `deliver_named_on`:
  - `device`: the first connected Kindle, mass storage or MTP, into `documents/`.
  - `<folder>`: written into that folder.
- **`--date YYYY-MM-DD`:** the date printed on the title page, default today. It is an input
  like any other, so a remade book keeps its original date and bytes *(builder's call)*.
- **Default seed:** a hash of the options (except `--to`, `-o` and `--replace`) and the date.
  Two runs on one day make the same book, and tomorrow's is new *(builder's call)*.
- **Remaking a book:** the output prints a command that remakes it, with its seed and date.
- **Output:** the path, page count, puzzle count, seed and date. `--json` gives
  `{path, pages, puzzles, seed, date, warnings}`.
- **Errors** say what to change:
  - `--size` too big for the screen;
  - `--size` given for sudoku;
  - `--theme` with no LLM set;
  - `--words` with too few usable words (below 8 for one word search).
- **`--words` with fewer words than the book needs:** words repeat across puzzles but never
  within one, and built-in words fill the rest. Deckle warns when more than half of a puzzle is
  built-in.

## 6. Increments

1. **Book engine + word-search book.**
   - The search generator takes a size and a difficulty, and the Daily's output stays
     unchanged.
   - Book model and PDF renderer: title page, puzzles, answers and outline.
   - `deckle puzzles search` with `--count`, `--size`, `--difficulty`, `--words`, `--title`,
     `--seed`, `--date`, `--screen`, `-o`, `--to` and `--replace`.
   - A user doc at `docs/puzzle-books.md`.
   - **Done:** the tests in §7 pass, and a 10-puzzle book reaches the Scribe and takes ink.
2. **Crossword book** (built 2026-11-01):
   - `words-long.tsv.gz`, built with its builder mode and README row;
   - block patterns;
   - the filler;
   - the clue page layout.
3. **Sudoku book:** the generator, the grader and the layout.
4. **Themes:**
   - LLM words and clues;
   - `--theme current-events` from the Daily's headlines;
   - the fallback and warnings.
5. **Later, separately designed:**
   - the app's Puzzle Books screen;
   - maybe the Kitchen Book's write-on PDF edition.

Each increment is its own PR.

## 7. Testing

- **Word search generator:**
  - Every placed word reads from its start along its direction, within the allowed
    directions.
  - No word reads twice.
  - No filler spells a blocklisted word.
  - It's deterministic for a seed.
  - Every size from 8 to 20 fills, at each difficulty, for 50 seeds.
  - The Daily's 9×9 output matches the pre-change generator for 60 consecutive dates.
- **Book PDF**, read back through `deckle-pdf` (PDFium), as the Daily's PDF tests do:
  - the page size matches the screen;
  - page count = 1 + puzzles + answer pages;
  - each puzzle page's text holds its heading and every listed word;
  - the outline has the entries;
  - there are no link annotations;
  - the same seed and date give identical bytes.
- **Size fit:** a size that can't meet the minimum cell is refused with the largest that fits.
- **CLI:**
  - `-o` writes the file;
  - a second run with the same name is refused without `--replace`;
  - `--to <folder>` delivers it;
  - `--json` has the fields;
  - `--size` for sudoku is refused (increment 3).
- **Device (manual, each increment):** deliver to the Scribe with `--to device`, write in ink,
  turn the page, reopen the book and check the ink is still there.
- **Later increments:**
  - crossword: every entry is a list word with a clue, symmetry holds, and fills for 30 seeds
    per size;
  - sudoku: exactly one solution and the grade matches;
  - LLM: validation drops bad words, and the fallback builds.

## 8. Left to the builder

- Exact typography: the heading and word-list sizes and the title page layout.
- Retry counts.
- The internals of the block-pattern generator and the fill heuristics.
- The sudoku grader's technique set beyond the grades named above.
- The LLM prompt wording.
