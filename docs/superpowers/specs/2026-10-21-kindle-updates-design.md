# Keeping Kindles current — design

**Status:** stale rule, duplicate handling and update flow settled by owner (2026-10-21); built in
the PR for #38 and #39 · **Date:** 2026-10-21 · **Owner:** Jonathan (CasaVargas LLC)
**Builds on:** `2026-10-21-library-and-device-view-design.md` (the library, send records, the
Kindle tab)

The Kindle tab says which of your books Deckle could build better now, and puts them back on
the Books tab with one click. Sending a book again no longer leaves the old copy behind under
its old name.

## 1. Intent and why

Deckle's X-Ray and Word Wise keep improving, but books already on a Kindle keep whatever they
were built with. Nothing tells you which ones are behind, and updating them means finding each
one. Separately, re-sending a book after its title or author changed lands a second copy next to
the first, because device file names come from the metadata.

Success: after a Word Wise database update, the Kindle tab says "3 books can be updated".
**Update all** puts them on the Books tab in the mode they were sent in. Send, and the Kindle
has three current copies and no duplicates.

## 2. Goals, non-goals

**Goals**

1. **Engine stamps.** Each send record notes the X-Ray engine version (when X-Ray shipped)
   and the Word Wise database version (when Word Wise shipped).
2. **"Can be updated"**, only for books the library has (there's a source to rebuild):
   - **Older build:** the record has no stamps (sent before this change); or X-Ray shipped
     with an older engine than this Deckle's; or Word Wise shipped with a database other than
     the one Deckle has now. When there's no database to compare with (never downloaded),
     Word Wise isn't judged.
   - **Missing features:** the record says X-Ray or Word Wise shipped, but its file isn't in
     the book's `.sdr` on the Kindle (removed, or lost in a Kindle reset).
3. **Update all** (Kindle tab) puts every book that can be updated on the Books tab, each with
   the mode it was last sent in (PDOC/EBOK) as its choice. You review and Send. Each row also
   offers **Update**. CLI: `deckle library reprocess --to device` lists them, asks (or `--yes`),
   then rebuilds and sends each one. One failure doesn't stop the rest, and the old copy stays
   until its replacement is written.
4. **No duplicates from Deckle.** When a send writes a book whose ASIN Deckle already put on
   that Kindle under another file name, the old file, its `.sdr` and its thumbnail (with its
   restore copy) are removed in the same connection. The result says `replaced <old name>`. Only
   Deckle's copies are touched: a `…DECK` ASIN, or a file the library's send record for this
   Kindle names.
5. **Duplicate badge.** The Kindle tab marks books that share an ASIN, or failing that a
   normalized title and first author. This catches copies from before this change and copies
   from elsewhere. Remove (from #34) cleans them up; nothing is removed automatically.

**Non-goals (this increment)**

- Updating books when your settings change (X-Ray turned on later, another Word Wise level).
  Re-send does that today.
- Updating books that aren't in the library, and automatic updates (e.g. on connect).
  Re-sending changes files on the device, so it always waits for you.
- Mode changes: an update keeps PDOC/EBOK. Upgrade (from #34) is the way to change it.

## 3. Engine stamps

- `deckle_xray::ENGINE`, a `u32` starting at `1`. It is bumped by hand whenever extraction,
  descriptions or the sidecar format change what a book gets. The X-Ray summary carries it.
- The Word Wise database version (`Lexicon::version`, e.g. `2026.10+3`) goes into the Word Wise
  summary. "Deckle has now" is the cached database's version, or the `--wordwise-db` file's.
- `SendRecord` gains `xray_engine: Option<u32>` and `wordwise_db: Option<String>`. Both are
  `None` when that feature didn't ship. A record with neither, written before this change, is
  told apart by a new `stamped: bool` (missing reads as `false`). The schema stays 1: older
  Deckles keep the new fields as unknown fields.

`deckle_library::freshness(book, device, current) -> Freshness { older: Vec<Reason>,
missing: Vec<Feature> }` decides from the record, the device listing's sidecar flags and
`current = { xray_engine, wordwise_db: Option<String> }`. It is a pure function, tested on its
own.

## 4. Replacing Deckle's older copy at send time

- `RunOptions.replaces: Vec<String>` names files under `documents/` that this send makes
  obsolete. Right after delivery, over the same connection (a second MTP session would reset the
  Kindle), the pipeline removes them with `inventory::remove_on`: the file, its `.sdr`, thumbnail
  and restore copy. They're reported as `RunReport.replaced`. A file that's already gone is
  skipped. A file that can't be removed is a warning; the new copy is already on the Kindle.
- **Who decides:**
  - On mass storage and folders, the pipeline finds other `documents/` KFX files with the same
    `…DECK` ASIN, using the inventory's cached details.
  - The caller (CLI, app) adds files that the library's send record for this Kindle names for
    this book, when the name differs. This is the only way on MTP, where books aren't read.
- A name that's equal to the one being written is never in `replaces`. Delivery already
  overwrites it.

## 5. Surfaces

- **Kindle tab:** a banner "N books can be updated · Update all". Rows get badges ("Older
  build", "Missing X-Ray", "Missing Word Wise", "Duplicate") and an **Update** item in their
  menu. Hovering a badge shows the reason ("X-Ray engine 1 → 2", "Word Wise database
  2026.09 → 2026.10", "sent before Deckle tracked versions").
- **Books tab:** an update arrives like a Re-send: the library source, with the recorded mode
  as the row's choice. The result line lists `replaced …` when it applied.
- **CLI:**
  - `deckle kindle books` shows the same badges.
  - `deckle library reprocess --to <device|folder> [--yes] [--dry-run]` prints the plan
    (book, reason, mode), then runs it.
  - `run` prints `replaced …`.

## 6. Verification

- `freshness`:
  - an unstamped record is older;
  - equal stamps are current;
  - an older X-Ray engine, or a different Word Wise database, is older;
  - Word Wise isn't judged when there's no current database;
  - a record that says X-Ray shipped, with no X-Ray file on the device, is missing X-Ray.
- Send records written after the change carry the stamps, and an old sidecar still loads and
  keeps its fields.
- A send with `replaces` removes the named old file and its `.sdr` after the new one is
  written, skips names already gone, and leaves a store book alone.
- The pipeline on a temp mass-storage Kindle finds a `…DECK` copy under an old name and
  replaces it.
- CLI `library reprocess --dry-run` changes nothing; `--yes` with the fake Previewer to a folder
  updates the stamps and leaves one copy per book.
- The duplicate badge groups by ASIN, then title+author.
- App: the banner, the badges and Update all are checked in the running app against the fake
  Kindle image with the real Previewer.
