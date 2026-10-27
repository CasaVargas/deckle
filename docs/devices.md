# Device matrix

What Deckle has been verified on. "Transport" is how the computer sees the Kindle; "Thumbnails"
says whether `system/thumbnails` is writable over that transport, which decides the cover rule
(design §5).

| Model | Firmware | Transport | Thumbnails writable | Tested | Result |
|---|---|---|---|---|---|
| Kindle Paperwhite (owner's, generation TBD) | TBD | mass storage | TBD | 2026-10-22 | Verified on real hardware. Reading positions (`.yjf`) decode, see *Reading position* below; the highlights format was checked on a mass-storage Kindle the same day (see the Scribe row). Generation and firmware still to record |
| Kindle Scribe (2024) | TBD | MTP only | no | 2026-10-19 | Spike 3 write: EBOK KFX + X-Ray `.sdr` delivered by the CLI (book opening on the device still to confirm). `system/` is not exposed over MTP: with `kll.en.en.klld` staged at `documents/deckle/` (copied on the device by a scriptlet), Word Wise shipped 3815 hints against the device dictionary; without it Word Wise is skipped with a warning. Highlights (#60) were checked on a mass-storage Kindle (firmware 5.19.2) on 2026-10-22: the English format parses, and an edited note shows up as several entries at one location. Highlights read `documents/My Clippings.txt`, which MTP does expose; on the Scribe 2024 (2026-10-22) an idle session is harmless: plugging in already shows "connected to computer", the session changes nothing, and the Kindle stays on the bus, so `IDLE_SESSION_OK` is `true`. The Scribe doesn't write `My Clippings.txt` at all, even right after a highlight in Amazon's reader (only a leftover `My Clippings.sdr/` exists): its annotations are per-book `.yjr`/`.yjf` files (binary, positions not text) inside each `.sdr` folder, so `deckle highlights` finds nothing there yet (#99). Highlights made in KOReader live in KOReader's own files and never reach `My Clippings.txt`. A USB reset on close drops it off the bus until replugged, so Deckle doesn't reset. Spike 2 passed: EBOK with the Kindle-edition ASIN typed by hand showed the store cover, and X-Ray and Word Wise kept working; marked *keeps Deckle's X-Ray for real ASINs* |
| Kindle Scribe (2024), Front page + topic sections test edition | TBD | MTP only | no | not yet | pending: front page, "In this section" lists and the ↩ back links not yet checked on the device (layout on e-ink, whether the ↩ glyph renders, link jumps) |
| Kobo (any model, firmware 4.x) | TBD | mass storage | n/a | not yet | pending, spike 1 (other readers design §11): `.kobo/version` field order, the `content` and `Bookmark` columns Deckle reads, a KEPUB paper's reading statistics, and books removed from `Deckle/` leaving the library after an eject. Tested only against a simulated drive (a folder with `.kobo/KoboReader.sqlite` built from the columns Deckle reads) |
| PocketBook (any model) | TBD | mass storage | n/a | not yet | pending, spike 3 (other readers design §11): the marker Deckle recognizes it by (`system/explorer-3/`, or `system/config/` with `system/profiles/`, and no `documents/`), and whether the cover inside the EPUB shows on the library tile. Tested only against a simulated drive |
| reMarkable 2 / Paper Pro / Paper Pro Move | TBD | USB web interface (`10.11.99.1`) | n/a | not yet | pending, spike 4 (other readers design §11): `GET /documents/` and `POST /upload` as Deckle uses them, that there's no delete, the PDF edition's page size for each model, and how the paper reads and takes handwriting. Tested only against a stand-in HTTP server |
| Simulated mass-storage Kindle (FAT32 disk image with `documents/`, `system/thumbnails`) | n/a | mass storage | yes | 2026-10-19 | EBOK + thumbnail + restore copy delivered by CLI and GUI; M4: X-Ray + Word Wise sidecars and Deckle's `system/kll/kll.en.zh.klld` delivered by the GUI, dictionary not rewritten on a second send |

## Reading position (`.yjf`)

Every book the Kindle has opened gets `documents/<stem>.sdr/<stem><suffix>.yjf`, where `<suffix>`
is a per-device hex string (e.g. `9d6e5f44fbf88f48a395735c25ed5d98`). It's Amazon's KRDS format
and holds `fpr` (furthest position), `lpr` (last position and its time), page-turn history and
reading-speed samples. Positions are `<base64>:<KFX position id>`; the number is the same
coordinate `deckle_kfx::text` uses. A never-opened book has no `.sdr` or no `.yjf`.

- **Scribe 2024 (MTP), verified 2026-10-22:** readable over MTP, written when you leave the book.
  In `deckle-daily-2026-10-21.kfx`, `fpr` 103303 is exactly the "On This Day" heading. The
  `.yjr` beside it holds font settings; its `annotation.cache.object` stays empty, because this
  firmware keeps highlights in storage MTP doesn't show (#99).
  End to end the same day: `deckle digest run --to device` recorded the 2026-10-22 edition's
  sections; after reading and replugging, `deckle digest push` (empty outbox) read `fpr` 14565,
  the first character of the Hacker News section, and `deckle digest readership` counted it.
- **From Your Shelf, checked on hardware 2026-10-22.**
  - `book.info.store` in a book's `.yjf` holds the Kindle's own fraction through the book and a
    word count; `timer.average.calculator`, nested in `timer.model`, holds pace samples.
  - **Paperwhite (5.19.2, mass storage):** `.yjf` files sit beside KFX, MOBI and AZW3 books
    (the shelf reads KFX only), under `documents/` and `documents/<Author>/`; books two folders
    down (`Downloads/Items01/`) are found since the shelf walks three folders deep. `system/vocabulary/vocab.db` and
    `My Clippings.txt` are readable (no `-wal` at the time). Percent against what the Kindle's
    library shows: The Devil Wears Prada shows 14%, position ÷ end 14.2%, stored fraction 9.1%;
    In the Shadow of Eight shows 16%, 12.4% and 14.1%; Harry Potter 1 shows 2%, 0.4% and 0.4%.
    Position ÷ end is within 4 points on all three; the stored fraction within 5, but only just
    (4.9).
  - **Scribe 2024 (MTP):** only `documents/` is visible, and store-bought books (in Amazon's
    cloud storage) aren't exposed, so the walk found the sideloaded book and the editions. Their
    `.yjf` read fine, but no KFX is read over MTP, so a book Deckle didn't send has no end and
    shows an "about N%" from the stored fraction: the Scribe shows 16% for a stored 15.05%, so
    `KINDLE_FRACTION_OK` is `true` (one Scribe book was comparable). No clippings or vocabulary are shared. The
    MTP product string ends in the serial ("Kindle Scribe G6G3X…"), which the page drops. An
    edition Deckle sent by hand before date-named files ("Deckle - The Deckle Daily -- …")
    counts as the paper and stays off the nightstand.
- **Paperwhite (mass storage), checked 2026-10-22:** the same `documents/<stem>.sdr/<stem><suffix>.yjf`
  (same suffix as the Scribe), decoded fine. Two differences: its `lpr` (89882) was ahead of its
  `fpr` (89410), so the report takes the further of the two; and a pruned edition's `.sdr` stayed
  behind without its `.kfx`, which Deckle treats as absent, not unopened.

## Compatibility warnings (how to add a row)

Deckle warns, never refuses, when a connected Kindle matches a row of `RULES` in
`crates/deckle-core/src/compat.rs` (spec: `docs/superpowers/specs/2026-10-20-compat-warnings-design.md`).
Matches show in the Kindles card in Settings and, for `on_send` rows, in a send's warnings.
Firmware comes from `system/version.txt` on mass storage and from PTP DeviceInfo on MTP (read by
the first send, then shown in the card).

1. Record the fact in the table above first: model, firmware (Settings → Device Options → Device
   Info), transport and what went wrong. A row without a device check behind it doesn't go in.
2. Add a `Rule`: `model` is a case-insensitive substring of the model hint (`None`: any model),
   `transport` narrows to `MassStorage` or `Mtp` (`None`: any), `firmware` is an inclusive
   `FirmwareRange { min, max }` (`None`: every firmware; unknown firmware never matches a range),
   `feature` is `Cover`, `Kfx`, `Xray` or `WordWise`, and `on_send: false` keeps it to the
   device card (use it when the pipeline already says the same thing). Say what to do in the
   message, not just what breaks.
3. Add a case to `crates/deckle-core/tests/it/compat.rs` with a firmware inside the range and one
   just outside it.

## Spikes 2 and 3 (M2, need the Scribe)

Close Android File Transfer, Send to Kindle and calibre first: only one program may hold the
MTP session.

**Spike 3 — MTP write from macOS.** `deckle devices` should list the Scribe under MTP
(Amazon vendor 0x1949). `deckle run <book>.epub --to device` sends into `documents/` over
Deckle's own PTP client. Pass criteria: the book appears in the library and opens. If every
SendObjectInfo fails with `0xA806`, the device wants a different ObjectInfo layout; capture
`deckle run … --json` output and the libmtp #436 notes in `docs/formats/kfx.md`.

**Spike 2 result (2026-10-19, Scribe 2024): passed with a Kindle-edition ASIN.** `deckle run
book.epub --to device --cde ebok --asin B0…` (the ASIN from the book's Amazon page): the
store cover appeared after the Kindle re-indexed with Wi-Fi on, and X-Ray and Word Wise kept
working. Print ASINs from Open Library are still untested.

**Spike 2 — real ASIN on an MTP Kindle.** Turn on *Match books to an Amazon edition* (Settings),
send a book with an ISBN as EBOK (`deckle run book.epub --to device --cde ebok --match`; the
`ebok` override makes Deckle stamp the matched ISBN-10 even before the Kindle is marked
verified), let the Scribe sync over Wi-Fi, then check: (a) the store cover appears, (b) that our X-Ray sidecar survives (open the book, long-press a name: the card should show Deckle's description). If both
hold, tick *Keeps Deckle's X-Ray for real ASINs* for that Kindle in Settings; the cover rule
then ships matched books as EBOK on it.

## M1 device check (run by the owner)

1. Connect the Paperwhite over USB. `deckle devices` should list it.
2. `deckle run <book>.epub --to device --pages auto`
3. On the device: the book opens; the cover shows in the library; the `Aa` menu offers the
   Enhanced Typesetting options (hyphenation / justification); page numbers appear.
4. `deckle run <book>.epub --to device --cde ebok` (or let the cover rule pick EBOK on a
   mass-storage Kindle): the book opens, long-pressing a character's name shows Deckle's X-Ray
   card, and the X-Ray entry in the reading menu lists our people and terms. If the sidecar is
   ignored, note the firmware: `docs/formats/xray.md` has the schema to compare against.
5. Record the row above with firmware (Settings → Device Options → Device Info) and the result.

If the Paperwhite is MTP-only, use `--to <folder>` and copy the `.kfx` with any MTP client for
now; the MTP transport is M2 work.

## Word Wise check (M4, run by the owner)

Each Kindle answers one question first: does it have Amazon's English Word Wise dictionary?
Look for `system/kll/kll.en.en.klld` (mass storage: Finder; MTP: Android File Transfer, then
close it). If it is missing, open any store book with Word Wise on and Wi-Fi on once, and check
again; record either way in the table above.

1. `deckle run <book>.epub --to device --cde ebok` (the result line names the dictionary).
2. **Kindle dictionary** (`kll.en.en.klld` present): open the book, *Aa → More → Word Wise* on.
   Hints should appear above harder words ("civility", "countenance" in Pride and Prejudice) and
   tapping one should show a sensible definition. A hint that shows the wrong word's definition
   means the sense-id join in `deckle-wordwise::device` guessed the wrong column
   (`display_lemma_id` vs `term_lemma_id`): note two examples and the firmware.
3. **Deckle dictionary** (no `kll.en.en.klld`): Settings → Language → Word Wise (or *Aa → More →
   Word Wise → Language*) → 中文. Hints should be English Wiktionary glosses.
4. Move the slider between "fewer" and "more hints": the count should change.
5. Record firmware and result above. If no hints show at all, compare the sidecar against
   `docs/formats/wordwise.md` and WordDumb's output for the same book.
