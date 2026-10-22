# Device matrix

What Deckle has been verified on. "Transport" is how the computer sees the Kindle; "Thumbnails"
says whether `system/thumbnails` is writable over that transport, which decides the cover rule
(design §5).

| Model | Firmware | Transport | Thumbnails writable | Tested | Result |
|---|---|---|---|---|---|
| Kindle Paperwhite (owner's, generation TBD) | TBD | TBD (mass storage if pre-2024, MTP if 2024) | TBD | not yet | pending M1 device check |
| Kindle Scribe (2024) | TBD | MTP only | no | 2026-10-19 | Spike 3 write: EBOK KFX + X-Ray `.sdr` delivered by the CLI (book opening on the device still to confirm). `system/` is not exposed over MTP: with `kll.en.en.klld` staged at `documents/deckle/` (copied on the device by a scriptlet), Word Wise shipped 3815 hints against the device dictionary; without it Word Wise is skipped with a warning. Highlights (#60) were checked on a mass-storage Kindle (firmware 5.19.2) on 2026-10-22: the English format parses, and an edited note shows up as several entries at one location. Highlights read `documents/My Clippings.txt`, which MTP does expose; on the Scribe 2024 (2026-10-22) an idle session is harmless: plugging in already shows "connected to computer", the session changes nothing, and the Kindle stays on the bus, so `IDLE_SESSION_OK` is `true`. The Scribe doesn't write `My Clippings.txt` at all, even right after a highlight in Amazon's reader (only a leftover `My Clippings.sdr/` exists): its annotations are per-book `.yjr`/`.yjf` files (binary, positions not text) inside each `.sdr` folder, so `deckle highlights` finds nothing there yet (#99). Highlights made in KOReader live in KOReader's own files and never reach `My Clippings.txt`. A USB reset on close drops it off the bus until replugged, so Deckle doesn't reset. Spike 2 passed: EBOK with the Kindle-edition ASIN typed by hand showed the store cover, and X-Ray and Word Wise kept working; marked *keeps Deckle's X-Ray for real ASINs* |
| Simulated mass-storage Kindle (FAT32 disk image with `documents/`, `system/thumbnails`) | n/a | mass storage | yes | 2026-10-19 | EBOK + thumbnail + restore copy delivered by CLI and GUI; M4: X-Ray + Word Wise sidecars and Deckle's `system/kll/kll.en.zh.klld` delivered by the GUI, dictionary not rewritten on a second send |

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
3. Add a case to `crates/deckle-core/tests/compat.rs` with a firmware inside the range and one
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
