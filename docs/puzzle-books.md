# Puzzle books (`deckle puzzles`)

`deckle puzzles` makes a puzzle book you solve in ink on a Kindle Scribe. The book is a PDF with
a title page, one puzzle per page, and the answers at the back. Design:
`docs/superpowers/specs/2026-11-01-puzzle-books-design.md`.

**Why a PDF:** on a Kindle Scribe 2024, only a PDF copied over USB takes pen ink. Fixed-layout
AZW3 books don't, whichever way they're sent. Ink on a PDF stays after you turn the page and
after you close and reopen the book.

Word searches are available now. Crosswords, sudoku and LLM-written themes come next.

## Commands

Add `--json` for machine-readable output: `{path, pages, puzzles, seed, date, warnings}`.

| Command | What it does |
|---|---|
| `deckle puzzles search --to device` | Make a book of 10 word searches and copy it to the first connected Kindle, in `documents/`. |
| `deckle puzzles search -o <folder>` | Write the book into a folder instead. |

### Options

| Option | Default | |
|---|---|---|
| `--count N` | 10 | Puzzles in the book, from 1 to 200. |
| `--size N` | 12 | Squares on a side, from 8 to 20. A bigger grid hides more words: 9 at 9×9, 16 at 12×12, 40 at 20×20. |
| `--difficulty` | `medium` | `easy`: across and down only. `medium`: adds the two diagonals that read left to right. `hard`: all eight directions, backwards too. |
| `--words FILE` | | Your own words, one per line. Lines starting `#` are skipped. Accents and punctuation are dropped ("Wi-Fi" → WIFI). Words go round the book, never twice in one puzzle, and built-in words fill any gaps. You need at least 8 words that fit the grid. |
| `--title TEXT` | Word Search | The title shown on the title page and in the Kindle's library. |
| `--seed N`, `--date YYYY-MM-DD` | today | The same options, seed and date make the same book, byte for byte. |
| `--screen` | `kindle-scribe` | The page size. The others are `remarkable-2`, `remarkable-paper-pro`, `remarkable-paper-pro-move`, `ipad` and `a5`. |
| `--replace` | | Overwrite a book with the same name (see below). |

After each run, Deckle prints the command that makes that book again, with its seed and date.

## Names and ink

The Kindle keeps a PDF's ink in a `.sdr` folder named after the PDF. So every book gets its own
name, `<Title> <date> <code>.pdf`, for example `Word Search 2026-11-01 0cb1.pdf`. A new book
never lands on top of a half-solved one.

Making the same book again (same seed and date) gives the same name. Deckle refuses to overwrite
it unless you pass `--replace`, and even then your old ink may still show on the new copy.

## Pages

- **Title page:** the title, the size, difficulty and number of puzzles, and the date.
- **Puzzles:** "No. N", the grid, and the words in alphabetical columns. Each square is at least
  18 pt (6.4 mm), big enough to circle a letter.
  - If a size won't fit the screen at that minimum, the command refuses and names the largest
    size that fits.
  - The footer says which page has the answers. It's plain text, not a link.
- **Answers:** four per page. Each hidden word is outlined, and the filler letters are printed
  smaller.

Puzzle pages are black on white, with no grays and no links, so a pen stroke can't jump away
mid-solve. To get around, use the Kindle's table of contents: Title, No. 1, No. 2 and so on, then
Answers.

## Not yet

- Crossword books, at sizes from 5×5 to 15×15.
- Sudoku books.
- `--theme "technology"` written by your LLM, and `--theme current-events` from the Daily's
  headlines.
- A Puzzle Books screen in the app.
- Checking whether the Scribe's eraser removes one letter at a time.
