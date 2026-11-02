# The Puzzles tab: puzzle books from the app, themed on what you read — design

**Status:** draft for discussion, 2026-11-02 · **Owner:** Jonathan (CasaVargas LLC)
**Builds on:** `2026-11-01-puzzle-books-design.md` (§3.5 themes, §6 increment 5 "the app's
Puzzle Books screen"), `2026-10-22-puzzle-page-design.md`, `2026-10-31-word-search-design.md`,
`2026-10-25-interests-design.md`, `2026-11-02-local-model-design.md`
**Mockup:** the "Deckle Puzzles Screen" design canvas (three artboards: the tab, the first page
on a Scribe, the lower half with your books and Sunday).

`deckle puzzles` makes word-search, crossword and sudoku books from the command line, themed by
a word list, and the Daily's puzzle page already themes the Mini and the word search on
yesterday's headlines. This design puts the books in the app as a fifth tab and themes them on
what Deckle already keeps about the reader, with the AI model writing clues when it's on.

## 1. The tab

**Puzzles**, after Cookbook. Two halves:

- **Make a puzzle book.** Kind (Crossword, Word search, Sudoku, Word ladder), Theme (§2),
  Options (size, difficulty, count, the screen, title), then **Send to the Scribe**, **Save
  PDF…** and **Add to Sunday's paper**. Beside it, the first puzzle as it will print, theme
  answers coloured in the preview only.
- **Your books** (`~/Deckle/Puzzles`, beside the Morgue): each book with its options, where it
  went, and how many its reader has solved (the Kindle's notes, as the Mini's are marked today).
  **Make again** rebuilds byte for byte from the saved options, seed and date.
- **Every Sunday, with the paper:** a switch and a recipe (so many crosswords from the week's
  paper, word searches from looked-up words, sudoku, a kid's page). The book rides along with
  Sunday's edition to a Scribe or reMarkable; a reader without a pen gets the paper alone.

## 2. Themes, and where their words come from

Every source is something on this machine. None needs the AI; with it on (Settings → AI), it
writes the clues and can add words the source didn't name.

| Theme | Words | Clues without the AI | Needs |
|---|---|---|---|
| Your interests | `puzzle::theme::harvest` over the interest sections' headlines in the Morgue, last 30 days, per interest ticked | "Cargo ____: the Rust package registry (Interests, Nov 1)", the Mini's form | a paper with interests |
| This week's paper | the same harvest over every news section, last 7 days (a month: a quiz at the back) | the headline form | the Morgue |
| The book you're reading | X-Ray / Who's who entities of the book open on the Kindle, only those first mentioned before the reader's position (`readership`), so nothing from later chapters | the entity's spoiler-free description | a book sent with X-Ray or Who's who |
| Words you looked up | the Kindle's `vocab.db` words (what From Your Shelf reads) | the definition the reader tapped, from the Kindle's dictionary record when it has one, else Wiktionary | a Kindle that shares `vocab.db` |
| Your cookbook | ingredient and dish names from the Cookbook's recipes | "In the Thursday soup" | recipes saved |
| Your own list | pasted `word: clue` lines, as `--words` | as written | nothing |
| A subject | the model's words | the model's clues | the AI |
| No theme | Deckle's list | Wiktionary | nothing |

Clue style, with the AI: **Plain** (as the table), **Trivia**, **Cryptic**, **For a kid**. The
request is the puzzle-books design's §3.5 one, with the style in the prompt; every word is
validated as there. Words that don't fit the grid are listed under the theme, never silently
dropped.

**Same book, whole house.** A paper per reader already names the readers; one seed per book
means everyone gets the same grid. A reader's own interests make their own book.

## 3. Backend

- `deckle_digest::puzzle::book` gains theme sources as a `Theme` enum resolved to a word list
  (with clues) before the book is made, so the generators stay as they are.
- Commands (`dispatch`, Tauri, the web server): `puzzles_sources` (which sources have words,
  and how many), `puzzles_preview` (the first puzzle for the options, cached by seed),
  `puzzles_make` (with `puzzles-progress` events, since 10 crosswords take ~15 s),
  `puzzles_list`, `puzzles_send`, `puzzles_sunday_set`.
- `deckle puzzles` gains `--theme interests|week|book|vocab|cookbook` and `--clues plain|trivia|
  cryptic|kid`, so the CLI and the app share one path.
- Sunday: a `[puzzles]` table in the paper's config (`sunday = true`, the recipe), built by the
  morning run after the edition, delivered through the same outbox as books to a reMarkable or
  over USB to a Scribe.

## 4. Non-goals

- Solving on screen: the books are for ink.
- New puzzle kinds (mazes, cryptograms); the ladder is the one addition, since the paper has it.
- Themes from anything online.

## 5. Increments

1. The tab with Crossword, Word search and Sudoku, **Your own list** and **No theme**, Your
   books, Send and Save. (The CLI's engine behind commands.)
2. Interests, This week's paper and Words you looked up; the preview; progress events.
3. The book you're reading and Your cookbook; the AI's clues and styles.
4. Every Sunday, and the solved count from the Kindle's notes.
