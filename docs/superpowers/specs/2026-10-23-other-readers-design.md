# Other readers: Kobo, PocketBook, Boox, reMarkable and an iPad folder

Status: accepted, 2026-10-23. No issue yet. The owner delegated the §13 answers; they are ruled there. Everything below about devices other than Kindle has to be checked on real hardware first (§11, spikes 1–4) before code depends on it.

## 1. Why

Deckle does two things: it makes good books from what you own, and it prints a morning paper. Only the last step, getting the result onto a Kindle, needs a Kindle. Everything before that already produces a plain EPUB 3:

- the imports (`deckle-docx`, `deckle-txt`, `deckle-pdf`, web articles) and binding several files into one book;
- normalize and typography;
- the whole Daily (`deckle-digest` writes an EPUB, and `deckle-morgue` keeps it).

The Kindle-only part comes after that:

- Previewer → KPF → KFX (`deckle-kfx`);
- X-Ray `.asc` and Word Wise `.kll`;
- cover thumbnails and PDOC/EBOK;
- MTP to Kindles;
- the reads that look at the device afterwards (`.yjf` positions, `My Clippings.txt`, `vocab.db`).

People with a Kobo, a reMarkable or an iPad can't use Deckle today, even though most of what it does is just as useful to them, and the paper most of all. This spec opens the delivery step to other readers. It doesn't port the Kindle features.

## 2. Goals

- **One idea of a delivery target**, with Kindle as one kind among several. The outbox, `watch`, `push`, prune and the per-device `[sync]` settings work for every kind.
- **Kobo is supported fully:** books and the paper over USB as KEPUB, pruning, and highlights, From Your Shelf and readership read from the Kobo's own database.
- **PocketBook and Boox:** books and the paper as EPUB over USB, with pruning. They get no device reads in this spec.
- **An export folder** (iCloud Drive, Dropbox, any folder): the paper and books dropped there each morning, in color where the reader has color. This is the iPad answer.
- **A PDF edition of the Daily**, laid out for a given screen, and **reMarkable delivery** through the tablet's USB web interface. These are phase 4; the PDF typesetting gets its own spec (§9).
- **Kindle behaves exactly as it does now**, byte for byte: every existing test passes unchanged.

## 3. Non-goals

- **No X-Ray, Word Wise or cover thumbnails off Kindle.** They are Kindle file formats. Off Kindle, the cover comes from inside the EPUB.
- **No vendor cloud accounts in this spec:** no Kobo/Dropbox sign-in through Deckle, and no reMarkable cloud API. The reMarkable cloud protocol is unofficial, it has changed under third-party tools before, and it would need an account token. It's a possible later increment, marked experimental (§13).
- **No iOS or iPadOS app.** That still stands, as in the save-to-paper design. The iPad gets files.
- **No jailbreak, SSH or developer-mode paths** (reMarkable SSH, KOReader plugins). They may be documented in `docs/devices.md` later.
- **No MTP to Boox in this spec.** Boox gets the folder target over the Android file transfer the owner already has, or USB storage where the model offers it. The MTP client in `deckle-transport` stays Kindle-only.

## 4. What each kind of reader gets

| | Kindle | Kobo | PocketBook | Boox | reMarkable | Folder (iPad…) |
|---|---|---|---|---|---|---|
| Connection | USB storage / MTP | USB storage | USB storage | folder | USB web interface | folder |
| Book format | KFX | KEPUB (`.kepub.epub`) | EPUB | EPUB | the paper as PDF, books as EPUB | EPUB, optional PDF |
| Paper photos | gray | gray (color on Libra/Clara Colour) | gray | per setting | gray (color on Paper Pro) | color |
| X-Ray, Word Wise | yes | – | – | – | – | – |
| Prune old editions | yes | yes | yes | yes | yes (spike 4) | yes |
| Highlights export | yes | yes (`Bookmark` table) | later | – | – | – |
| From Your Shelf | yes | yes | later | – | – | – |
| Readership | yes (`.yjf`) | yes (per-chapter progress) | – | – | – | – |
| Needs Previewer | yes | no | no | no | no | no |

Without Previewer installed, Deckle still works fully for every column except Kindle. The README's install steps say this.

## 5. The delivery target

### 5.1 Types

`deckle-core::deliver::Target` grows from three variants to one per kind. The existing ones keep their meaning:

```rust
pub enum Target {
    Folder(FolderTarget),          // was Folder(PathBuf)
    MassStorage(Device),           // a Kindle, unchanged
    Mtp(MtpInfo),                  // a Kindle, unchanged
    Kobo(KoboDevice),
    PocketBook(PlainDevice),
    Remarkable(RemarkableUsb),
}

pub struct FolderTarget {
    pub dir: PathBuf,
    pub output: Output,
}
```

`Output` says what a target takes. Each kind has a fixed default, and the owner sets it only for a folder:

```rust
pub struct Output {
    pub format: Format,            // Kfx | Epub | Kepub | Pdf
    pub photos: Photos,            // Gray | Color
    pub screen: Option<Screen>,    // px and dpi, for the PDF edition and the cover size
}
```

`Target::output()` returns it. The Kindle variants return `Kfx`/`Gray`, and nothing in the Kindle path reads the new field.

### 5.2 Finding devices

`detect_in` becomes a list of probes over the same mount roots. Each probe gets a root and returns a `Found` or nothing. The first probe that matches wins:

- **Kindle:** `documents/` and `system/`, as now.
- **Kobo:** `.kobo/` with `KoboReader.sqlite`. The model and firmware come from `.kobo/version` (spike 1 confirms the field order).
- **PocketBook:** `system/config/` and a PocketBook marker file. Spike 3 picks a marker that no Kindle has.
- **reMarkable:** isn't a volume. When the reMarkable option is on, `watch` asks `http://10.11.99.1/` with a 1 s timeout on each check; the address is only reachable while the tablet is plugged in with the USB web interface enabled.
- **Folder targets** aren't found. They're listed in `digest.toml` (§7) and are always "connected".

`detect_mass_storage` keeps its name and signature for the Kindle callers. A new `detect_readers` returns every kind. `resolve_target("device")` in the CLI moves to `detect_readers`, so `--to device` takes the first reader of any kind, mass-storage Kindles first as now.

### 5.3 Device ids and profiles

`sync::device_id` gets one arm per kind:

- Kobo: `kobo/<serial>`, with the serial from `.kobo/version`;
- PocketBook: `pocketbook/<volume>`;
- reMarkable: `remarkable/usb`;
- folder: `folder/<name>`, using the name from `digest.toml`.

`[sync.devices."<id>"]` and `devices.json` then work as they do now. `DeviceProfile` gets a `kind` field, defaulting to `kindle` so existing files load unchanged. The Kindle-only fields (`cde_override`, `verified_real_asin`, `thumbnails_writable`) stay and are ignored for other kinds.

## 6. The outbox and finishing

The outbox (`deckle-core::queue`) holds Kindle output today: a finished `.kfx`, or a `.book/` with the KPF and a fallback KFX. It gains an EPUB entry:

```text
<name>.book/
  book.kpf        Previewer's output   (only when a Kindle is known, see below)
  book.epub       the normalized EPUB   (always)
  prepared.json   versioned; v2 adds the EPUB
  fallback.kfx    (only with book.kpf)
```

The paper is queued the same way: `<edition>.book/` with `book.epub`, plus the PDOC KFX when a Kindle is known. Today's bare `<edition>.kfx` entries are still read, so an outbox written by 0.3.x flushes after the upgrade.

**When a Kindle is known:** Previewer is slow, and doing it at build time is what lets a 05:30 paper land the moment a Kindle connects (device-aware build). The KPF is made at build time only when `devices.json` holds a Kindle, or no device at all (a fresh install behaves as now). An owner with only a Kobo never needs Previewer.

**Finishing at push**, by kind:

- **Kindle:** as now.
- **Kobo:** `kepub::convert(book.epub)` (§8.1), named `<name>.kepub.epub`.
- **PocketBook, Boox, folder with `Epub`:** `book.epub` as is.
- **Folder with color photos:** the color build (§6.1).
- **reMarkable, folder with `Pdf`:** the PDF edition for that screen (§9). A book that isn't the paper goes out as EPUB: books reflow, and only the paper gets a PDF layout.

**Prune** compares names by stem and ignores the extension, so `deckle-daily-2026-10-20.kepub.epub` counts as an edition the way `.kfx` does now. The rule that only prunes names Deckle wrote (`is_deckle_sidecar`, `prunable`) stays.

### 6.1 Color photos

`article.rs` converts photos to grayscale when it fetches them (`img.grayscale()`). That moves to render time:

- the fetch keeps the color JPEG at 1000 px, as now;
- `render` takes `Photos` and converts to gray when asked.

The paper's EPUB in the outbox is the gray one. When any configured target wants color, the run renders a second EPUB, `book.color.epub`. Rendering is cheap; the network fetch, which isn't, happens once.

The cover stays 1264 px wide (`cover.rs` `WIDTH`) for EPUB. The PDF edition sets its own size from `Screen`.

## 7. Config

`digest.toml` gets one table per folder target. Devices that Deckle finds need no table: `[sync]` and `[sync.devices]` already cover them.

```toml
# Also put each edition in a folder: an iCloud Drive folder reaches an iPad's Files app.
[[export]]
name = "ipad"                      # the device id is folder/ipad
dir = "~/Library/Mobile Documents/com~apple~CloudDocs/Deckle Daily"
format = "epub"                    # epub | pdf | kepub
photos = "color"                   # color | gray
keep = 7                           # editions kept in the folder; 0 keeps all
books = false                      # also long reads and Inbox books, not only the paper

# reMarkable over its USB web interface (Settings → Storage → USB web interface).
[remarkable]
enabled = false                    # the paper goes as PDF, books as EPUB (§13)
```

- An export runs after every build, plugged in or not. A folder is always reachable, and it has no outbox.
- `dir` follows the `~` rule `[reading] inbox` uses. A missing folder is a report line, not a failed run, the same as an email failure (send-to-Kindle email §13).
- `deny_unknown_fields` and the `toml_edit` merge apply, as for every table.
- **Email stays Kindle-only** in this spec (the `to` check in `mail.rs`). Opening it to any address is §13.

## 8. Kobo

### 8.1 KEPUB

KEPUB is an EPUB whose text is wrapped in `<span class="koboSpan" id="kobo.P.S">` for paragraph `P` and sentence `S`. With it a Kobo shows page and chapter counts, reading statistics and per-chapter progress; plain EPUBs go through an older renderer. The conversion is a pass over each XHTML file:

- wrap sentences in spans, skipping `<pre>`, `<code>`, `<svg>`, `<math>`, and tables Deckle marks as layout (the paper's weather strip and puzzle grid);
- add Kobo's two wrapper divs (`book-columns`, `book-inner`);
- leave everything else unchanged.

It goes in a new module, `deckle-core::kepub`. It reuses the XHTML parser normalize already uses and adds no new dependency. The behaviour is checked against the output of kepubify (MIT) on the fixtures, without vendoring its code.

### 8.2 Reading the Kobo

`.kobo/KoboReader.sqlite` is opened read-only, from a copy taken at plug-in, so the Kobo's own database is never locked. Spike 1 confirms each column on current firmware.

- **Highlights:** rows in `Bookmark` with `Text` set, joined to `content` for the title and the chapter. They feed the existing highlights Markdown writer in `deckle-shelf::highlights` through a new source, so the files look the same as a Kindle's.
- **From Your Shelf:** books in progress from `content` (`ContentType = 6`, with `___PercentRead` and `DateLastRead`). Looked-up words come from `WordList`, if the firmware keeps one (spike 1).
- **Readership:** a KEPUB paper has a `content` row per chapter with its own `___PercentRead`. That is section-level reading, already in the shape `deckle-readership::report` wants, so Kobo skips the `.yjf`/layout mapping completely. A new `Positions` source feeds the same `report`.

No new dependency: `rusqlite` is already in the workspace (`deckle-core`, `deckle-shelf`) for `vocab.db`.

### 8.3 Eject

A Kobo only picks up new books after it's ejected, when it rescans. The push report and the app say "Eject the Kobo to see the new paper". Deckle doesn't eject the device itself.

## 9. The PDF edition and reMarkable (phase 4)

The paper's EPUB looks poor in reMarkable's EPUB reader: the tables, the masthead and the photos lose their layout. The reMarkable gets a PDF made for its page size instead:

- **reMarkable 2:** 1404 × 1872 px at 226 dpi;
- **Paper Pro:** 1620 × 2160 px, in color;
- **Paper Pro Move:** 954 × 1696 px.

Spike 4 confirms each.

The same PDF also serves iPad (`format = "pdf"`) and Boox.

**The typesetting needs its own spec**, `2026-xx-xx-pdf-edition-design.md`, before any code. That spec decides between:

- **(a) embedding Typst** (the `typst` crate): a template per page, fed from `Gathered`. It's large, but it does real line breaking, columns and hyphenation.
- **(b) a small layout engine** on `printpdf`, using the fonts the cover already bundles. It's lighter, but it means building our own columns, justification and image flow.

This draft leans towards (a).

**Delivery:** `POST http://10.11.99.1/upload` with the file as multipart, after `GET /documents/` to find editions to prune. Spike 4 confirms both calls, and whether the interface can delete at all. If it can't, prune is off for reMarkable and the report says so.

## 10. App

- The **Kindle tab** becomes **Your reader**. It lists every reader found or configured, each with its kind. Kindle-only controls (PDOC/EBOK, X-Ray, Word Wise, Anki export) appear only on Kindles.
- **Settings → Kindles** becomes **Settings → Readers**, with an **Add export folder** button that writes an `[[export]]` table. On macOS, the folder picker opens on iCloud Drive.
- The **Daily tab's press bar** names every place the paper goes ("Kindle Scribe, Kobo Libra Colour, iPad folder").
- **First run without Previewer** no longer starts with "install Kindle Previewer". It asks which reader you have, and points to Previewer only for Kindle.

## 11. Spikes before code

1. **Kobo** (Libra Colour or Clara BW, current firmware):
   - the `.kobo/version` fields;
   - the `content`, `Bookmark` and `WordList` columns;
   - that a KEPUB paper shows per-chapter `___PercentRead`;
   - that deleting `.kepub.epub` files and ejecting removes them from the library cleanly.
2. **KEPUB:** Deckle's conversion of three fixtures plus a paper, side by side with kepubify's, on the device. Sentence counts don't have to match; reading statistics have to work.
3. **PocketBook:** a marker file no Kindle has, and whether the cover in the EPUB shows on the library tile.
4. **reMarkable** (rM2 and Paper Pro):
   - the USB web interface's upload, listing and delete;
   - the screen sizes;
   - how the paper's EPUB looks there now, to settle whether EPUB is offered at all.

Results go in `docs/devices.md`, as for Kindles.

## 12. Tests the build must add

- **Detection:** fixture roots for a Kindle, a Kobo and a PocketBook, plus one with both Kindle and Kobo markers, each found as the right kind. The existing `detect_in` tests pass unchanged.
- **Kindle unchanged:** every existing `deliver`, `queue`, `sync` and CLI test passes without edits. A 0.3.x outbox with bare `.kfx` papers still flushes.
- **Outbox v2:**
  - `prepared.json` v1 loads;
  - with no Kindle in `devices.json` and some other device present, Previewer isn't run (the fake Previewer records no call);
  - with a Kindle present it is.
- **KEPUB:**
  - spans in every text paragraph and none inside `<pre>`, `<svg>` or the layout tables;
  - ids unique and in order;
  - the result still passes the EPUB checks normalize runs;
  - a fixture compared against a stored golden.
- **Kobo reads**, against a small `KoboReader.sqlite` fixture built in the test:
  - highlights Markdown the same shape as from a Kindle's clippings;
  - shelf books;
  - readership rows per section;
  - the live file is never opened for writing.
- **Prune** across extensions: `.kfx`, `.epub`, `.kepub.epub` and `.pdf` editions are kept to `keep`, and a file Deckle didn't write is never touched.
- **Export:**
  - color and gray builds differ only in the photos;
  - a missing `dir` is a report line with exit code 0;
  - `keep` applies in the folder.
- **reMarkable**, against a recording fake HTTP server:
  - an upload names the edition;
  - a 1 s timeout when the tablet isn't there.
- **Config:** the starter file's commented `[[export]]` and `[remarkable]` parse once uncommented.

## 13. Rulings (the owner's open questions)

The owner delegated these; each is ruled as below and can be reopened on the PR.

- **Folder first, then Kobo.** The export folder is the smallest piece that exercises the new `Target` and the outbox v2, and it unblocks iPad and Boox at once. Kobo then builds on a refactor already proven in use.
- **reMarkable: the paper as PDF only, books as EPUB.** A newspaper layout is what reMarkable's EPUB reader handles worst, and books reflow fine there; typesetting whole books as PDF isn't planned. So `[remarkable]` has no `format` key. If spike 4 finds EPUB books unusable too, reMarkable takes the paper only.
- **reMarkable cloud: not in this spec.** It's revisited only after the USB path ships, and only if owners ask. If it comes, it's an experimental opt-in with its own spec and the token in the keychain (as the SMTP password).
- **Email stays Kindle-only.** The `@kindle.com` check is a guard against sending the paper to the wrong place, and the export folder already covers iPad. Kobo, PocketBook and reMarkable have no email delivery of their own.
- **Keep the Kindle pitch.** The README headline and app copy still lead with Kindle, where Deckle does the most. One line is added: "Kindle gets the full treatment; Kobo, PocketBook, reMarkable and iPad get the paper and clean books." The tab renames in §10 stand.

## 14. Phasing and sizing

1. **Export folder (M), built.** §6.1, §7 `[[export]]`, and the app's export-folder card. iPad and Boox work at the end of it. The `Target` refactor (§5) and outbox v2 (§6) moved to phase 2 (§15).
2. **Kobo (M–L), built** (§16). Detection, KEPUB, the EPUB outbox, prune, highlights and the shelf. Readership and the app tab moved on. Spikes 1–2 are still owed.
3. **PocketBook (S), built** (§17). Detection, delivery and prune. Its reads are left for later. Spike 3 is still owed.
4. **PDF edition and reMarkable (L), built** (§18). Its own typesetting spec (`2026-10-23-pdf-edition-design.md`), then §9. Spike 4 is still owed.

None of this can be tested in CI against a real Kobo, PocketBook or reMarkable. Each phase ends with the owner's hardware check, recorded in `docs/devices.md`.

## 15. Build notes, phase 1 (rulings made while building)

Phase 1 shipped the export folder, color photos and the Daily tab's card. Two parts of §14's phase 1 moved to phase 2, where Kobo is the first thing that needs them:

- **No `Target` refactor yet (§5).** An export folder has no outbox and nothing to detect, so it runs as a step of `digest run` after the build (`deckle-digest::export`), not as a `Target`. `Target` gains its kinds in phase 2, with Kobo detection. `[[export]]` still gets its `folder/<name>` id there.
- **No outbox v2 yet (§6).** The rule "Previewer only when a Kindle is known" depended on `devices.json`, which only the app writes: a Kindle owner who only uses the CLI would have lost the paper. Phase 1 uses a narrower rule instead. **When Previewer can't be found and an `[[export]]` is set, `digest run` sends the paper to the export folders only.** It notes "no Kindle copy" in the build notes and queues nothing for a Kindle. With no export set, a missing Previewer fails the run as before. Phase 2 revisits this together with the EPUB in the outbox.

Other rulings:

- **Color is opt-in at fetch.** Photos are fetched in color only when some export has `photos = "color"` (`Config::wants_color`). Every EPUB writer (the paper, long reads, articles sent on their own) grays photos unless told otherwise. `gray_jpeg` reads only the header of a JPEG that's already grayscale and passes it through untouched. So with no color export, the Kindle's paper is the same as before, photo bytes included.
- **The color edition** is written to `color/` beside the paper, under the same file names (`BuildReport.color_epub`, `color_long_reads`). The Morgue, email and the Kindle keep the grayscale paper.
- **Only the morning run exports:** `digest run` without `--to <folder>`, as with email. `digest build` writes no exports.
- **Long reads without Previewer** print inside the paper (`long_read_words` is lifted for that run), unless an export has `books = true`. Then they go to that folder as books.
- **`format`** accepts only `"epub"` until KEPUB (phase 2) and PDF (phase 4) arrive. Any other value is a config error.
- **Folders:** a missing export folder is created only when its parent exists, so a turned-off iCloud Drive is reported, not recreated. Files are copied through a hidden `.<name>.tmp` and renamed, so a sync client never uploads half a book. Pruning removes only `deckle-daily-YYYY-MM-DD.epub` names beyond `keep`.
- **App:** the **Export folders** card sits on the Daily tab, next to **Email to Kindle**, because `[[export]]` lives in `digest.toml` like the Daily's other cards. Settings → Readers and the tab renames in §10 wait for phase 2, when there is more than one kind of reader to list. On a Mac, **Add folder…** opens on iCloud Drive. Each change saves at once, as on the Your Day card.
- **Not checked on an iPad yet.** The owner's check for phase 1: an export to iCloud Drive, then the paper opened from the Files app in Books, with color photos.

## 16. Build notes, phase 2 (Kobo)

Built without a Kobo: against Kobo's files as the e-reader community documents them, and tested on a simulated drive (a folder with `.kobo/version` and a `KoboReader.sqlite` built from the columns Deckle reads). Spikes 1 and 2 (§11) are still owed, and `docs/devices.md` has the Kobo row pending.

- **A Kobo isn't a `Target`.** §5 planned new `Target` variants, but every consumer of `Target` is Kindle-specific (KFX, MTP, `.sdr` sidecars, `.yjf`, `vocab.db`). A variant would have meant a "not on a Kobo" arm in some 16 files. So a Kobo is its own type, `deckle_core::kobo::Kobo`, with its own sync, `sync::run_kobo`, and the CLI picks one or the other. The `Target` refactor is dropped, not deferred.
- **Outbox v2 became a second outbox.** §6's single outbox with an EPUB inside each entry became `outbox-epub/`, beside the Kindle's untouched `outbox/`.
  - It holds `<name>.epub` files, made KEPUBs as they go over.
  - The Kindle's outbox never holds an EPUB, so a Kindle owner's pushes and outbox are exactly as before.
  - An owner with both readers gets the paper on both, each from its own outbox.
- **When the EPUB outbox is filled:** a Kobo has been seen (`kobos.json` beside the app's `devices.json`), or one is plugged in now, or there is no Previewer. `devices.json` itself isn't written: the app writes it too, and the CLI saving it could race the app.
- **Previewer missing:** `digest run` fails, as before, only when nothing else can take the paper: no export folder and no Kobo seen. Otherwise it notes "no Kindle copy" and carries on (phase 1's rule, widened).
- **Long reads without Previewer** print inside the paper only when neither a Kobo nor an export with `books = true` takes them.
- **Inbox books** are converted for the Kobo first (imported, cleaned, KEPUB; no Previewer). A Kindle step that has to wait for Previewer doesn't hold the book in the Inbox once the Kobo has its copy. A `.kfx` dropped in the Inbox stays Kindle-only.
- **`deckle run … --to device`** sends to a Kobo when no Kindle is plugged in, as `Deckle/<file stem>.kepub.epub`, cleaned like any book but without typography, Open Library or X-Ray. `deckle highlights` reads a Kobo the same way.
- **KEPUB conversion** is Deckle's own (`deckle_core::kepub`), not kepubify's code. It makes one pass over each XHTML file, with no tree rewrite:
  - each sentence of each text run goes in a `koboSpan`, numbered by paragraph (block element) and sentence;
  - each `img` gets a span of its own;
  - Kobo's `book-columns`/`book-inner` divs wrap the body;
  - text in `pre`, `code`, `script`, `style`, `svg`, `math` and tables is left alone;
  - a book that already has `koboSpan`s comes back unchanged.
  
  Spike 2 (side by side with kepubify on a device) is still owed.
- **Kobo files:** books go to `<drive>/Deckle/`, flushed to the drive and renamed into place. The prune only removes `deckle-daily-*.kepub.epub` there, and a Morgue edition sent again is exempt by its stem.
- **Reads** use a copy of `KoboReader.sqlite` (with its `-wal`/`-shm` when present), opened read-only. Books in progress are `content` rows with `ContentType = 6` and `ReadStatus = 1`, read for `___PercentRead` and `DateLastRead`. Highlights and notes are `Bookmark` rows with `Text` or `Annotation`, joined to their book. Both are written out as a Kindle's `My Clippings.txt`, so the highlights export, the puzzle's notes and From Your Shelf read them unchanged.
- **Kept for later:**
  - **Readership** needs spike 1 to show per-chapter `___PercentRead` on a KEPUB paper.
  - **Looked-up words:** a Kobo's word list isn't read.
  - **The Morgue's "on the Kindle" marks** stay Kindle-only.
  - **The desktop app** sends books to a Kindle only. A Kobo gets the paper through the scheduled `digest run`/`watch`, which the app already sets up. The tab renames in §10 wait until the app sends to a Kobo too.
- **`DECKLE_MOUNTS`** (a path list) replaces the places Deckle looks for a drive. It's for a reader mounted somewhere unusual, and for the CLI tests' simulated Kobo.

## 17. Build notes, phase 3 (PocketBook)

Built without a PocketBook: tested on a simulated drive, so spike 3 (§11) is still owed.

- **One type for both.** Phase 2's Kobo-only code became `deckle_core::reader`, whose `Reader` is a Kobo or a PocketBook. Everything that doesn't depend on the device kind now lives there: delivery into `Deckle/`, the prune, `readers.json`, and the book made for the EPUB outbox. `sync::run_kobo` became `sync::run_reader`. Kobo-specific code stays in `deckle_core::kobo`: detection, the version file and the database reads.
- **`kobos.json` is `readers.json`.** It was renamed before any release, so nothing needs migrating.
- **Detection:** a PocketBook is a drive with `system/explorer-3/`, or with both `system/config/` and `system/profiles/`, and in either case no `documents/`. That rules out a Kindle, which has `documents/` and `system/`. A Kobo is checked for first.
- **Format:** a PocketBook gets the EPUB as it is, as `Deckle/<stem>.epub`, once it opens as a zip.
  - The EPUB outbox now holds cleaned, plain EPUBs for both kinds. An Inbox book is no longer queued as a KEPUB.
  - The Kobo's KEPUB is made at push, as the paper's always was.
  - A PocketBook's prune counts `deckle-daily-*.epub` and never a `.kepub.epub`.
- **Not read:** a PocketBook has no highlights export and no shelf, and `deckle highlights` with only a PocketBook plugged in says there's no reader to read from. Its library database (`system/explorer-3/explorer-3.db`) is the obvious source when this is picked up.
- **JSON:** `digest run`'s `kobo_queued` and `kobo` fields became `epub_queued` and `reader`, also before any release.

## 18. Build notes, phase 4 (PDF edition and reMarkable)

Built without a reMarkable: the USB web interface is tested against a stand-in HTTP server, and the PDF is read back through PDFium. Spike 4 (§11) is still owed.

- **The PDF edition** follows its own spec (`2026-10-23-pdf-edition-design.md`). In short:
  - it's built on `pdf-writer`, laid out by Deckle from the same XHTML the EPUB carries;
  - it uses the paper's fonts, embedded whole, with a ToUnicode map;
  - it has an outline and links from the front page;
  - the crossword is drawn as a real grid.
  
  The build writes one PDF per screen and photos asked for, under `pdf/<screen>-<photos>/` beside the paper (`BuildReport.pdfs`). An export folder or the reMarkable takes the one for its screen.
- **`[remarkable]`** has `enabled`, `screen`, `photos` (the screen's own choice when unset: color on a Paper Pro) and `books`. There is no `format` key, per the §13 ruling: the paper is always the PDF, and books are always EPUB.
- **The tablet** is found by asking `GET /documents/` at `10.11.99.1`, with a one-second timeout and never through a proxy. `DECKLE_REMARKABLE_URL` stands in for the address. `digest watch` only asks while something waits, so an absent tablet costs at most a second a tick.
- **Its outbox** is `outbox-remarkable/`, holding the paper's PDF and, with `books`, the long reads' EPUBs.
  - A push lists the tablet first. A file whose name (without its extension) the tablet already shows is taken as delivered and not uploaded again; the rest go up one at a time.
  - A failed upload stays queued.
  - There's no prune, because the interface can't delete.
- **Without Previewer**, a reMarkable counts as somewhere the paper can go, like an export folder or a Kobo. Long reads print inside the paper unless `books = true`.
- **App:** a **reMarkable** card on the Daily tab (on/off, model, long reads), and **As** and **Screen** on each export folder.
- **Kept for later:** reMarkable's cloud (§13), Inbox books to a reMarkable, and subsetting the embedded fonts (about 400 KB a PDF).
