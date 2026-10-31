# calibre bridge: Deckle as calibre's wireless server, reading data back into calibre, and shelves from calibre — design

**Status:** proposed, 2026-10-31 · **Date:** 2026-10-31 · **Owner:** Jonathan (CasaVargas LLC)
**Builds on:** `2026-10-23-books-and-kindle-design.md` (calibre read for originals),
`2026-10-23-other-readers-design.md` (Kobo), `2026-10-30-koreader-reads-design.md`,
`2026-10-30-koreader-hub-design.md` (reader server, progress sync),
`2026-10-31-koreader-plugins-design.md` (the Deckle plugin and its pending file)

Deckle reads a calibre library today, read-only, to find originals and show them on the Books tab.
Nothing goes the other way. This makes Deckle the bridge between a calibre library and the
e-readers, in three parts:

- **A. A wireless server.** It speaks calibre's "smart device" protocol, so KOReader's built-in
  *calibre wireless connection* finds Deckle on the home network. Books then go to the reader over
  Wi-Fi, from Deckle's library or calibre's, with no cable and no plugin to install.
- **B. Reading data back into calibre.** What Deckle already reads from the readers (progress,
  finished, last read, rating, highlights) is written into the owner's calibre library through
  `calibredb`, never by writing its database directly. It is off until the owner turns it on.
- **C. Shelves from calibre.** Rules like "tag Sci-Fi → shelf Sci-Fi" keep shelves on a Kobo and
  collections in KOReader matching calibre: over USB, and over part A's wireless connection.

**Changes to earlier rulings.**
- *Books and Kindle §4* opened calibre read-only and never wrote to it. Part B writes to calibre,
  but only through calibre's own `calibredb`, and only while the owner has it turned on.
- *Other readers §8* never changes a Kobo's `KoboReader.sqlite`. Part C writes to two shelf tables
  in it, with the safeguards in §6.3. The owner asked for both on 2026-10-31.

## 1. Facts this rests on (checked 2026-10-31)

**calibre's wireless protocol** (`src/calibre/devices/smart_device_app/driver.py`, calibre
master, and `plugins/calibre.koplugin/wireless.lua`, KOReader master):

- **Discovery.** The *device* sends any UDP datagram ("hello") to broadcast ports
  `54982, 48123, 39001, 44044, 59678`, in that order, from local port 8134. The server listens on
  those ports and answers with
  `calibre wireless device client (on <hostname>);<content server port>,<socket port>`. KOReader
  takes the last number as the TCP port to connect to; the content server port may be empty.
- **Direction.** The *device* connects over TCP, and the *server* drives everything after that.
  It sends a request and the device answers. KOReader only ever replies, plus one `NOOP` when it
  hangs up.
- **Framing.** Every message is `<decimal byte length>` followed immediately by a JSON array
  `[opcode, {args}]`, e.g. `34[0, {"key": 1}]`, with no separator. Book bytes go as raw binary
  outside that framing, right after the request that announces their length.
- **Opcodes:** OK 0, SET_CALIBRE_DEVICE_INFO 1, SET_CALIBRE_DEVICE_NAME 2,
  GET_DEVICE_INFORMATION 3, TOTAL_SPACE 4, FREE_SPACE 5, GET_BOOK_COUNT 6, SEND_BOOKLISTS 7,
  SEND_BOOK 8, GET_INITIALIZATION_INFO 9, BOOK_DONE 11, NOOP 12, DELETE_BOOK 13,
  GET_BOOK_FILE_SEGMENT 14, GET_BOOK_METADATA 15, SEND_BOOK_METADATA 16, DISPLAY_MESSAGE 17,
  CALIBRE_BUSY 18, SET_LIBRARY_INFO 19, ERROR 20, GET_COLLECTIONS 21, UPDATE_COLLECTIONS 22.
- **Handshake.** The server sends `GET_INITIALIZATION_INFO` with:
  - `serverProtocolVersion`, `validExtensions`, `passwordChallenge`;
  - `currentLibraryName`, `currentLibraryUUID`;
  - `calibre_version` (an array, e.g. `[8,0,0]`), `canSupportLpathChanges`,
    `canSupportUpdateBooks`, and the date formats.

  KOReader answers OK with:
  - `appName "KOReader"`, `acceptedExtensions`, `extensionPathLengths`;
  - `canStreamBooks`, `canStreamMetadata`, `canReceiveBookBinary`, `canDeleteMultipleBooks`,
    `canSendOkToSendbook`, `canUseCachedMetadata`, `cacheUsesLpaths`, `canAcceptLibraryInfo`;
  - `maxBookContentPacketLen 4096`, `coverHeight 240`, `deviceKind`, `deviceName`,
    `useUuidFileNames false`, `versionOK true`, and `passwordHash`.

  Then the server sends GET_DEVICE_INFORMATION and the device answers with its
  `device_store_uuid` and `device_name`.
- **Password.** If the server has a password, `passwordChallenge` is an ISO timestamp and the
  device returns `sha1(password + challenge)` as hex. On a mismatch the server sends
  `DISPLAY_MESSAGE {messageKind: 1}`, and KOReader reports "invalid password".
- **Books on the device.**
  - The server sends `GET_BOOK_COUNT {canStream, canScan, willUseCachedMetadata, …}`.
  - KOReader replies `OK {count, willStream, willScan}`, then one `OK {priKey, uuid, lpath,
    last_modified}` per book, from its own `metadata.calibre` in the inbox folder.
  - For books the server wants in full, it sends `NOOP {count: n}`, then `NOOP {priKey: i}` once
    per book, and KOReader answers each with that book's stored metadata.
- **Sending a book.**
  - The server sends `SEND_BOOK {lpath, length, metadata, thisBook, totalBooks,
    willStreamBooks, willStreamBinary, wantsSendOkToSendbook, canSupportLpathChanges}`.
  - KOReader checks the `lpath` (relative, no `..`, an accepted extension, inside its inbox) and
    the free space, then answers OK (or `ERROR {message}` to calibre ≥ 4.18).
  - Then exactly `length` raw bytes follow.
  - KOReader adds `metadata` (it keeps `uuid, lpath, last_modified, size, title, authors,
    author_sort, tags, series, series_index`) to `metadata.calibre` and refreshes the file browser.
- **Other requests:**
  - **Delete:** `DELETE_BOOK {lpaths}`, answered with one OK, then one `OK {uuid}` per book.
  - **Fetch a book:** `GET_BOOK_FILE_SEGMENT {lpath}`, answered with `OK {fileLength}` and the bytes.
  - **Space:** `FREE_SPACE` answers `{free_space_on_device}`.
  - **Collections:** `GET_COLLECTIONS` answers `{collections: {name: [absolute file paths]}}`.
    `UPDATE_COLLECTIONS {add_collections, remove_collections, add: {name: [paths]},
    remove: {name: [paths]}}` changes KOReader's collections and answers OK. Both are recent; an
    older KOReader ignores them and never answers.
  - **Keep-alive and hang-up:** `NOOP {}` keeps the connection alive. `NOOP {ejecting: true}` ends
    it. KOReader drops the connection after 5 minutes with no traffic.
- **The device's settings.** The inbox folder (`inbox_dir`) and an optional fixed server address
  (`calibre_wireless_url`) are in KOReader's settings. With no fixed address, KOReader searches by
  broadcast. A second device connecting while one is connected gets `CALIBRE_BUSY`.

**calibre itself:**
- `metadata.db` is SQLite, and only calibre should write it. `calibredb` refuses to change a
  library while the calibre app or `calibre-server` has it open, saying another calibre program
  is running. With either open, `calibredb --with-library http://host:port/#<library_id>`
  makes the change through the content server (with `--username/--password` if it asks for them).
- `calibredb set_custom <column> <id> <value>`, `calibredb set_metadata --field`,
  `calibredb list --fields … --for-machine` (JSON), `calibredb search <query>` (prints ids), and
  `calibredb custom_columns` are the commands this needs. Adding a custom column
  (`add_custom_column`) changes the library's layout and needs calibre closed.
- A calibre book's `uuid` is in its EPUB's OPF as `<dc:identifier opf:scheme="calibre">`. Deckle
  already reads it into `Source::calibre_id`.
- **Not settled:** whether highlights can be written in calibre's annotation format through
  `calibredb` or the content server. See §5.3.

**KOReader collections:** `<data dir>/settings/collection.lua`, a `LuaSettings` table of
`name → { {file = "<absolute device path>", order = n}, …, settings = {…} }`. KOReader rereads it
when its modification time changes, and writes it only when a collection changes in the app.

**Kobo shelves:** `.kobo/KoboReader.sqlite`, tables `Shelf` (`Name`, `InternalName`, `Id`,
`CreationDate`, `LastModified`, `_IsDeleted`, `_IsVisible`, `_IsSynced`, `Type`) and
`ShelfContent` (`ShelfName`, `ContentId`, `DateModified`, `_IsDeleted`, `_IsSynced`). A sideloaded
book's `ContentId` is `file:///mnt/onboard/<path>`. calibre's own Kobo driver has written these
tables for years. A Kobo signed in to a Kobo account syncs shelves with Kobo's servers. This
schema is the community's description, not Kobo's, so it must be checked on a device (§9).

## 2. Goals

- A KOReader reader gets books from Deckle over Wi-Fi with no cable and no plugin to install. This
  covers books from Deckle's library, from the calibre library, and the morning paper.
- Deckle shows what's on that reader and can take books off it.
- calibre shows how far the owner got in each book, whether it's finished, and when it was last
  read, so calibre stays the library of record.
- One set of rules keeps shelves on every reader in step with calibre's tags, series, authors or
  saved searches.
- calibre and Deckle can both be in use without fighting over the library or the network.

## 3. Non-goals

- **Replacing calibre.** Deckle doesn't edit calibre's metadata, convert inside calibre, or manage
  its library layout. Part B writes reading data only, into columns the owner chose.
- **Kindle collections.** They're stored differently by firmware and are risky to write. They
  stay out until a separate design.
- **Two-way shelf sync.** Shelves follow calibre. A shelf the owner makes on the reader by hand
  is left alone and never pulled back.
- **Calibre Companion and other smart-device apps.** Only KOReader is targeted and tested, though
  anything speaking the protocol may work.
- **Running beside calibre's own wireless server on the same ports.** See §4.5.
- **Anything over the internet.** Part A is for the home network only, like the reader server.

## 4. Part A — the wireless server

### 4.1 Where it runs

The watcher runs it, like the reader server (`deckle-readerserver`), when `[wireless]` is on in
Deckle's config. The desktop app's watcher and the Linux/Docker server both run it. It's a plain
TCP listener plus a UDP responder, not HTTP, so it is its own module, `deckle-readerserver::wireless`
(the builder may make it its own crate). It needs nothing from axum.

```toml
[wireless]
enabled = false
port = 9090             # TCP; 0 = pick a free one
password = ""           # optional; same rules as calibre's
discovery = true        # answer UDP broadcasts
library_name = "Deckle"
```

- **Discovery** binds every broadcast port it can. If none bind (calibre's own server holds
  them), discovery is off for that run, a warning says why, and the address to type on the
  reader is shown instead (§4.5).
- **Docker:** UDP broadcast doesn't reach a container on Docker's default bridge network. The
  Docker guide says to use `network_mode: host`, or to type the address on the reader.

### 4.2 The session

One reader at a time. Another reader gets `CALIBRE_BUSY` with the connected reader's name,
exactly as calibre does it. A session:

1. Handshake (§1): checks the password, and records `appName`, `deviceKind`, `deviceName`,
   `acceptedExtensions`, `maxBookContentPacketLen` and `device_store_uuid`. A reply missing
   `canStreamBooks`, `canStreamMetadata`, `canReceiveBookBinary` or `canDeleteMultipleBooks` ends
   the session with a message, as calibre does.
2. `GET_DEVICE_INFORMATION`, then `SET_CALIBRE_DEVICE_INFO` with a drive-info record. The record
   has `location_code main`, `device_store_uuid`, `device_name`, `calibre_version`,
   `last_library_uuid` (Deckle's library id, a UUID kept in its config), `date_last_connected`
   and `prefix ""`. It is the same shape calibre writes, so a reader can go back and forth.
3. `SET_LIBRARY_INFO` (when `canAcceptLibraryInfo`) with Deckle's library name and id.
4. `FREE_SPACE`, then `GET_BOOK_COUNT`: the list of books on the reader, kept as **the reader's
   shelf** for the app (§4.4). Full metadata (`NOOP {count}` / `{priKey}`) is asked only for
   books Deckle has no record of.
5. The owner's queue runs (§4.3).
6. With nothing to do, a `NOOP {}` keep-alive every 2 minutes (KOReader's limit is 5). The
   session ends when the owner asks (`NOOP {ejecting: true}`), the reader hangs up, or nothing
   happens for 15 minutes.

A device that sends something malformed (bad length prefix, JSON that won't parse, an unknown
reply) ends the session. Every read has a timeout, and a single message over 16 MiB of JSON is
refused.

### 4.3 What gets sent

The **wireless queue** is per reader, keyed by `device_store_uuid` and named by `deviceName`. The
owner fills it from the app or the CLI, and three kinds of entry go in it:

- **A book in Deckle's library.** It is built through the normal pipeline for a KOReader reader
  (EPUB, or KEPUB when the reader is a Kobo running KOReader).
- **A book in the calibre library.** Its EPUB is taken read-only and run through the same
  pipeline (typography, covers), keeping the calibre `uuid` as the book's identity.
- **The morning paper.** The main paper is sent when a reader connects and the newest edition
  isn't on it; this follows the reader server's main-paper rule and can be turned off.

Each send is a `SEND_BOOK`:

- **`lpath`:** `<author>/<title> - <author>.<ext>`, made safe and within `extensionPathLengths`.
  Papers go in `Deckle/<stem>.epub`.
- **`metadata`:**
  - from calibre: `uuid`, `title`, `authors`, `author_sort`, `tags`, `series`, `series_index`;
  - for a Deckle book without one, a `uuid` minted and kept in the library's sidecar so it stays
    the same;
  - `last_modified` (ISO), `size`, `lpath`, and a cover thumbnail at `coverHeight`
    (`thumbnail: [w, h, base64 jpeg]`).
- **Bytes:** sent in chunks of `maxBookContentPacketLen`.
- **After sending:** with `canSendOkToSendbook`, Deckle waits for the OK, and on `ERROR` reports
  the message and skips the book. It takes the `lpath` from the reply when the reader changed it.

**Removing:** `DELETE_BOOK` for books the owner takes off the reader in the app. **Fetching back**
(`GET_BOOK_FILE_SEGMENT`) isn't offered in this design.

A KOReader sidecar is never sent, so the reader's own `.sdr` progress stays as it is.

### 4.4 App and CLI

- **App, Settings → Readers → Wireless (calibre protocol):**
  - an on switch, the port, the password, and the address to type on the reader;
  - one line on setting it up: KOReader → Tools → calibre → Connect, after choosing an inbox
    folder there.
  - It is also listed under Online & privacy as a home-network listener, like the reader server.
- **App, the reader's view:** a connected reader shows on the Kindle/readers tab with the books
  it reports, free space, a "Send…" picker over Deckle's and calibre's books, and Remove.
  Disconnected readers show their last-known shelf, greyed.
- **CLI:**
  - `deckle wireless status`: whether it's listening, the address, and the connected reader;
  - `deckle wireless queue <reader> <book>…`;
  - `deckle wireless serve`: runs in the foreground, for testing.

### 4.5 Living with calibre's own wireless server

calibre's server uses TCP 9090 by default and the same five UDP ports. When the calibre app has
its wireless server on, the owner picks which one the reader talks to, and the app says so in
plain words. If port 9090 is taken, Deckle offers the next free port, and the reader then needs
the address typed in (Tools → calibre → Wireless settings → server address). Deckle never tries
to take over ports calibre holds.

## 5. Part B — reading data into calibre

### 5.1 What gets written

| Column (the owner maps each; all optional) | Value | From |
|---|---|---|
| Progress, int 0–100 or float | percent read | KOReader sidecar `percent_finished`, else Deckle's progress-sync record, else Kobo `___PercentRead` |
| Read, yes/no | finished | sidecar `summary.status == "complete"`, Kobo `ReadStatus == 2` |
| Last read, date | last opened | the sidecar's modified time or `summary.modified`, the sync record's timestamp, Kobo `DateLastRead` |
| Rating, calibre's built-in `rating` or a column | 1–5 stars | sidecar `summary.rating` |
| Highlights, long text (Markdown) | highlights and notes | everything Deckle already reads (sidecars, Kobo `Bookmark`) |

Deckle doesn't create columns by itself. Settings → calibre lists the library's custom columns
(`calibredb custom_columns`) and lets the owner map each row above to one of matching type. When
none fit, **Create the usual columns** adds `#deckle_progress` (int), `#deckle_read` (bool),
`#deckle_last_read` (datetime) and `#deckle_highlights` (comments). That takes
`calibredb add_custom_column`, so it is offered only while calibre is closed, and it says so.

### 5.2 How a book is matched

In order:
1. The calibre `uuid` in the book file on the reader (its OPF).
2. Part A's record of what it sent.
3. The KOReader `metadata.calibre` in the inbox folder.
4. `Source::calibre_id` from the Kindle-upgrade match.
5. As a last resort, title + first author, exactly as `matcher.rs` already compares them. A
   match found only this way is listed for the owner to confirm once, and then remembered.

A book with no match is skipped and listed.

### 5.3 When and how it writes

- **When:** after a reader's reads are taken (USB plug-in, a wireless session, a KOReader backup,
  a progress-sync push). Writes are batched and run at most once a minute. They never run while
  a book is being sent.
- **Only what changed:** Deckle keeps the last value it wrote for each book and column in its
  state, and writes only changes. It never writes over a value someone set in calibre after
  Deckle last wrote it: it reads the column first (`calibredb list --for-machine`), and if the
  value differs from Deckle's last write, it leaves it and lists a conflict.
- **Progress only goes forward**, unless the book was started again (Read went from yes to no on
  the reader).
- **How calibredb is reached:**
  - the configured path, else `calibredb` on `PATH`, else the platform's usual install
    (`/Applications/calibre.app/Contents/MacOS/calibredb`,
    `C:\Program Files\Calibre2\calibredb.exe`);
  - with the library path when calibre is closed, or with `--with-library http://…#id` when the
    owner has given a content server address (and its user and password, kept in the OS
    keychain like the other secrets);
  - when calibre is open and no server is set, the batch waits and the app says "Open calibre's
    content server, or close calibre, to write reading data";
  - each `calibredb` run has a 60-second timeout.
- **Highlights:** they are written to the mapped long-text column as Markdown, grouped by chapter,
  replacing that column's value. calibre's own annotation format (what its viewer shows) is
  attempted only if a spike in phase 4 shows it can be written through a supported route (the content server
  annotations endpoint). If it can't, the column is the result, and the build notes say so.
- **Never** `sqlite3` on `metadata.db`. The read-only reader that exists stays read-only.

### 5.4 App and CLI

- **App:** Settings → calibre gains:
  - Write reading data to calibre (off by default), the column mapping, and the content server
    address;
  - a **Preview** that shows the next batch as a table (book, column, old → new) before turning
    it on;
  - the last batch's results and conflicts.
- **CLI:**
  - `deckle calibre columns` lists the custom columns and the mapping;
  - `deckle calibre writeback --dry-run` shows the next batch, and `--run` runs it;
  - `deckle calibre conflicts`.

## 6. Part C — shelves from calibre

### 6.1 Rules

```toml
[[shelves]]
name = "Sci-Fi"            # the shelf or collection on the reader
from = "tag:Sci-Fi"        # tag:<t> | series:<s> | author:<a> | column:#<name>=<v> | search:<saved search>
readers = ["all"]          # or reader keys: kobo/<serial>, koreader/<device_store_uuid>
```

- **Most rules** are worked out from Deckle's read-only view of `metadata.db`.
- **`search:`** runs `calibredb search 'search:"<name>"'`, which calibre evaluates, so it needs
  `calibredb` (§5.3) and is skipped with a note when it can't run.
- **`series:*`** makes one shelf per series of books on the reader.

A rule only ever covers **books that are on the reader**. Shelves don't send books, so the
wireless queue and USB sends are what put books there.

### 6.2 KOReader collections

- **Over Wi-Fi (part A):** when a session starts, Deckle asks `GET_COLLECTIONS`, works out the
  difference from the rules for the books on the reader, and sends `UPDATE_COLLECTIONS`.
  - **Paths:** KOReader's are absolute device paths, made from the inbox folder in its drive
    info plus the `lpath`.
  - **No answer:** if `GET_COLLECTIONS` isn't answered in 5 seconds, that reader is marked "no
    wireless collections" and the session goes on, because a KOReader too old for it ignores it.
- **Over USB:** Deckle doesn't write `collection.lua` itself, since KOReader may write it from
  memory afterwards. Instead it puts the change in the Deckle plugin's pending file
  (`deckle-pending.lua`, KOReader plugins design §5.4) as `collections = { add = …, remove = … }`.
  The plugin applies it through `ReadCollection` at its next start and deletes it, the same way
  pending plugin switches work. Without the Deckle plugin installed, the app offers to install
  it, or to use Wi-Fi.
- **Deckle's collections only:** collections a rule made are named as the rule says and listed
  in `koreader/deckle-shelves.json` on the reader. Deckle removes books only from collections
  in that list, and removes a collection only when its rule is deleted. The owner's own
  collections are never touched.

### 6.3 Kobo shelves (USB)

**When Deckle writes:**
- only to `Shelf` and `ShelfContent`;
- when the Kobo is mounted, and only when a rule's result changed.

**How Deckle writes:**
- It backs up the database first, to `KoboReader.sqlite.deckle-<timestamp>`, beside the
  original. It keeps the last three.
- It opens the database in place in a single transaction, using `BEGIN IMMEDIATE`, and gives up
  if it's locked.
- It sets `_IsSynced = false` on what it changes.
- It checks the schema first: if the columns in §1 aren't all there, it writes nothing and
  reports why.

**Which shelves:**
- Deckle-made shelves are listed in `.kobo/deckle-shelves.json`. Deckle removes books only from
  those, and deletes a shelf by setting `_IsDeleted = true`, as the firmware does.
- A shelf with the same name the owner made by hand is joined: Deckle adds books to it but never
  removes them.

**Kobo account sync:** the first time Deckle writes shelves, the app says that a Kobo signed in to
a Kobo account may sync shelves to Kobo's servers and back, and that Deckle's shelves follow
calibre, not the cloud. The owner confirms once per Kobo.

**Ejecting:** the app tells the owner to eject the Kobo so it re-reads its database. On reconnect,
Deckle checks that its shelves are still there and lists any that the firmware or cloud undid.

### 6.4 App and CLI

- **App:** Settings → calibre → Shelves edits the rules with a live preview: for each reader, the
  shelves and the count of books each would hold.
- **App, the reader's view:** shows the shelves as they are, with a **Fix shelves** button when
  they're out of step.
- **CLI:**
  - `deckle shelves list | preview [<reader>] | apply [<reader>]`;
  - `apply` runs over USB for mounted readers, and queues the change for wireless ones.

## 7. Safety

- Part A only accepts connections on private networks: RFC 1918, link-local and loopback
  addresses. Anything else is closed on accept. The password is optional, as in calibre, but the
  app recommends it.
- Every length is checked before it allocates. There is no file access from a device's words:
  Deckle sends files and never takes paths from the reader beyond matching its book list.
- `calibredb` is run with arguments, never through a shell. Values are passed as single
  arguments.
- The Kobo database is backed up before every write, and Deckle writes nothing when anything
  about the schema is unexpected.
- Everything here is off until switched on. Nothing goes on the internet.

## 8. Tests the build must add

- **Protocol (part A), against a scripted fake KOReader in Rust:**
  - framing, including several messages in one read and one split across reads;
  - the handshake with and without a password, and a wrong password;
  - `CALIBRE_BUSY` for a second device;
  - the book list with and without cached metadata;
  - `SEND_BOOK` with OK and with ERROR, and an lpath changed in the reply;
  - `DELETE_BOOK`;
  - keep-alive, and eject;
  - malformed input ending the session;
  - non-private peers refused.
- **Discovery:** the reply string, byte for byte, as `find_calibre_server` parses it.
- **A run of KOReader's own `wireless.lua`** under LuaJIT in the `koplugin` CI job, with
  KOReader's widgets stubbed as the plugin harness already does. It points at the real server and
  receives a book end to end. If the stubbing is too heavy, a recorded transcript from a real
  KOReader replayed against the server stands in, and the build notes say so.
- **Write-back (part B):**
  - a fake `calibredb` on `PATH` (a script that records its arguments and plays back canned
    `list`/`custom_columns` JSON);
  - mapping by type;
  - only-changes, progress only forward, and conflicts left alone;
  - the content-server form of the arguments;
  - the "calibre is open" wait.
- **Matching:** each step in §5.2, and the confirm-once path.
- **Shelves (part C):**
  - rule evaluation on a fixture `metadata.db`;
  - the KOReader diff and the pending-file Lua it writes, run through the plugin's apply under
    LuaJIT;
  - Kobo writes on a fixture `KoboReader.sqlite` (backup made, the transaction, schema check
    refusal, joining a hand-made shelf, removing only from Deckle's shelves);
  - `UPDATE_COLLECTIONS` paths.

## 9. Device checks owed (`docs/devices.md`)

- KOReader on a Kobo, a Kindle and a PocketBook:
  - finds Deckle by broadcast and connects with a password;
  - receives a calibre book and a paper, and the books appear in its calibre search;
  - after a wireless session, its collections match the rules, or the reader is marked "no
    wireless collections".
- After a Deckle session, the same reader still connects to the real calibre (drive info shape).
- A Kobo shows Deckle's shelves after eject, and they survive a reboot and a Kobo account sync.
- calibre shows the written columns, with the calibre app closed and through the content server.

## 10. Phases (each its own PR)

1. **The protocol and the server.** Framing, discovery, the handshake, the book list, sending,
   delete, keep-alive and busy; the config, the watcher starting it, `deckle wireless`, and the
   fake-KOReader tests.
2. **Sending from the app.** The wireless queue, sends from Deckle's and calibre's books and the
   paper, the reader's view, Settings → Readers → Wireless, Online & privacy, and the Docker
   guide.
3. **Write-back.** Finding `calibredb`, the column mapping, the batch with preview and conflicts,
   the matching, `deckle calibre`, and Settings → calibre.
4. **Highlights.** The spike on calibre's annotations, then the highlights column (and
   annotations if the spike says yes).
5. **Shelves.** Rules and preview; KOReader over Wi-Fi and through the pending file; Kobo
   shelves; `deckle shelves`; the app.

## Build notes (phase 1)

- **Where it lives:** `deckle-readerserver::wireless`: `frame` (the framing), `session` (one
  reader, driven from Deckle's side), `book` (the path and calibre record for a file), `store`
  (the queue and `readers.json`), and the server in `mod.rs`. It uses plain threads and std
  sockets, not tokio. The CLI's `wireless` module and the watcher's `Supervisor` match the
  reader server's.
- **Queue and state:** the queue is a folder, one JSON file per job, under the readership state
  folder (`wireless/queue/`), so the CLI and the app can add jobs while the server takes them,
  with no lock. A job names its reader by `store_uuid`, name, kind, or `any`.
  `wireless/readers.json` keeps each reader's last-known books, free space, whether it's
  connected, and a 50-line log. The server is its only writer.
- **The password** is in `wireless.json` beside `digest.toml`, not in `[wireless]`, kept as is
  (owner-only), together with the library id Deckle makes once. A `password = ""` key in the
  TOML would have put a secret in a file people share, so it was left out.
- **ASCII JSON:** calibre's `json.dumps` escapes non-ASCII, so its length prefix counts both
  characters and bytes. Deckle does the same; KOReader counts bytes.
- **When a send is refused:** KOReader answers `ERROR` and stays reading JSON, so no bytes may
  follow (calibre stops there too). Deckle logs the refusal, drops the job and sends the next.
- **Hanging up:** a `NOOP` where an answer was due means the reader hung up, which is not an
  error.
- **Discovery** answers with the reader server's port as the content-server number, else `0`:
  KOReader's pattern needs digits there.
- **`calibre_version` is sent as 8.0.0**, so KOReader sends `ERROR` replies (it needs at least
  4.18).
- **`BookInfo` gains `calibre_id`,** read from `<dc:identifier opf:scheme="calibre">`.
- **Tests:**
  - **The stand-in:** `tests/wireless.rs` runs the server against a Rust stand-in for
    `wireless.lua`. It covers sending, refusal, removal, busy, a wrong password, junk input,
    asking for details only for new books, and idle eject.
  - **KOReader's own client:** `tests/koreader_wireless.rs` runs KOReader's own `wireless.lua`,
    `metadata.lua` and `extensions.lua` (koreader@7e6c9a4, fetched and checksummed by
    `tools/fetch-koreader-calibre.sh`) on LuaJIT through `tests/koreader/client.lua`. That
    harness stands in KOReader's other modules and relays the socket over stdin and stdout. The
    test checks the received book, `.metadata.calibre`, reconnecting, removal and the password.
  - **CI:** on Linux it fetches the client and sets `DECKLE_REQUIRE_KOREADER`, so the test can't
    skip there.
- **Live run** (2026-10-31): `deckle wireless serve` from a throwaway home was found by a UDP
  broadcast to 255.255.255.255:54982 from port 8134, as KOReader sends it. KOReader's client
  then received a queued book, and a removal queued during the session ran within a poll.
- **Left for phase 2:** sending from Deckle's and calibre's libraries, the paper on connect,
  cover thumbnails, the app, and the Docker guide.

## Build notes (phase 3)

- **Where the data comes from:** `deckle_core::reading::Record`.
  - KOReader's sidecar read gains `records` (with `summary.rating`, new in `Parsed`) for every
    sidecar that gives a percent, `complete` or stars.
  - The Kobo read gains `records` for sideloaded books with `ReadStatus` 1 or 2.
  - Each record carries the calibre id read from the book file on the reader
    (`BookInfo::calibre_id`), and `merge` joins them.
  - `FlushReport::reading` brings them to the CLI, which adds them to `reading-log.json` beside
    `library.json`. KOReader's progress sync adds records by title (it knows no file).
  - The wireless connection's readers (part A) add nothing yet: KOReader doesn't report reading
    state over it.
- **Matching (§5.2), as built:** first the calibre id in the book, then a confirmed match, then a
  unique title + first author match, asked about once (`confirmed` in `calibre-writeback.json`).
  The wireless record and `Source::calibre_id` steps aren't needed: a book calibre or Deckle put
  on a reader already carries the id.
- **Reading calibre:** columns, books and current values come from `metadata.db` opened
  read-only, not from `calibredb list`/`custom_columns`. It's faster, works the same with the
  content server, and gives each column's type. Writes still go only through `calibredb`.
- **calibre's value formats** (checked in `src/calibre/db/write.py`):
  - `set_custom` hands its string to calibre's adapters;
  - bool is `true`/`false`, datetime is ISO, int and float are plain;
  - a rating column is half-stars, 0–10, so 4 stars is `8`.
- **Ratings go to a custom column only,** not calibre's built-in rating: writing the owner's own
  ratings is a step further than the design needs.
- **Conflicts:** a value Deckle didn't write (or that changed since it wrote) is left alone and
  listed, including a value that was there before Deckle ever wrote.
- **The watcher** writes once a minute at most. A batch stops at the first "Another calibre
  program is running" and says to use the content server or close calibre.
- **The app:** Settings → Library → Reading data into calibre, with the column choices (columns
  another field uses aren't offered), the content server, the next batch with Yes for matches,
  conflicts, and Write now. The commands are `calibre_writeback_*`, `calibre_create_columns`
  and `calibre_set_password`.
- **Tests:**
  - plan and run in `deckle-library` (newest record wins, conflicts, forward-only progress and
    restart, confirm-once, `set_custom` arguments, content-server arguments, calibre-open stop,
    creating columns);
  - `calibre.rs` reads values from a fixture library;
  - the frontend helpers;
  - **real calibre:** `tests/it/real_calibre.rs` runs real `calibredb` (a CI job installs
    calibre on Ubuntu). It makes a library, adds the usual columns, writes a batch, reads it back
    from the database and `calibredb list`, and checks an owner's later edit becomes a conflict.
- **Live run** (2026-10-31): in the web app, from a throwaway home with a fixture library and a
  stand-in `calibredb`.
  - The preview showed 8 values and Emma's own "read" left alone.
  - Yes confirmed a title match.
  - **Write now** wrote all 8, the database read back the same, and the next batch was empty.

## Build notes (phase 4)

- **The spike (§5.3), answered:** yes, but not in a released calibre yet.
  - calibre added `calibredb annotations` (`list|search|export|import|remove`) to its main
    branch on 2026-10-28 (`src/calibre/db/cli/cmd_annotations.py`).
  - The latest release, 9.17.0 of 2026-10-23, doesn't have it, so "9.18 or later" in Deckle's
    words is a guess at the next release.
  - `import BOOK_ID FORMAT FILE` takes a `calibre_annotation_collection`, one book per file, and
    merges it as the viewer does: by highlight `uuid`, the newer timestamp wins, nothing is
    duplicated.
  - Required: `type`, `uuid`, `timestamp`, `book_id`, `format`. Optional: `highlighted_text`,
    `notes`, `spine_index`, `toc_family_titles`, `start_cfi`/`end_cfi`, `style`.
- **Highlights as data:** `reading::Record::highlights`, each with text, note, chapter, page,
  time and spine file.
  - KOReader's come from `annotations` (`chapter`, and `pos0`'s `DocFragment[N]` → spine
    `N - 1`).
  - A Kobo's come from `Bookmark`, with the chapter from its `content` row when there is one.
- **The column:**
  - `Field::Highlights` goes to a `comments` column as Markdown: `### chapter`, each highlight a
    quote, its note under it, then page and date in italics.
  - "Create the usual columns" adds `#deckle_highlights` with
    `--display '{"interpret_as": "markdown"}'`.
  - The column is replaced whole when the highlights change, and left alone if the owner edited
    it.
- **calibre's annotations:**
  - `Writeback::annotations` turns them on (off by default).
  - A book's highlights become one collection for its EPUB (else KEPUB, AZW3, else its first
    format). Each highlight gets a `uuid` from the book, text, note and time, so re-imports
    merge. Timestamps are made calibre's UTC form; KOReader's carry no zone and are taken as
    they are.
  - `spine_index` and the chapter go in. There's no `start_cfi`, since KOReader's xpointers
    aren't CFIs, so calibre lists them in Browse annotations and its viewer can't place them.
  - A digest of each book's collection is kept in `written` under `@annotations`, so the same
    set isn't imported twice.
  - `supports_annotations` asks `calibredb annotations --help` once per calibredb, and a batch
    leaves annotations out when it's no.
- **Tests:**
  - the Markdown layout;
  - the column and the collection checked against `annotations_to_import`'s rules (one book,
    known fields only, `local:viewer`, a stable uuid);
  - the run's `annotations import` arguments and the file it reads;
  - once only, and off;
  - support detection cached per calibredb;
  - KOReader's chapter and spine;
  - a Kobo highlight with its chapter.
- **Real calibre in CI:** the job now uses calibre's official Linux installer (the current
  release) instead of Ubuntu's old package. The test checks the fifth column is created as
  comments and the Markdown lands. It imports and lists annotations when that `calibredb` can,
  and says it waited when it can't (9.17).
- **Live run** (2026-10-31): the web app with a fixture library.
  - Mapping a comments column showed "Dune: Highlights empty → 2 highlights".
  - Write now put the Markdown in.
  - The annotations switch stayed off with its "needs calibre 9.18" note, since the stand-in
    `calibredb` has no annotations command.
