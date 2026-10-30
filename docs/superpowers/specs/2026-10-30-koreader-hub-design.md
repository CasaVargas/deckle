# KOReader hub: catalog, progress sync, exact readership, backup and tuned papers — design

**Status:** accepted, 2026-10-30. The owner picked five KOReader features from a list (1, 2, 3, 4
and 7) and handed the design calls to the builder. · **Date:** 2026-10-30 · **Owner:** Jonathan
(CasaVargas LLC)
**Builds on:** `2026-10-30-koreader-reads-design.md` (sidecar reads, PR #242),
`2026-10-23-other-readers-design.md`, `2026-10-22-readership-report-design.md`,
`2026-10-22-morgue-design.md`

KOReader runs on every brand of e-ink reader and speaks two open protocols: OPDS catalogs and its
own progress sync. Deckle can serve both from the computer it already runs on. Every KOReader
device in the house then gets the paper over Wi-Fi and reports what was read, with no cable, no
vendor account and no cloud. This document covers five pieces, each shipped as its own PR.

| Part | What | Depends on |
|---|---|---|
| A | The **reader server** in the watcher, and an **OPDS catalog** (feature 2) | — |
| B | A **KOReader progress sync server** (feature 1) | A |
| C | **Exact readership** of the paper from KOReader positions (feature 3) | #242; B for the sync source |
| D | **Backup and restore** of KOReader's state (feature 4) | #242 |
| E | **Tuned papers**: a ready-made sidecar for each paper Deckle sends (feature 7) | #242 |

**A change to an earlier ruling.** `2026-10-30-koreader-reads-design.md` §2 said Deckle never
writes KOReader's files. The owner's choice of features 4 and 7 relaxes that in exactly two
places. **D**'s restore writes sidecars the owner asked to put back. **E** writes a sidecar only
beside a file Deckle itself wrote, and only when none exists. Nothing else writes to KOReader's
files, and Deckle never overwrites a sidecar KOReader made.

## A. The reader server and the OPDS catalog

### A.1 Where it runs

- A new crate, `deckle-readerserver` (axum, as `deckle-server` uses), started by the watcher
  (`deckle digest watch`, which the app's daemon and `deckle-web serve` both run) on its own
  thread with its own tokio runtime, only when it's turned on.
- **Off by default.** It's on when `[reader_server] enabled = true` and a password is set.
- Binds `0.0.0.0:<port>` (default **47220**; `DECKLE_READER_SERVER_BIND` overrides the address,
  as `DECKLE_BOOKMARKLET_BIND` does). Plain HTTP: it's meant for the home network or Tailscale.
  The docs say so and recommend Tailscale away from home.
- A failure to bind is a line in the watcher's log, never a stopped watcher.

### A.2 Config and the password

```toml
[reader_server]
enabled = false
port = 47220
username = "deckle"
```

- The password isn't in `digest.toml`. KOReader's sync protocol sends `md5(password)` as the
  key, so the server stores that MD5 in `reader-server.json` beside `digest.toml`, created with
  owner-only permissions (0600 on Unix). Setting it: `deckle reader-server password` (prompts, or
  `--stdin`) and the app's field.
- **HTTP Basic** auth on every OPDS route (KOReader's OPDS browser supports a username and a
  password). The given password is MD5'd and compared in constant time with the stored one.
- `deckle reader-server status` prints the URLs to type into KOReader: the OPDS catalog
  (`http://<lan-ip>:47220/opds`) and the sync server (`http://<lan-ip>:47220`).

### A.3 The catalog (OPDS 1.2, Atom)

- `GET /opds`: a navigation feed with **Today's paper**, **Recent papers** and **Books**.
- **Today's paper**: an acquisition feed with the newest edition. **Recent papers**: the newest
  14 editions from the Morgue. Each entry has the title, date, cover thumbnail (the front page,
  `rel="http://opds-spec.org/image"`) and two acquisition links: EPUB, and KEPUB
  (`?format=kepub`, converted on request with `deckle_core::kepub::convert`) for a Kobo.
- **Books**: the EPUBs Deckle prepared for other readers (the EPUB outbox and what export folders
  got), newest first, at most 200. The builder may narrow this to what's simple to list reliably,
  and must say so in the PR.
- `GET /opds/file/<id>` serves a file. Ids are opaque: a SHA-1 of the absolute path, looked up in
  a table built per request. A path never comes from the client, so nothing outside those
  folders can be reached.
- `Content-Type: application/atom+xml;profile=opds-catalog;kind=acquisition` (or
  `kind=navigation`). Downloads are `application/epub+zip` with a `Content-Disposition` file name
  that ends in `.epub` or `.kepub.epub`.

### A.4 App

A **Reader server** card on the Daily tab's Delivery screen (under "Other readers"): on/off, port,
username, password, and when it's on, the two URLs, each with a copy button. On the web build,
the same card. Verified in the running app before the PR.

## B. KOReader progress sync (`/users`, `/syncs`)

KOReader's kosync plugin with a custom server URL (`http://<lan-ip>:47220`). Endpoints, as in
`plugins/kosync.koplugin/api.json`:

- `POST /users/create` → **402**, "registration is closed": the account is the one set in A.2.
- `GET /users/auth` with `x-auth-user` and `x-auth-key` → `200 {"authorized":"OK"}` or 401.
- `PUT /syncs/progress` with `{document, progress, percentage, device, device_id, metadata?}`
  → `200 {"document", "timestamp"}`. `metadata` (`filename`, `title`, `authors`) arrives when the
  owner turns on KOReader's "Send document metadata", and is kept.
- `GET /syncs/progress/<document>` → the stored record, or `{}`.
- Every response is JSON. Requests must accept `application/vnd.koreader.v1+json`, but the server
  doesn't refuse others.

**Storage:** `koreader-sync.json` beside `readership.json`: per document, the newest record
(`progress`, `percentage`, `device`, `device_id`, `timestamp`, `metadata`). Written atomically.

**Knowing which book a document is.** KOReader names a document by the partial MD5 of its file
(the default; `util.partialMD5`: 1 KiB samples at offsets `1024 << 2i` for i = −1…10, where LuaJIT
masks the shift so the first sample is at offset 0) or by `md5(file name)`. When Deckle writes a
paper or a book anywhere (outbox, export folder, reader drive, Morgue), it records both digests in
`koreader-documents.json` (digest → `{file name, edition date if a paper, title}`). The test
vectors for `partial_md5` come from KOReader's algorithm run on fixed byte patterns, written into
the test.

**Where progress goes:**
- A paper edition: to C (`record_koreader_position`).
- A book: to From Your Shelf, as a book in progress (a `kobo::Book` keyed `koreader:<digest>`
  unless the document is known, then by its file name), under a device named after `device`.

## C. Exact readership from KOReader positions

- **An EPUB layout for every edition.** When the paper is built, record its spine:
  `EpubLayout { files: Vec<String>, sections: Vec<(key, name, spine_index)> }`. The paper writes
  `front.xhtml` and then `s<N>.xhtml` per section. This is recorded even with no Kindle (today a
  Layout exists only after a KFX build). It's stored in `readership.json` beside the KFX layout,
  as an optional field, so older files still load.
- **A KOReader position** for EPUB looks like `/body/DocFragment[N]/body/…`. `N` is the 1-based
  index into the spine, so `N − 1` is the spine index of the file being read.
- `record_koreader_position(date, device_id, xpointer, percentage, when)` turns that into a
  `Read`. It works in the KFX coordinates when the edition has a KFX layout (the start of the
  section whose file it is, plus one), else in an EPUB coordinate space (`spine_index * 1_000_000`
  for a section start). The existing report then works unchanged.
- **Sources:** B's sync records for a paper, and #242's sidecar read on USB (each paper sidecar's
  `last_xpointer` / `percent_finished` under the paper's file name).
- Device ids: `koreader/<device_id>` from sync, `koreader/<drive id>` from a drive.

## D. Backup and restore

- `deckle koreader backup [--from device|<folder>] [--to <dir>]` copies, from the drive or folder:
  - every `.sdr` folder found by `koreader::find` (with its path), and
  - KOReader's own data folder when present (`.adds/koreader/` on a Kobo, `koreader/` on a Kindle,
    `applications/koreader/` on a PocketBook): `settings.reader.lua`, `history.lua`,
    `settings/statistics.sqlite3` and `settings/vocabulary_builder.sqlite3`, `docsettings/` and
    `hashdocsettings/`.

  The default target is `~/Deckle/KOReader backups/<device>/<YYYY-MM-DD HHMMSS>/`, with
  `manifest.json` (device, when, files, and each sidecar's `partial_md5_checksum` and book path).
  Plain files, nothing packed.
- `deckle koreader restore <backup> [--to device|<folder>] [--settings] [--force]`:
  - Each sidecar goes beside its book. The book is found by its relative path, else by the
    partial MD5 recorded in the sidecar (`partial_md5_checksum`) against the books on the
    target. That's what moves a reading life from a Kobo to a PocketBook, where the folders
    differ.
  - A target sidecar that's newer (by the modification time recorded in the manifest) is kept
    unless `--force`. Each is reported: restored, kept (newer), or no book found.
  - `--settings` also restores the settings, statistics and vocabulary files into the target's
    KOReader folder. Never by default.
  - Writes go through a hidden temporary file and a rename.
- CLI only in this part. An app button can come later.

## E. Tuned papers

- When Deckle writes a paper to a Kobo's or PocketBook's `Deckle/` folder, or to an export
  folder in EPUB or KEPUB, it also writes `<stem>.sdr/metadata.<ext>.lua`, but only when that
  folder doesn't exist yet.
- The sidecar holds only settings verified against KOReader's source (`crereader` and
  `readerfont`), chosen so the paper reads as designed: the book's embedded fonts and styles on,
  footnote popups on, and hyphenation from the book's language. Each key the builder uses is
  cited in the code with its KOReader source file. Font size and margins are left to the owner's
  defaults.
- It's written as KOReader writes sidecars (`-- … \nreturn { … }`), so `koreader::lua` reads it
  back. A test does that.
- The prune removes `<stem>.sdr` with the edition, and only an `.sdr` whose book Deckle prunes.

## Tests (each part adds its own; the essentials)

- A: catalog feeds parse as XML with the OPDS namespaces; a file id outside the table is 404;
  Basic auth required; KEPUB variant converts.
- B: auth, put then get, 402 on register, a paper's progress reaches C, digests recorded
  (partial MD5 vectors).
- C: an xpointer in `s3.xhtml` marks the sections up to the third as reached; an edition with
  only an EPUB layout reports; an older `readership.json` loads.
- D: backup then restore onto a second fake drive with the book in another folder (matched by
  MD5); a newer target sidecar kept; `--settings` off by default.
- E: a sidecar written once and never overwritten; it reads back; pruned with its edition.

## Device checks owed (`docs/devices.md`)

KOReader on a real device against the server: the OPDS catalog opens with a username and
password, the paper downloads, sync logs in and pushes progress, and the paper's reached sections
match what was read.

## Build notes

### D (backup and restore)

- **`--to` on backup** names the folder that holds backups (default `~/Deckle/KOReader backups`);
  each lands in `<device>/<YYYY-MM-DD HHMMSS>/` under it, local time. `<device>` is `Kobo`,
  `PocketBook` or `Kindle`, or a folder's own name. A backup never lands on a folder that exists.
- **`device`** is found as `deckle highlights` finds it: a Kindle on USB storage, else a Kobo,
  else a PocketBook. A Kindle over MTP is refused for backup and restore alike.
- **What's copied:** each `.sdr` folder whole (with `metadata.<ext>.lua.old` and anything else in
  it), the four settings files, and `docsettings/` and `hashdocsettings/`. Nothing else of
  KOReader's folder (`cache/`, fonts, plugins).
- **The MD5 in the manifest** is the sidecar's `partial_md5_checksum`; a sidecar without one gets
  its book's partial MD5 worked out at backup time, or, in `hashdocsettings/`, the folder's name.
- **Every sidecar goes back beside its book**, including those backed up from `docsettings/` or
  `hashdocsettings/`. A book found by MD5 under another name gets `<its stem>.sdr/metadata.<its
  ext>.lua`. When two sidecars in a backup find the same book, the newer goes back and the other
  is reported as kept.
- **Times:** restored files keep the backup's modification time, so a second restore of the same
  backup is the same, and From Your Shelf's last-read time stays true. "Newer" means more than
  2 seconds past the backup's (FAT keeps times to 2 seconds).
- **`--settings`** obeys the same newer rule and `--force`. A target with no KOReader folder
  reports each settings file as failed and writes none. `docsettings/` and `hashdocsettings/` go
  back only as sidecars beside books, never as folders.
- **Exit status:** non-zero when any file failed to copy; "kept" and "no book found" are not
  failures.
