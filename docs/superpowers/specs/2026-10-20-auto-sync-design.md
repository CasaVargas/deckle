# Auto-sync on plug-in — design

**Status:** call made on the issue (#43, owner, 2026-10-20: option 1); built in #43's PR ·
**Date:** 2026-10-20 · **Owner:** Jonathan (CasaVargas LLC)
**Builds on:** `2026-10-20-daily-digest-design.md` (§ delivery: outbox, `watch`, pruning; #49
scheduling)

Plug the Kindle in and the things you asked for happen by themselves, per device: the outbox is
pushed and old editions are pruned, over one connection. A device you never configured gets
exactly what `deckle digest watch` did before this change.

## 1. Trigger

`deckle digest watch`, which the #49 LaunchAgent / scheduled task already keeps running from
login. No resident watcher in the desktop app yet; a toggle there is a follow-up.

## 2. Device id

Config is keyed by a stable id for the device, shown by `deckle devices` as `sync id`:

| Transport | Id |
|---|---|
| MTP with a USB serial | the serial, e.g. `G0000000000` |
| MTP without a serial | `<USB product>/mtp`, e.g. `Kindle/mtp` |
| Mass storage | `<model>/<volume name>`, e.g. `Kindle Paperwhite/Kindle` |

The transport exposes no serial for mass-storage Kindles, so they use the model (first line of
`system/version.txt`, else the volume name) plus the volume name.

## 3. Config

In `digest.toml`. `[sync]` sets the defaults; `[sync.devices."<id>"]` overrides any of them
for one device. Unknown keys are an error, like the rest of the file.

```toml
[sync]
push = true        # deliver the outbox when the Kindle connects
prune = true       # keep only the newest `keep` editions on it
clippings = false  # reserved for #60; does nothing yet

[sync.devices."G0000000000"]
prune = false
```

Saving (the desktop app) goes through `config::save`, which keeps comments (`toml_edit`).

## 4. Actions and when they run

The watcher checks every `--interval` seconds. A *connection* runs from the tick a device
appears until the tick it is gone (or another device's id appears).

- **push** (default on): when the outbox is not empty and holds something not yet tried on this
  connection, deliver it. A failed delivery waits for a reconnect or a new outbox item, as
  before.
- **prune** (default on): with push on, runs after each push over the *same* connection; with
  nothing to push nothing is opened (opening an MTP session resets the Kindle to its home
  screen). With push off, runs once per connection.
- **clippings** (default off): reserved. Accepted in the file, ignored until #60 lands; the
  watcher says so once at start when it is on.

Both actions share one `Conn`, so an MTP Kindle sees exactly one session per run.

The explicit commands (`digest push`, `digest run`) always push; they honour the device's
`prune`. `push = false` only stops the watcher.

## 5. Unconfigured device

No `[sync]` table, or a device without an entry, gets push + prune with the old triggers. A
regression test drives the watcher's decisions against the pre-change loop.

## 6. Tests

- config: an empty file gives the defaults; `[sync]` and per-device overrides load; an unknown
  key fails; saving keeps comments.
- device ids for each transport.
- watcher: a connection runs only the enabled actions, once; push off + prune on prunes once per
  connection; a reconnect runs again.
- a fake MTP Kindle: push then prune open one session.
- regression: default actions decide exactly as the old loop did over a scripted run.

## 7. Out of scope

Clippings pull (#60), a desktop-app toggle/background watcher, a CLI to edit `[sync]`.
