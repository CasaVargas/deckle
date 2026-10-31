# The puzzle page — design

**Status:** direction approved by owner (2026-10-22, "Deckle Daily ideas" canvas, board 4,
"Puzzle page, answers tomorrow"); document pending review · **Date:** 2026-10-22 ·
**Owner:** Jonathan (CasaVargas LLC) · **Builds on:** `2026-10-20-daily-digest-design.md`
(sections), `2026-10-22-highlights-export-design.md` (the clippings reader),
`2026-10-22-readership-report-design.md` (section keys, per-connection reads).

A new page near the back of the paper: a **5×5 mini crossword** whose theme answers come from
yesterday's headlines, a **word ladder**, and **yesterday's answers**. On a Kindle that keeps
`My Clippings.txt`, answers you write as notes are checked: "You solved 4 of 6."

*Board 5, The Serial, is parked.* It reads worse than the book itself on the same Kindle. The
part worth keeping, the paper nudging you back into what you're reading, belongs with From
Your Shelf (board 6).

## 1. Intent and why

A newspaper's puzzle page is the part people keep for later. It's also the one page a Kindle
can't get from any app: a crossword made **from your own paper**, printed with yesterday's
answers, with no account, no streak counter and nothing online. It gives a reason to open
tomorrow's edition, and that's the habit the Daily is built on.

**Success:** on 2026-10-22 the paper's *Puzzle* page shows the following:

- "The Mini · No. 41": a 5×5 grid with black squares, numbered.
- Across and Down clues. Two of the clues are yesterday's headlines with a word blanked out:
  "Leaf ____ peaks on the ridge this weekend (Local, yesterday)".
- A word ladder, COLD → WARM in four steps.
- *Yesterday's answers*: the filled grid and the ladder.
- On the Paperwhite, after answering two clues as notes yesterday, "You solved 2 of 10."

Rebuilding today's paper shows the same puzzle. Tomorrow's answers match what you saw.

## 2. Goals, non-goals, anti-goals

**Goals**

1. **A new section, `puzzle`.** It has its own config name and readership key (`puzzle`).
   It's on in the starter config, at the back, after the horoscope and before "On This Day".
   Existing configs don't get it added on their own; the Daily tab lists it to switch on.
2. **The mini.** A 5×5 grid from a fixed set of symmetric templates, filled from Deckle's own
   word list (§4).
   - **Theme entries:** one or two entries are words taken from yesterday's headlines,
     clued as that headline with the word blanked. The headline's section is named.
   - **The rest** are clued from the word list's short dictionary definitions.
3. **The word ladder.** It turns one 4-letter word into another, one letter per rung, every
   rung a real word. The shortest path is 3 to 5 rungs. The pair comes from a built-in list of
   opposites and near-pairs (COLD/WARM, LOVE/HATE, …).
4. **Yesterday's answers.** Yesterday's filled grid (small) and its ladder, when Deckle built
   a puzzle yesterday.
5. **Stable.** The puzzle for a date is made once and stored. Every rebuild that day, and the
   email and USB copies, print the same one, and tomorrow's answers are its answers.
6. **Checked answers, on Kindles that allow it.** Highlight a clue's text and add a note with
   the answer. When that Kindle next connects, Deckle reads the notes from `My Clippings.txt`
   for the edition (§5), and the next paper prints "You solved N of M" beside the answers.
   Kindles whose notes can't be read over USB (Scribe 2024, #99) get the answers without a
   score.
7. **Degrades.** If there are no headlines from yesterday (first run, or a day missed), the
   mini has no theme entries and is clued from the dictionary only. If the grid can't be
   filled, the mini is left out and the ladder still prints. If nothing can be made, the
   section prints "Unavailable today (…)" like any other.
8. **CLI parity:** `deckle digest puzzle` prints today's stored puzzle and yesterday's
   answers as text, and `--json` gives the same as data.

**Non-goals (this increment)**

- **AI-written clues.** No headline or story text goes to any model. The blanked headline is
  the news flavour, offline.
- **Sudoku, cryptics, bigger grids, difficulty settings.** Later, if the page gets read.
- **Solving on the device beyond notes.** No interactive KFX, no handwriting recognition on
  the Scribe.
- **Streaks, stats pages, leaderboards.**
- **The Serial** (board 5): parked, see above.

**Anti-goals**

- The answers printed tomorrow don't match the puzzle printed today.
- An answer or a clue that's offensive, a slur, or a proper noun nobody could guess. The word
  list is filtered (§4).
- A clue that contains its own answer, or a blanked headline that still shows the word
  elsewhere.
- A puzzle failure that stops the paper from building.

## 3. Constraints

- Rust, in `crates/deckle-digest` (a `puzzle` module or a small `deckle-puzzle` crate; the
  builder decides). The section goes through the normal `gather` path and is keyed `puzzle`.
- **No network.** The word list is embedded in the binary, gzipped, and stays well under 1 MB.
- **State** goes in `puzzles.json` next to `digest.toml`, keeping the last 60 days. For each
  date it holds:
  - the grid, entries, clues and ladder;
  - the headline words saved for the next day;
  - the solve result, once it's known.

  It's written through a temp file and a rename, like the other state files.
- **Deterministic:** generation is seeded by the date, so a lost `puzzles.json` regenerates
  the same puzzle as long as the word list and headlines are unchanged.
- **E-ink:** the grid is an XHTML table. Black squares are filled cells, numbers are small in
  the corner, and nothing relies on colour. It must look right on the Scribe and the
  Paperwhite (§7).
- **Speed:** a whole puzzle in under a second on the build machine. Grid filling is a
  backtracking search with a time cap (the builder sets it; about 500 ms). On timeout it tries
  the next template, then gives up on the mini.

## 4. Words and clues

**The list.** Deckle's Word Wise database can't be used: it drops the most common words on
purpose, so it's missing CARD, AREA and the like. The list is new, built by the same offline
builder (`data/wordwise`) from the same sources, Wiktionary via kaikki.org and wordfreq (both
CC BY-SA 4.0, credited on the page):

- Lowercase words of 3 to 5 letters, a–z only, at least zipf 3.0, so a person knows them.
- One everyday sense each, with its short definition as the clue. Senses marked
  archaic, rare, slang, vulgar, derogatory or obsolete are skipped, as are proper nouns,
  abbreviations and plain inflections ("cards" when "card" is in).
- A checked-in blocklist file for anything the filters miss.
- Each clue must not contain the answer or an obvious form of it. Entries that fail are
  dropped at build time.
- The builder's README records the counts and sizes.

**Theme words.** When today's paper is built, Deckle saves the words from each news section's
headlines that are in the list, are 3 to 5 letters, aren't on a stop list ("the", "says") and
aren't in the edition's place name. It saves the word, the headline and the section. Tomorrow's
mini tries to place up to two of them, preferring longer words and different sections.

- The clue is the headline with that word replaced by `____` (every occurrence), then
  "(<Section>, yesterday)".
- A headline over 90 characters is cut at a word boundary with "…", keeping the blank.

**The grid.**

- A handful of rotationally symmetric 5×5 templates (open, two corners, four corners,
  centre-cross); the builder picks them from standard mini layouts.
- Every white run of 3 or more is an entry. Templates with 2-letter runs aren't allowed.
- The fill uses each word at most once per grid.
- Theme words go into slots first, then the rest are filled by search, most-constrained slot
  first.
- A grid never repeats one from the last 60 days. A 5×5 has few fills: one theme word can
  allow only a handful, and a headline that runs two days running gives the same theme word
  twice. When the fill matches a recent grid, the search runs again with the next seed for
  the date (still deterministic). After a few tries it drops the theme words. Otherwise
  "Yesterday's answers" would solve the puzzle printed above it (2026-10-26, No. 5 was No. 4).

**The ladder.**

- A built-in list of about 60 pairs of common 4-letter words.
- Each day's pair is chosen by the date, never reusing one from the last 60 days.
- The path is found by breadth-first search over the list's 4-letter words.
- A pair whose shortest path is under 3 or over 5 rungs is skipped for the next.
- The page shows the two ends and blank rungs. The answer shown is one shortest path.

## 5. Checking answers

- **Reading notes.** When the Kindle connects, Deckle reads `My Clippings.txt` in the same
  connection as push, prune, readership and clippings. It uses the existing reader and does
  this whenever the puzzle section is on, even with `[sync] clippings` off. It looks for the
  last 7 editions' entries: author `Deckle`, title the edition's.
- **Matching a note to a clue.** A note attaches to its highlight by location (highlights
  §4). If the highlighted text contains a clue's text, ignoring case, punctuation and the
  number, the note is that clue's answer.
- **Counting.** The note's letters are uppercased and spaces dropped, then compared to the
  entry. Each clue counts once; the latest note wins.
- **What gets printed.** The result is stored with that date's puzzle. The next paper prints
  "You solved N of M" (M is the number of clues). It also prints the ladder as "Ladder: solved"
  when a note on the ladder line spells the full path, or the answer path otherwise.
- **Kindles without readable notes, and days with no notes:** no score line, just the
  answers.
- **On the Kindle tab:** nothing new. The score lives in the paper.

## 6. Where it shows up

- **The paper:** the *Puzzle* section.
  - Its kicker: "Answers in tomorrow's paper".
  - The articles: "The Mini · No. N" (grid, then Across and Down), "Word ladder", then
    "Yesterday's answers" (small grid, ladder path, score line).
  - Under the mini, one line: "Highlight a clue and add your answer as a note; tomorrow's
    paper checks it." It's left out on Kindles where notes can't be read (`[puzzle] notes =
    false`, set automatically for MTP-only Kindles; the builder decides how).
  - The credit, small at the bottom: "Words and definitions from Wiktionary and wordfreq
    (CC BY-SA 4.0)".
- **`digest.toml`:** `puzzle` in `sections`. An optional `[puzzle]` table:
  - `ladder = true`;
  - `theme = true` (blanked headlines on or off).

  Defaults apply when it's absent.
- **Daily tab:** "Puzzle" appears in the section list with the others. No new card.
- **CLI:** `deckle digest puzzle [--json]` (goal 8).

## 7. Testing

- **The word list:**
  - Its size is in range.
  - No entry is on the blocklist, and none has a capital or a non-letter.
  - No clue contains its answer.
  - The embedded list round-trips through the gzip.
- **Grid:**
  - Every template fills for 30 consecutive dates within the time cap.
  - Every entry is in the list and no word repeats.
  - Across and Down agree letter by letter.
  - It's deterministic for a fixed date and inputs.
- **Theme:**
  - From fixture headlines, the chosen words appear blanked in their clues with the section
    named.
  - The word appears nowhere else in that clue.
  - The 90-character cut keeps the blank.
  - With no headlines, the clues come from the dictionary only.
- **Ladder:**
  - Every rung is a list word and differs from the last by one letter.
  - The length is 3 to 5.
  - No pair repeats within 60 days.
- **State:**
  - A second build the same day reuses the stored puzzle, byte for byte.
  - Tomorrow's "Yesterday's answers" equals today's entries.
  - Entries older than 60 days are pruned.
  - A missing state file regenerates the same puzzle.
- **Checking:** fixture `My Clippings.txt` entries test each of these:
  - a correct note;
  - a wrong note;
  - a note with spaces and lower case;
  - two notes on one clue (the latest wins);
  - a note on a non-clue highlight (ignored);
  - a note on another edition (ignored).

  The score line follows. With no clippings source, no line.
- **Degrading:**
  - A template fill that times out falls back to the next template.
  - All templates failing gives a ladder-only page.
  - Everything failing gives "Unavailable today".
  - The paper still builds in every case.
- **Golden page:** a fixed date, list and headlines render to a checked-in XHTML snapshot that
  passes `xmllint`.
- **On the owner's Kindles:**
  - The page reads well on the Scribe 2024 and the Paperwhite, and the grid stays square.
  - On the Paperwhite: highlight two clues, add notes, plug in, and the next paper shows the
    score.

## 8. Left to the builder

- Module or crate placement, and the template set.
- The search heuristics and the time cap.
- The stop list and the exact zipf cut.
- The ladder pair list.
- How the "notes can't be read" line is turned off for MTP-only Kindles.
- The CSS for the grid.

The builder also updates the docs: a "Puzzle page" section in `docs/daily.md`, the starter
config, and the CHANGELOG.
