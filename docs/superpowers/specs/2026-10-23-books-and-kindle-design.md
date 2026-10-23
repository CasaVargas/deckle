# Books, Kindle, and a paper fit for the iPad: design

**Status:** accepted, 2026-10-23. The owner took the recommendations in §12 · **Date:**
2026-10-23 · **Owner:** Jonathan (CasaVargas LLC)
**Builds on:** `2026-10-21-library-and-device-view-design.md` (library, Kindle tab, Upgrade),
`2026-10-21-kindle-updates-design.md` (freshness, `replaces`),
`2026-10-21-device-aware-build-design.md`, `2026-10-22-highlights-export-design.md`,
`2026-10-23-edition-bundle-design.md` (bundle 1.0, §7 motion).

Three changes that ship in phases (§10):

- **A.** Upgrade any DRM-free book that's already on a Kindle, not only the ones Deckle sent.
- **B.** The Kindle tab and the Books tab grow up: rooms, reading progress, a library you can
  browse, and a check before you send.
- **C.** The paper as data gets photos and story details good enough for a 12.9-inch iPad:
  bundle 1.1.

## 1. Intent and why

Most books on a Kindle aren't Deckle's. They came from calibre, Send to Kindle, another
converter, or the Kindle Store. Today the Kindle tab labels them "not from Deckle" and stops.
Upgrade only works for books in Deckle's library, because that's where the source EPUB is.
Yet the reasons people use Deckle apply to these books too: no Enhanced Typesetting, made-up
page numbers, no X-Ray, no Word Wise, a grey cover.

The Books tab is a send queue. A book you sent last month is in the library folder, but the
app has no way to show it, find it, or send it again except through the Kindle tab.

The paper's edition bundle carries photos at 1000 px and JPEG quality 70, the e-ink setting.
That's soft on a 2732-px iPad screen. Captions survive only as text inside the story, and there
is no credit, alt text, or hint of where the subject is. The iPad app's moving pictures are
parallax, made in the app from the stills (§9), so the stills have to be good.

**Success:**

- Plug in a Kindle with 200 books. The Kindle tab says: "31 can be upgraded: 19 from your
  calibre library, 9 from the Kindle's own copy, 3 older Deckle builds. 158 are Amazon's own.
  2 are locked."
- **Upgrade all** brings each one back with Enhanced Typesetting, real page numbers, X-Ray,
  Word Wise and a cover that shows. Highlights are saved first and the old copy is kept, so
  any upgrade can be put back.
- On the iPad the lead photo fills a 12.9-inch screen sharply, with its caption and credit.
  Its subject stays in frame when the layout crops it.

## 2. Goals and non-goals

**Goals**

1. Sort every book on a Kindle into one of the classes in §3. Read the metadata of AZW3 and
   MOBI files as well as KFX: title, author, ASIN, PDOC/EBOK, cover, DRM.
2. Find a book's source on this computer: Deckle's library, a calibre library, or folders you
   name (§4).
3. Without a source, rebuild a DRM-free AZW3/MOBI from the Kindle's own copy (§5).
4. Replace safely (§6). Highlights are exported first, and the old file and its `.sdr` are
   kept in the library. **Put back** undoes an upgrade. A book you're partway through says
   where you were.
5. Kindle tab rooms (§7): Shelf with reading progress, Upgrades, Highlights, Words, This
   Kindle (space, make room, backup).
6. Books tab (§8): your library beside the send queue, a check of what Deckle will do to a
   book, a page preview, a cover picker, and title/author/series fixes.
7. Bundle 1.1 (§9): photo masters in color at up to 2400 px. Captions, credits, alt text, a
   focus point and a dominant color for each photo. Byline, publish time, reading time,
   prominence, pull quote and a paywall flag for each story. A lint pass on story bodies.

**Non-goals**

- **Removing DRM, ever.** A DRM-protected file is read for its metadata and left alone.
  This line doesn't move.
- **Touching Kindle Store books.** Amazon's own already have what Deckle adds (§3).
- KFX → EPUB. Most KFX files that weren't sent by Deckle come from calibre's KFX Output, and
  calibre keeps the EPUB, so §4 finds it. Converting KFX back to EPUB is a later phase if
  owners need it (§11).
- Editing the Kindle's collections database. On firmware that keeps collections there,
  collections can lose an upgraded book (§11).
- Writing to calibre's library. It is read-only to Deckle.
- Video or generated motion for the paper. Motion is parallax, which the iPad app makes from
  the stills at view time (§9).

## 3. What's on a Kindle, sorted

Each file in `documents/` gets one class:

| Class | Meaning | Upgrade |
|---|---|---|
| **Deckle, current** | sent by Deckle, nothing newer to give | no |
| **Deckle, older build** | the existing freshness rule (kindle-updates §3) | from the library, as today |
| **Has a source** | not Deckle's, DRM-free, and §4 found its EPUB/DOCX | from the source |
| **From the Kindle's copy** | not Deckle's, DRM-free AZW3/MOBI, no source found | rebuilt (§5) |
| **Amazon's own** | a Kindle Store book: EBOK, an Amazon ASIN (`B0…`), DRM | never |
| **Locked** | DRM-protected and not a store book (a library loan, another shop) | never |
| **Fixed layout** | a comic, a children's book, a fixed-layout AZW3 | never: left as it is |
| **PDF** | a sideloaded PDF | optional: reflow it with `deckle-pdf` (asks) |
| **Other** | TXT, unknown | no |

**Reading AZW3, MOBI and PRC.** These are PalmDB files.

- Record 0 holds the MOBI header and the EXTH records:
  - its encryption type (`0` none, `1` or `2` DRM);
  - EXTH 100 (author), 503 (title), 113 and 504 (ASIN), 501 (`EBOK`/`PDOC`), 201 (the cover
    record), 121 (the KF8 boundary) and 122 (fixed layout).
- A small reader in the new `deckle-mobi` crate (§5) reads record 0, and the cover record when
  there is one.
- On mass storage that's a partial read.
- Over MTP, which has no partial reads, a file has to be read whole, once. Its details are
  cached by (file, size, modified), as KFX details are today. The first listing of a big
  Kindle over MTP takes a while: "Reading 200 books (the first time only)…".

**KFX** that wasn't sent by Deckle: the container's DRM scheme (`$411`), or a `.voucher` in its
`.sdr`, marks DRM. A non-`DECK` ASIN with DRM is Amazon's own. KFX without DRM looks for a
source (§4). Without one it's shown as "No source on this Mac", and §5 doesn't apply.

`deckle kindle books` prints each book's class. The app shows it as a badge.

## 4. Finding a book's source

Each check runs in this order. The first match wins.

1. **Deckle's library**, as today: by ASIN, or a send record for this Kindle.
2. **A calibre library.** `metadata.db` is opened read-only (SQLite, `immutable=1`, so a running
   calibre is never disturbed).
   - Its `books`, `identifiers` and `data` tables give each book's formats, ISBN, `mobi-asin`
     and calibre id.
   - Where the Kindle copy carries one, calibre's id (EXTH 112, `calibre:<uuid>`) or an ASIN
     equal to the book's `mobi-asin` is an exact match.
   - Otherwise an ISBN match, then the normalized title and first author.
   - Only EPUB, DOCX and TXT formats count as sources.
   - `~/Calibre Library` is found by itself. Settings → Library lets you choose another one.
3. **Folders you name** (Settings → Library → "Find originals in"). They are scanned for EPUB,
   DOCX and TXT, with OPF titles and authors cached by (path, size, modified).

A book upgraded from a source is added to Deckle's library (a copy of the source with its
sidecar), so its next update is the usual kind. Where the source came from is kept in the
sidecar (`origin: "calibre" | "folder" | "kindle-copy"`).

## 5. Rebuilding from the Kindle's copy

`deckle-mobi` (new crate, GPL-3.0) turns a DRM-free KF8 file back into an EPUB. KF8 is AZW3,
or the KF8 half of a joint MOBI. The steps:

- PalmDOC and HUFF/CDIC decompression of the text records.
- The FDST flows, and the SKEL and FRAG indexes, which put the XHTML parts back together.
- Resources: images, CSS flows, and fonts (with their XOR obfuscation undone).
- The NCX index becomes a nav document, and the OPF is built from the EXTH metadata.

This ports the KF8 path of KindleUnpack (GPL-3.0, credited in `NOTICE`). A MOBI 6-only file
becomes one XHTML file with its images, and its `filepos` anchors become links. Its badge says
"older format: rebuilt from its text and images".

A **quality gate** runs before anything is replaced:

- the rebuilt EPUB has to pass `inspect`;
- its text has to be at least 98% of the source's decompressed text length;
- it has to keep every image record.

A book that fails says "Couldn't rebuild this one cleanly" and stays as it is.

The file is pulled over the connection the listing holds. The EPUB is built like any book,
and the result replaces the old copy (§6).

## 6. Replacing safely, and your place in the book

Step by step, over one connection:

1. **Highlights.** The book's highlights are written to the highlights folder (highlights
   export design) before anything changes. The Kindle's `My Clippings.txt` keeps them too.
2. **The old copy is kept.** The file and its `.sdr` are copied into the library book's
   `kindle-originals/<device>/` folder. **Put back** (Kindle tab → Upgrades → History)
   restores them and removes Deckle's copy.
3. **The new copy goes on**, in the mode the device-aware rule picks. Usually that's EBOK, for
   X-Ray and Word Wise. The old file goes through `replaces` (kindle-updates §4).
4. **Your place.**
   - The old `.sdr` gives the last position as a fraction:
     - KFX `.yjr`/`.yjf`;
     - AZW3 `.azw3r`/`.azw3f`;
     - MOBI `.mbp`.
   - Carrying it into the new copy means writing a KRDS position file. That's a research
     spike (§11).
   - Until it works, a book you're partway through says so in the row: "You're 48% in (page
     212 of 440). The new copy opens at the start. Go to page 212."
   - Such books are **not** ticked in Upgrade all (§12, Q1).
5. **On-device highlights.** These aren't carried into the new copy. The Kindle ties them to
   the old file. The row says so before you upgrade a book that has any.

## 7. The Kindle tab, in rooms

Like the Daily tab (Daily rooms): a rail with a line under each room's name.

- **Shelf** ("200 books · 3 reading"). Every book on the Kindle shows:
  - its cover, class badge and progress bar (from the position files the listing already
    reads, now for every book), and when it was last opened;
  - filters: Reading, Not started, Finished, Amazon's own, Sideloaded, Deckle;
  - sorting: recently read, title, author, size; and search.
- **Upgrades** ("31 can be upgraded"):
  - the books from §3, grouped by where the new copy comes from;
  - for each book, what it gains, as chips: Enhanced Typesetting, page numbers, X-Ray, Word
    Wise, cover;
  - **Upgrade all**, **Upgrade** on each row, and **History** with Put back.
- **Highlights** and **Words**: today's panels, each in its own room.
- **This Kindle** ("2.1 GB free"):
  - space split by kind: Amazon's own, sideloaded, Deckle papers, other;
  - **Make room**: finished books over 10 MB, duplicates, and papers past `keep`, each with
    Remove;
  - **Back up** (#42): every DRM-free book and its `.sdr` (highlights, position) is copied to
    a folder. Amazon's own are skipped, since Amazon downloads them again. **Restore** puts a
    backup on another Kindle.

## 8. The Books tab: a library as well as a queue

- **Your library** is a grid of covers from Deckle's library, plus calibre and your folders
  when they're on. It has:
  - search, series grouping ("The Expanse, 1–4 · 5 isn't in your library"), and a "Not on
    this Kindle" filter;
  - selecting books adds them to the queue.
- **To send** is today's queue, docked beside the library. With no library set up, it's the
  whole tab, as now.
- **Before you send**: opening a queued book shows what Deckle will do to it:
  - typography fixes, with a count;
  - pop-up notes made;
  - the cover: its own, generated, or replaced;
  - the mode and why (the cover rule);
  - X-Ray and Word Wise expected;
  - warnings.
- **Page preview**: the first page of chapter one, drawn as a Kindle would draw it. The EPUB's
  XHTML goes in a sandboxed frame with a Kindle-like stylesheet. **Before / After** shows the
  typography pass.
- **Cover**: the book's own, Deckle's typographic cover, or Open Library's covers (network,
  under the existing metadata setting).
- **Fix details**: title, author, series and number. This writes to the library copy's OPF,
  never to the original file or calibre.
- **Watch a folder** (off by default): EPUBs that land in it (Downloads, say) are added to
  the library.

## 9. The paper, fit for the iPad: bundle 1.1

**Motion is parallax.** The iPad app makes it at view time from the stills: it lifts the
subject with Vision's foreground mask and moves the layers with the device. `photos[].motion`
stays `null` in 1.1, and no clip is written. The bundle spec's §7 rule stands: nothing invents
movement inside a news photo. Parallax only tilts the still in layers. So the Mac's job is
excellent stills and where to look in them.

**Photo masters.** A photo is fetched once and kept as a master. Every output is made from the
master:

- **Bundles:** color, sRGB, EXIF orientation applied, metadata stripped. At most 2400 px on
  the long side, never enlarged, JPEG quality 85.
- **E-ink outputs:** as today, gray, 1000 px wide, quality 70.

A typical edition's bundle grows from about 4 MB to about 15 MB.

**New photo fields.** All are optional, so 1.0 readers ignore them.

```json
{ "file": "media/….jpg", "width": 2400, "height": 1600, "lead": true, "motion": null,
  "caption": "Stall holders at the freight shed on Saturday.", "credit": "Jane Doe / The Pilot",
  "alt": "A crowd under a steel roof", "focus": { "x": 0.62, "y": 0.41 }, "color": "#8a6f52" }
```

Where each value comes from:

- `caption`, from the photo's `figcaption`, or else schema.org `ImageObject.caption`.
- `credit`, from a credit element in the caption (`.credit`, `small`, a trailing "Photo: …").
  Otherwise from `ImageObject.creditText` or `copyrightHolder`, or the JPEG's IPTC by-line.
- `alt`, from the `img` alt text, or else `og:image:alt`.
- `focus` (0–1, from the top left), from a crop scorer in pure Rust that weighs edges, skin
  tones and saturation, the way smartcrop does. It's used for crops; the app may refine it with
  Vision.
- `color`, the photo's dominant color, for the page behind it while it loads.

**New story fields.** All are optional.

- `byline` is already extracted.
- `published` is an ISO date-time.
- `minutes` is reading time at 230 words a minute.
- `prominence` is `"lead"`, `"front"` (a tile) or `"inside"`, from the front plan.
- `partial` is `true` when the text looks cut off by a paywall: a story ending in "Subscribe…",
  or far shorter than its feed summary promised.
- `pull_quote` is a sentence of 60–200 characters picked from the body: a quotation if there is
  one, else the sentence that best stands alone. It's not set on stories under 600 words.

**Lint before it ships.** It applies to every output, not only bundles.

- Boilerplate lines are dropped: newsletter sign-ups, "Advertisement", share prompts, lists of
  related links, cookie notices.
- A story printed in two sections keeps the first.
- Story bodies get the typography pass books get (curly quotes, dashes).
- `deckle digest lint <edition>` prints what it would drop, to tune the rules.

**Versioning:** 1.1, a minor version (bundle §10). The spec, the JSON Schema, the fixtures and
the Deckle Reader prompt are updated together.

## 10. Phases

Each phase is its own PR.

1. **Sorting and upgrading from a source** (A):
   - `deckle-mobi`'s metadata reader;
   - the classes (§3) in the listing and the CLI;
   - the source finder (§4);
   - safe replace with kept originals and Put back (§6);
   - the Kindle tab's Upgrades list, inside today's tab.
2. **Bundle 1.1** (C): photo masters, the photo and story fields, and lint. It comes second
   because the iPad app is being built now and wants this data.
3. **Rebuilding from the Kindle's copy** (A, §5): the KF8 → EPUB converter and its quality
   gate.
4. **Kindle tab rooms** (B, §7): Shelf with progress, This Kindle (space, make room, backup and
   restore).
5. **Books tab library** (B, §8): the library grid, before you send, page preview, the cover
   picker and fix details.

Spike, alongside phase 1: carrying the reading position (§11).

## 11. Risks and research

- **Reading position.** The Kindle's position files (KRDS) are undocumented. Deckle reads
  them, but writing one the Kindle accepts for a new book is untested.
  - The spike: write a position file for a test book on the Scribe and the Paperwhite, and
    see whether the book opens there.
  - If it doesn't, §6's "Go to page 212" is the answer.
- **Collections.** On firmware that keeps collections in `cc.db`, a replaced file may drop
  out of its collections. Before Upgrade all is turned on, check this on the two Kindles we
  have. If it happens, the row says "It leaves its collections".
- **MTP listing cost.** The first full read of a big Kindle over MTP is slow. It's cached
  after that, and the Shelf shows file names until each book's details arrive.
- **KF8 edge cases.** Fixed layout is skipped. Odd files fail the quality gate and stay as
  they are, so nothing is replaced on a guess.
- **KFX → EPUB later.** If owners have KFX books with no calibre source, kfxlib's EPUB output
  (GPL-3.0, already the basis of `deckle-kfx`) is the path. It's a large port, so it waits for
  demand.
- **Bundle size.** About 15 MB a day, 30 days kept: about 450 MB in iCloud Drive. Each export
  folder's `keep` setting applies.

## 12. Rulings

The owner accepted each recommendation below (2026-10-23).

1. **Books you're partway through, in Upgrade all?** Recommend **not ticked**. They're listed
   with "You're 48% in", and you tick them yourself. This changes if the position spike works.
2. **Keep the old copies?** Recommend **yes**, in the library under `kindle-originals/`, with
   Put back. It costs disk space, and a "Forget old copies" button clears it.
3. **Mode for an upgraded personal document?** Recommend **the device-aware rule** (usually
   EBOK, for X-Ray and Word Wise), as for any book. The row shows the choice and you can change
   it.
4. **calibre:** recommend **read-only, always**. Deckle never writes calibre's database or its
   files.
5. **Sideloaded PDFs:** recommend **offered, never in Upgrade all**. Reflowing changes how a
   PDF looks, so it asks.
6. **Photo size in bundles:** recommend **2400 px long side, quality 85**. That's sharp on a
   12.9-inch iPad, at about 400 KB a photo.
7. **Watch a folder:** recommend **off by default**.
8. **Books tab layout:** recommend **library first, queue docked**. With no library set up,
   the queue is the whole tab, as now.

## 13. Verification

- **`deckle-mobi`:**
  - EXTH metadata, the encryption flag and the cover from fixture AZW3 and MOBI files made
    from the repo's DRM-free fixture EPUBs;
  - a KF8 round trip (EPUB → AZW3 → EPUB) keeps 98% of the text and every image;
  - fixed layout is detected.
- **Classes:** a fake mass-storage Kindle holding:
  - a Deckle KFX;
  - a calibre-made AZW3 whose `mobi-asin` is in a fixture `metadata.db`;
  - a DRM-flagged store AZW3;
  - a locked file;
  - a PDF.

  Each lands in its class, and `deckle kindle books` prints them.
- **Replace:**
  - highlights are written first;
  - the original and its `.sdr` land under `kindle-originals/`;
  - Put back restores them;
  - a failed rebuild replaces nothing;
  - nothing with DRM is ever opened past record 0.
- **Bundle 1.1:**
  - fixtures with captions, credits, alt text and focus points validate against the 1.1
    schema, and a 1.0 bundle still validates;
  - masters make both the gray 1000-px and the color 2400-px outputs;
  - lint drops the boilerplate in a fixture article and keeps its body.
- **App:** the Upgrades list, the rooms and the Books library are checked against the fake
  Kindle image in light and dark.

## 14. Build notes, phase 1

- **`deckle-mobi`** reads record 0's MOBI header and EXTH records, and the cover record, with
  a partial read on disk. Its `fixture` module builds small PalmDB books for tests in other
  crates. The KF8 → EPUB rebuild (§5) comes in phase 3.
- **The inventory** reads AZW3, AZW, MOBI and PRC details as well as KFX, cached as before.
  DRM is a DRMION KFX, a `.voucher` in the `.sdr`, or a PalmDB encryption type. A cache entry
  from an older Deckle is read again once. `list_detailed` reads details over MTP too (each
  uncached book whole, once) and the reading positions; only the upgrade scan uses it, so the
  Kindle tab's listing stays as fast as before.
- **Where old copies are kept:** `kindle-originals/<device>/<when> <file stem>/` beside
  `library.json`, not in the library book's folder. The upgrade can happen before the book is
  in the library, and there may be no library at all. `upgrades.json` beside it records each
  upgrade for Put back.
- **Highlights first** runs the highlights export when there's a highlights folder. Either way,
  the kept `.sdr` holds the Kindle's annotations, and `My Clippings.txt` stays on the Kindle.
- **Your place:** the reading-position spike (§11) needs a Kindle, so it hasn't run. The row
  says how far in you are, from the Kindle's own fraction in the `.yjf`, and that the new copy
  opens at the start. A page number would need the old file's page map, which isn't read.
- **Folders** are searched for EPUBs only. calibre's DOCX and TXT formats count as originals.
- **The app:** the Upgrades panel sits in today's Kindle tab until its rooms come (phase 4).
  An upgrade goes through the Books tab like Update, so the run shows its progress there. A
  send replaces the Kindle's copy only on the Kindle it was listed from.

## 16. Build notes, phase 3

- **What's rebuilt:** KF8 only, an AZW3 or the KF8 half of a joint MOBI. Text records with
  PalmDOC or no compression, trailing entries stripped. The FDST flows. The XHTML parts from
  the skeleton and fragment indexes, with KindleUnpack's fix for a fragment whose insert point
  falls inside a tag. `kindle:pos` links become `partNNNN.xhtml#anchor`: the nearest `id` or
  `name` before the position, else the element's `aid` as `aid-…`. Images and fonts (XOR and
  zlib undone) come back as files, CSS and SVG flows too. The NCX becomes `nav.xhtml` and
  `toc.ncx`, and EXTH becomes the OPF (cover from EXTH 201).
- **Not rebuilt:** MOBI 6-only files (their HTML isn't XHTML and needs a real HTML-to-XHTML
  pass), HUFF/CDIC compression (rare outside store books), and fixed layout. Each says why and
  stays as it is. The class is still "rebuild from the Kindle's copy", and the attempt is what
  fails.
- **The quality gate** is stricter than the spec's text-length rule:
  - the text must decompress to at least 98% of the PalmDOC header's length;
  - the parts must cover 98% of the main flow;
  - every `kindle:embed` a part or stylesheet uses must resolve;
  - every part must be well-formed XML.

  Failing any of these writes nothing.
- **Checked on real files:** Project Gutenberg's AZW3s of *Frankenstein* (34 parts, 2 images)
  and *Pride and Prejudice* (76 parts, 165 images, the cover found by EXTH 201) rebuilt and
  passed EPUBCheck 5.1.0 with 0 errors and 0 warnings. That took two fixes: the NCX's
  `playOrder` is shared by entries with one target, and the Kindle's `cid` attributes are
  dropped like unlinked `aid`s. CI tests use `fixture::build_kf8`, a small KF8 writer with real
  skeleton, fragment and NCX indexes. `cargo run -p deckle-mobi --example rebuild -- in.azw3
  out.epub` tries a file by hand.
- **Rebuilt EPUBs** live in `kindle-rebuilt/<device>/` beside `library.json` and are what the
  upgrade sends, with `origin: "kindle-copy"`. The app rebuilds when you press Upgrade, not when
  it lists the books.
