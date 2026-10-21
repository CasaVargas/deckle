# Library and device view — design

**Status:** layout, auto-add and source-only settled by owner (2026-10-21); built in the PR for
#35 and #34 · **Date:** 2026-10-21 · **Owner:** Jonathan (CasaVargas LLC)
**Builds on:** `2026-10-18-deckle-design.md` (§2 non-goals, §5 cover rule, §6 devices)
**Amends:** v1 non-goal "Library management" (§2). A plain-folder library is now in scope.
Tagging, reading and metadata editing stay out.

Deckle remembers the books you send, in a folder you can open in Finder. It also shows what's
on each Kindle, so you can remove a book, send it again or upgrade it, without hunting through
`documents/`.

## 1. Intent and why

Today Deckle forgets a book once it's on the Kindle. To send it again (to a second Kindle,
after a Word Wise update, or as EBOK now that the store edition matched) you have to find the
EPUB again. And nothing shows what's actually on the device: which books are Deckle's, which
are PDOC, which have X-Ray, or how full the Kindle is.

Several roadmap issues need these two surfaces: re-process (#39), search (#41), duplicates
(#38), household profiles (#37), backup (#42) and collections (#36).

Success: send *The Dispossessed* once. Later, open the Kindle tab, see it with its cover,
"PDOC" and "X-Ray: no". Click **Upgrade** and it goes back to the Kindle as EBOK, without
the original file at hand.

## 2. Goals, non-goals

**Goals**

1. **A library that is just folders.** One folder per book under a root you choose. Each
   holds the source file exactly as given (EPUB or DOCX) and a `deckle.json` sidecar. There
   is no database, index or lock file, so you can move, sync or delete folders with any tool.
2. **Every send adds the book** (on by default once a library root is set; a setting turns it
   off). The send is recorded in the sidecar: which Kindle, the file name, PDOC/EBOK, the
   ASIN, and whether X-Ray and Word Wise went with it.
3. **What's on each Kindle**, over mass storage and MTP: every book file in `documents/`,
   free and total space, the cover, PDOC/EBOK, the X-Ray and Word Wise sidecars, and whether
   Deckle sent it.
4. **Remove** a book from the Kindle: the file and its `.sdr` folder (and Deckle's thumbnail
   on mass storage), after a confirmation inside the app.
5. **Re-send** and **Upgrade to EBOK** for a book the library has. The book goes onto the
   Books tab for that Kindle (with EBOK chosen, for an upgrade), where the cover rule's badge
   and warnings apply as usual.
6. The CLI does all of it: `deckle library init|add|list|scan` and `deckle kindle books|remove`.

**Non-goals (this increment)**

- Keeping built KFX files. A re-send converts again (owner, 2026-10-21): builds depend on
  the device anyway, and #39 wants rebuilds.
- Tagging, search (#41), duplicates (#38), per-device shelves (#37), Collections (#36).
- Reading every KFX on an MTP Kindle to show its details. MTP has no partial reads in
  Deckle's transport, and books are large. On MTP, details come from the library's send
  records (§5); other books show their file name and size.
- Removing books that aren't in `documents/`, and touching the Kindle's own folders.

## 3. The library

**Where.** `library.json` in Deckle's config folder (`~/Library/Application Support/deckle/`,
`%APPDATA%\deckle\`, `$XDG_CONFIG_HOME/deckle/`; the same folder as `digest.toml`):

```json
{ "version": 1, "root": "/Users/me/Books/Deckle Library", "auto_add": true }
```

With no file, or no `root`, there is no library and nothing changes. `deckle library init
<DIR>` and the app's Settings write the file. The CLI and the app read the same file.

**Layout.**

```
<root>/<Author sort>/<Title>/<source file name>
<root>/<Author sort>/<Title>/deckle.json
```

- Author sort is the first creator's `file-as`, else `deckle_kfx` `author_sort` ("Le Guin,
  Ursula K."). With no author it is `Unknown Author`.
- Names are ASCII-folded and filesystem-safe, as for device file names.
- When `<Title>` already holds a different book (a different `id`), the new one goes in
  `<Title> (2)`, and so on.

**Identity.** A book's `id` is the seed of its generated ASIN (`pipeline::asin_seed`: the
EPUB's first identifier, else `title|author`), so `asin` matches the `B0…DECK` ASIN in every
KFX Deckle built from it. Adding a book whose `id` is already in the library updates that
folder in place: the source is replaced when its SHA-256 differs, the metadata is refreshed,
and the user fields and send records are kept.

**`deckle.json`, schema 1.**

```json
{
  "schema": 1,
  "id": "urn:uuid:3f2a…",
  "source": "The Dispossessed.epub",
  "sha256": "9c1e…",
  "added": "2026-10-21T14:03:11Z",
  "asin": "B0Q7ZKDECK",
  "metadata": {
    "title": "The Dispossessed",
    "authors": ["Ursula K. Le Guin"],
    "author_sort": "Le Guin, Ursula K.",
    "series": { "name": "Hainish Cycle", "index": 5.0 },
    "language": "en",
    "isbn": "9780061054884"
  },
  "sends": [
    {
      "device": "mtp:G0X1…",
      "device_name": "Kindle Scribe",
      "file": "Le Guin, Ursula K. - The Dispossessed.kfx",
      "at": "2026-10-21T14:05:40Z",
      "cde": "PDOC",
      "asin": "B0Q7ZKDECK",
      "xray": false,
      "word_wise": false
    }
  ]
}
```

- `metadata` is a cache of the source: `deckle library scan` rebuilds it, and the source wins
  over it. `id`, `added`, `asin` and `sends` belong to the sidecar.
- `sends` keeps the latest send per `device`. A folder export is recorded as `folder:<path>`.
- Unknown fields are kept on rewrite, so a newer Deckle's sidecar survives an older one.
  A `schema` greater than Deckle knows is listed but never rewritten.
- Written atomically: a temp file in the same folder, then a rename.

**Scan.** `scan` walks the root. A folder with `deckle.json` is a book.

- A folder with exactly one source file (`.epub`, `.docx`) and no sidecar is adopted where
  it is.
- Source files at the root, or several in one sidecar-less folder, are filed into their own
  folders (moved). The root belongs to Deckle.
- A sidecar whose source is missing is reported, not deleted. So is a second folder for the
  same `id`.
- Hidden folders and `.sdr` folders are skipped.
- A sidecar is rewritten only when its source changed.

**Auto-add.** After each successful send, `deckle run` and the app's Send add the source to
the library and record the send. A failure to add is a warning on the book; the send still
succeeded. KFX inputs (sent as they are) aren't added: there's no source to keep.

## 4. What's on a Kindle

`deckle_core::inventory` lists one target over a `Conn` (one MTP session, as for delivery):

```rust
pub struct Inventory { pub books: Vec<DeviceBook>, pub space: Option<Space> }
pub struct Space { pub total: u64, pub free: u64 }
pub struct DeviceBook {
    pub file: String,           // name in documents/ (or documents/<folder>/<name>)
    pub size: u64,
    pub format: String,         // kfx | azw3 | azw | mobi | pdf | epub | txt …
    pub title: Option<String>,
    pub authors: Vec<String>,
    pub cde: Option<String>,    // PDOC | EBOK (KFX only)
    pub asin: Option<String>,
    pub from_deckle: bool,      // ASIN ends in DECK, or a library send record names this file
    pub xray: bool,             // XRAY.entities.*.asc in <stem>.sdr
    pub word_wise: bool,        // LanguageLayer.*.kll in <stem>.sdr
    pub cover: Option<Vec<u8>>, // JPEG
}
```

- **Mass storage:** files in `documents/` and one level of subfolders (not `.sdr`). A KFX is
  read and inspected for title, authors, CDE type, ASIN and cover. Results are cached by
  path, size and modified time in Deckle's cache folder, so reopening the tab doesn't re-read
  every book. Free space comes from the filesystem (`fs4`).
- **MTP:** `documents/` by `object_info`. Free space comes from `GetStorageInfo` (new in
  `deckle-transport`). Title, authors, CDE type, ASIN and X-Ray/Word Wise come from the
  library's send record for that device and file name; the cover comes from the library
  source. Other books show their file name.
- **Library match** (for Re-send and Upgrade): the KFX's ASIN equals a library book's `asin`,
  or a send record for this device names the file.
- **Remove** takes a file name from the inventory. It deletes `documents/<file>` and
  `documents/<stem>.sdr` (over MTP: the folder's children, then the folder). On mass storage
  it also deletes Deckle's `system/thumbnails/thumbnail_<ASIN>_EBOK_portrait.jpg` for a
  `…DECK` ASIN, and its restore copy in `deckle-covers/` (which every send copies back). The library's send record for that device is dropped. A name that isn't in
  `documents/` (or contains `..`) is refused.

## 5. Surfaces

**CLI**

```
deckle library init <DIR> [--no-auto-add]   # write library.json
deckle library add <book>...                 # EPUB or DOCX
deckle library list [--json]                 # title, author, series, last send per Kindle
deckle library scan                          # adopt, refresh metadata, report problems
deckle kindle books [--json]                 # first connected Kindle, like run --to device
deckle kindle remove <file> [--yes]          # asks unless --yes
```

**App**

- Settings → **Library**: the folder (Choose…, Reveal), and "Add every book I send".
- A **Kindle** tab next to Books and Daily. It lists the selected Kindle's books, Deckle's
  first, then by title. Each row shows the cover, title and author (or the file name),
  badges (PDOC/EBOK, X-Ray, Word Wise, "Not from Deckle") and the size. A bar at the top shows
  free space.
- Row actions: **Re-send** and **Upgrade to EBOK** (only for a PDOC book the library has)
  put the library copy on the Books tab with the right choice and switch there. **Remove**
  asks inside the row (no browser dialog) and refreshes the list.

## 6. Verification

- `deckle-library`:
  - add files a fixture EPUB under Author/Title with a sidecar whose `asin` equals the
    pipeline's generated ASIN;
  - re-adding the same book updates in place and keeps `sends`;
  - a different book with the same title goes to `(2)`;
  - scan adopts a bare EPUB and reports a missing source;
  - unknown fields survive a rewrite, and a newer schema isn't rewritten;
  - no other files are created under the root.
- `deckle-transport`: the GetStorageInfo dataset decodes into capacity and free space.
- `deckle-core` inventory:
  - a temp mass-storage root lists KFX (with mode, ASIN, sidecar flags and cover) and other
    formats, and skips `.sdr`;
  - remove deletes the book, its `.sdr` and its thumbnail, and refuses `../x`;
  - the fake MTP Kindle lists, reports space and removes over one session.
- CLI: `library init/add/list` on a temp config and root; `run --to <folder>` with a library
  records the send.
- App: the inventory command builds rows with covers and badges, and the Kindle tab is checked
  by hand in the running app against a fake mass-storage root. `--kindle` opens the app on the
  Kindle tab, like `--daily`.

**Known limit.** A mass-storage Kindle's key is its `devices.json` key, `ms:<model hint>`, and
the hint is the first line of `system/version.txt`, which includes the firmware. After a
firmware update, older send records no longer match that Kindle by file name. Books on mass
storage are still matched by the ASIN read from the KFX, so the view is unaffected; only MTP
relies on the records, and its key is the USB serial.
