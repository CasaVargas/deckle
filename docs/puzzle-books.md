# Puzzle books (`deckle puzzles`)

`deckle puzzles` makes a puzzle book you solve in ink on a Kindle Scribe. The book is a PDF with
a title page, one puzzle per page, and the answers at the back. Design:
`docs/superpowers/specs/2026-11-01-puzzle-books-design.md`.

**Why a PDF:** on a Kindle Scribe 2024, only a PDF copied over USB takes pen ink. Fixed-layout
AZW3 books don't, whichever way they're sent. Ink on a PDF stays after you turn the page and
after you close and reopen the book.

Seven kinds: word searches, crosswords, sudoku, word ladders, cryptograms, nonograms and mazes.
The app's **Puzzles** tab makes the same books with a preview of the first page, keeps them in
`~/Deckle/Puzzles` with the options that made them (**Make again** gives the same book), and
sends them to a Kindle or saves a copy for a reMarkable or an iPad. A crossword's or word
search's **theme** can come from your own paper, your Kindle, your cookbook, or the AI
([Themes](#themes)).

## Commands

Add `--json` for machine-readable output: `{path, pages, puzzles, seed, date, warnings}`.

| Command | What it does |
|---|---|
| `deckle puzzles search --to device` | Make a book of 10 word searches and copy it to the first connected Kindle, in `documents/`. |
| `deckle puzzles search -o <folder>` | Write the book into a folder instead. |
| `deckle puzzles crossword --to device` | Make a book of 10 crosswords and copy it to the Kindle. |
| `deckle puzzles sudoku --to device` | Make a book of 10 sudoku and copy it to the Kindle. |

### Options

These are the word-search options. Crosswords take the same ones, with different sizes and
difficulties, and a `--words` file that has clues (see [Crosswords](#crosswords)).

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

## Crosswords

`deckle puzzles crossword` takes `--size` from 5 to 15 (default 11), plus the same `--count`,
`--title`, `--seed`, `--date`, `--screen`, `-o`/`--to` and `--replace` as word searches.

- **Grids:** symmetric, with about a fifth of the squares black and no entry longer than 6
  letters. Every letter is in both an across and a down entry. A 5×5 uses the Daily's mini
  layouts.
- **Difficulty:**
  - `easy` keeps to everyday words;
  - `medium` mixes in some less common ones;
  - `hard` draws from the whole list of about 45,000 words.
- **Clues** come from Wiktionary. Plurals and verb forms say so, for example "To cook in an
  oven (past tense)". Some clues use an unexpected sense of the word.
- **Pages:** the numbered grid is at the top and the clues sit below in two columns. Clues
  that don't fit continue on the next page as "No. N (continued)". A 15×15 takes three pages.
  The answers are filled grids, four per page.
- **Your own words:** `--words FILE` with one `word: clue` per line (a tab instead of the
  colon works too). Each puzzle puts in as many of your words as it can before filling the
  rest from the list, and marks them as theme answers. A long list usually gives 3 to 6 per
  puzzle.
  - Words have to be 3 to 6 letters to fit the grids. Longer ones, and lines with no clue, are
    skipped, and the book warns which ones.
  - You need at least 3 usable words. The book warns about any puzzle that gets fewer than 3.

## Sudoku

`deckle puzzles sudoku` makes classic 9×9 sudoku. It takes `--difficulty`, plus the same
`--count`, `--title`, `--seed`, `--date`, `--screen`, `-o`/`--to` and `--replace` as the other
kinds. There's no `--size` and no `--words`.

- **Difficulty** depends on which techniques you need:
  - `easy`: naked and hidden singles are enough, and at least 36 numbers are given;
  - `medium`: you need pairs or pointing/claiming at least once;
  - `hard`: you also need triples, X-wing or XY-wing. You never need to guess, so every
    hard puzzle can be finished in ink.
- **One solution:** every puzzle has exactly one, and its given numbers are symmetric when
  you turn the page upside down.
- **Pages:** one big grid per page, with squares of at least 40 pt (14 mm), so there's room
  for pencil marks. Givens are bold.
- **Answers:** six per page, with the givens in bold.

## Themes

A crossword or a word search takes its theme words from one of these, and Deckle's list fills
the rest of each grid. On the tab they're the chips under **Theme**; on the command line
`--theme`:

| Theme | `--theme` | Words | Clues |
|---|---|---|---|
| Your interests | `interests`, with `--interest NAME` to pick some | The headline words of your papers' interest sections over the last 30 days, from the Daily's puzzle store | The headline with the word blanked: "____ build gets faster (Rust, Monday)" |
| This week's paper | `week` | Every news section's headline words, last 7 days | The same |
| Words you looked up | `vocab` | The Kindle's vocabulary builder, every Kindle that shares it, newest first | The definition the Kindle kept, else the sentence it was looked up in |
| Your cookbook | `cookbook` | Each recipe title's words and each ingredient's last word | "In the title of a recipe you saved", "Goes into Thursday soup" |
| A subject | `subject`, with `--subject TEXT` | The AI picks 12 to 60 words about it, sized for the grid | The AI's, in the clue style |
| Your own list | `--words FILE` | As before | As before |

The AI (Settings → AI, or `deckle llm check`) can also **rewrite the clues** of any crossword in
a style, `--clues trivia|cryptic|kid`: one request a puzzle, every word checked so a clue never
names its answer, and a puzzle whose request fails keeps its plain clues with a warning. A
subject and styled clues both need the AI; the other themes don't. A theme with fewer than 8
words that fit a word search, or 3 that fit a crossword, falls back to the list with a warning.

The book's title defaults to the theme ("Rust & Panthers, November"), and the title page's
report says where the words came from.

## Word ladders

`deckle puzzles ladder` makes pages of three ladders each: a 4-letter word at the top, another
at the bottom, and blank rungs between them to climb, changing one letter a rung, every rung a
word. `--difficulty` sets the rungs: `easy` 3, `medium` 4, `hard` 5 or 6. The pairs are Deckle's
opposites and near-pairs first ("cold" to "warm"), then any two words that far apart; no pair
twice in a book. `--count` is pages. The answers list each ladder's words.

## Cryptograms

`deckle puzzles cryptogram` enciphers a sentence letter for letter, the same way throughout,
with a blank above each letter to write the real one in and a key, A to Z, to work in.
`--difficulty`: `easy` picks short quotes and gives 3 letters away, `medium` gives 1, `hard`
none. The quotes are public domain (Austen, Dickens, Thoreau, Twain, Wilde…), or yours with
`--quotes FILE`: one a line as `quote — who said it` (a tab or ` | ` works too), 24 to 120
letters each; the answers name who said it. Quotes go round the book and repeat with a new
key only once all are used.

## Nonograms

`deckle puzzles nonogram` makes picture-logic grids: shade the squares so each row and column
has runs of the lengths given, in order, and a picture appears. `--size` is 5 to 25 (default
15). The pictures are Deckle's own (a cat, a boat, a star…) and random shapes, every one
checked to solve by row-and-column logic alone. `--picture FILE` makes your own photo the
first puzzle: the darker half of its squares are shaded, and the book warns if that one needs
a guess. The answers show the filled grids, each with its name.

## Mazes

`deckle puzzles maze` makes mazes with one way through, in at the top left and out at the
bottom right. `--size` is 6 to 40 squares a side (default 20): 10 for a child, 30 for an
evening. The answers draw the path.

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

- `--theme "technology"` written by your LLM, and `--theme current-events` from the Daily's
  headlines.
- Checking whether the Scribe's eraser removes one letter at a time.
