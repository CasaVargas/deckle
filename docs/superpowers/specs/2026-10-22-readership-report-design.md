# Readership report — design

**Status:** direction approved by owner (2026-10-22, "Deckle Daily ideas" canvas, board 2); document
pending review · **Date:** 2026-10-22 · **Owner:** Jonathan (CasaVargas LLC)
**Builds on:** `2026-10-20-daily-digest-design.md` (the edition and its sections),
`2026-10-20-auto-sync-design.md` (one connection per plug-in), `2026-10-22-highlights-export-design.md`
(reading back from the Kindle in that same connection) · **Amends:** v1 non-goal "reading"
(§2 of `2026-10-18-deckle-design.md`): Deckle now records how far you got in each *edition of the
paper*. It still doesn't track books.

How far you read each morning's paper, taken from the Kindle's own reading position when it
connects, shown on the Daily tab as "how far you got" per section, with a few suggested edits
to the paper you can apply with one click.

## 1. Intent and why

The paper is built from a config the owner guesses at: which sections, in which order. Nothing
tells him whether the horoscope near the end is ever reached. The Kindle already knows: every
book it opens gets a `.yjf` sidecar holding the furthest position reached and the last read
position. Deckle builds every edition itself, so it knows where each section starts. Put the two
together and the paper can be edited by evidence instead of guesses.

It's local and passive: no tracking pixel, no account, nothing leaves the Mac. The Kindle wrote
the numbers; Deckle reads them back over the cable it already uses.

**Found on hardware, 2026-10-22 (#99):** on a Kindle Scribe 2024 the `.yjf` is readable over MTP
and is written when you leave the book, e.g.

```
fpr  {position: "AQoEAAAAAAAA:103303"}                          furthest position reached
lpr  {position: "AQoEAAAAAAAA:103303", time: 2026-10-21T22:06:30}   last read
page.history.store  [821, 1015, …]                              origins of a few jumps
```

The number after `:` is the KFX position id, the coordinate system `deckle_kfx::text` already
uses. A never-opened edition has no `.yjf`. *(Checked against a Deckle edition the same day: in
`deckle-daily-2026-10-21.kfx`, `fpr` 103303 is exactly the first character of the "On This Day"
heading, reached by a contents jump, and the page-history positions land on front-page items.)*

**Success:** for two weeks the Scribe gets the paper over USB each morning. The Daily tab shows
"opened 12 of 14", how far into the paper you usually get, and one bar per section: the share of
opened editions where you got to it. A section you almost never reach comes with **Drop it**,
and clicking it removes the section from `digest.toml`, keeping the file's comments.

## 2. Goals, non-goals, anti-goals

**Goals**

1. **Record each edition's layout at build time:** the start position of every section, the end
   position, the title and date, and the file name it ships under. Written next to the other
   state files, so it survives the edition being pruned off the Kindle.
2. **Read the reading position back** each time a Kindle connects: for every edition still on
   it, its `.yjf` (`fpr`, `lpr` and its time). Over mass storage and MTP, in the same connection
   as push, prune and clippings, **before** prune removes old editions.
3. **Turn it into a report:** per edition: opened or not, how far in (percent and section), last
   read time; across the last 14 editions built: editions opened, the median furthest point of
   the opened ones, and per section (in the current paper order) the share of opened editions
   where you got to it.
4. **Suggest edits, applied with one click** (§6): drop a section you rarely reach, or move a
   feed up.
5. **Show it** on the Daily tab (the board 2 layout) and in the CLI:
   `deckle digest readership [--json]`.
6. **On by default.** It's local, reads a few hundred bytes per edition, and needs no setup.
   `[sync] readership = false` (and per device) turns it off.

**Non-goals (this increment)**

- **Books.** Only Deckle Daily editions. Reading stats for books are a different feature with
  different privacy expectations.
- **Page numbers.** The paper is built without a page list (`paper_options` sets `pages: None`),
  so the Kindle shows no page numbers for it, and "page 19" would mean nothing on the device. The
  report speaks in sections and in percent through the paper. Turning page numbers on for the
  paper changes what ships every morning; that's a separate decision.
- **Telling reading from jumping.** `fpr` moves forward on a contents jump too, so jumping to *On
  This Day* marks every section before it as reached. The report says "got to or past", and this
  error only ever *hides* a Drop suggestion, never invents one. `page.history.store` records the
  origins of a few jumps, too few to reconstruct a session; not used in v1.
- **Time spent per section.** `timer.model` isn't per section. "Got to" is the honest unit.
- **Editions that went by email only.** No KFX is built for them on the Mac (the edition goes as
  EPUB), so there's no layout to compare against. If the same edition also went by USB
  (`usb_too`), it's tracked.
- **A page cap setting.** The board's "Cap the paper at about 24 pages" needs the build to fit a
  budget, which is its own design. The report states the fact ("you usually stop about 55% in")
  without a button.
- **Readership inside the paper** (e.g. "yesterday you got to Science" on the front page). A
  later, small add-on once the numbers have proven useful.
- **The Scribe's highlights** (#99, closed: not reachable over MTP).

**Anti-goals**

- A suggestion that is really just "it's late in the paper": every later section is reached less,
  so a bare low share proves nothing on its own (§6 says how suggestions avoid this).
- Editing `digest.toml` without a click, or losing its comments or order when it is edited.
- A failed `.yjf` read failing a delivery (same rule as clippings).
- Anything leaving the Mac.
- Counting the same morning twice because two Kindles both had it.

## 3. Constraints

- Rust, in the existing workspace; the desktop app reaches it through Tauri commands. CLI parity.
- **One connection.** The read rides in the `Conn` the sync already holds: Kindle tab
  (`device_books` / `shelf_on`), the watcher (`sync.rs`, `run_with`), `deckle digest push`. On an
  idle plug-in it runs alone (`IDLE_SESSION_OK` is `true`, checked on the Scribe).
- **Order inside the connection:** read positions → push → prune. Prune deletes the edition and its
  `.sdr`; once it's gone, so is the `.yjf`.
- **Where the `.yjf` is:** `documents/<stem>.sdr/<stem><suffix>.yjf`, where `<stem>` is the
  edition's file name without `.kfx` and `<suffix>` is a per-device hex string (e.g.
  `9d6e5f44…`). Match the folder by stem and take the one `.yjf` inside; ignore `.yjr.bad_file`
  and other files.
- **KRDS decoding:** port the subset of `krds.py` (K-R-D-S/KRDS, GPL-3.0, Deckle's licence) needed
  for `.yjf`: the container (signature `00 00 00 00 00 1A B1 26`, typed values, named objects) and
  the `fpr` and `lpr` objects. Skip unknown objects and trailing fields without failing: this
  firmware already adds fields `krds.py` doesn't know. Keep the attribution.
- **Positions → sections:** the layout recorded in Goal 1 is a list of section starts as KFX
  position ids plus the end position (`content_runs`' second value). A position belongs to the
  last section starting at or before it; percent through = position / end.
- **Section keys aren't in the edition today.** The gathered `Section` has a title, kicker,
  articles and note, but no kind or feed name, and sections are skipped on some days (Your
  Reading with nothing saved). `build` has to carry each section's key out alongside the EPUB.
- **`digest.toml` writes keep comments, including inside `[[feeds]]`.** `config::save` merges
  tables and scalars in place but replaces arrays of tables whole (`merge_table`), which would
  drop every comment in the feeds. Extend it to merge `[[feeds]]` entries element by element,
  matched by `name` (dropped entries removed, order taken from the new config). This also fixes
  the Daily tab's existing feed saves.
- Plain JSON state, like `highlights.json`: no database.

## 4. The layout of an edition

Recorded right after the edition's KFX is built (the `kfx` path in `digest run`, before it's
queued). Each section of the paper is one XHTML file in the EPUB, so one section of the KFX.

```json
{
  "date": "2026-10-21",
  "title": "The Deckle Daily",
  "file": "deckle-daily-2026-10-21",
  "end": 106062,
  "sections": [
    {"key": "front",             "name": "Front page",   "start": 4},
    {"key": "reading",           "name": "Your Reading", "start": 1204},
    {"key": "weather",           "name": "Weather",      "start": 20511},
    {"key": "feeds:Ars Science", "name": "Science",      "start": 61873}
  ]
}
```

- `key` identifies the section across editions: the `SectionKind` (`reading`, `weather`, `local`,
  `lottery`, `horoscope`, `onthisday`), `front` for the front page, and `feeds:<name>` for each
  `[[feeds]]` entry (by its `name`). `name` is what the paper prints.
- `file` is the stem the edition ships under, which is the `.sdr` folder name on the Kindle.
- Older editions (built before this) have no layout and are left out of the report.
- **Rebuilt the same day:** the new layout replaces the old one. If the section starts changed,
  that date's `reads` are discarded too: positions from the old build would map to the wrong
  sections.

## 5. What a connect records

`readership.json` in Deckle's config folder (next to `digest.toml`, `library.json`,
`highlights.json`):

```json
{
  "editions": { "2026-10-21": { "...layout from §4..." } },
  "reads": {
    "2026-10-21": {
      "<device id>": {"opened": true, "furthest": 103303, "last": 103303,
                      "last_read": "2026-10-21T22:06:30", "seen": "2026-10-22T07:02:11"}
    }
  },
  "dismissed": { "drop:lottery": "2026-10-22" }
}
```

- For every edition with a layout and still on the Kindle: no `.sdr` or no `.yjf` → `opened:
  false`; otherwise the positions. A later connect overwrites that device's entry with the newer
  numbers (the Kindle only moves `fpr` forward).
- **Two Kindles, one edition:** the report uses the furthest of the devices for that date. One
  morning counts once.
- Keep the last 60 editions; drop older entries on save.
- Written with a temp file and a rename. A missing or corrupt file starts empty.

## 6. The report and its suggestions

**The report** covers the last 14 editions that have a layout (so a week off doesn't empty it).

- **Opened:** editions with any device reporting `opened`, out of those 14.
- **How far in:** the median of `furthest / end` over the opened ones, as a percentage, and the
  section it usually falls in.
- **Per section**, in the **current** `digest.toml` order (sections that no longer exist drop out;
  a new section shows "not enough editions yet" until it has appeared in 5): the share of opened
  editions, among those that had the section, in which `furthest` reached that section's `start`.

**Suggestions** need at least 7 opened editions. Each is a rule, chosen so that a section isn't
condemned just for being late. Never suggested for the front page or Your Reading (it's added
back whenever you save something, so dropping it wouldn't stick, and it's first by default).

1. **Drop it.** You got to the section in at most 20% of opened editions, **and** it starts in
   the last third of the paper (median `start / end` across the window). Copy: "*Lottery*: you
   got to it on 2 of 14 days. It's about 6% of the paper." Button **Drop it** removes it: a
   `SectionKind` from `sections`, or that entry from `[[feeds]]`.
2. **Move it up.** Same low share, for a `[[feeds]]` entry (a source you chose on purpose).
   Copy: "*Science*: you got to it on 3 of 12 days, about 60% in. Move it ahead of *Local*?"
   Button **Move up** moves it one place earlier: within `[[feeds]]`, and when it's already
   first among feeds, `feeds` moves one place earlier in `sections`. A feed that meets both rules
   gets this one, never both.
3. **Where you stop** (no button): when the median furthest point is below 75%, "You usually stop
   about 55% in, in *Science*." This is the fact behind the board's page cap.

At most three suggestions, rule order then lowest share. **Dismiss** hides one for 14 editions
(`dismissed` in `readership.json`). Applying one writes `digest.toml` through
`deckle_digest::config::save` (with the `[[feeds]]` merge from §3), then the report recomputes.

## 7. Where it shows up

- **Daily tab:** a **How far you read** panel (board 2 layout, the app's existing styles): the two
  numbers, the section bars (low shares shown muted), and a **Suggested edits** list with its
  buttons. With fewer than 3 opened editions: "Plug your Kindle in after reading and this fills
  in." With readership off: one line and a link to the setting.
- **Settings → Kindles:** the checkbox *Note how far I read the paper*, writing `[sync]
  readership` (and per device under `[sync.devices."<id>"]`, like `clippings`).
- **CLI:** `deckle digest readership [--json]` prints the report; `deckle digest readership
  apply <n>` applies suggestion `n`. `deckle digest push` and `watch` record positions as part of
  their connection and say so in one line ("Readership: 3 editions read").
- **Kindle tab:** nothing new.

## 8. Testing

- KRDS: decode the real `.yjf` files pulled from the Scribe on 2026-10-22 (fixtures; they hold
  reading positions and speeds, nothing personal): `fpr`, `lpr`. A synthetic file with an
  unknown object and extra trailing fields still yields `fpr` and `lpr`.
- Layout: build a small edition through the real pipeline (or the existing KFX fixture path) and
  check each section's `start` is the position of its heading's text run (`deckle_kfx::text`),
  starts are increasing, and keys match the config (a day without Your Reading has no `reading`).
- Mapping: position → section at boundaries (exactly at a start, one before); percent of `end`.
- Report math: fixed `readership.json` → opened count, median, per-section shares; two devices
  take the furthest; a section added mid-window; fewer than 7 opened → no suggestions.
- Suggestion rules, including the anti-goal: a section with a low share that starts before the
  last third is not dropped; a feed meeting both rules gets Move up only; front page and Your
  Reading never get one.
- `config::save` with a commented `[[feeds]]`: dropping and reordering a feed keeps the other
  entries' comments; the Daily tab's existing save tests still pass.
- Rebuild the same date with different sections: the layout is replaced and that date's reads
  dropped.
- Sync order: through the fake `Transport`, positions are read before prune deletes the edition;
  a `.yjf` read failure lands in the report and the delivery still succeeds.
- **On the owner's Scribe:** read an edition partway, go Home, plug in, check `deckle digest
  readership`; then the Daily tab visually. **On the Paperwhite:** confirm mass-storage editions
  get a `.yjf` in the same place (not yet checked).

## 9. Left to the builder

Crate placement (a `deckle-readership` crate, or modules in `deckle-digest` for the report and
`deckle-core` for the device read), how the section starts are pulled from the KFX (the section
order and position maps, or the heading run of each section), the exact Tauri command names, and
the panel's markup. Docs: a "How far you read" paragraph in `docs/daily.md`, the CLI lines in the
README, a CHANGELOG entry, and a `docs/devices.md` note on `.yjf`.
