# KOReader plugins: manage them from Deckle, a plugin store, and a Deckle plugin that fetches the paper — design

**Status:** proposed, 2026-10-31 · **Date:** 2026-10-31 · **Owner:** Jonathan (CasaVargas LLC)
**Builds on:** `2026-10-30-koreader-hub-design.md` (the reader server, OPDS, sync, backup and
restore, tuned papers), `2026-10-30-koreader-reads-design.md`

Deckle already serves KOReader a catalog and takes its progress sync, but the paper still waits
until someone opens the catalog and taps it. This adds three things. **Plugin management:**
Deckle lists, installs, updates, removes and switches KOReader plugins on a reader plugged in over
USB. **The Deckle plugin:** a small KOReader plugin, installed that way, that downloads the
morning paper from the reader server by itself when the reader wakes or joins Wi-Fi. **A plugin
store:** community plugins found on GitHub, plus a short list Deckle vouches for, browsed in the
app on the computer and installed through plugin management.

**A change to an earlier ruling.** The other-readers design (§3) and the KOReader reads design
(§2) ruled out KOReader plugins. The owner asked for this on 2026-10-31. The rule now: Deckle
ships one plugin of its own, installed only when the owner asks, and changes other plugins only
on the owner's command.

## 1. Facts this rests on (KOReader master, checked 2026-10-31)

- **A plugin is a folder `<name>.koplugin`** holding `main.lua` and `_meta.lua` (`fullname`,
  `description`). `frontend/pluginloader.lua` scans the built-in `plugins/` and every path in
  `extra_plugin_paths`, which defaults to `<data dir>/plugins/`. The data dir is `.adds/koreader`
  on a Kobo, `koreader` on a Kindle, `applications/koreader` on a PocketBook.
- **Switching off** is `plugins_disabled = { [name] = true }` in `settings.reader.lua`, read once
  at startup. `plugins_disable_external` turns off every plugin that isn't built in.
- **User patches** are Lua files in `<data dir>/patches/`. A `.patches_disabled` file there turns
  them all off (`frontend/userpatch.lua`).
- **KOReader writes `settings.reader.lua` when it closes.** A computer can't tell over USB whether
  KOReader is running, so an edit there can be silently undone.
- **No device can wake itself on a timer.** A plugin acts when KOReader is running: on `Resume`
  (wake from sleep), on `NetworkConnected`, when the file browser opens, or from a menu item.
- **`NetworkMgr:runWhenOnline` needs internet access, but `runWhenConnected` only needs a
  connection** (`frontend/ui/network/manager.lua`). Deckle's server is on the LAN, so the plugin
  uses `runWhenConnected`. Both go through `beforeWifiAction`, which follows the owner's
  `wifi_enable_action` (`turn_on`, `prompt` or `ignore`).
- **The News Downloader plugin** is the model for downloading into a folder and refreshing the file
  browser (`FileManager.instance:reinit(dir)`).
- **Progress sync settings** live in `settings/kosync.lua` (`custom_server`, `username`,
  `userkey` = MD5 of the password, `auto_sync`), loaded by the kosync plugin through `LuaSettings`.
- **Licensing:** KOReader is AGPL-3.0, and Deckle's plugin is GPL-3.0, which is compatible.
- **There is no official plugin registry.** Community plugins are GitHub repos, by convention
  tagged with the topic `koreader-plugin` (about 485 repos on 2026-10-31) or `koreader-user-patch`.
  The community plugin **appstore.koplugin** (omer-faruq, GPL-3.0) browses those topics on the
  device and tracks updates by hash. On e-ink it's slow to browse, and it shares GitHub's
  unauthenticated search limit (about 10 requests a minute), so heavy use needs a token.

## 2. Goals

- See every plugin on a reader (built-in and yours, on or off) and install, update, remove or
  switch them from Deckle, with an undo for anything Deckle replaced.
- Back up and restore plugins and patches with the rest of KOReader's state.
- With the Deckle plugin, the morning paper is on the reader when it wakes or joins Wi-Fi, with
  no taps. Old papers it fetched are pruned.
- Set up progress sync to Deckle from the plugin, with one tap.
- Find and install community plugins and patches from the computer, with a short reviewed list
  that installs exactly what was reviewed.

## 3. Non-goals

- No automatic installs or updates of third-party plugins: the store says an update exists and
  the owner installs it.
- No Deckle server: the store's community list is a live GitHub search from the owner's computer,
  and the reviewed list is a file in Deckle's own repository.
- No reviews, ratings or accounts in the store.
- Nothing over MTP or Wi-Fi in part A: plugin management is a USB-drive operation on a Kobo,
  PocketBook or jailbroken Kindle, or on a folder.
- No editing of built-in plugins, only switching them off.
- The plugin doesn't sync reading positions itself: KOReader's own progress sync does.

## 4. Part A — plugin management over USB

`deckle_core::koreader::plugins`, the CLI `deckle koreader plugins …`, and the app.

**Finding KOReader.** The data dir is found as backup finds it (`.adds/koreader`, `koreader`,
`applications/koreader`), or given with `--from <folder>`. Built-in plugins are found in that
folder's `plugins/`; user plugins in its `plugins/` beside `settings.reader.lua`, or the paths
`extra_plugin_paths` names in it.

**`list`.** Each plugin's name (the folder less `.koplugin`), `fullname` and `description` read
from `_meta.lua` with `koreader::lua` (a `_meta.lua` that calls `_()` around strings is read by
unwrapping the call; anything else is shown by folder name), built-in or user, and on or off from
`plugins_disabled` plus `plugins_disable_external`. A pending change (below) is shown as such.
Patches are listed too, with the folder-wide off switch.

**`install <zip|folder|github-url>` / `update <name> [<source>]`.**
- A GitHub URL means the newest release's `.koplugin` zip asset, or else the repository's
  default-branch zip. The URL and the release tag are recorded so `update` can fetch again.
- The archive must hold exactly one `*.koplugin` folder with a `main.lua`, no absolute or `..`
  paths, under 20 MB. Anything else is refused, naming what was wrong.
- It's written to `<user plugins>/<name>.koplugin.tmp/`, then the existing folder (if any) is
  moved to `<data dir>/deckle-backup/plugins/<name>.koplugin.<stamp>/`, then the new one is
  renamed in. `rollback <name>` puts the newest backup back.
- Before writing, Deckle shows the source, the version, and whether it replaces something, and
  asks. `--yes` skips the question for scripts.
- Installing over a built-in plugin's name is refused.

**`remove <name>`.** User plugins only; the folder goes to `deckle-backup/` as above.

**`enable` / `disable <name>`.** Not written to `settings.reader.lua` (§1). Deckle writes
`<data dir>/deckle-pending.lua`, a Lua table of wanted changes, and says the change applies the
next time KOReader starts with the Deckle plugin in it. Without the Deckle plugin, Deckle says the
change can't be applied safely and offers to install it.

**Backup and restore** (hub design part D) gain plugins and patches: user plugin folders and
`patches/` are copied whole, and `restore --settings` puts them back. Built-in plugins are never
copied.

**App.** On the Kindle tab's device view for a Kobo or PocketBook (and Settings → Readers), a
**KOReader** panel: the plugin list with on/off switches (pending changes marked), Install from a
file or URL, Update, Remove, Undo, and the Deckle plugin's own row at the top.

## 5. Part B — the Deckle plugin (`deckle.koplugin`)

Lua, kept in the repo at `koreader/deckle.koplugin/`, embedded in the Deckle binary
(`include_str!`), and installed by part A (`deckle koreader plugins install deckle`). Its version
is Deckle's.

### 5.1 Its settings

Written by Deckle at install to `<data dir>/settings/deckle.lua` (a `LuaSettings` file, never
inside the plugin folder, so updates keep it):

```lua
return {
    ["server"] = "http://192.168.1.228:47220",   -- DECKLE_READER_SERVER_URL or the LAN address
    ["username"] = "deckle",
    ["key"] = "<md5 of the password>",           -- never the password
    ["folder"] = "<home dir>/Deckle",            -- where papers go; the plugin makes it
    ["keep"] = 3,                                -- papers it fetched that it keeps
    ["format"] = "kepub",                        -- kepub on a Kobo, else epub
    ["auto"] = true,                             -- fetch on wake and on Wi-Fi
    ["wifi_on_wake"] = false,                    -- also turn Wi-Fi on when waking, to fetch
}
```

Every key is also settable from the plugin's menu, except `key`, which only Deckle writes.

### 5.2 When it fetches

- `onResume` and `onNetworkConnected`, when `auto` is on, at most once per 10 minutes.
- When the file browser opens on `folder`.
- **Tools → Deckle → Get today's paper**, at any time, which says what happened.
- On wake with `wifi_on_wake`, through `NetworkMgr:runWhenConnected`, which follows the owner's
  `wifi_enable_action`. Off by default: turning Wi-Fi on costs battery.
- Never while a book is open in the foreground. A fetch while reading waits until the reader goes
  back to the file browser or sleeps.

### 5.3 How it fetches

1. `GET <server>/deckle/v1/today` with `x-auth-user` / `x-auth-key` (the kosync header pair) →
   `{ "date", "file", "size", "sha256", "url" }` for the newest edition of the papers the account
   reads, in the plugin's `format`. `204` when there's none.
2. If `folder/file` already exists with that size, stop.
3. Download `url` (same headers) to `folder/.file.part`, check size and SHA-256, rename into
   place. A failure leaves nothing behind and is said once, in the plugin's menu ("Last fetch:
   couldn't reach Deckle at 07:02").
4. A short notification: "Today's paper is here", tappable to open it.
5. Prune: papers the plugin itself downloaded (it keeps their names in its settings) beyond `keep`,
   oldest first, with their `.sdr` folders. Nothing else in the folder is touched.
6. Refresh the file browser if it's showing `folder`.

The download is registered as served (hub design §B), so the paper's progress sync places it.

### 5.4 Pending plugin changes

At startup the plugin reads `deckle-pending.lua`, applies each change to
`G_reader_settings`'s `plugins_disabled`, deletes the file, and says "Restart KOReader to finish
the plugin changes Deckle made" (a change to `plugins_disabled` only takes effect at the next
start). KOReader saves its settings on exit, so the change sticks.

### 5.5 Progress sync, set up

Menu: **Use Deckle for progress sync**. It shows what it will set and asks first, then sets the
kosync plugin's `custom_server`, `username` and `userkey` through the kosync plugin instance
(`self.ui.kosync`) when it's loaded, else in `settings/kosync.lua` before kosync loads it. It
never turns on `auto_sync`; that stays the owner's choice in KOReader.

## 6. Part C — the plugin store

In the app (the KOReader panel's **Store** view) and the CLI (`deckle koreader store search …`,
`… store show <repo>`). Installs go through part A; the store only chooses the source.

### 6.1 Two shelves

- **Recommended.** `koreader/recommended.json` in the Deckle repository, fetched from the main
  branch's raw URL when the store opens (a copy ships in the binary for offline use). Each entry:
  `repo`, `name`, `kind` (`plugin` or `patch`), one-line `why`, `license`, and a pinned `release`
  with its asset name and **SHA-256**. An install downloads that asset and refuses it if the hash
  differs, so it's byte for byte what was reviewed. A newer release appears only when the file is
  bumped. The owner curates it through ordinary PRs; it starts with appstore.koplugin and a few
  well-kept plugins chosen during the build, each with a reason.
- **Discover.** A live GitHub search of the `koreader-plugin` and `koreader-user-patch` topics,
  sorted by stars or last update, filtered by text, and labeled **Community, not reviewed**. Each
  result shows stars, last push, license (or "no license"), the README rendered in the app, and
  the newest release. Installing from Discover takes the newest release's `.koplugin` zip, or the
  default-branch zip, through part A's checks, and says plainly that Deckle hasn't reviewed it.

### 6.2 Updates

The panel shows, for each installed plugin with a recorded source (part A records the URL and the
release), whether a newer release exists, with its notes. Recommended entries update only when the
pin moves. Nothing installs itself; the owner presses Update.

### 6.3 Network and privacy

- The store goes online only while it's open, or when the owner runs a `store` command. A new
  line in **Settings → Online & privacy**: "Plugin store: GitHub search, READMEs and downloads,
  only when you open it."
- Unauthenticated GitHub API by default (60 requests an hour from one address). Results are cached
  for an hour in Deckle's cache folder. A GitHub token is optional, kept in the keychain (or
  `DECKLE_GITHUB_TOKEN` without one), and only needs public read access.
- No telemetry: Deckle doesn't report what was searched or installed.

## 7. Server side

On the reader server (hub design §A):
- `GET /deckle/v1/today`, and the existing `/opds/file/<id>`, also accept the kosync header pair,
  not only HTTP Basic. Both compare against the same key.
- `today` picks the newest edition from the catalog's source. With several papers, the household's
  main paper; a per-account choice is later work.
- `sha256` is computed once per file and cached by path and modification time.

## 8. Safety

- Plugins run code on the owner's reader. Deckle never installs or updates one without the owner
  asking, always names the source, never fetches without being told to, and keeps the previous
  version for an undo.
- Archives are checked (one `.koplugin`, `main.lua` present, no path escapes, size cap) before
  anything is written to the reader.
- The store never hides where a plugin comes from. Discover is labeled unreviewed, and
  Recommended installs only the reviewed bytes.
- The plugin stores the MD5 key, not the password, and only talks to the `server` it was given.
  It makes no other network calls.
- Plain HTTP on the home network or Tailscale, as the reader server already is.

## 9. Tests the build must add

- `list` on fake Kobo, PocketBook and Kindle drives: built-in and user plugins, `_meta.lua` read
  (with `_()` wrappers), on/off from both settings, pending changes shown, patches listed.
- `install` from a zip, a folder and a recorded GitHub release (HTTP injected): good archives
  installed through the temp folder, bad ones refused (two plugins, no `main.lua`, `../` path,
  too big, a built-in's name); replacing keeps a backup and `rollback` restores it.
- `enable` / `disable` write `deckle-pending.lua` and never touch `settings.reader.lua`.
- Backup and restore carry user plugins and patches, and never built-ins.
- The plugin's logic (deciding whether to fetch, the prune, applying pending changes, building the
  request) lives in `koreader/deckle.koplugin/logic.lua` with no KOReader dependencies, and is
  tested with plain LuaJIT in CI.
- Server: `/deckle/v1/today` with kosync headers, 204 when empty, 401 without auth; `opds/file`
  with kosync headers.
- Store, with GitHub injected:
  - a Recommended install with the right hash succeeds and a wrong hash is refused;
  - Discover results parse from recorded API responses, with "no license" and no releases handled;
  - the update check finds a newer release for a recorded source;
  - the cache is used within the hour;
  - nothing goes online while the store is closed.
- `recommended.json` in CI: every entry's pinned asset downloads and matches its hash (a scheduled
  job, so a deleted release shows up).

## 10. Device checks owed (`docs/devices.md`)

On a Kobo and a PocketBook running KOReader:
- the plugin installs and shows in Tools;
- waking with Wi-Fi on fetches today's paper once;
- the prune keeps three;
- a pending disable applies after a restart;
- "Use Deckle for progress sync" works.

On a jailbroken Kindle, the plugin list and an install.

## 11. Phases (each its own PR)

1. **Plugin management, CLI.** `list`, `install`, `update`, `remove`, `rollback`; backup and
   restore of plugins and patches.
2. **Server endpoint and the plugin.** `/deckle/v1/today`, kosync-header auth on downloads,
   `deckle.koplugin` with fetch, prune and menu, and its install with settings.
3. **Pending changes and sync setup.** `enable`/`disable` through `deckle-pending.lua`, and "Use
   Deckle for progress sync".
4. **App.** The KOReader panel, and the Deckle plugin's row with Install, Fetch now and its
   settings.
5. **Store.** `recommended.json` with its first entries and the CI check, Discover, updates, and
   the Store view.

## Build notes (phase 2)

- **Where the plugin lives:** `crates/deckle-core/koplugin/deckle.koplugin/`, not `koreader/` at
  the top, so it's inside the crate that embeds it (`include_str!`) and every build, the
  container's included, has it. `_meta.lua`'s `version` is Deckle's, filled in at install.
- **Loading its own logic:** `dofile(self.path .. "/logic.lua")`. KOReader puts every plugin's
  folder on `package.path` (pluginloader.lua), so `require("logic")` could load another plugin's.
- **What it calls in KOReader:** `socket.http` with `socketutil` timeouts and `file_sink` (as the
  OPDS and News Downloader plugins do), `json`, `ffi/sha2`, `LuaSettings`,
  `NetworkMgr:runWhenConnected`, `Notification:notify`, `FileManager.instance:onRefresh`,
  `ffi/util.purgeDir`.
- **The fetch is checked:** the server's answer must name a bare `.epub` file, a size and a
  64-hex SHA-256, and the download must match both before it's renamed into place (through a
  hidden `.part` file; a FAT drive won't rename over a file, so the old one goes first).
- **Tests:** `logic.lua` by `koplugin/tests/logic_test.lua` on LuaJIT in CI, where every plugin
  file is also compiled. `koplugin/tests/harness.lua` runs the real `main.lua` against a live
  server with KOReader's modules stood in for; it's a manual check, not CI.
- **Server:** `/deckle/v1/today` and `/opds/file` accept HTTP Basic or the kosync header pair.
  Served bytes (a KEPUB made on request) are cached by path, modification time and format, so the
  size and SHA-256 `today` gives are what the download sends.
- **Install:** `deckle koreader plugins install deckle` (and `update deckle`) requires the reader
  server on with a password, and an address the reader can reach (`DECKLE_READER_SERVER_URL`, else
  this computer's LAN address). The format is KEPUB when the device is a Kobo. Settings already in
  `settings/deckle.lua` (keep, auto, the fetched list) are kept; server, username, key and format
  are Deckle's.

## Build notes (phase 3)

- **Pending changes:** `deckle-pending.lua` is `return { ["plugins"] = { [name] = on } }`.
  `set_enabled` refuses without the Deckle plugin on (and not itself waiting to go off), refuses
  to switch the Deckle plugin itself off (remove it instead), and drops a change that matches the
  plugin's current state, removing the file when nothing waits. `list` shows each waiting change.
- **Applying:** the plugin applies the file once per KOReader start, from whichever instance
  initialises first, with `Logic.apply_pending` (counting only real changes), saves
  `G_reader_settings`, deletes the file, and calls `UIManager:askForRestart`. An unreadable file
  is deleted and logged.
- **Patches:** `patches/.patches_disabled` is only read by KOReader (userpatch.lua), so Deckle sets
  it directly: `deckle koreader plugins patches on|off`.
- **Progress sync:** through the running kosync instance (`self.ui.kosync`, as the plugin loader
  registers it) when there is one, marking it `updated` and flushing; else `settings/kosync.lua`
  through `LuaSettings`. `Logic.sync_settings` sets `custom_server`, `username` and `userkey` and
  keeps everything else, `auto_sync` included.
- **Checked with the harness:** the real `main.lua` applied a pending file (Zen on, App Store off,
  two changes, restart asked, file gone), and set progress sync on a stand-in kosync, leaving
  auto-sync off.

## Build notes (phase 4)

- **Where the panel lives:** Settings → **KOReader**, not the Kindle tab: the app has no device
  view for a Kobo or a PocketBook to hang it on. The page finds the plugged-in reader (a Kindle on
  USB storage, else a Kobo or a PocketBook) or works on a folder chosen with **Use a folder…**.
- **Cards:** Reader; the Deckle plugin (Install or Update to this Deckle's version, or why it can't
  be installed yet); Plugins (added first, then built-in, each with its state, a switch, and for
  added ones Update, Remove and Undo last change); Add a plugin (`owner/repo` or a GitHub link, a
  .zip, a folder), which shows what it would install and asks first; Patches.
- **Switches** are off until the Deckle plugin is on the reader, with the reason shown. A waiting
  change shows as "On/Off after a restart" and the switch shows what it will be.
- **Commands:** `koreader_status`, `koreader_preview` (fetches and checks, writes nothing),
  `koreader_install` (fetches again, then writes), `koreader_update`, `koreader_remove`,
  `koreader_rollback`, `koreader_set_enabled`, `koreader_set_patches`, each answering with the new
  page. On the desktop, the ones that may download run off the main thread.
- **A Kobo is told by its layout:** KOReader in `.adds/koreader` means a Kobo, so the Deckle plugin
  gets KEPUB from a copied drive too (`koreader_plugins::is_kobo`), in the app and the CLI.
- **The reader-server check** the plugin install needs moved to `deckle_readerserver::plugin_settings`,
  shared by the CLI and the app.
