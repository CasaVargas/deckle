# Highlights out, as Markdown — design

**Status:** direction approved by owner (2026-10-22, "Deckle Daily ideas" canvas, board 9);
document pending review · **Date:** 2026-10-22 · **Owner:** Jonathan (CasaVargas LLC)
**Builds on:** `2026-10-21-library-and-device-view-design.md` (Kindle tab, device access over
mass storage and MTP) · **Amends:** v1 non-goal "reading" (§2 of `2026-10-18-deckle-design.md`):
Deckle now *reads back* what you marked on the Kindle. It still doesn't track reading.

Every highlight and note on a Kindle, written as one Markdown file per book into a folder you
choose (an Obsidian or Tolaria vault, iCloud Drive, anything), and kept up to date each time the
Kindle connects.

## 1. Intent and why

Amazon's own highlight export (the notebook on read.amazon.com, "Export notes" by email) covers
store books only. Sideloaded books, which is every book Deckle sends, never show up there. The
Kindle does record them, in `documents/My Clippings.txt`, but that's one long, append-only text
file mixing every book. Nobody reads it.

Deckle already talks to the Kindle over both transports. Reading one small text file back and
turning it into tidy, plain Markdown fits Deckle's files-first approach: no account, nothing in
the cloud, output that works in any notes app.

This is also the first of three ideas that read data back from the Kindle. The clippings reader
built here is reused by **From Your Shelf** (a past highlight in the paper) and by the **puzzle
page** (answers typed as notes). Those have their own specs.

**Success:** highlight two passages in a sideloaded *Walden* and add a note to one of them. Plug
the Scribe in and press **Write highlights** on the Kindle tab (or let the morning schedule do it
with the paper). `~/SecondBrain/Reading/Walden.md` appears with both
passages, the note under its passage, and page and date for each. Highlight one more passage and
plug in again: the file gains one entry, and anything you typed into it yourself is still there.

> **Found on hardware, 2026-10-22:** the Kindle Scribe (2024) doesn't write `My Clippings.txt`;
> its annotations are per-book `.yjr`/`.yjf` files. This design covers Kindles that keep the
> clippings file (checked on a Paperwhite); reading `.yjr` is #99.

## 2. Goals, non-goals, anti-goals

**Goals**

1. **Read** `documents/My Clippings.txt` from the connected Kindle, over mass storage and MTP.
2. **Parse** it into entries: book title, author, kind (highlight, note or bookmark), page if
   present, location range, date added, and text.
3. **Write one Markdown file per book** into the chosen folder: YAML frontmatter, then each
   highlight as a blockquote with its page, location and date, and any note attached under it.
4. **Append only.** A later export adds only entries Deckle hasn't written before. It never
   rewrites, reorders or deletes what's already in a file, so your own edits are safe.
5. **Three ways to run it:** automatically by the background watcher (`[sync] clippings`, §6),
   a **Write highlights** button on the Kindle tab, and `deckle highlights export` in the CLI
   (with `--to <dir>` and `--json`, like the other commands). This is issue #60, the
   "clippings pull" that `digest.toml` already reserves a key for.
6. **The Kindle tab shows it:** a Highlights section listing each book with its count and how
   many are new since the last export, and a preview of the Markdown that would be written.

**Non-goals (this increment)**

- **Looked-up words** (`system/vocabulary/vocab.db`). `system/` isn't exposed over MTP on the
  Scribe (`docs/devices.md`), so this only works on mass-storage Kindles. It belongs to the From
  Your Shelf spec, which decides what to do on MTP.
- **Handwriting.** Ink on the Scribe (on PDFs, sticky notes, Active Canvas) is stored in Amazon's
  own format and isn't in the clippings file. Out of scope.
- **Reading position and progress.** That belongs to the Readership spec. The frontmatter
  carries no `progress:` field, because append-only means Deckle never rewrites the frontmatter
  (owner's mockup showed one; dropped for that reason).
- **Chapter headings.** The clippings file doesn't record chapters. Entries are ordered by
  location, with page and location shown. (The mockup grouped by chapter; dropped.)
- **Removing entries you deleted on the Kindle.** The clippings file keeps deleted highlights
  too, and append-only rules out retracting anything already written.
- Syncing anything back to the Kindle, or any network call.
- **An in-app on-connect watcher.** The desktop app doesn't push or prune on connect today (the
  auto-sync spec puts that out of scope), and two processes can't share an MTP Kindle. Automatic
  export belongs to `deckle digest watch`, which already owns the Kindle on plug-in. The app
  exports when you press the button.

**Anti-goals**

- Deckle overwrites or reorders text the user typed into an exported file.
- The same highlight appears twice in a file after repeated exports or after plugging in a
  second Kindle.
- An export fails silently. A clippings file Deckle can't parse is reported with the line it
  stopped at, and the entries it could parse are still written.
- Anything leaves the Mac.

## 3. Constraints

- Rust, in the existing workspace. The desktop app reaches it through Tauri commands like the
  rest of the Kindle tab. CLI parity is required (v1 goal 7).
- MTP: `Mtp::find_path` + `get_object` read whole files. The clippings file is usually well under
  a few MB. Read it once per connection and never open an extra session for it:
  - **Kindle tab:** fold the read into the existing `device_books` call (one `Conn`), and return
    the parsed entries with the book list.
  - **Watcher:** read it in the same `Conn` as push and prune (`sync.rs`, auto-sync spec §4).
    An idle plug-in (nothing to push) opens no session today, because opening one used to send
    the Scribe to its home screen. Since `usb.rs` stopped resetting on close, that may no longer
    happen. **Check this on the owner's Scribe before building the watcher part.** If opening a
    session is now harmless, the watcher also exports on idle plug-ins, once per connection. If
    it still disrupts, export rides along with a push only, which is still at least daily while
    the paper is scheduled. *(Checked 2026-10-22 on a Scribe 2024: harmless, since plugging
    in already shows "connected to computer" and the Kindle stays on the bus. The watcher
    exports on idle plug-ins.)*
- Mass storage: read `<mount>/documents/My Clippings.txt` directly.
- The file is UTF-8, usually starting with a BOM, with CRLF or LF line endings.
- Plain files only: no database, no index in the output folder (library design §2.1 spirit).

## 4. The clippings file

Each entry is separated by a line of exactly `==========`:

```
Walden (Henry David Thoreau)
- Your Highlight on page 88 | Location 1340-1342 | Added on Monday, March 3, 2025 10:14:22 AM

I went to the woods because I wished to live deliberately…
==========
Walden (Henry David Thoreau)
- Your Note on page 88 | Location 1342 | Added on Monday, March 3, 2025 10:15:03 AM

read this again before the cabin trip
==========
```

- **Line 1** is `Title (Author)`. The author is the last parenthesised group. Titles can contain
  parentheses, and some books have no author group at all (then the author is empty).
- **Line 2** starts with `- ` and has `|`-separated fields. The first field gives the kind and,
  when present, `page N`. A `Location A-B` or `Location A` field follows. The last field starts
  with `Added on`. Some entries have no page field. PDFs give `page` only.
- A **blank line**, then the **text** (can be several lines; empty for bookmarks).
- The words are in the Kindle's UI language. **English is required.** Other languages are
  best-effort: if the kind word isn't recognised, treat an entry with text as a highlight and keep
  the raw line-2 text in the entry (so nothing is lost). The date is kept as written if it can't
  be parsed.
- **Edited highlights:** extending a highlight on the Kindle appends a *new* entry with an
  overlapping location range. Within one export, a highlight collapses into a later one when the
  ranges overlap **and** one passage contains the other (ignoring punctuation at the edges); the
  later one is kept. Two different passages that merely share a location (a location is about
  150 bytes, so consecutive sentences often do) both stay. *(Amended after the final review: the
  first rule, overlap alone, silently dropped the earlier of two adjacent highlights.)* Across exports, whatever was already written stays (append-only).
- **Edited notes:** a Kindle keeps one note per location, and editing or re-saving it appends a
  new entry while the old one stays. Notes of the same book at the same location count as one:
  the latest. *(Found on a real Paperwhite, 2026-10-22: one note saved three times.)*
- **Bookmarks** are skipped (no text).
- **Notes attach** to the highlight of the same book whose location range contains the note's
  location (or ends at it). A note with no such highlight stands on its own.
- **A note added later** to a highlight that's already in the file can't go under it
  (append-only). It's appended as its own entry, quoting the start of its passage for context:
  `- Note on “I went to the woods because…” (location 1342): read this again`. Notes are
  hashed on their own, separately from the highlight, so this never duplicates the passage.

## 5. The output

**Folder.** Chosen in Settings or with `--to`. Until one is chosen, automatic export is off and
the Kindle tab's button asks for a folder first.

**File name.** `<Title>.md`, with characters not allowed on macOS/Windows replaced by `-` and
trimmed to a safe length. If two different books (same title, different author) would collide,
the later one becomes `<Title> (<Author>).md`.

**Deckle Daily editions** all go into one file, one `## <date>` heading per edition, instead of a
file per day. An edition is any entry with author `Deckle` and a title of the form
`<paper> — <long date>`. The file is named after the paper title the first time and recorded in
`highlights.json`, so renaming the paper later keeps writing to the same file. Long reads and
Inbox books are ordinary books, each with its own file.

**A file Deckle creates:**

```markdown
---
title: Walden
author: Henry David Thoreau
source: kindle
tags: [highlights]
---

# Walden

> I went to the woods because I wished to live deliberately…

- page 88 · location 1340–1342 · 2025-03-03
- Note: read this again before the cabin trip
```

New entries in later exports go at the **end of the file**, in location order among themselves,
each under the same shape. Frontmatter is written once, at creation.

**State.** `highlights.json` in Deckle's config folder (next to `library.json` and
`digest.toml`) records, per book, the output file name and the set of entries already written.
An entry's identity is a hash of the normalised title, author, kind, location range and text.
That way the same highlight from a second Kindle isn't written twice, and an edited highlight
(different range or text) counts as new. If the mapped file no longer exists (the user deleted
or moved it), the next export writes a fresh file with every entry for that book.

## 6. Where it shows up

- **Kindle tab**, when a Kindle is connected: a **Highlights** section (left: books with counts
  and "+N new"; right: the target path and a preview of the Markdown for the selected book; a
  **Write N files** button). It follows the layout of the canvas mockup, in the app's existing
  styles.
- **Settings:** the highlights folder (**Change…**) and the checkbox *Update these files every
  time this Kindle connects*, which reads and writes the existing `[sync] clippings` key in
  `digest.toml` (default `false`, per-device overrides under `[sync.devices."<id>"]`). No new
  key and no migration: existing files that say `clippings = false` stay off until the box is
  ticked. Remove the CLI's "clippings isn't built yet" warning and the `# not built yet` comment
  in the starter template.
- **On connect:** `deckle digest watch` exports when `clippings` is on for that Kindle and a
  folder is set, in the same connection as push and prune (§3 says when idle plug-ins count).
- **CLI:** `deckle highlights list` (books and counts, `--json`), `deckle highlights export
  [--to <dir>] [--json]`. Exit code non-zero only if nothing could be read.

## 7. Testing

- A fixture `fixtures/clippings/` holding the variants: BOM + CRLF, a title with parentheses, no
  author, no page, a PDF entry, a note attached and one standalone, a bookmark, an extended
  highlight, a non-English entry. Plus golden Markdown output.
- Append-only: export, hand-edit the file, add entries, export again. The hand edit must
  survive byte for byte, and only the new entries are appended.
- Two devices with the same highlight produce one entry.
- MTP: through the existing fake `Transport` used by the transport tests.
- **On the owner's Scribe, first:** open and close a session with nothing to push, and see
  whether the Kindle leaves the book it was showing (§3).
- **On the owner's Scribe** before calling it done: highlight in a sideloaded book, plug in, run
  `deckle highlights export --to <tmp>`, check the file. Then check the Kindle tab visually.

## 8. Left to the builder

Crate placement (a small `deckle-highlights` crate or a module in `deckle-library`), the exact
normalisation for the entry hash, the date parser's accepted formats beyond English, and the
Tauri command names. Docs: add a "Highlights" section to the README and `docs/devices.md` notes
for whatever the Scribe check finds.
