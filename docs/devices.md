# Device matrix

What Deckle has been verified on. "Transport" is how the computer sees the Kindle; "Thumbnails"
says whether `system/thumbnails` is writable over that transport, which decides the cover rule
(design §5).

| Model | Firmware | Transport | Thumbnails writable | Tested | Result |
|---|---|---|---|---|---|
| Kindle Paperwhite (owner's, generation TBD) | TBD | TBD (mass storage if pre-2024, MTP if 2024) | TBD | not yet | pending M1 device check |
| Kindle Scribe (2024) | TBD | MTP only | no | not yet | pending M2 (MTP transport) |

## M1 device check (run by the owner)

1. Connect the Paperwhite over USB. `deckle devices` should list it.
2. `deckle run <book>.epub --to device --pages auto`
3. On the device: the book opens; the cover shows in the library; the `Aa` menu offers the
   Enhanced Typesetting options (hyphenation / justification); page numbers appear.
4. Record the row above with firmware (Settings → Device Options → Device Info) and the result.

If the Paperwhite is MTP-only, use `--to <folder>` and copy the `.kfx` with any MTP client for
now; the MTP transport is M2 work.
