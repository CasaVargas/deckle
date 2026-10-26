# Kindle tab redesign: reading first, one place to fix each thing

Mockups: the "Deckle Kindle Tab Redesign" canvas (Shelf, a book's drawer, Upgrades, This Kindle,
Unplugged). Builds on the Books tab redesign (`2026-10-26-books-tab-redesign-design.md`): the
same buttons, tabs, drawer and tray patterns. Replaces the look of Books and Kindle design §7;
what each room does stays.

## 1. Why

- A Shelf row can carry seven labels: EBOK or PDOC, Older build, Missing X-Ray, Duplicate, Can
  be upgraded, Amazon, and a "X-Ray · Word Wise · 4.2 MB · in your library" line. Most are
  jargon, and together they hide the book.
- Seven filter chips with counts, a search field and a native sort menu sit above the list.
- The Shelf is a list while the Books tab is covers. What you're reading looks like everything
  else.
- Fixes live in different rooms with no way to see them at once: upgrades, duplicates, old
  papers, new highlights.
- No Kindle connected is an empty dashed box, though Deckle knows what it sent there.

## 2. Frame

- **Rooms become tabs** across the top of the tab, in place of the rail: Shelf, Upgrades,
  Highlights, Words, This Kindle. Each carries its count where it has one: a filled badge for
  Upgrades ("6"), plain text for Highlights ("8 new"). The rail's notes go.
- **Kindle picker** moves from the footer to the top bar (a pill: status dot, name, free space,
  ▾ when there's more than one). The footer goes from the Kindle tab too, so no tab has one; the
  version stays in Settings › About and an update shows as the top bar's pill on every tab.
- **Refresh** becomes a small ↻ at the end of the tabs, with "Read just now" / "Read 5 min ago".

## 3. Shelf

In order, top to bottom:

1. **Worth a look**: up to three cards, each one line of what and one link, shown only when
   there's something to say:
   - "*n* books can get X-Ray and Word Wise" → Upgrades (the shelf's `upgradable` +
     `updatable`, or the Upgrades room's count once it has looked);
   - "*n* new highlights" → Highlights (the sum of `new`), naming up to two books;
   - "*size* you could free" → This Kindle (the sum of `spare`), with what it is: "2
     duplicates and 6 finished books over 10 MB".
2. **Continue reading**: books whose state is Reading, most recently opened first, at most four:
   a card with the cover, title, author, a progress bar and "64% · opened yesterday".
   Hidden when nothing is being read.
3. **On this Kindle *n***: a grid of covers, as the Books tab's library.
   - Show: All / Reading / Unread / Finished (a segmented control). "From anywhere ▾" picks
     Amazon / Sideloaded / Deckle. Sort is a quiet menu: Recently read, Title, Author, Size.
     Search moves to the top bar ("Search this Kindle"), as on the Books tab.
   - Each cover has a thin progress bar along its foot (green when finished) and, when it can
     be upgraded or updated, a small "Upgrade" mark. Under it: title, then one line: progress
     and what it is in plain words.
   - What it is: **Kindle book** (EBOK), **Document** (PDOC), **Amazon** (Amazon's own),
     **Today's paper** / **Paper** (Deckle papers), else its format (MOBI, PDF). Duplicate,
     Locked and Fixed layout are said in the drawer, not on the cover.
   - Papers stay out of All, as now; they show under From › Deckle.

## 4. A book's drawer

Clicking a cover opens a drawer like the Books tab's, with three tabs:

- **Reading**: the cover, title and author; a progress bar with "22% · opened 3 days ago";
  then, when Deckle can do better, one box saying what and why, with its button:
  - *Upgrade* (has-source or kindle-copy): "Deckle can make this a Kindle book. It went over as a
    document, so it has no X-Ray, Word Wise or real page numbers." → the Upgrades flow for this
    one book;
  - *Update* (Deckle's own, `updates` not empty): the update's own text → as Update does now;
  - *Upgrade to Kindle book* (in the library, sent as PDOC): → Re-send as EBOK, as now.
  Then **On the Kindle**, a short list in plain words: Shows up as (a Kindle book / a document,
  with its own cover / from Amazon), X-Ray and Word Wise (yes, not yet, or can't: why), Size,
  Sent (by Deckle on a date, in your library / not from Deckle), and Duplicate when it is one.
- **Highlights**: this book's count and new count, and its new highlights as the export would
  write them; **Save as Markdown** writes this book's file. Disabled with "No highlights" when
  it has none.
- **Details**: the file name and folder, format, mode, ASIN when known, and the class reason
  (Locked by DRM, Fixed layout…).

Footer: **Send again** (when it's in the library), and **Remove from Kindle…** (asks first,
saying the reading position goes too).

## 5. Upgrades

- A heading that counts ("6 books could be better") and one sentence on what an upgrade does:
  rebuilt, put back in place, place and highlights kept, History can put it back.
- Groups, each with a tick-all box: **From your library**, **Rebuilt from the Kindle's copy**,
  and **Built by an older Deckle** (the shelf's `updatable` books, today's update banner).
- Each row: tick, cover, title and author, "Gains X-Ray, Word Wise and page numbers" in plain
  words, and why on the right ("Sent as a document"). Partway books start unticked, as now.
- **Deckle leaves alone *n***, folded: Amazon's own, locked, fixed layout, each with its count
  and reason.
- A dark bar at the foot, like the Books tab's selection bar: "*n* of *m* selected · keep the
  Kindle plugged in while it runs", **History**, **Upgrade *n* books**. Upgrading goes through
  To send as it does now.

## 6. This Kindle

Two columns that stack when narrow:

- **Space**: "2.1 GB free of 8 GB", the split bar, and a legend with sizes.
- **Make room**: the spare books grouped by why (Duplicates, Old papers, Finished over 10 MB),
  each group with its total; ticks as now (finished start unticked); "*n* ticked · frees *size*"
  and **Remove *n* from the Kindle**.
- Beside them: **Name** (field and Save, then model and how it's connected) and **Back up**
  (what it copies, when the last backup was, Back up now…, Restore…).

## 7. Unplugged

When no Kindle is connected:

- If Deckle has sent books to a Kindle before, the tab shows the one sent to most recently
  (the picker offers the others): a banner saying it isn't plugged in, that what's below is
  only what Deckle sent, and that places and highlights need it connected, with a line on
  connecting. Then **Sent by Deckle**, newest first: covers, title, "Sent 12 Oct".
- If Deckle never sent to a Kindle: a centred welcome that says what the tab will show and how
  to connect one.
- New command `library_sent_kindles`: the Kindles in the library's send records (key, name,
  last send) with the books sent to each (path, title, authors, when, mode). Exports to
  folders are left out. The picker names them by the name in the records.
- "Last sent to on [date]" rather than "last plugged in": Deckle records sends, not
  connections.

## 8. Pull requests

1. **Frame and Shelf**: tabs instead of the rail, the Kindle pill in the top bar, no footer,
   ↻; Worth a look, Continue reading, the cover grid with plain words, the new filters, search
   in the top bar.
2. **Drawer**: the three tabs, the upgrade/update box, plain On the Kindle list, Send again and
   Remove; the row menu goes.
3. **Upgrades**: groups including older builds, the folded leave-alone list, the foot bar.
4. **This Kindle**: the two-column layout, grouped Make room with totals.
5. **Unplugged**: `library_sent_kindles` and the two empty states.

## 9. Verification

- Unit tests for the new pure parts: the Worth a look cards, Continue reading's choice and
  order, the plain-word kind line, the Make room groups and totals, the sent-Kindles listing.
- Each PR checked in the running app against a stand-in Kindle: a folder with `system/` and
  `documents/` named by `DECKLE_MOUNTS`, holding fixture books. Reading positions, highlights
  and a real MTP Kindle can't be faked this way; those wait for a real Kindle.
