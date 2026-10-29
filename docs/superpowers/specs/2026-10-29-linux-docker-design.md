# Deckle on Linux: a Docker container with a web UI

Date: 2026-10-29. Status: approved by Jonathan on 2026-10-29. Jonathan decided Docker for Linux and Unraid,
both phases, catch-up of missed editions before noon, and modest migration support; choices
marked "author's call" were approved with the full design.
Components: CI, `deckle-cli`, a new server binary, the desktop frontend, packaging.

## Intent

People are asking for a headless Deckle: a box that is always on (a Linux server, an Unraid NAS)
builds the morning paper and gets it to their e-reader overnight, with no Mac or PC left awake.
Deckle has no Linux build today; the README says "Linux isn't supported" because Kindle Previewer
only runs on Mac and Windows.

Previewer is only needed for store-quality Kindle *books* (Enhanced Typesetting, KFX, X-Ray and
Word Wise over USB). The Daily doesn't need it: it already builds without Previewer and reaches a
Kindle through Send-to-Kindle email, and Kobo, PocketBook, reMarkable and export folders need
nothing from Amazon. So the headless product is the Daily, honestly labelled, not full parity.

Jonathan's decisions: ship as **Docker containers for Linux and Unraid**, and build **both phases**
below. Feature parity is welcome later but not required.

## Goals

**Phase 1: a headless container, configured by file.**
- Deckle builds, tests and releases on Linux (x86_64 and aarch64).
- A container image that prints every paper on its schedule and keeps running, with no cron and
  no desktop session inside it.
- Configured by the existing `digest.toml` / `papers/*.toml`, with passwords from environment
  variables (already supported: `DECKLE_SMTP_PASSWORD`, `DECKLE_SITE_PASSWORD_<SITE>`).
- An Unraid Community Applications template.

**Phase 2: the same container gains a web UI.**
- The existing React app, served by the container and used from a browser. Same screens, same
  words, one codebase: the desktop app and the web UI must not drift into two products.
- Everything the Daily tab, the Morgue, readership, papers, sources, interests, Your Reading, the
  Cookbook and Settings do on the desktop works in the browser, except what needs hardware or
  Previewer the container doesn't have (below).

## Non-goals

- Kindle Previewer on Linux (Wine, or handing books to a Mac/PC). Possible Phase 3, separately
  designed. Until then, Kindle book conversion in the container is unavailable and says why.
- A Linux desktop app (Tauri AppImage/.deb). Not ruled out, not part of this.
- Multi-user accounts, roles, or one container serving several households' logins. Papers for the
  whole house already exist inside one install; that's the model.
- Any hosted Deckle service. The container is something the user runs.

## Anti-goals (a failure even if it works)

- Deckle starts phoning home, or the container needs inbound access from the internet to work.
- The web UI is open to anyone on the LAN with no password by default.
- A forked frontend that has to be kept in step with the desktop by hand.
- Subscription or paid-tier anything. The container is free and GPL like the rest.
- The container quietly does less than the user thinks (e.g. claims a Kindle copy went out when it
  didn't). Missing capability is shown, not hidden behind success.

## Constraints

- Rust workspace + Tauri + React, as now. Match existing patterns before adding new ones.
- GPL-3.0. No telemetry; every new network call is listed in the README's "What goes online".
- Plain files stay the source of truth: `digest.toml`, `reading.jsonl`, the Morgue folders. The
  web UI edits the same files the CLI does, through the same code paths and config-write lock
  (`press::config_write`).
- Config paths already resolve via `XDG_CONFIG_HOME` on Linux; the container sets the XDG
  variables rather than adding a Deckle-specific path scheme.

## Phase 1 design

**Linux in CI.** Add `ubuntu-latest` to the `rust` job's matrix and fix whatever breaks.
Whether the workspace compiles on Linux today is unverified; that's the first task. Release
workflow gains Linux binaries for `deckle` (x86_64, aarch64).

**A long-running mode.** A new CLI command (name is the builder's call; `deckle digest daemon`
fits the existing verbs) that runs in the foreground forever and does what launchd/Task Scheduler
plus `digest watch` do on the desktop: each paper's edition at its `schedule` time and days, in
the container's `TZ`, plus the watcher's work (Inbox, outbox push, press requests, bookmarklet).
It reuses the existing schedule and watch code, not a second implementation. It logs to stdout,
and handles SIGTERM cleanly (finish or abandon the current edition safely, never leave a
half-written Morgue entry).

*What counts as scheduled* (author's call): in the daemon, a paper is scheduled exactly when its
config has `[schedule] at`; days come from its config as now. There are no installed jobs to
disagree with. `digest schedule --off` run against a daemon install clears `at` (for the main
paper too); on the desktop its behaviour is unchanged. "Stop the presses" (`digest stop`) is
honoured as on the desktop.

*Missed editions* (Jonathan, 2026-10-29): container restarts happen overnight (Unraid
auto-update, host reboots, `docker compose pull`), so the daemon catches up. It records each
paper's last printed edition on disk under `/config`. On start, and after any gap, if a paper's
scheduled time today has passed, it's before 12:00 local time, today is one of its days, and
today's edition doesn't already exist, it prints it once. Otherwise it logs, by paper, that
today's edition was skipped and why. It never sends a second copy of an edition that exists.

**The image.**
- Multi-stage build; small Debian-slim runtime (needs CA certs and fonts the paper uses; the
  builder confirms which system libraries the crates link against).
- Multi-arch: `linux/amd64`, `linux/arm64`. Published to GHCR (`ghcr.io/casavargas/deckle`),
  tagged by release version and `latest`.
- Runs as non-root. Honours `PUID`/`PGID` (Unraid convention: files on the array owned by
  `nobody:users`, 99:100) and `TZ`.
- Volumes: `/config` (XDG config: `digest.toml`, papers, state, `reading.jsonl`, keys) and `/data`
  (Morgue, library, Inbox, export folders). Defaults in the image point there; the user can still
  point an export at any other mounted path.
- First start with an empty `/config` writes the `deckle digest init` starter config and logs
  what to edit, rather than crashing.
- No keychain in the container: secrets come from environment variables only. The CLI commands
  that would write to a keychain say so and point at the variable.
- A `docker-compose.yml` example in the docs.

**Moving in from the desktop app** (Jonathan, 2026-10-29): supported, modestly. Most people
asking are existing users, so their config will hold Mac/Windows paths (export `dir`s, the Inbox,
library and Morgue locations) and their passwords will be in a keychain. On start, the daemon
checks every configured path and lists each one that doesn't exist in the container, with the
config key to change, before any run, rather than failing partway through an edition. A path that's
wrong only stops the part that uses it; the rest of the paper still ships, as with a missing
export folder today. No automatic path rewriting. `docs/docker.md` gets a "Moving from the
desktop app" section: copy the config folder into `/config`, re-point paths at `/data/…`, move
passwords into environment variables. Identity that must not be duplicated (the press-request
producer identity, the bookmarklet token) is called out there: two live installs must not share
one, so the desktop's press requests are disabled before the container takes over.

**Bookmarklet.** The listener binds `127.0.0.1`. In the container it must listen on all
interfaces (configurable), and the bookmarklet must point at the server's address. Browsers block
an `http://` call from an `https://` page in some cases; the builder confirms what works and the
docs say plainly what the user needs (e.g. a reverse proxy with TLS). Saving by dropping links in
the mounted Inbox works regardless.

**Unraid.** A Community Applications XML template (repo, ports, the two paths, `PUID`/`PGID`/`TZ`,
the password variables masked). Submitting it to the CA feed is a separate step Jonathan does.

**Docs.** README: replace "Linux isn't supported" with what the container does and doesn't do.
A `docs/docker.md` with compose, Unraid, and the email-to-Kindle setup, which is the Kindle path
in the container.

## Phase 2 design

**One frontend, two transports.** `apps/desktop/src/api.ts` is the frontend's only door to the
backend (about 99 commands plus 4 event streams). It gets a transport switch: Tauri `invoke`/`listen`
in the desktop build, HTTP + Server-Sent Events in the web build. Components don't change except
where they call desktop-only plugins directly (file dialog in 13 files, opener, process, updater),
which move behind `api.ts` too.

**Shared command layer.** The command bodies live in `apps/desktop/src-tauri/src/*.rs` (~8.7k
lines) as `#[tauri::command]` functions. Move them into a new workspace crate that doesn't depend
on Tauri; the desktop keeps thin `#[tauri::command]` wrappers, the server exposes the same
functions over HTTP with the same names and JSON shapes. Event emission goes through a small trait
both sides implement. The desktop app must behave identically after the move; this refactor lands
on its own before any server code.

**The server.** A new binary in the same image (axum or similar; builder's call) that serves the
built frontend and the API on one port (default `8080`, configurable), and runs the Phase 1
daemon loop in the same process so there's one thing to start. Phase 1's file-only mode stays
available (UI off) for people who don't want a web port.

**Login.** One password, from `DECKLE_WEB_PASSWORD`. With no password set, the UI refuses to
start and the log says why. Unless `DECKLE_WEB_AUTH=none` is set explicitly, for people behind
an authenticating reverse proxy. Session cookie, HttpOnly, SameSite=Strict; CSRF-safe for state-changing
calls. No user accounts.

**What replaces desktop-only pieces.**
- *File and folder pickers*: a server-side picker over the container's mounted paths
  (`/config`, `/data`, and any extra mounts), never outside them; plus browser upload where the user
  is choosing a file from their own computer (a book, a cover image, a recipe).
- *"Open in Finder"/open URL*: URLs open in a new browser tab; reveal-in-folder actions are hidden.
- *Updater, relaunch*: hidden; the image tag is the update path.
- *Keychain password fields*: replaced by a note naming the environment variable.
- *Schedule install* (launchd/schtasks): the daemon owns the schedule, so the UI shows the
  schedule and edits the time/days in config. No install step.

**Capability-gated features.** The server reports what this install can do (Previewer present,
USB devices visible, keychain present). Features that need something missing stay visible with a
one-line reason ("Kindle book conversion needs Kindle Previewer, which doesn't run on Linux"),
rather than disappearing or failing at send time. In the container that covers the Books tab's
send-to-Kindle conversion, the Kindle tab, From Your Shelf, readership read from a Kindle, Kindle
backup/restore and upgrades. If a user passes a USB device into the container, device features
that don't need Previewer (Kobo, PocketBook, reMarkable, Kindle `.sdr` reading for Shelf) may
light up; supporting USB passthrough is best-effort and documented as such.

**Book conversion to non-Kindle targets** (EPUB/KEPUB to Kobo, PocketBook, export folders) does
not need Previewer and stays available.

## Left to the builder

Crate and binary names, HTTP framework, how the event trait is shaped, the exact base image,
the daemon's internal scheduling mechanism (as long as it reuses existing schedule/watch logic),
how the transport switch is selected at build time, test strategy, and how the work is split into
PRs (the command-layer refactor first, on its own).
