# KOReader reads: highlights and progress from any reader running KOReader — design

**Status:** accepted, 2026-10-30 (the owner picked it from the e-reader parity list and handed
the design calls to the builder) · **Date:** 2026-10-30 · **Owner:** Jonathan (CasaVargas LLC)
**Builds on:** `2026-10-23-other-readers-design.md` (§3 rules out KOReader *plugins*; §8.2 Kobo
reads), `2026-10-22-readership-report-design.md`

KOReader runs on Kindles (jailbroken), Kobos, PocketBooks, Boox tablets and reMarkables. It keeps
each book's highlights, notes and progress in a plain Lua file beside the book:
`<book>.sdr/metadata.<ext>.lua`. Deckle reads these files, never writes them, and nothing gets
installed on the reader. Highlights then reach the highlights export, and progress reaches From
Your Shelf, on readers whose own reader app Deckle can't read: a PocketBook, a Kindle reading in
KOReader, or a Boox whose folder syncs back to the computer.

## 1. Goals

- **Highlights and notes** from KOReader on any drive Deckle already reads (Kindle over USB
  storage, Kobo, PocketBook), and in any folder `deckle highlights --from <folder>` is pointed at.
- **Books in progress** from KOReader on a Kobo or PocketBook feed From Your Shelf, beside a Kobo's
  own.
- Read-only. A sidecar that doesn't parse is skipped with a warning, never fatal.

## 2. Non-goals

- No KOReader plugin, no writing to KOReader's files, no KOReader sync server.
- No Readership (per-section reading of the paper). KOReader's position is an xpointer into the
  EPUB, which needs its own mapping to the paper's sections. Later.
- No From Your Shelf progress from KOReader on a Kindle: the Kindle shelf is built from `.yjf`
  positions and KFX ends. Highlights from it are in.
- No MTP. A Kindle over MTP and a Boox are only reached through a folder (for a Boox, a folder a
  two-way sync app like Syncthing brings back).

## 3. Finding sidecars

`deckle_core::koreader::find(root)` walks `root` at most 8 folders deep and at most 20,000
entries, and returns each folder named `*.sdr` that holds a `metadata.<ext>.lua` (not `.old`, not
`.bak`). It skips `.kobo`, `.Trashes`, `.Spotlight-V100`, `.fseventsd` and `System Volume
Information`. KOReader keeps its sidecars in three places:

- **Beside the book (KOReader's default):** `Deckle/book.sdr/metadata.epub.lua` belongs to
  `Deckle/book.epub`.
- **In KOReader's `docsettings/` folder:** the book's absolute path on the device, mirrored. The
  device prefix (`mnt/onboard/` on a Kobo, `mnt/us/` on a Kindle, `mnt/ext1/` on a PocketBook) is
  dropped to give the path on the drive.
- **In `hashdocsettings/`:** no path. Highlights still go in under the sidecar's own title, and
  the shelf keys the book as `koreader:<sidecar folder name>`.

## 4. Reading a sidecar

The file is a Lua table literal (`return { … }`), parsed by a small reader in
`deckle_core::koreader::lua`: strings with KOReader's `%q` escapes, numbers, booleans, `nil`, and
tables with `["key"]`, `[n]` or bare-name keys. Nothing is evaluated.

- **Title and authors:** `doc_props.title` and `doc_props.authors` (newline-separated). With no
  title, the book file's stem.
- **Progress:** `percent_finished` (0–1). A book is in progress when `summary.status` is absent
  or `"reading"` and 0 < percent < 1. Its last-read time is the sidecar's modification time,
  since KOReader writes the file when the book closes.
- **Highlights (KOReader 2024.01 and later):** each entry of `annotations` that has `pos0` or a
  `drawer` is a highlight of its `text`. A `note` makes a second, note entry. `pageno` gives the
  page and `datetime` the date.
- **Highlights (older KOReader):** each entry in each page of `highlight` is a highlight with
  `text`, `datetime` and the page key. Notes in the old `bookmarks` table are not read.

## 5. Into Deckle's shapes

- **Highlights** are written as `My Clippings.txt` entries, as a Kobo's are:
  `Title (Author)\n- Your Highlight on page N | Added on <English date>\n\n<text>\n==========`.
  Oldest first. The writer is shared with `kobo` (`kobo::clipping`).
- **Books** are `kobo::Book`, with `content_id` set to the book's path on the drive. That matches
  the `file` key the shelf already uses for a Kobo's books, so a book a Kobo and KOReader both
  know is one book: the more recently read copy wins.

## 6. Where it's read

- **`clippings::read_on`:** for a folder target, or a Kindle on USB storage, KOReader's
  clippings are appended to `My Clippings.txt`'s (or stand alone when there's none). This reaches
  `deckle highlights` and the Kindle sync's highlights and shelf highlights.
- **`sync::run_reader`:** a Kobo's database read and a KOReader read of the drive are merged. A
  PocketBook, read for nothing until now, gets the KOReader read alone. Its shelf books go through
  `kobo_snapshot` as a Kobo's do.
- **`deckle highlights --from device` with a Kobo or PocketBook:** the same merge.

## 7. Tests the build must add

- The Lua reader: nested tables, `%q` escapes (`\"`, `\\`, a backslash-newline, `\ddd`), negative
  and float numbers, and comments before `return`.
- A new-format sidecar (annotations with a note) and an old-format one (`highlight` by page).
- `find` on a fake Kobo with a sidecar beside the book, one in `.adds/koreader/docsettings/mnt/
  onboard/…`, and one in `hashdocsettings`.
- A PocketBook drive with a KOReader sidecar: `run_reader` reports its clippings and books.
- A folder target with only KOReader sidecars: `read_on` returns their clippings.
- The clippings round-trip through `deckle_highlights::parse` with title, author, page and date.

## 8. Device checks owed (`docs/devices.md`)

A real KOReader sidecar from each of a Kobo, a PocketBook and a jailbroken Kindle: the fields
named in §4 are present, and highlights reach the export.
