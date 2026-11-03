# The puzzle book bundle: puzzle books as data, for the iPad — design

**Status:** draft for owner review · **Date:** 2026-11-03 · **Owner:** Jonathan (CasaVargas LLC)
· **Builds on:** `2026-11-01-puzzle-books-design.md` (the books, the PDF),
`2026-11-02-puzzles-tab-design.md` (the tab, themes, Sunday), `2026-10-23-edition-bundle-design.md`
(the bundle idea, on-disk rules, returns, versioning) · **Reader side:** CasaVargas/deckle-reader,
`docs/superpowers/specs/2026-11-03-puzzle-books-design.md` · **Rejected:** putting books inside
the edition bundle (books are made on demand, not every morning, and a reader's Archive is a
ledger of papers); sending the PDF to the iPad (it can't be solved natively, and the reader has
no PDF view by design).

Calls marked *(builder's call)* are mine, not the owner's; review them.

A puzzle book today is a PDF for a pen on e-ink. The iPad is the other place the owner solves,
and there a book should be the thing itself: a crossword you type or write in, a word search you
circle, a sudoku that checks itself, a nonogram that fills in. This design writes a puzzle book
as **data** the way an edition is written, into the same folder the iPad already watches, and
reads back what was solved.

## 1. Goals, non-goals, anti-goals

**Goals**

1. **Every kind of book the tab makes** reaches the iPad as a solvable bundle: crossword, word
   search, sudoku, word ladder, cryptogram, nonogram, maze. One kind per book, as today.
2. **The same book.** A book made from the same options, seed and date is the same PDF *and* the
   same bundle: one `Book` value, two writers.
3. **The same delivery as the paper.** A folder export with `format = "bundle"` (iCloud Drive for
   an iPad) gets books beside editions. **Send to iPad** in the Puzzles tab and `--to <folder>`
   with `--as bundle` on the CLI write one. Sunday's book rides along with Sunday's edition into
   the same folder.
4. **Two-way.** The reader writes what was solved into `.deckle-returns/`, and **Your books**
   shows the solved count from the iPad as it will from the Kindle's notes.
5. **Documented and MIT-licensed**, like the edition bundle: a spec here, a schema in
   `docs/formats/puzzle-book.schema.json`, so the reader (not GPL) can vendor it.

**Non-goals**

- **Mixed books** (a crossword page then a sudoku page). The tab makes one kind per book.
- **Solving the Daily's mini or word search** through this format: they stay in the edition
  bundle (`§6.6`), and the reader already has the mini. (The reader design adds the daily word
  search from the field the edition bundle already carries.)
- **A fresh-paper operation for books** ("print me a puzzle book from the iPad"). The press
  protocol's `operations` stays `[fresh_full]` with `reading_editions`; a `puzzle_book`
  operation is a later spec once the format has shipped *(builder's call)*.
- **Pictures in nonograms shipped as photos.** The solution cells are the picture; the source
  photo isn't in the bundle.

**Anti-goals**

- **Answers the reader can't check.** Every puzzle carries its full solution, as the mini does,
  so Check and Reveal work offline. The reader shouldn't show them before it's asked.
- **A bundle that is "arriving" forever.** Same rules as editions: a `.tmp` folder renamed into
  place, `book.json` last, every file listed with size and SHA-256.
- **A second generator.** The bundle writer serializes the `Book` the PDF writer draws. No
  puzzle is made twice.

## 2. On disk

A puzzle book is a **folder** named `deckle-puzzles-<slug>-<YYYY-MM-DD>-<seed4>.decklepuzzles`,
where `slug` is the title's lower-case ASCII words joined by `-` (at most 40 characters) and
`seed4` the first four hex digits of the seed, as the PDF's file name has:

```text
deckle-puzzles-word-search-2026-11-03-9f3a.decklepuzzles/
  book.json             the book (§3); written last
  cover.jpg             the title page as an image, 600×800, for the reader's shelf
  media/
    4c1d…e2.png         optional: a nonogram's source picture, downsampled, for the answer page
```

- **One folder beside the editions,** in the same export directory. The reader declares
  `.decklepuzzles` as a package type (`com.casavargas.deckle-puzzles`), so Files shows one item.
- **Pruning:** the export's `keep` applies to editions only. Books are never pruned by Deckle; the
  owner deletes a book in Files or in the tab's **Your books** (which removes the PDF, the
  sidecar and any bundle it wrote) *(builder's call)*.
- **Written safely,** exactly as §5 of the edition bundle design: `.deckle-puzzles-….tmp/`,
  renamed; `book.json` last, listing every other file.

## 3. `book.json`

```json
{
  "format": "deckle-puzzles",
  "version": "1.0",
  "generator": "deckle 0.13.0",
  "book": {
    "id": "6c1e3b0a-…",
    "title": "Word Search",
    "subtitle": "Ten puzzles from this week's paper",
    "kind": "search",
    "date": "2026-11-03",
    "seed": "0000000000009f3a",
    "difficulty": "medium",
    "size": 12,
    "theme": { "kind": "week", "label": "This week's paper" },
    "clue_style": "plain",
    "built_at": "2026-11-03T07:12:04Z",
    "cover": "cover.jpg",
    "paper_id": null,
    "sunday_of": null,
    "warnings": []
  },
  "puzzles": [ … one object per puzzle, in book order (§3.2) … ],
  "files": [ { "path": "cover.jpg", "size": 48122, "sha256": "…" } ]
}
```

### 3.1 `book`

- **`id`:** a UUID v5 of `(kind, seed, date, size, difficulty, count)` so the same book has the
  same id on every machine; returns key on it.
- **`kind`:** `crossword`, `search`, `sudoku`, `ladder`, `cryptogram`, `nonogram`, `maze`.
- **`seed`:** the `u64` as 16 lower-case hex digits.
- **`theme`:** `{ "kind": "none" | "own" | "interests" | "week" | "vocab" | "cookbook" | "subject",
  "label": "…" }`; `subject` carries `"text"`. For the reader's shelf line only.
- **`clue_style`:** `plain`, `trivia`, `cryptic`, `kid`; crosswords only, else absent.
- **`paper_id`** and **`sunday_of`** (an edition date): set when the book was made as Sunday's
  book for a paper, so the reader can shelve it with that edition. Otherwise `null`.
- **`warnings`:** the book's `warnings` (words that didn't fit), for the shelf's detail.

### 3.2 `puzzles[]`

Every puzzle has `"n"` (1-based, its page order) and `"kind"` (the book's), then its own fields.
Grids are arrays of equal-length strings, row by row, top to bottom, as the edition bundle's
`grid`. Letters are upper-case ASCII.

**`crossword`**, the mini's shape with any size:

```json
{ "n": 1, "kind": "crossword", "size": 11,
  "grid": ["ABODE#SCARF", …], "numbers": [[0,0,1], …],
  "clues": [ { "number": 1, "dir": "across", "clue": "Home", "answer": "ABODE", "theme": true } ] }
```

**`search`**, the edition bundle's `search` with all eight directions:

```json
{ "n": 1, "kind": "search", "size": 12, "grid": ["…12 letters…", …],
  "words": [ { "word": "COURT", "row": 0, "col": 2, "dir": "left", "theme": true } ] }
```

`dir` is `across`, `down`, `down-right`, `up-right`, `left`, `up`, `up-left`, `down-left`; the
word reads forward from `row`, `col` in that direction. The edition bundle keeps its four.

**`sudoku`**:

```json
{ "n": 1, "kind": "sudoku", "givens": ["53..7....", …9 rows…], "solution": ["534678912", …],
  "grade": "medium" }
```

`.` is an empty square.

**`ladder`**, one puzzle per ladder (the PDF prints three a page; `n` still counts ladders):

```json
{ "n": 1, "kind": "ladder", "from": "COLD", "to": "WARM", "steps": 3,
  "answer": ["COLD", "CORD", "CARD", "WARD", "WARM"] }
```

**`cryptogram`**:

```json
{ "n": 1, "kind": "cryptogram", "cipher": "QEB NRFZH YOLTK…", "plain": "THE QUICK BROWN…",
  "given": ["E", "T"], "source": "Anonymous" }
```

`cipher` and `plain` are the same length with the same non-letters; `given` are plaintext
letters the page prints as solved.

**`nonogram`**:

```json
{ "n": 1, "kind": "nonogram", "size": 15, "title": "Cat",
  "rows": [[3], [1,1], …], "cols": [[2], …], "solution": ["..###..", …],
  "picture": null }
```

`rows` and `cols` are the printed clue runs (derived, but written so a reader needn't derive
them); `solution` uses `#` shaded and `.` clear; `picture` is a `media/` path or `null`.

**`maze`**:

```json
{ "n": 1, "kind": "maze", "size": 20, "walls": [15, 13, …400 ints…],
  "start": [0, 0], "end": [19, 19], "solution": [[0,0], [0,1], …] }
```

`walls` is row-major, each square's standing walls as bits N=1, E=2, S=4, W=8, as
`puzzle::maze` keeps them.

### 3.3 `files`

As the edition bundle's `files` (§6.4): every file but `book.json`, with `path`, `size`,
`sha256`. The reader verifies a file's hash only when it opens it.

## 4. Returns

The reader writes one file per book and reader:

```text
.deckle-returns/<reader-id>/puzzles/<book-id>.json
```

```json
{
  "format": "deckle-puzzle-returns",
  "version": "1.0",
  "reader": { "id": "ipad-7F3A…", "name": "Jon's iPad", "app": "…" },
  "book": "6c1e3b0a-…",
  "updated_at": "2026-11-03T21:40:00Z",
  "puzzles": [
    { "n": 1, "done": true, "seconds": 412, "last": "2026-11-03T21:40:00Z",
      "grid": ["ABODE#SCARF", …] },
    { "n": 2, "done": false, "seconds": 90, "last": "…", "found": ["COURT", "MILL"] },
    { "n": 3, "done": false, "seconds": 0, "last": "…", "cells": ["53.67....", …] },
    { "n": 4, "done": true, "seconds": 30, "last": "…", "rungs": ["CORD", "CARD", "WARD"] },
    { "n": 5, "done": false, "seconds": 12, "last": "…", "key": "..E..T…" },
    { "n": 6, "done": false, "seconds": 0, "last": "…", "marks": [".#..", …] },
    { "n": 7, "done": true, "seconds": 300, "last": "…" }
  ]
}
```

- **Per kind,** the reader's state: `grid` (crossword letters, `#` blacks, space empty), `found`
  (words found), `cells` (sudoku), `rungs` (the blank rungs only), `key` (26 characters,
  plaintext letter guessed for each cipher letter A–Z, `.` unknown), `marks` (nonogram, `#`
  shaded, `x` marked clear, `.` untouched); a maze has `done` only.
- **`done`** is the reader's check (every answer matches), sticky once true.
- **Deckle reads it** into the tab's **Your books**: solved *n* of *N*, and time spent, beside the
  Kindle's note-derived count. Reader folders are merged by latest `last` per puzzle, as edition
  returns are (`§12` of the edition bundle design). A return for a book Deckle no longer has in
  `~/Deckle/Puzzles` is ignored.
- A schema: `docs/formats/deckle-puzzle-returns.schema.json` (phase 2).

### 4.1 The daily word search's return

The edition bundle has carried the daily word search since §6.6's `search` field, and the
reader is adding its control. The edition return's `puzzle` object gains an optional
`"search": { "found": ["COURT", "MILL"], "done": false }` (returns **1.2**, additive: the
schema's `version` enum gains `"1.2"` and `puzzle.search` is optional). Deckle marks the
found words in tomorrow's "yesterday's answers" as it marks the mini's; a 1.1 reader never
writes it, and Deckle before this change ignores it.

## 5. Changes to the build

- **`puzzle::book::bundle`** (new, in `deckle-digest`): `write(book: &Book, options:
  &BookOptions, dir) -> PathBuf`, the `.tmp` dance from `bundle.rs` lifted into a shared helper
  (`bundle::package::write_package`) so both writers use one. `cover.jpg` is the PDF's title
  page rendered through the existing cover drawing at 600×800.
- **Serialization types** live beside the PDF's (`book/bundle.rs`), mapping `Puzzles` to §3.2:
  crossword numbers from the slots as `bundle.rs` does for the mini; the word search's eight
  `Way`s to the eight `dir` strings; nonogram `rows`/`cols` derived from `cells`; the maze's
  `walls` as is.
- **`MadeBook`** (the tab's sidecar) gains `bundle: Option<PathBuf>` per destination written,
  and `solved_ipad: Option<(usize, usize)>` from returns.
- **Delivery:**
  - `puzzles_send` accepts `RunTarget::Folder` with `as_bundle: true`; the tab's menu shows
    **Send to iPad** for every export folder whose `format = "bundle"` (named by the export's
    `name`), beside **Send to <Kindle>** and **Save PDF…**. Never overwrites an existing bundle
    unless `replace` (the reader may hold ink on it) *(builder's call)*.
  - CLI: `deckle puzzles <kind> … --to <folder> --as bundle`, and `--as pdf` (the default).
  - **Sunday:** when a paper's `[puzzles] sunday = true` and any export is `bundle`, the morning
    run writes the book as a bundle beside the edition with `paper_id` and `sunday_of` set, after
    the edition is in place.
- **Returns:** `deckle-digest::returns` gains the `puzzles/` subfolder reader (phase 2) and the
  tab reads solved counts from it.
- **Schema and provenance:** `docs/formats/puzzle-book.schema.json` (MIT, ships with this spec),
  validated in the writer's tests, with a fixture book of every kind under `fixtures/puzzles/`.

## 6. Tests the build must add

- **Round trip:** every kind written and read back equals the `Book` (`cells`, `walls`,
  `solution`, clues), on the fixtures, with `--seed` fixed.
- **Schema:** every fixture validates against `puzzle-book.schema.json`; a bundle with a file
  missing from `files` fails the writer's own check.
- **Same book:** the PDF's puzzles and the bundle's puzzles come from one `make` call per
  book (asserted by the seed and the title page's text).
- **Package safety:** the `.tmp` rename, `book.json` last, hashes match; an interrupted write
  leaves no `.decklepuzzles` folder.
- **Sunday:** with a `bundle` export and `sunday = true`, the morning run writes both the edition
  and the book, in that order.
- **Returns (phase 2):** a returns file for the fixture book merges into the solved count; one
  for an unknown book is ignored; a malformed one is logged and skipped.

## 7. Versioning

As the edition bundle: `"<major>.<minor>"`, minors only add, a major is a change of meaning. The
reader ignores a `kind` it doesn't know and says so ("This book has puzzles this version can't
solve yet"). `1.0` is the first version and it carries all seven kinds.

## 8. Phasing

1. **Writer, schema, fixtures, CLI `--as bundle` (M).**
2. **Send to iPad and Sunday in the app (S–M).**
3. **Returns into Your books (M).**
4. **Later:** a `puzzle_book` press operation so the iPad can ask for a book.

## 9. Rulings asked of the owner

- **Where books go in iCloud:** beside the editions in the same folder (this design), or a
  `Puzzles/` subfolder? Beside keeps one folder picker in the reader; a subfolder keeps Files
  tidier.
- **Delete from the tab removes the bundle too?** This design says yes, with the same
  confirmation as the PDF.
