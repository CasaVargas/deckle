# How it works

The interesting parts, with format notes in [formats/](formats/).

**KPF → KFX.** Kindle Previewer's CLI outputs a KPF: a zip around a SQLite database of Ion
fragments, with 1 KiB fingerprint records injected every MiB. A Kindle wants a monolithic KFX.
jhowell's kfxlib (GPL-3, part of the KFX Input calibre plugin) has the complete conversion.
Deckle's KFX writer is a Rust port of it, tested byte for byte against kfxlib's output, so no
Python is involved. Page numbers come from the same port (1,850 positions per page).

**Covers vs. features.** X-Ray and Word Wise only load for books marked **EBOK** with an ASIN.
An EBOK whose ASIN the store doesn't know shows a grey cover, and Amazon overwrites any cover
thumbnail with a "no image" file once the Kindle goes online. **PDOC** books always show their
embedded cover but get no X-Ray or Word Wise. So Deckle decides per Kindle:

| Kindle | What Deckle sends |
|---|---|
| Mass storage (`system/thumbnails` writable) | EBOK plus Deckle's own thumbnail, restored on every send: cover and features |
| MTP only (Scribe 2024, Colorsoft, Paperwhite 2024) | PDOC (cover wins), unless you give the book its real Amazon ASIN, in which case the Kindle fetches the store cover and keeps the features |
| Export to a folder | PDOC |

Amazon reuses a print ISBN-10 as that edition's ASIN, so Open Library is enough to find a
candidate without scraping Amazon. You can override the mode per book.

**MTP on macOS.** macOS has no MTP stack, so Deckle has a small PTP/MTP client over USB. Kindle
specifics: always the 32-bit ObjectInfo layout (libmtp switches layouts on the Kindle's nameless
root folders and every write then fails with `0xA806`), zero-length packets after full-packet
data phases, TransactionID 0 for OpenSession, and never create a `documents` folder.

**X-Ray** is a SQLite sidecar (`<book>.sdr/XRAY.entities.<ASIN>.asc`) in the schema WordDumb
reverse-engineered, with occurrences at KFX positions. Entities come from a heuristic tuned on
*Pride and Prejudice*: honorifics, speech verbs, stop lists, alias merging.

**Word Wise** is a sidecar that holds no text. `LanguageLayer.en.<ASIN>.kll` stores positions
and sense ids into a dictionary already on the Kindle (`system/kll/*.klld`). Deckle matches the
book against its own lexicon (174k senses, inflections mapped to lemmas, difficulty from
wordfreq) and points the hints at the Kindle's English dictionary. If the Kindle doesn't have
one, Deckle installs its own in the Chinese slot: set *Word Wise → Language → 中文* on the
Kindle; the hints are still English. Per-book proper-noun detection keeps "Darcy" from being
glossed as a unit of permeability.

MTP Kindles hide `system/`, so Word Wise there needs the Kindle's `kll.en.en.klld` copied to
`documents/deckle/` once (a jailbroken Kindle with KOReader can do it). Without it, Word Wise is
skipped and the book and X-Ray still ship.

**The paper.** Every source sits behind one small network interface and is gathered on its own,
so a feed that's down becomes a line in its section and the edition still builds. The front page
is drawn in Rust with bundled open-licence fonts and becomes the EPUB's cover. Reading positions
come back from the Kindle's `.yjf` files (Amazon's KRDS format), which is how the paper knows
which sections you reach. Each edition can also be written as a documented, versioned data
bundle for reader apps (spec CC-BY-4.0, schema MIT). In **Daily → Delivery**, bundle folders can
opt into **Fresh-paper requests** from compatible clients. The existing watcher builds a new
full paper into that folder while this computer is awake; cloud sync adds delay. Requests are
off by default. [Setup and troubleshooting](daily.md#fresh-paper-requests) explains ownership,
optional model costs, and disconnecting a folder.
