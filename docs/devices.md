# Device matrix

What Deckle has been verified on. "Transport" is how the computer sees the Kindle; "Thumbnails"
says whether `system/thumbnails` is writable over that transport, which decides the cover rule
(design §5).

| Model | Firmware | Transport | Thumbnails writable | Tested | Result |
|---|---|---|---|---|---|
| Kindle Paperwhite (owner's, generation TBD) | TBD | TBD (mass storage if pre-2024, MTP if 2024) | TBD | not yet | pending M1 device check |
| Kindle Scribe (2024) | TBD | MTP only | no | not yet | pending spikes 2–3 (MTP client built, untested on hardware) |
| Simulated mass-storage Kindle (FAT32 disk image with `documents/`, `system/thumbnails`) | n/a | mass storage | yes | 2026-10-19 | EBOK + thumbnail + restore copy delivered by CLI and GUI |

## Spikes 2 and 3 (M2, need the Scribe)

Close Android File Transfer, Send to Kindle and calibre first: only one program may hold the
MTP session.

**Spike 3 — MTP write from macOS.** `deckle devices` should list the Scribe under MTP
(Amazon vendor 0x1949). `deckle run <book>.epub --to device` sends into `documents/` over
Deckle's own PTP client. Pass criteria: the book appears in the library and opens. If every
SendObjectInfo fails with `0xA806`, the device wants a different ObjectInfo layout; capture
`deckle run … --json` output and the libmtp #436 notes in `docs/formats/kfx.md`.

**Spike 2 — real ASIN on an MTP Kindle.** Turn on *Match books to an Amazon edition* (Settings),
send a book with an ISBN as EBOK (`--cde ebok --match`), let the Scribe sync over Wi-Fi, then
check: (a) the store cover appears, (b) after M3, that our X-Ray sidecar survives. If both
hold, tick *Keeps Deckle's X-Ray for real ASINs* for that Kindle in Settings; the cover rule
then ships matched books as EBOK on it.

## M1 device check (run by the owner)

1. Connect the Paperwhite over USB. `deckle devices` should list it.
2. `deckle run <book>.epub --to device --pages auto`
3. On the device: the book opens; the cover shows in the library; the `Aa` menu offers the
   Enhanced Typesetting options (hyphenation / justification); page numbers appear.
4. Record the row above with firmware (Settings → Device Options → Device Info) and the result.

If the Paperwhite is MTP-only, use `--to <folder>` and copy the `.kfx` with any MTP client for
now; the MTP transport is M2 work.
