# The Morgue — design

**Status:** direction approved by owner (2026-10-22, "Deckle Daily ideas" canvas, board 3, "The
Morgue (every edition, searchable)"); document pending review · **Date:** 2026-10-22 ·
**Owner:** Jonathan (CasaVargas LLC) · **Builds on:** `2026-10-20-daily-digest-design.md`
(editions), `2026-10-22-readership-report-design.md` (how far you read),
`2026-10-22-highlights-export-design.md` (highlights), `2026-10-20-auto-sync-design.md` (push
and prune).

A newspaper's morgue is its archive of back issues. Deckle's keeps **every edition** you've had
in a plain folder on your Mac. It's **searchable**, it shows on the Daily tab as a month of
front pages, and any edition can be **sent to the Kindle again**.

## 1. Intent and why

The Kindle keeps only the newest three editions (`keep = 3`), and then a story is gone. Today
Deckle's copies sit in `~/Library/Caches/deckle/digest`. That's a cache folder, which macOS and
cleaners such as CleanMyMac empty, nothing lists it, and nothing can search it. "That piece
about the mill being sold, three weeks ago" can't be found.

The paper is already built and stored on the Mac; it only needs to be kept on purpose and made
findable. It's files-first: plain EPUBs and small JSON files in a folder you can see, back up or
point at iCloud Drive, with no database.

**Success:** on 2026-10-25 the owner opens the Daily tab and sees **Every edition**:

- October 2026 as a calendar, each day with its masthead and lead headline. Days with no
  paper are blank, and tomorrow says "Tomorrow, 05:30".
- They type `mill` and three days are marked, with the matching story titles listed.
- Clicking October 21 shows "Wednesday, October 21 · No. 294", the front page's weather line
  and lead story, "You read about 91%, to Local", "2 passages highlighted", and "No longer on
  the Kindle".
- They click **Send again**. The edition goes over at the next plug-in and stays on the Kindle
  for a week, even though it's older than the newest three.
- `deckle digest morgue search mill` lists the same three stories.

## 2. Goals, non-goals, anti-goals

**Goals**

1. **Keep every edition.**
   - When an edition is built (`digest build`, `run`, the schedule, the app's Build), Deckle
     copies its EPUB into the Morgue with a small sidecar (§4).
   - After conversion it also copies the KFX, for fast re-sending.
   - Email-only editions are kept too: the EPUB exists.
2. **A plain folder.** The default is `~/Deckle/Morgue`, next to the Inbox. It can be changed
   in `digest.toml` (`[morgue] folder`) or on the Daily tab. Layout: `YYYY/YYYY-MM-DD.epub`,
   `YYYY-MM-DD.json` and `YYYY-MM-DD.kfx`.
3. **First run imports** the editions still in the cache folder, `deckle-daily-*.epub` with
   any matching KFX in `digest/kfx/`, so existing papers aren't lost.
4. **Search.**
   - It matches words in headlines, teasers and the stories' full text, across every kept
     edition.
   - Results group by edition, newest first, with the matching story titles and their
     section.
   - It's case- and accent-insensitive, and every word must match (`mill brewery` finds
     stories with both).
5. **The Daily tab: Every edition.**
   - A month calendar, Monday to Sunday, with ‹ › to change month. Each day with an edition
     shows the paper's name and its lead headline. While searching, matched days are marked
     and the rest dimmed.
   - A detail panel for the selected day:
     - "Weekday, Month D · No. N";
     - the front page's weather line and lead story (headline and teaser);
     - how far you read, from Readership: "You read about N%, to <section>", or "Not opened"
       or nothing when unknown;
     - "N passages highlighted", from the highlights export state, when known;
     - "On the Kindle" or "No longer on the Kindle", from the last connection;
     - **Send again**, **Open** (the EPUB in the system's default app) and **Show in Finder**.
   - Under a narrow window, the calendar becomes a list of days.
6. **Send again.**
   - The edition goes into the outbox: its kept KFX, or one converted again from the EPUB when
     the KFX is gone (Kindle Previewer needed, as for any book).
   - It's delivered at the next connection, or now if a Kindle is plugged in, like any queued
     edition.
   - A re-sent edition is **exempt from prune for 7 days** after it's delivered, so `keep = 3`
     doesn't remove it on the spot.
7. **How long.** `[morgue] keep_days = 365` keeps editions for a year and removes older ones
   when a new edition is archived; `0` keeps everything. KFX copies are kept for 14 days only,
   since the EPUB is enough to send again. The Every edition card shows the folder and its size
   ("Morgue: 214 editions, 590 MB").
8. **CLI parity:**
   - `deckle digest morgue list [--month YYYY-MM] [--json]`;
   - `deckle digest morgue search <words…> [--json]`;
   - `deckle digest morgue show <date> [--json]`;
   - `deckle digest morgue send <date>`.

**Non-goals (this increment)**

- **Long reads and Inbox books.** They're books and belong in the library, not the Morgue.
- **Reading inside the app.** Open hands the EPUB to the system's reader. No in-app viewer.
- **Syncing the Morgue between Macs.** That's #8 territory. A folder on iCloud Drive works
  today without Deckle doing anything.
- **Search over photos, or highlights' own text.**
- **Re-sending by email.** Send again goes over USB. Email delivery stays a once-each-morning
  thing.

**Anti-goals**

- An edition is lost because a cache was cleaned.
- Search sends anything off the Mac, or needs an index database you have to manage.
- Send again's edition is pruned off the Kindle at the same plug-in it arrives.
- Archiving fails a build. An archive error is a note on the run; the paper still ships.
- The Morgue rewrites or moves files you put there yourself, or deletes anything but its own
  dated editions past `keep_days`.

## 3. Constraints

- Rust. The Morgue logic goes in a small crate (`deckle-morgue`) or a module of
  `deckle-digest`; the builder decides. It's used by the CLI (`digest build`, `run`, `push`,
  `watch`) and the desktop app (Daily tab commands).
- **The sidecar is written at build time** from the gathered `Edition`. Search reads the
  sidecars, never the EPUBs. Sidecars stay small: plain text, no HTML, no photos.
- **Search speed:** a year of editions (365 sidecars) searched in well under a second on the
  owner's Mac, scanned in memory each time. No index file. If that ever gets slow, an index is
  a later change.
- **Paths:** `~` is expanded the same way as `[reading] inbox`. Writes go through a temp file
  and a rename. Two processes archiving the same date (the app and the schedule) end with one
  complete copy; the later one wins.
- **Prune exemption:** the delivery record (the date it was re-sent and delivered) lives in the
  Morgue's own state file, `morgue.json` next to `digest.toml`. The prune step in
  `deckle_core::sync` consults the exemption through the `Prune` options it already takes.
- **Readership and highlights** are read from their existing state files. If those are absent
  the lines are left out; the Morgue never fails because of them.

## 4. The sidecar

`YYYY-MM-DD.json`, one per edition:

```json
{
  "date": "2026-10-21", "number": 294, "paper": "The Morning Deckle", "place": "Linden Falls, NC",
  "weather": "Clear · 63°/38°F · 0% rain",
  "lead": { "section": "Local", "title": "Leaf season peaks on Hogback Ridge this week",
            "teaser": "If you have been waiting for the right weekend…" },
  "sections": [
    { "key": "local", "title": "Local", "stories": [
      { "title": "…", "teaser": "…", "text": "plain text of the story, whitespace collapsed" } ] }
  ],
  "files": { "epub": "2026-10-21.epub", "kfx": "2026-10-21.kfx" },
  "stem": "deckle-daily-2026-10-21"
}
```

- `lead` is the front page's first lead headline: the same choice the front page makes
  (`Edition::headlines`).
- `text` is each article body with tags stripped and entities decoded. It's capped per story
  (the builder sets the cap; about 20,000 characters), so a long read can't bloat the file.
- `stem` ties the edition to its Readership and highlights records.
- Imported editions (goal 3) get their sidecar by reading the EPUB: the section pages and the
  article headings the renderer wrote. Fields Deckle can't recover (weather, teasers) are left
  empty.

## 5. Where it shows up

- **Builds:** a successful archive is quiet. A failed one adds a note to the run report
  ("Morgue: couldn't save the edition (…)").
- **Daily tab:** the **Every edition** card goes after **How far you read**. It has the
  search field, the calendar and the detail panel. The folder line and **Change…** go in the
  card's footer.
- **`digest.toml`:** the starter config gets a commented `[morgue]` block (`folder =
  "~/Deckle/Morgue"`, `keep_days = 365`). When the block is absent the defaults apply.
- **CLI:** goal 8. `search` prints `date · section · title` lines, newest first.

## 6. Testing

- **Archive:**
  - Building an edition writes the EPUB and a sidecar whose stories match the gathered
    edition.
  - Converting copies the KFX.
  - An archive error becomes a note and the build still succeeds.
- **Import:** a cache folder with two EPUBs and one KFX gives two archived editions, sidecars
  recovered from the EPUB, and one KFX.
- **Search:**
  - Case and accents are ignored (`cafe` finds "Café").
  - All words must match.
  - Results come newest first, grouped by edition.
  - Matches are found in a story's text, not just its headline.
  - Empty and whitespace-only queries return nothing.
  - 365 synthetic sidecars are searched under a second (release build).
- **Retention:**
  - At `keep_days = 365`, a 400-day-old edition is removed when a new one is archived.
  - At `0`, nothing is removed.
  - KFX copies older than 14 days are removed and their EPUBs kept.
  - A file that isn't a dated edition is never touched.
- **Send again:**
  - The kept KFX is queued.
  - With the KFX gone, a conversion from the EPUB is queued.
  - After delivery, prune with `keep = 3` keeps the re-sent edition for 7 days and removes it
    on day 8.
- **Detail:**
  - The Readership line comes from a fixture `readership.json`; it's absent with no file.
  - The highlights count comes from a fixture highlights state.
  - The on-Kindle line follows the last recorded presence.
- **CLI:** `list`, `search`, `show` and `send`, with and without `--json`, against a temp
  Morgue.
- **Desktop:** a vitest for the calendar layout (the month grid starts on Monday, with blanks
  before the first day). Check the card visually in the running app: browse months, search,
  open a day, Send again.

## 7. Left to the builder

- Crate or module placement, and the story text cap.
- Recovering sidecars from imported EPUBs.
- How the prune exemption is passed into `deckle_core::sync`.
- The Tauri command names, the calendar's CSS, and the narrow-window breakpoint.

The builder also updates the docs: "The Morgue" in `docs/daily.md`, the CHANGELOG, and the
starter config.
