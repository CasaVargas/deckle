# From Your Shelf — design

**Status:** direction approved by owner (canvas board 6), redesigned for the Scribe after #99;
document pending owner review · **Date:** 2026-10-22 · **Owner:** Jonathan (CasaVargas LLC)
**Builds on:** `2026-10-22-readership-report-design.md` (the `.yjf` decoder, reads in the
connection the sync already holds, section keys), `2026-10-22-highlights-export-design.md` (the
clippings reader), `2026-10-20-daily-digest-design.md` (sections) · **Amends:** the readership
non-goal "Books": Deckle now notes where you are in each *book* on the Kindle, for this page
only. It keeps no reading history beyond the last snapshot per Kindle. · **Absorbs:** the useful
part of the parked Serial (board 5): the paper nudging you back into the book you're reading.

A page near the back of the paper, **From Your Shelf**, taken from your own Kindle the last time
it was plugged in: **On your nightstand** (the books you're in the middle of, how far in, and how
long since you opened them), and, on Kindles that share them, **a highlight from long ago** and
**words you looked up this week**.

## 1. Intent and why

Every other page of the paper is the world. This one is you: the book you left at 62% on
Tuesday, the passage you marked a year and a half ago, the word you had to look up. A news app
can't print it, and Deckle already holds the cable that can read it.

**What #99 changed.** The owner's main Kindle is a Scribe 2024, over MTP. It shares neither
highlights nor notes over USB (they're in system storage, #99, closed as not possible), and it
hides `system/`, where the looked-up words live (`system/vocabulary/vocab.db`). So the mockup's
first and third parts can't work on it. What the Scribe *does* share is the `.yjf` sidecar in
every opened book's `.sdr` folder, which readership already decodes. So the page is redesigned
**Scribe-first**: the nightstand, built from `.yjf` positions, is the core and works everywhere.
The highlight and the words are extras that appear when the Kindle that connected shares them
(the Paperwhite, over mass storage) and are simply absent otherwise.

**Found in the fixtures, 2026-10-22.** Besides `fpr` and `lpr`, every `.yjf` carries a
`book.info.store` object: a long and a double. In `fixtures/yjf/book.yjf` they are 9519 and
0.1505; in `daily-2026-10-21.yjf`, 4611 and 0.2984. The double reads as the Kindle's own
"fraction of the book read" and the long as words read so far (4611 / 0.2984 ≈ 15,500 words,
about right for that edition's 106,062 positions). On the paper it is *not* `lpr / end` (that's
97% after a contents jump), so it counts words read, not where you are. It is the only
progress number the Kindle shares for a book whose length Deckle doesn't know. The `.yjf` also
holds `timer.average.calculator`: words-per-minute samples (322, 333, 104, 629 in `book.yjf`),
which is what the Kindle's own "time left in book" uses. Both meanings are inferred from three
files and get checked on the Scribe before anything prints them (§7, task 1 of the plan).

**Success:** the owner reads *The Count of Monte Cristo* on the Scribe, puts it down on Tuesday
night, and leaves a mushroom guide untouched for three weeks. Wednesday morning the Scribe takes
the paper over USB. Thursday's paper has a *From Your Shelf* page:

> **ON YOUR NIGHTSTAND**
> *The Count of Monte Cristo* — 62% · last read Tuesday
> *A Field Guide to Fall Mushrooms* — 18% · not opened in 23 days
> *The Wind in the Willows* — 91% · nearly done
>
> Pick up *The Count of Monte Cristo* tonight? You left off 62% in.

When the Paperwhite was the last Kindle plugged in, the page also has "You highlighted this 19
months ago" with the passage and the note, and "Words you looked up this week".

## 2. Goals, non-goals, anti-goals

**Goals**

1. **A new section, `shelf`,** titled *From Your Shelf*, with config name and readership key
   `shelf`. In the starter config, after `feeds` and before `lottery`. Existing configs don't
   get it on their own; the Daily tab lists it to switch on.
2. **On connect, take a snapshot** of each Kindle's shelf, once per connection, in the same
   session as push, prune, readership and clippings: every book's `.yjf` (§4), and when the
   Kindle shares them, `My Clippings.txt` and `system/vocabulary/vocab.db` (§5, §6). Stored in
   `shelf.json`, one snapshot per Kindle, replaced each time.
3. **On your nightstand** (§4): up to three books in progress, each with how far in and how
   long since you last opened it, and one nudge line for the most recent.
4. **You highlighted this…** (§5): one past highlight, with its note, chosen fresh each day
   without repeats.
5. **Words you looked up this week** (§6): up to three, with the sentence you met each in and,
   when Deckle's Word Wise database is already on the Mac, a short definition.
6. **Degrade by part.** Each part prints only when it has something. The section is left out
   of the paper (not "unavailable") when none of the three has anything, like Your Reading on a
   day with nothing saved. Kindles that don't share highlights or words never produce an error
   for it.
7. **Deckle-sent books know their length.** A send records the KFX's end position, so the
   nightstand's percent for those books is exact (§4.2).
8. **CLI parity:** `deckle digest shelf [--json]` prints the snapshot summary and what today's
   page would show. `digest push` and `watch` take the snapshot and say so in one line
   ("Shelf: 3 books in progress, 1 highlight source, 0 words").

**Non-goals (this increment)**

- **"About 9 hours left at your pace"** and "one sitting from the end". The Kindle's pace
  samples are in the `.yjf`, but their meaning is inferred, and a wrong time on the page is
  worse than none. The snapshot keeps the decoded numbers (`words_read`, `wpm`) so a follow-up
  can print it once checked against the Kindle's own "time left in book".
- **Chapters** ("chapter 40 tonight?"). Needs the KFX's navigation, which means reading the
  whole book over MTP. Later.
- **The Scribe's highlights and notes.** Not reachable over USB (#99). If a firmware ever
  writes them back to `.yjr`, they join §5.
- **KOReader's own files** (`metadata.*.lua`: progress and annotations, seen on the owner's
  Scribe in #99). A separate source, later.
- **`.azw3f` / `.mbp1` sidecars** (older sideloaded formats). Same KRDS container, but their
  positions aren't KFX ids. `.yjf` (KFX) only.
- **Reading history, streaks, minutes per day.** One snapshot per Kindle, replaced, no log.
- **A vocabulary quiz** on the puzzle page (the mockup's "Quiz on Saturday's puzzle page").
- **Store books the inventory walk doesn't reach** (if the Scribe keeps them deeper than
  `documents/<folder>/`; checked in task 1). The walk is the inventory's, unchanged.

**Anti-goals**

- A wrong number on the page. A percent comes from a known source (§4.2) or isn't printed.
- The paper telling anyone else what you read: nothing leaves the Mac, except that an
  **emailed** paper carries this page through Amazon, said on the Daily tab when email is on.
- A family member's Kindle filling *your* paper. `[sync] shelf` per device turns it off for one
  Kindle.
- A shelf read failing or slowing a delivery past reason: a failed read is reported beside the
  delivery, and the walk's cost is measured on the Scribe (§3).
- Today's Deckle Daily (or any edition) listed as a book on the nightstand.

## 3. Constraints

- Rust, in the existing workspace. CLI parity. Plain JSON state, temp file and rename.
- **Crates.** `deckle-digest` depends on no other Deckle crate, and `deckle-readership` already
  depends on `deckle-digest`. So:
  - `deckle_digest::shelf` owns the `shelf.json` types, the selection rules (§4–6) and the
    section. The build only reads the file.
  - A new crate `deckle-shelf` takes the snapshot: it turns what the device read returned into
    `shelf.json` entries, using `deckle-readership` (KRDS), `deckle-highlights` (clippings),
    `deckle-library` (titles via `shelve`), `rusqlite` (vocab) and `deckle-core` (Word Wise
    database path). It is kept apart from readership, whose report stays editions-only.
- **The device read** lives in `deckle-core` and rides in the caller's `Conn`:
  - **Books:** the inventory walk (`inventory::list_on`), extended to also fetch each book's
    `.yjf` when asked. Over MTP the walk already lists every `.sdr` folder (for the X-Ray and
    Word Wise flags), so the added cost is one `get_object` of a few hundred bytes per opened
    book, nothing else. Mass storage reads the files. Editions (`deckle-daily-*`) are skipped.
  - **Clippings:** the existing `clippings::read_on`. It runs when the shelf is on for that
    Kindle even with `[sync] clippings` off (as the puzzle plan does for its notes).
  - **Words:** `system/vocabulary/vocab.db` (and `vocab.db-wal` beside it, if present), by path
    on mass storage, and tried once with `find_path` over MTP (absent on the Scribe; another
    Kindle may share it). Copied to a temp folder and opened read-only there, never in place.
- **Once per connection,** like readership: `Plan` and `Actions` gain `shelf`, and the watcher
  marks it done for the connection. An idle plug-in runs it alone (`IDLE_SESSION_OK`).
- **Cost on the Scribe:** measured in task 1 (the owner's library, over MTP). MTP object info
  carries no usable modified time here and `.yjf` sizes barely change, so there's no cheap
  "unchanged" test. If the walk plus reads takes over 10 seconds, the fallback is a cap: the
  first 300 `.yjf` files read, the rest counted as skipped. The plan records the measurement.
- **Gating:** the snapshot is taken when `shelf` is in `sections` and `[sync] shelf` is on for
  that Kindle (default `true`, per-device override under `[sync.devices."<id>"]`). The Kindle
  tab takes it in its own session too (`shelf_on`, like readership's `record_on`).
- **Section shape:** a normal `Section` with key `shelf`, title "From Your Shelf", kicker
  "Taken from your Kindle (<name>) when it was last plugged in, <weekday>" (or "<n> days ago"
  past 6 days), and one `Article` per part that has content. E-ink: text and a plain progress
  bar drawn with a bordered `div` (no colour meaning; the percent is printed beside it).

## 4. On your nightstand

### 4.1 The snapshot of a book

For every book file the walk finds (`kfx` only, §2) whose `.sdr` holds a `.yjf`:

```json
{"file": "The Count of Monte Cristo.kfx", "title": "The Count of Monte Cristo",
 "authors": ["Alexandre Dumas"], "asin": "B0ABCDDECK",
 "last": 412004, "furthest": 413590, "last_read": "2026-10-20T03:12:00Z",
 "end": 664520, "kindle_fraction": 0.61, "words_read": 280113}
```

- **Title:** the KFX's own metadata (mass storage, through the existing details cache), else the
  library book matched by `shelve` (ASIN or send record), else the file name with its extension,
  an `_EBOK`/`_PDOC` suffix and a trailing ASIN removed. No usable title: the book is left out.
- **Positions:** `fpr`, `lpr` and its time, from `deckle_readership::parse_yjf`, which gains
  `book.info.store` (`words_read: Option<i64>`, `kindle_fraction: Option<f64>`) and the
  `timer.average.calculator` samples (`wpm: Vec<f64>`, kept, not printed). An unreadable
  `.yjf` is a counted error, not a failure.
- **End:** see §4.2.

### 4.2 How far in

In this order, the first that applies:

1. **Deckle-sent, with a recorded end.** `SendRecord` gains `end: Option<i64>`, the KFX's end
   position (`deckle_kfx::text::content_runs`' second value), written at send time from the KFX
   Deckle just built. Matched through the library (`shelve`). Percent = `last / end`.
2. **Mass storage, any KFX.** The details cache already reads the whole KFX once per file, size
   and time; it also records `end`. Percent = `last / end`.
3. **Otherwise** (store books and older Deckle sends over MTP): the Kindle's own
   `kindle_fraction`, printed as "about N%". Only when task 1 confirms it tracks the Kindle's
   progress display within 5 points on two books; if it doesn't, this rule is dropped and those
   books print without a percent ("last read Tuesday").

Percent is rounded down and capped at 99 while `last < end`. DRM'd store KFX that Deckle can't
parse fall to rule 3.

### 4.3 Which books

A book is **in progress** when all of these hold:

- it has an `lpr` time, within the last **60 days** of the edition date;
- its percent, when known, is from **1 to 97**: below is a peek, above is finished;
- its file isn't a Deckle Daily edition (`EDITION_PREFIX`);
- its title isn't on `[shelf] hide` (case-insensitive substring).

Two Kindles with the same book (same ASIN, else same title and first author): the one with the
later `lpr` time wins. Ordered by `lpr` time, newest first; **at most three** printed.

### 4.4 What each line says

`<Title>` in italics, a bar, then `<percent> · <state>`, where state is the first that applies:

| Condition | Words |
| --- | --- |
| percent ≥ 90 | "nearly done" |
| last read ≥ 14 days before the edition date | "not opened in N days" |
| last read on the edition date or the day before | "last read yesterday" (or "today") |
| within 6 days | "last read <Weekday>" |
| otherwise | "last read <Mon D>" |

Without a percent: the bar is omitted and the line is just the state. Days count in the Mac's
local time zone.

**The nudge** (the Serial's useful part): under the list, for the newest book when it was read
within 7 days and isn't "nearly done": "Pick up *<Title>* tonight? You left off <percent> in."
(Without a percent: "Pick up *<Title>* tonight?") When it's nearly done: "*<Title>*: one more
evening should finish it." No nudge when the newest book is 7 or more days old.

## 5. You highlighted this…

**Source:** the clippings snapshot. Each connection that read `My Clippings.txt` replaces that
Kindle's highlight list in `shelf.json`: parsed and grouped by `deckle-highlights` (`parse`,
`group`), editions dropped, each highlight kept with its attached notes, page, location, added
date and its `entry_id`. A Kindle with no clippings file (the Scribe) keeps an empty list and
`highlights_shared: false`.

**Which one, each day:**

- Candidates: highlights 40 to 600 characters long, with a parsed added date.
- Prefer age: those added at least **180 days** before the edition date; if none, at least
  **30 days**; if none, the part is left out.
- Among those, ones with a note first (a note is the owner talking to his future self).
- Not shown in the last **90** editions (`shown` in `shelf.json`, id → date).
- Picked by a generator seeded with the date, so a rebuild the same day prints the same one.

**Printed:** the heading "You highlighted this <age> ago" (age in weeks under 2 months, months
under 2 years, else years: "19 months", "3 years"), the passage as a blockquote, then
"<Title>, <Author> · page <p> · <Month D, YYYY>" (page left out when absent), then "Your note:
<text>" when there is one.

## 6. Words you looked up this week

**Source:** `vocab.db` (the Kindle's Vocabulary Builder, SQLite):

- `LOOKUPS(word_key, book_key, usage, timestamp)` joined to `WORDS(id, word, stem, lang,
  category)` and `BOOK_INFO(id, title)`.
- Kept per Kindle in `shelf.json`: lookups from the last **14 days** before the snapshot, as
  `{word, stem, usage, book, at}`, English only (`lang = "en"`), words marked mastered
  (`category = 100`) skipped.

**Which:** lookups in the **7 days** before the edition date, one per stem, newest first, at
most **three**.

**Printed:** "**<word>**, <short definition>." when Deckle's Word Wise database is already
downloaded on the Mac (`deckle_core::wordwise`, looked up by stem at snapshot time; this feature
never downloads it), then the sentence you met it in (`usage`, trimmed to 160 characters at a
word boundary) and "<Book>, <Weekday>". No database: word, sentence, book and day.

## 7. `shelf.json`

Next to `digest.toml` (with a `#[serde(skip)] shelf_path` test override, like `almanac_path`):

```json
{
  "kindles": {
    "<sync id>": {
      "name": "Kindle Scribe", "seen": "2026-10-21T11:02:00Z",
      "books": [ "…§4.1…" ],
      "highlights_shared": false, "highlights": [],
      "words_shared": false, "words": []
    }
  },
  "shown": { "<entry_id>": "2026-10-22" }
}
```

- A snapshot replaces that Kindle's entry whole. `shown` keeps 90 days.
- A Kindle whose snapshot is over **30 days** old is ignored by the page.
- Missing or corrupt: empty.

**Privacy.** The file holds titles, reading positions, highlight text and looked-up words, on
the Mac only, like the highlights export and `readership.json`. It's never logged, sent or
cached elsewhere. Turning the section off doesn't delete it; `deckle digest shelf --forget`
does (and the Daily tab has a Forget button).

## 8. Where it shows up

- **The paper:** the section, as above. The kicker names the Kindle the newest snapshot came
  from; parts from other Kindles (say, highlights from the Paperwhite) add "(from your
  <name>)" to their heading.
- **`digest.toml`:** `shelf` in `sections`; `[sync] shelf = true` with per-device overrides; an
  optional `[shelf]` table: `nightstand = true`, `highlights = true`, `words = true`, `hide =
  []`. Left out of a save while untouched.
- **Daily tab:** "From Your Shelf" in the section list, with one status line under it: "From
  your Scribe, Tuesday: 3 books in progress. Highlights and words aren't shared by this
  Kindle." A Forget button. When email delivery is on: "Emailed papers include this page, so
  it passes through Amazon."
- **Settings → Kindles:** the per-Kindle checkbox *Use this Kindle for From Your Shelf*
  (`[sync.devices."<id>"] shelf`).
- **CLI:** `deckle digest shelf [--json] [--forget]`; push and watch report one line.
- **Kindle tab:** nothing new on screen; its session records the snapshot.

**Readership.** The section is keyed `shelf` and gets its bar like any other. On days it's left
out it has no layout entry, which the report already handles ("not enough editions yet"). Its
heading is matched by title like every section.

## 9. Testing

- **KRDS:** `book.yjf` → `kindle_fraction` ≈ 0.1505, `words_read` 9519, four `wpm` samples;
  `daily-2026-10-21.yjf` → 0.2984 and 4611; existing `fpr`/`lpr` tests unchanged.
- **Device read:** through a folder target and the fake MTP: books with and without `.yjf`, an
  edition skipped, a book in a subfolder, `vocab.db` present and absent, one MTP session.
- **End recorded at send:** a send of the fixture EPUB records `end` equal to
  `content_runs(kfx).1`.
- **Selection rules:** fixed snapshots and dates for each rule in §4.3–4.4, §5 and §6
  (boundaries: 60 days, 1/97%, 14 days, 90 days, 180/30 days, 7 days).
- **Two Kindles:** the later `lpr` wins; highlights from one, books from the other; a 31-day-old
  snapshot is ignored.
- **Vocab:** a fixture `vocab.db` built in the test with the real schema: English only,
  mastered skipped, one per stem, WAL present.
- **Degrading:** each part absent alone; all absent → no section and a build note; the
  Scribe-shaped snapshot (books only) prints the nightstand alone.
- **Golden page:** fixed snapshot and date → checked-in XHTML that parses as XML.
- **Sync:** the shelf read rides in the same connection as push (one MTP open); a failed read
  leaves the delivery intact; once per connection in the watcher; off for a device with
  `shelf = false`.
- **On the owner's Kindles (task 1, before building the page):** on the Scribe over MTP, time
  the walk on the real library; compare `kindle_fraction` with the progress the Kindle shows
  for two books; note where store books sit. On the Paperwhite: confirm `.yjf` files exist for
  KFX books over mass storage and `vocab.db` copies and opens. **At the end:** the page on both
  Kindles, and the Daily tab visually.

## 10. Calls I made (for the owner)

1. **Scribe-first.** The nightstand, from `.yjf`, is the core; the highlight and words appear
   only when the Kindle shares them, with no error when it doesn't.
2. **No "time left at your pace"** in this increment. The data seems to be there
   (`timer.average.calculator`), but its meaning is inferred from three files. The snapshot
   keeps it for a checked follow-up.
3. **Percent by source:** exact for Deckle-sent books (end recorded at send) and for any KFX on
   mass storage; the Kindle's own fraction ("about N%") for the rest, only if it checks out on
   the Scribe; otherwise no percent. No whole-book reads over MTP.
4. **"In progress"** = opened in the last 60 days, 1–97% through, not an edition; three books,
   newest first. The Serial's nudge is one line for the newest book.
5. **Highlight:** 180+ days old (fallback 30), notes first, no repeat within 90 editions,
   stable per date.
6. **Words:** last 7 days, three, the Kindle's own sentence; a definition only from a Word Wise
   database already on the Mac.
7. **A new `deckle-shelf` crate** for the snapshot, with the page in `deckle_digest::shelf`;
   readership stays editions-only.
8. **One snapshot per Kindle, replaced;** no history. Ignored after 30 days.
9. **`[sync] shelf`,** default on, per device, so a shared or family Kindle can be left out.
10. **The read reuses the inventory walk;** cost measured on the Scribe, with a 300-book cap as
    the fallback if it's slow.
11. **Section placement:** after `feeds`, before `lottery`, in the starter only.
12. **Clippings are read whenever the shelf is on,** even with the highlights export off.

## 11. Left to the builder

The exact Tauri command names, the CSS for the bar, the file-name cleaning beyond §4.1, and the
heading wording's small variations within §4–6. Docs: a "From Your Shelf" section in
`docs/daily.md` (what each Kindle shares and why the Scribe shows only the nightstand), the
`.yjf` `book.info.store` note in `docs/devices.md`, the starter config, the README's CLI lines,
and the CHANGELOG.
