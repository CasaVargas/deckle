# KOReader plugins: manage them from Deckle, and a Deckle plugin that fetches the paper — design

**Status:** proposed, 2026-10-31 · **Date:** 2026-10-31 · **Owner:** Jonathan (CasaVargas LLC)
**Builds on:** `2026-10-30-koreader-hub-design.md` (the reader server, OPDS, sync, backup and
restore, tuned papers), `2026-10-30-koreader-reads-design.md`

Deckle already serves KOReader a catalog and takes its progress sync, but the paper still waits
until someone opens the catalog and taps it. This adds two things. **Plugin management:** Deckle
lists, installs, updates, removes and switches KOReader plugins on a reader plugged in over USB.
**The Deckle plugin:** a small KOReader plugin, installed that way, that downloads the morning
paper from the reader server by itself when the reader wakes or joins Wi-Fi.

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

## 2. Goals

- See every plugin on a reader (built-in and yours, on or off) and install, update, remove or
  switch them from Deckle, with an undo for anything Deckle replaced.
- Back up and restore plugins and patches with the rest of KOReader's state.
- With the Deckle plugin, the morning paper is on the reader when it wakes or joins Wi-Fi, with
  no taps. Old papers it fetched are pruned.
- Set up progress sync to Deckle from the plugin, with one tap.

## 3. Non-goals

- No plugin store, no browsing GitHub for plugins, no automatic updates of third-party plugins.
  Updates are something the owner runs.
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

## 6. Server side

On the reader server (hub design §A):
- `GET /deckle/v1/today`, and the existing `/opds/file/<id>`, also accept the kosync header pair,
  not only HTTP Basic. Both compare against the same key.
- `today` picks the newest edition from the catalog's source. With several papers, the household's
  main paper; a per-account choice is later work.
- `sha256` is computed once per file and cached by path and modification time.

## 7. Safety

- Plugins run code on the owner's reader. Deckle never installs or updates one without the owner
  asking, always names the source, never fetches without being told to, and keeps the previous
  version for an undo.
- Archives are checked (one `.koplugin`, `main.lua` present, no path escapes, size cap) before
  anything is written to the reader.
- The plugin stores the MD5 key, not the password, and only talks to the `server` it was given.
  It makes no other network calls.
- Plain HTTP on the home network or Tailscale, as the reader server already is.

## 8. Tests the build must add

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

## 9. Device checks owed (`docs/devices.md`)

On a Kobo and a PocketBook running KOReader:
- the plugin installs and shows in Tools;
- waking with Wi-Fi on fetches today's paper once;
- the prune keeps three;
- a pending disable applies after a restart;
- "Use Deckle for progress sync" works.

On a jailbroken Kindle, the plugin list and an install.

## 10. Phases (each its own PR)

1. **Plugin management, CLI.** `list`, `install`, `update`, `remove`, `rollback`; backup and
   restore of plugins and patches.
2. **Server endpoint and the plugin.** `/deckle/v1/today`, kosync-header auth on downloads,
   `deckle.koplugin` with fetch, prune and menu, and its install with settings.
3. **Pending changes and sync setup.** `enable`/`disable` through `deckle-pending.lua`, and "Use
   Deckle for progress sync".
4. **App.** The KOReader panel, and the Deckle plugin's row with Install, Fetch now and its
   settings.
