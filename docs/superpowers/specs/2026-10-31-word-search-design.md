# The word search — design

**Status:** requested by owner 2026-10-31 ("Let's do the word search"); the owner skipped the
design questions, so the calls below marked *(builder's call)* are mine, for review ·
**Date:** 2026-10-31 · **Owner:** Jonathan (CasaVargas LLC) · **Builds on:**
`2026-10-22-puzzle-page-design.md` (the puzzle page, `puzzles.json`, theme words),
`2026-10-23-edition-bundle-design.md` (§6.6 `puzzle`), `2026-10-23-pdf-reflow-design.md`
(PDF screens).

A third puzzle on the puzzle page: a **word search** of 8–10 hidden words, some taken from
yesterday's headlines. It's printed in every edition, answered in tomorrow's paper like the
mini, and **made for a pen**: you play it by dragging on the iPad, and on the Kindle Scribe you
circle words in ink on a puzzle PDF sent next to the paper.

## 1. Intent and why

The iPad and the Kindle Scribe both have pens, and the puzzle page doesn't use them. A
crossword needs letters typed or written into squares. A word search needs only a stroke
through a word, which is the most natural thing to do with a pen and a paper, and it's
playable at a glance on e-ink.

**Success:** on the first paper after this ships:

- The *Puzzle* section has "Word search · No. N" after the word ladder: a 9×9 letter grid,
  then the words to find, with the headline words marked "(from yesterday's paper)".
- On the iPad, dragging across a hidden word strikes it from the list. Finding all of them
  says so.
- On the Kindle Scribe, a second email brings "Puzzles · Sat, Oct 31" as a PDF. You open it
  and circle the words in ink.
- The next day's "Yesterday's answers" shows yesterday's grid with only the hidden words'
  letters printed, the rest left as dots.

## 2. Goals, non-goals, anti-goals

**Goals**

1. **The puzzle.** A 9×9 grid of capital letters hiding 8–10 words of 4–5 letters. Words run
   forward only, in four directions: across, down, and the two diagonals that read
   left to right (↘ ↗). No backwards words, which are hard to find and harder to drag on
   e-ink *(builder's call)*.
2. **Theme words.** Up to three of the hidden words come from yesterday's headlines, chosen
   the way the mini chooses them (`theme::pick`) and avoiding the mini's own theme words, so
   the two puzzles don't share them. With no headlines, every word comes from the list.
3. **Made once, stored, deterministic.** The search is part of the day's puzzle in
   `puzzles.json`, seeded by the date like the mini. Every rebuild that day prints the same
   one, and a grid never repeats one from the last 60 days.
4. **Everywhere the puzzle page goes.** EPUB (Kindle, KOReader), PDF screens, and the edition
   bundle (iPad and iPhone). Old bundles and old readers are unaffected.
5. **Kindle Scribe ink.** An opt-in second email carries a puzzle PDF (crossword, ladder, word
   search) sized for the Scribe, which you can write on.
6. **iPad play.** Drag across letters to mark a word. It's checked against the bundle's
   answers, found words are struck, and progress survives relaunch like the crossword's.
7. **Answers tomorrow.** "Yesterday's answers" adds the search, with its words shown in the
   grid.

**Non-goals (this increment)**

- **Reading the Scribe's ink back.** Deckle can't read the Scribe's notes over USB (#99), and
  ink on a PDF stays on the device. Answers come in tomorrow's paper, with no score.
- **Handwriting recognition**, on any device.
- **Bigger grids, backwards words, difficulty settings.**
- **The comic strip.** It's parked until the owner picks "read" or "draw".
- **A Paperwhite pen.** The Paperwhite gets the printed grid only.

**Anti-goals**

- A hidden word that's offensive or a slur. The word list's filters and blocklist apply.
- Filler letters that spell a blocklisted word along any line, in any of the 8 directions.
  This is checked square by square as the filler goes in, never by rejecting whole grids. An
  ordinary word that turns up by chance is allowed: it isn't on the list to find, so it's a
  bonus, not a second answer *(builder's call: a strict "no accidental words" rule would rarely
  fill in time)*.
- Tomorrow's answers that don't match today's grid.
- A word search failure that stops the paper or the mini. If no grid fills, the search is
  left out and the rest of the page prints.

## 3. Constraints

- **Rust, in `crates/deckle-digest/src/puzzle/`**: a `search.rs` next to `grid.rs`. It uses
  the same embedded word list (3–5 letters, so hidden words are 4–5) and the same date
  seeding.
- **State:** `store::Puzzle` gets an optional `search`, which is the grid rows and the
  placed words with start cell and direction. It's `#[serde(default)]`, so existing
  `puzzles.json` files load, and days stored before this ship have no search.
- **E-ink:** the grid is an XHTML table (class `search`), letters centered in square cells,
  and nothing relies on colour. It must read on a Scribe and a Paperwhite at default font
  size.
- **Bundle:** `puzzle.search` is optional and additive. The schema and §6.6 are updated, and a
  reader that doesn't know `search` ignores it. The version is 1.5 for every bundle that
  carries `search`. Today a bundle is 1.4 only when it comes fresh from the press and 1.3
  otherwise (`bundle.rs:24, 695`), so the builder picks how the two paths become one rule.
- **Email:** today's mailer sends one EPUB per email (content type fixed in `mail.rs:318`,
  body text about the paper). PR 2 makes the attachment type and body a parameter. The puzzle
  PDF is a second email to the same Kindle address, sent only when it's on.
- **PDF:** `pdf::render` lays out a whole edition with its front page (`pdf.rs:2311`). PR 2
  adds rendering a single section on its own pages. `kindle-scribe` joins `Screen`, which also
  makes it a choice for the whole paper's PDF export. That's fine and wanted.
- **Speed:** filling a 9×9 with forward-only words is fast. Cap it at about 200 ms. On
  timeout it drops one word and tries again, then gives up.

## 4. The puzzle

**Placing words.**

- Pick the words: up to three theme words first, then 4–5-letter words from the embedded list
  (already filtered for zipf ≥ 3 and the blocklist when it's built) that aren't used in the mini
  that day.
- Place the longest first, each at a random start and direction (seeded) where it fits,
  sharing letters only where they match.
- Fill the remaining squares one at a time with seeded letters, weighted toward English letter
  frequency. Skip any letter that would complete a blocklisted word along a line through that
  square. The blocklist (`data/wordwise/puzzle-blocklist.txt`, today only read when the list is
  built) gets embedded for this.

**No repeats.** The same rule as the mini (puzzle design §4, added by CasaVargas/deckle#263,
which this builds on): if the grid equals one from the last 60 days, fill again with the
date's next seed.

**The word list printed.** Alphabetical, upper case. Theme words carry "(from yesterday's
paper)", and the headline isn't printed, which keeps the list short *(builder's call)*.

## 5. Where it shows up

- **The paper (EPUB and PDF):** the article "Word search · No. N" (the mini's number) goes
  after "Word ladder":
  - the grid table;
  - "Find these words; they run across, down, or diagonally, never backwards.";
  - the word list, in two columns where the format allows;
  - nothing about notes on any Kindle: the word search isn't scored from Kindle notes. You'd
    have nothing sensible to highlight *(builder's call)*.
- **Yesterday's answers:** a small grid where only the hidden words' letters print and every
  other square shows "·", followed by the words.
- **Bundle `puzzle.search`:**
  `{ "size": 9, "grid": ["…9 letters…", …], "words": [{ "word": "COURT", "row": 0, "col": 2,
  "dir": "across" | "down" | "down-right" | "up-right", "theme": true }] }`. Yesterday's adds
  `yesterday.search` (the same shape).
- **Kindle Scribe PDF (opt-in):** `email.puzzle_pdf = true` in `digest.toml` *(builder's call
  for the name)*.
  - Today's puzzle page (mini, ladder, word search, without yesterday's answers) is rendered
    as a PDF for a new screen, `kindle-scribe` (1860×2480 px at 300 ppi), with large cells
    for ink.
  - It's emailed to the same `to` address with subject "Puzzles · <date>" and file name
    `deckle-puzzles-<date>.pdf`.
  - **To verify on the device before building on it:** Send to Kindle keeps an emailed PDF
    as a PDF the Scribe can write on, as long as the subject isn't "convert".
- **iPad and iPhone (Deckle Reader):** under the ladder.
  - Drag across letters in a straight line along one of the four directions to select them.
  - If the selection is a hidden word, it's struck in the list and outlined in the grid.
  - The list shows found words struck. "Reveal answers" in the puzzle menu shows the rest.
  - Progress is saved with the crossword's snapshot (`CrosswordSnapshot` gains found words;
    returns carry them, which means extending the returns format, `ReaderReturns.swift`).
    Deckle stores returned found words but prints no score from them in this increment
    *(builder's call; scoring from returns is a later step for every puzzle at once)*.
  - Work in `deckle-reader`. It follows bundle §6.6 only, never Deckle's code.
- **CLI:** `deckle digest puzzle` prints the search grid and words, and `--json` includes
  `search`.

## 6. Testing

- **Generator:** every placed word reads from its start along its direction. Words don't
  repeat and no word is on the blocklist. There are no backwards words, and no unintended
  listed word appears along any line. It's deterministic for a date and its inputs, and it
  fills for 60 consecutive dates within the cap. Theme words are placed when given and never
  equal the mini's.
- **Store:** an old `puzzles.json` without `search` loads, and a day is made once and kept.
- **No repeats:** a store whose yesterday holds the grid today would make gets a different
  grid.
- **Section:** the article order is mini, ladder, search. Yesterday's answers show dots and
  the words. A golden XHTML fixture covers it.
- **Bundle:** `search` matches the schema, and a 1.3 reader fixture still decodes.
- **PDF:** the `kindle-scribe` page size is right, and the puzzle PDF holds the three puzzles
  and no answers.
- **Email:** with `puzzle_pdf` on, two emails are sent (the paper, then the puzzles); with it
  off, one.
- **Reader (deckle-reader):** selection along each direction, a wrong selection doesn't
  strike anything, and progress restores. A bundle without `search` shows no word search.

## 7. Order of work

1. **Deckle:** generator, store, section, CLI and bundle (one PR).
2. **Deckle:** the `kindle-scribe` screen, section-only PDF rendering, and the puzzle PDF
   email (one PR). It starts only after the on-device check in §5 passes.
3. **Deckle Reader:** iPad and iPhone play (one PR).

## 8. Left to the builder

- Grid size if 9×9 crowds a Paperwhite at default font size (8×8 is fine).
- The exact zipf floor, letter weights and refill counts.
- How the Scribe PDF lays out three puzzles on one or two pages.
- The bundle's next minor version number.
