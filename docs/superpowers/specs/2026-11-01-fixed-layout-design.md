# Fixed layout: comics, manga and picture books (#20)

Status: agreed scope, phase 1 built (this PR). The owner asked for no questions, so every
choice below is the builder's call; §10 lists them for review.

## 1. Why

Every Deckle spec so far refuses fixed layout or leaves it alone: MOBI import refuses EXTH 122,
the inventory lists fixed-layout Kindle books as "left as it is", presets skip `pre-paginated`
EPUBs, and Kindle Previewer's fixed-layout output is treated as a failed conversion. People with
comics (CBZ), manga (right to left) and picture books (fixed-layout EPUB) get nothing from
Deckle.

What the owner wants (issue #20): keep the layout instead of reflowing it, and fit the pages to
the target device's screen.

## 2. Goals, non-goals, anti-goals

**Goals**

- Take a CBZ, a folder of page images, and a fixed-layout EPUB. Later: a fixed-layout AZW3, a
  CBR and an image-only PDF (§9).
- Send each one to every target Deckle has (Kindle, Kobo, PocketBook, reMarkable, folder) in
  the format that target shows best (§5).
- Read manga right to left on every target that can.
- Fit the pages to each device's screen, in grey on a black-and-white screen (phase 3).

**Non-goals**

- Kindle Panel View and region magnification authoring. Previewer's own virtual panels (the
  `book-type: comic` metadata) are all Deckle asks for.
- Upscaling, denoising, OCR, or editing the art beyond fit, crop and grey.
- Native fixed-layout KFX. Previewer doesn't make it from an EPUB (§4), and building one by
  hand means undocumented KFX structures.
- DRM. A protected comic is refused like any other protected book.

**Anti-goals**

- Reflowing a fixed-layout book, or running text enrichments on it (typography, pop-up notes,
  presets, X-Ray, Word Wise, page lists). Any of these breaks a page that was laid out on
  purpose.
- Changing a publisher's fixed-layout EPUB beyond what the target needs to open it.
- Any cloud service or account. Everything runs locally, on files.

## 3. Detection

| Input | How it's recognised | Phase |
| --- | --- | --- |
| CBZ | extension `.cbz` (any case); a zip of images | 1 |
| Image folder | a folder with page images and no Markdown at its top level | 1 |
| Fixed-layout EPUB | OPF `<meta property="rendition:layout">pre-paginated</meta>`, or every spine item with `rendition:layout-pre-paginated` | 1 (seen by `inspect`), 2 (routed) |
| Fixed-layout AZW3/MOBI | EXTH 122 = `true` (already read by `deckle-mobi`) | 4 |
| CBR | extension `.cbr`, unpacked by the system's `bsdtar` | 4 |
| Image-only PDF | every page is one full-page image and has no text layer | 4 |

`inspect` gains `fixed_layout` and `right_to_left` on `BookInfo`. Every later stage decides
from those two flags, never from the file extension.

Reading direction, first match wins:

1. the owner's choice for this book (phase 4: a per-book toggle and `--rtl` / `--ltr`);
2. `ComicInfo.xml` `<Manga>YesAndRightToLeft</Manga>` in a CBZ or folder (`Yes` alone says
   nothing about direction, so it's ignored);
3. a fixed-layout EPUB's `<spine page-progression-direction="rtl">`;
4. left to right.

## 4. What Kindle Previewer actually does

Tested on 2026-11-01 with Previewer 4.0.3 on macOS: a 3-page pre-paginated EPUB with KCC-style
metadata, converted with `--convert`.

- **It doesn't refuse the book.** It prints "Enhanced Typesetting not supported for this book.
  Generating Mobi file", and `Summary_Log.csv` says `Not Supported` / `Success` with 0 errors.
  It writes `Mobi/<stem>_epub.mobi`, a KF7+KF8 file, and no KPF.
- **The per-book log has no Error row.** `tests/fake_previewer/unsupported.sh` writes one, so it
  stands for a *reflowable* book refused Enhanced Typesetting. Phase 2a adds `fixed_layout.sh`,
  which copies the real MOBI (`fixtures/comic/fixed-layout-rtl.mobi`) and writes the real
  logs.
- **The MOBI's EXTH keeps the layout:**
  - 122 fixed layout, 123 `comic`, 124 orientation lock, 126 original resolution, 127/128
    zero gutter and zero margin;
  - with an RTL spine and `primary-writing-mode` = `horizontal-rl`, also 527 = `rtl` and
    525 = `horizontal-rl`.
- **It has no EXTH 501 (CDE type) and no 113/504 (ASIN).** Deckle stamps these itself.

So the Kindle output for fixed layout is Previewer's MOBI, cut down to its KF8 half and
delivered as `.azw3`, not KFX.

## 5. What "passthrough" means on each target

| Target | Sends | Notes |
| --- | --- | --- |
| Kindle | `.azw3`: Previewer's MOBI, KF8 half only, with EXTH 501 and 113/504 stamped | CDE type follows the cover rule; X-Ray, Word Wise and page list are skipped |
| Kobo | the fixed-layout EPUB **without KEPUB markup**, still named `.kepub.epub` | `kepub::convert` only adds text spans and wrapper divs, which do nothing on image pages but risk the layout. The `.kepub.epub` name stays because a Kobo opens plain `.epub` in Adobe's renderer, which handles fixed layout poorly, and `.kepub.epub` in its own (changed in 3b; the spike confirms). Phase 3 spike: whether Kobo honours `page-progression-direction` |
| PocketBook, Boox, folder with `Epub` | the fixed-layout EPUB as `.epub` | PocketBook also reads CBZ; EPUB keeps direction and metadata |
| reMarkable, folder with `Pdf` | an image-only PDF, one page per image, sized to the screen | reMarkable's EPUB reader reflows; PDF is the only reliable fixed page there. Built in §12 (#279) on `deckle run --to remarkable`, which is also the first book send to a reMarkable |

A publisher's fixed-layout EPUB goes through **without content changes**. `normalize` makes no
typography, pop-up note, preset or cover changes to it, and phase 3's image step doesn't
resample it unless the owner asks. A CBZ or folder is Deckle's own EPUB, so the image step may
resize it.

## 6. The comic EPUB (phase 1, `deckle-comic`)

A CBZ or folder becomes an EPUB 3 that every later stage reads like any other book.

- **Pages:**
  - Pages are every JPEG, PNG and GIF, sorted naturally by path, case-insensitively and with
    digit runs compared as numbers (`page2` before `page10`).
  - These are skipped: `__MACOSX/`, hidden files, `ComicInfo.xml`, and anything that isn't an
    image (with no warning).
- **One page per image:**
  - Each page is an XHTML file with `<meta name="viewport" content="width=W, height=H">` set to
    the image's own pixel size, and the image fills it.
  - Image bytes are copied unchanged (stored, not recompressed).
- **OPF:**
  - `rendition:layout` `pre-paginated`, `rendition:spread` `landscape`, `rendition:orientation`
    `auto`;
  - Kindle's `book-type` `comic`, `original-resolution` (the first page's size),
    `zero-gutter`, `zero-margin`, `fixed-layout` `true`;
  - the first page is the cover (`cover-image` and `<meta name="cover">`).
- **Right to left:** `<spine page-progression-direction="rtl">` plus `primary-writing-mode`
  `horizontal-rl`. §4 shows Previewer turns these into EXTH 527/525.
- **Metadata from `ComicInfo.xml`** when present:
  - `Title`; else `Series` with `Number` ("Akira 3"); else the file name;
  - `Writer` as creator;
  - `Series` and `Number` as both `belongs-to-collection` and `calibre:series`;
  - `LanguageISO` as the language (else `en`);
  - `Summary` as the description.
- **Navigation:** one nav entry pointing at the first page.
- **Identifier:** `urn:deckle:comic:<sha256(title|writer|page count)[..8]>`, stable across runs.
- **Refused, with a clear message:**
  - a CBZ that isn't a zip;
  - no pages;
  - a page that won't decode;
  - a WebP page, because Kindle can't show WebP (phase 3 re-encodes it instead);
  - an encrypted zip entry.

## 7. Screens (phase 3)

`DeviceProfile` holds transport only. Phase 3 adds a `screen` table keyed by model, with
width × height in pixels, ppi and colour. The table is data, unit tested so every row is
non-zero. An unknown model falls back to 1264 × 1680 grey, the most common current Kindle
panel. Starting values, each confirmed on a device or the maker's spec page before shipping:

| Model | px | ppi | colour |
| --- | --- | --- | --- |
| Kindle (2022, 2024) | 1072 × 1448 | 300 | no |
| Paperwhite 5 / Signature (2021) | 1236 × 1648 | 300 | no |
| Paperwhite 6 (2024), Oasis 2/3 | 1264 × 1680 | 300 | no |
| Colorsoft (2024) | 1264 × 1680 | 300 (150 colour) | yes |
| Scribe (2022, 2024) | 1860 × 2480 | 300 | no |
| Kobo Clara BW / Colour | 1072 × 1448 | 300 | Colour: yes |
| Kobo Libra Colour | 1264 × 1680 | 300 | yes |
| Kobo Sage | 1440 × 1920 | 300 | no |
| reMarkable 2 | 1404 × 1872 | 226 | no |
| reMarkable Paper Pro | 1620 × 2160 | 229 | yes |
| reMarkable Paper Pro Move | 954 × 1696 | 264 | yes |

The image step for Deckle's own comic EPUBs:

1. **Spreads:** a landscape page on a portrait screen is split into two pages, right half first
   when the book reads right to left.
2. **Fit:** scale down to fit the screen, keeping the aspect ratio. Never scale up.
3. **Grey:** convert to greyscale on a black-and-white screen.
4. **Re-encode:** JPEG quality 85 for photos and colour, PNG for line art that stays smaller as
   PNG, and WebP pages decoded and re-encoded.
5. **Crop (opt-in):** auto-trim white margins, off by default.

The work is per device, so it runs at finish time, after the device-independent EPUB is
cached, the same split the device-aware build uses for KPF/KFX.

### 7.1 How phase 3a does it

- **Reading the model:**
  - **Mass storage:** a Kindle only gives "Kindle" and its firmware (`system/version.txt`).
  - **MTP:** it gives a product name ("Kindle Scribe", "Kindle Paperwhite").
  - So `screens::for_kindle` goes by **family**, with the largest panel in each: Scribe 1860 ×
    2480; Paperwhite and Oasis 1264 × 1680; Colorsoft 1264 × 1680 in colour; anything else the
    1264 × 1680 grey default.
  - Fitting to a panel a little larger than the real one costs nothing, since pages are never
    upscaled and the Kindle scales the rest of the way.
  - Telling models apart by their serial-number prefix is left for later.
- **Spreads split at import,** in `deckle-comic`, because splitting changes the page count and
  Previewer's MOBI is made before the device is known.
  - A landscape page becomes two portrait pages: left half first, or right half first for right
    to left.
  - The first page is never split: a wraparound cover would put the back cover first.
  - Split halves are re-encoded in the page's own format (JPEG quality 90).
- **Fit and grey at finish,** per Kindle, on the AZW3's image records
  (`deckle_mobi::azw3::map_images`), so Previewer doesn't run again for each device.
  - Each page is scaled down to fit the screen (never up) and turned grey on a black-and-white
    screen.
  - It's re-encoded **in its own format** (JPEG stays JPEG, quality 85; PNG and GIF stay what
    they are), because KF8 markup names each image's MIME type.
  - Only Deckle's own comics are touched (identifier `urn:deckle:comic:…`); a publisher's
    fixed-layout book goes as made.
  - A folder export has no device, so it isn't fitted. The queue's fallback is that build.
  - A page that won't decode stays as it was, and the run gets one warning.

## 8. Pipeline changes

- **Import (phase 1):** `import::import` routes `.cbz` and image folders to `deckle-comic`
  before the Markdown folder rule.
- **Normalize (phase 2):** when `fixed_layout` is set, it skips typography, pop-up notes, presets
  (already skipped, with a warning), generated covers and the accessibility rewrites. It still
  repackages the EPUB.
- **Convert (phase 2):**
  - `previewer::convert` returns the MOBI path when the summary says `Not Supported` /
    `Success` and the book is fixed layout. `Prepared` carries `Output::Kpf` or `Output::Mobi`.
  - A reflowable book that Previewer refuses Enhanced Typesetting for stays an error, as
    `enhanced_typesetting_refusal_is_reported_not_downgraded` requires.
- **Finish on Kindle (phase 2):**
  - a new `deckle-mobi` writer keeps only the KF8 records (as kindlestrip does) and sets EXTH
    501 and 113/504;
  - delivery writes `<Author sort> - <Title>.azw3` to `documents/`;
  - pruning and send records treat `.azw3` like `.kfx`;
  - X-Ray, Word Wise and the page list are skipped with one warning.
- **Finish on other readers (phase 3b):** `Reader::prepare` sends a fixed-layout EPUB with no
  KEPUB markup (still `.kepub.epub` on a Kobo), fitting Deckle's own comics to the reader. The
  reMarkable image PDF waits for book sends to the reMarkable.

## 9. Phases

Each phase is its own PR with its own tests, and each is useful when it lands.

1. **Comic import** (this PR): the `deckle-comic` crate, `import.rs` routing, and
   `BookInfo.fixed_layout` / `right_to_left`. With it, a CBZ already reaches a PocketBook or a
   folder as a fixed-layout EPUB, through `reader::book_epub`. Kindle still stops at convert
   until phase 2.
2. **Kindle**, in two PRs:
   - **2a, the pipeline:**
     - `previewer::convert_as` accepts the MOBI for a fixed-layout book;
     - `deckle_mobi::azw3::kf8_only` does the KF8 strip and EXTH stamp;
     - `finish` delivers `.azw3` under the cover rule's CDE type and ASIN, with the thumbnail
       taken from the AZW3's cover on mass storage;
     - `prepare` skips typography and pop-up notes, with one warning naming every skipped
       extra;
     - plus the `fixed_layout.sh` fake.
   - **2b, the wiring:**
     - the app's file picker, folder walk, book rows and library take `.cbz` and page folders;
       the Inbox takes `.cbz`;
     - a page folder in the library is kept as its EPUB snapshot, as a notes folder is;
     - a folder holding books of its own (EPUB, CBZ, PDF…) is a shelf to walk, not a comic,
       even with a stray image in it;
     - the outbox queue already carries the MOBI as `book.kpf` (2a recognises it by its bytes);
       a fallback that is an AZW3 is delivered as `.azw3`, never under the `.kfx` name;
     - pruning needs nothing: it only touches dated papers. The Kindle inventory already lists
       `.azw3`, and send records keep the delivered name;
     - plus README, CLI help and a `docs/devices.md` row.
   - **Device spike** (after 2a) on a Paperwhite and a Scribe: the cover shows, pages turn the
     right way for RTL, and PDOC vs EBOK behave as for KFX.
3. **Screens and other readers**, in two PRs:
   - **3a:** the screen table, spread splitting at import, and fit/grey on the Kindle AZW3
     (§7.1).
   - **3b:** Kobo and PocketBook take a fixed-layout book with no KEPUB markup, and Deckle's
     own comics fitted to the reader's screen. The model isn't read yet, so these are the
     largest current panels, in colour: Kobo 1440 × 1920, PocketBook 1404 × 1872. They only
     stop oversized scans. A publisher's book goes byte for byte. The reMarkable PDF is
     deferred (§5).
   - Spike: Kobo RTL, and PocketBook's cover tile.
4. **More inputs and control**, in two PRs:
   - **4a:**
     - **CBR:** a CBR that is really a zip is read as one. Any other is unpacked by the
       system's `bsdtar` into a temp folder (found as `DECKLE_BSDTAR`, `bsdtar`, or a `tar`
       that reports bsdtar), and bsdtar's own refusal of absolute and `..` paths keeps entries
       inside it.
     - **Fixed-layout AZW3/MOBI in:** on a Kindle it goes as it is. It skips import, normalize
       and Previewer (no Previewer needed), gets restamped by `kf8_only`, and is delivered as
       `.azw3`. The library reads its title and authors from its own header. Other readers
       refuse it, since there's no EPUB to make.
   - **4b:**
     - **Scanned PDFs:** a PDF the reflow refuses as a scan has each page rendered by PDFium at
       2480 px tall into a page folder, with a `ComicInfo.xml` holding the PDF's own title and
       author, and becomes Deckle's own comic, fitted per device. No OCR.
     - **Reading direction:** `deckle run --direction rtl|ltr` sets it for comics (CBZ, CBR,
       page folders) through `deckle_comic::convert_with`. The CLI makes the EPUB itself, so
       `RunOptions` doesn't change. A publisher's fixed-layout EPUB keeps its own direction.
       In the app (#280), "Reads right to left" in a comic's row menu sets it per book, kept in
       the settings' `book_directions` by path; the send makes the comic's EPUB the same way.
       A comic the library keeps remembers it: a CBZ or CBR as `"direction"` in its
       `deckle.json`, a page folder as the EPUB made in that order.

## 10. Calls made without the owner

- **Kindle output is `.azw3` from Previewer's MOBI, not KFX.** It's what Previewer makes (§4).
  KFX would mean reverse-engineering fixed-layout structures for a gain readers wouldn't see.
- **No KEPUB markup for fixed layout on Kobo,** but the `.kepub.epub` name kept (§5).
- **reMarkable gets PDF,** once Deckle sends books to it at all.
- **CBR uses the system `bsdtar`, never a bundled unRAR.** unRAR's licence is a poor fit for
  GPL-3. macOS and Windows 10+ ship `bsdtar` as `tar`; on Linux, when it's missing, the CBR is
  refused with a message saying so.
- **`ComicInfo` `Manga=Yes` doesn't flip direction**; only `YesAndRightToLeft` does.
- **No upscaling; cropping is opt-in.** Spreads split only on portrait screens.
- **Phase 1 copies images untouched and refuses WebP** rather than re-encoding them. Phase 3
  owns all image changes.
- **The unknown-model screen is 1264 × 1680 grey.**

## 11. Tests (phase 1)

- `deckle-comic` (`tests/import.rs`, CBZs built in memory):
  - **natural order:** 3 pages including `page2` and `page10` come out in natural order, with
    a pre-paginated OPF, the viewport set to each image's size, and the first page as cover;
  - **right to left:** `ComicInfo.xml` with `YesAndRightToLeft` gives an RTL spine and
    `horizontal-rl`;
  - **metadata:** `ComicInfo` title, writer, series and language land in the OPF;
  - **folders:** a folder of images converts the same way as a CBZ;
  - **refusals:** an empty CBZ, a corrupt page, a WebP page and a non-zip `.cbz` each give
    their own message.
- `deckle-core`:
  - `inspect` reports `fixed_layout` and `right_to_left` for the imported comic, and neither
    for a reflowable book;
  - `import::import` routes a `.cbz`;
  - `reader::book_epub` on a CBZ keeps `pre-paginated` and every page through normalize.

## 12. Build notes: comics to a reMarkable (#279)

The reMarkable row of §5 was deferred because nothing but the paper went to a reMarkable.
This phase adds the book send and the comic PDF together, since the send alone is small.

### 12.1 `deckle run --to remarkable`

- A new `--to` word beside `device` and a folder path. The books go into the reMarkable's
  outbox (`outbox-remarkable/`, the paper's) and are delivered by the same push the paper
  uses (`sync::run_remarkable`): right away when the tablet answers at `10.11.99.1`, else by
  the next `digest push` or `watch` that finds it. `--kindle` with it is refused as before.
- **A text book** goes as an EPUB, made by `reader::book_epub` (import, normalize, recaps and
  Who's who as asked): the same file the paper's long reads are, and what `[remarkable]
  books` sends. reMarkable's reader reflows it itself.
- **A comic** (CBZ, CBR, page folder, or a scanned PDF the import turns into one) goes as an
  image-only PDF (§12.2), named `<stem>.pdf`.
- **A publisher's fixed-layout EPUB** is refused with a message: its pages are XHTML that
  would need rendering, and reMarkable's EPUB reader would reflow them. Deckle's own comic is
  told from a publisher's by its `urn:deckle:comic:` identifier, as `Reader::prepare` does.
- The outbox is pushed whenever something waits in it, whether or not `[remarkable] enabled`
  is set: `enabled` says the *paper* goes there. The watcher's cost is unchanged (it only asks
  the tablet while the outbox is non-empty).
- No Kindle Previewer is needed. The books aren't added to the library (as for a Kobo).

### 12.2 The image PDF (`deckle_comic::pdf`)

- `pdf::from_epub(epub, screen, page)` reads Deckle's comic EPUB in spine order (the OPF's
  `itemref`s, each page's one `<img>`), fits each image with `pages::fit` for `screen`, and
  writes one PDF page per image with `pdf-writer`.
- **Page size** is the tablet's screen in points (`digest::pdf::Screen::page`: pixels at the
  model's dpi), the same page the paper uses, so a comic and the paper read alike. The image
  is centred on the page at the screen's pixel-to-point scale; a page smaller than the screen
  (never scaled up) sits in white margins.
- **Encoding:** a JPEG goes in as it is (`DCTDecode`, grey or RGB); a PNG or GIF is decoded
  to raw samples and deflated (`FlateDecode`), so line art stays lossless. A page that won't
  decode is skipped, counted, and reported as a warning, like `fit_epub`.
- **Colour** follows `[remarkable] photos` (the screen's own choice when unset): grey on a
  reMarkable 2, colour on a Paper Pro.
- **Direction:** the spine already reads in page order (a right-to-left comic's spread halves
  were cut right half first at import), so the PDF's pages are the spine's; nothing more.
- The title and author from the OPF go in the PDF's Info dictionary.

### 12.4 Easier to reach (follow-up to #279)

- **`[remarkable] address`**: the tablet's address when it isn't `http://10.11.99.1` — the
  community `webinterface-wifi` tweak, or an SSH port-forward, puts the same web interface on
  the home network. `DECKLE_REMARKABLE_URL` still wins (tests); then `address`; then the USB
  address. On the app's reMarkable card as **Address**, blank for USB.
- **`deckle devices`** lists a reMarkable that answers, beside the Kindles, with its address
  and sync id, so the owner can tell the USB web interface toggle took.
- **App: Send to reMarkable** on the Inbox, beside Export to folder. A `remarkable` run
  target: each book through `remarkable::book` (comic → PDF, else EPUB, with recaps and
  Who's who as the settings ask, and the comic's reading direction from the settings), into
  the outbox, then one push when the tablet answers. A book's final event says it went or
  that it waits in the outbox. No Previewer check for this target. Not in the web app.
- Still owed on hardware (spike 4): whether the web interface can upload into a folder, and
  whether it answers over Wi-Fi with the toggle alone. SSH delivery (prune, model detection,
  a Deckle folder) stays a research issue.

### 12.5 Over SSH (research 2026-11-02, built without a tablet)

**What the research found.** Every reMarkable runs dropbear with a root login; the password
is under *Settings → Help → About → Copyrights and licenses → GPLv3 Compliance*. On a
reMarkable 1 or 2 that's all. On a **Paper Pro, Paper Pro Move or Paper Pure, SSH exists
only after reMarkable's own Developer Mode, which factory-resets the tablet and ends
warranty support** — the owner's decision, made on the tablet; Deckle never enables it,
never asks for it, and the docs say so. SSH over Wi-Fi is off by default on the Paper Pro
family and on a reMarkable 2 past OS 3.20; the owner turns it on once over USB with
reMarkable's `rm-ssh-over-wlan on`, which Deckle also never runs. **Deckle only ever
connects to an SSH that already answers.** Documents live in
`/home/root/.local/share/remarkable/xochitl/` as `<uuid>.pdf|epub`, `<uuid>.metadata`
(`type`, `visibleName`, `parent`, `lastModified` in epoch ms, `deleted`, `pinned`,
`version`) and `<uuid>.content` (`fileType`). xochitl notices new files only on
`systemctl restart xochitl` (with `reset-failed` first, or repeated restarts trip systemd's
start limit and reboot the tablet); the restart kicks the owner out of whatever they're
writing for a few seconds. A folder is a `CollectionType` entry; a document is removed by
`deleted: true`. Dropbear has no SFTP; `scp` rides the exec channel. Old dropbear offers
only `ssh-rsa` host keys.

**What Deckle does** (`deckle_core::remarkable_ssh`, `[remarkable] transport = "ssh"`):

- Connects with `russh` (pure Rust, `ring`), user `root`, password from the system keychain
  (`deckle digest remarkable password`; `DECKLE_REMARKABLE_PASSWORD` only without a
  keychain), to `ssh_host` (default `10.11.99.1`) port `ssh_port` (22). The host key is
  trusted on first use and pinned in `remarkable-hostkey.json` beside `digest.toml`; a key
  that changes refuses with a message naming the file to delete.
- Uses only the exec channel: `cat > file` with the bytes on stdin, `cat`, `mkdir -p`,
  `date +%s`, and the one `systemctl reset-failed xochitl.service; systemctl restart
  xochitl.service`. Nothing else is ever run on the tablet.
- Keeps Deckle's documents in a **`Deckle` folder** (`ssh_folder`), made on first push.
- The outbox and the push are the USB path's: a file whose name the folder already shows is
  taken as delivered; the rest go up. **Prune works here**: with `keep`, older
  `deckle-daily-*` papers in the folder are marked deleted, as on a Kindle.
- **The restart (ruling b):** after uploading or pruning, Deckle restarts xochitl only when
  the tablet looks idle — no document's `lastModified` within the last 10 minutes by the
  tablet's own clock. Otherwise the push says the books are on the tablet and will show
  after its next restart, leaves a `.restart-owed` marker in the outbox, and the next push
  or watch tick tries again. Never more than one restart per push.
- A Paper Pro screen with `transport = "ssh"` gets one note in the output that SSH on that
  model is reMarkable's Developer Mode, the owner's own doing; Deckle neither refuses nor
  enables anything.

### 12.3 Tests

- `deckle-comic`: a three-page comic (JPEG, PNG, oversized page) becomes a three-page PDF,
  pages the screen's size, JPEG passed through and PNG deflated, the big page fitted; a
  broken page is counted and skipped; the right-to-left spread order holds.
- `deckle-cli` (`tests/it/remarkable.rs`, the stand-in tablet): `run --to remarkable` with a
  CBZ uploads `<stem>.pdf` holding `%PDF-`; with an EPUB uploads `<stem>.epub`; with a
  publisher's fixed-layout EPUB fails with the refusal; with no tablet, the books wait and
  `digest push` sends them without `[remarkable] enabled`.
