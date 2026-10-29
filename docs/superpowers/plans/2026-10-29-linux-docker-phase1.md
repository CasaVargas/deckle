# Deckle on Linux, Phase 1: headless container — Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Deckle builds and tests on Linux, and ships a Docker image that prints every paper on schedule with no cron and no desktop, configured by file.

**Architecture:** A new `deckle digest daemon` command is a clock and a supervisor: it re-reads the papers every tick, decides what's due with a pure function in `deckle-digest` (catch-up before noon, never a second copy), and runs the existing `deckle digest run` / `deckle digest watch` as child processes with the same arguments launchd and Task Scheduler use. The image wraps that binary with an entrypoint for `PUID`/`PGID`/`TZ` and the XDG paths.

**Tech Stack:** Rust 2024 workspace (chrono, chrono-tz, serde, clap, `libc` already in deckle-digest), GitHub Actions (public repo: hosted runners are free and allowed), Docker Buildx multi-arch, GHCR.

**Spec:** `docs/superpowers/specs/2026-10-29-linux-docker-design.md` (Phase 1 sections). Phase 2 (web UI) gets its own plan once this lands.

## Global Constraints

- Targets: `linux/amd64` and `linux/arm64`; Linux CLI binaries `x86_64-unknown-linux-gnu`, `aarch64-unknown-linux-gnu`.
- Image: `ghcr.io/casavargas/deckle`, tags = release version and `latest`.
- Volumes `/config` (XDG config) and `/data` (Morgue, library, Inbox, exports). Env: `PUID`, `PGID` (Unraid default 99/100), `TZ`.
- Secrets in the container only from env: `DECKLE_SMTP_PASSWORD`, `DECKLE_SITE_PASSWORD_<SITE>`.
- Catch-up rule, verbatim: if a paper's scheduled time today has passed, it's before 12:00 local time, today is one of its days, and today's edition doesn't already exist, print it once; otherwise log, by paper, that today's edition was skipped and why. Never send a second copy of an edition that exists.
- In the daemon, a paper is scheduled exactly when its config has `[schedule] at`. Desktop `digest schedule` behaviour is unchanged.
- The daemon reuses the existing `run`/`watch` code paths (as child processes); no second implementation of either.
- No new network calls. GPL-3.0 SPDX header on every new source file. Match the repo's comment style (short, design-section references).
- Commit per task on a feature branch; never to `main`.

## Review Focus

1. **The container restarts at the paper's minute** (Unraid auto-update at 05:30, paper at 05:30). Expected: printed once after start, never twice — `daemon_restart_inside_the_minute_prints_once` in Task 3.
2. **The host clock jumps** (suspend/resume, NTP step, DST change). Expected: the due rule is evaluated on wall-clock "now" each tick, so a skipped minute catches up and a repeated hour (DST fall-back) doesn't reprint — `dst_fall_back_does_not_reprint` in Task 3.
3. **A run fails** (network down at 05:30). Expected: not recorded as printed; retried on the next tick only while still before noon, at most once per 15 minutes; the failure is logged — `failed_run_retries_until_noon` in Task 3.
4. **Config edited while the daemon runs** (new `at`, a paper added/removed, `digest stop`). Expected: picked up next tick without restart; a removed paper stops — `config_changes_apply_next_tick` in Task 4.
5. **Email is the only Kindle route and there's no Previewer** (the normal container case). Expected: the paper is built and emailed, not an error — Task 2.

---

### Task 1: Linux in CI

**Files:**
- Modify: `.github/workflows/ci.yml` (the `rust` job matrix and steps)
- Modify: `tools/fetch-pdfium.sh` (add `linux-arm64`)
- Modify: whatever fails to compile or test on Linux (unknown until run)

**Interfaces:**
- Produces: a green `rust (ubuntu-latest)` job; `tools/fetch-pdfium.sh linux-arm64` works.

- [ ] **Step 1:** Add `ubuntu-latest` to `matrix.os` in the `rust` job. Add a step, `if: runner.os == 'Linux'`, before clippy: `sudo apt-get update && sudo apt-get install -y libwebkit2gtk-4.1-dev libgtk-3-dev libayatana-appindicator3-dev librsvg2-dev libudev-dev libdbus-1-dev pkg-config` (the workspace includes `apps/desktop/src-tauri`, and clippy runs `--workspace`). Add any further `-dev` packages the build asks for.
- [ ] **Step 2:** In `tools/fetch-pdfium.sh` add `Linux-aarch64) platform=linux-arm64 ;;` and the pinned `linux-arm64` SHA-256 from the same pdfium-binaries release the other four use (download it, `shasum -a 256`, paste).
- [ ] **Step 3:** Run it on Linux (push the branch, or WSL2 on the Windows PC: `cargo clippy --workspace --all-targets -- -D warnings && tools/fetch-pdfium.sh && cargo nextest run --workspace --profile ci`). Expected first time: failures.
- [ ] **Step 4:** Fix each failure at its cause. Rules: a test that assumes macOS paths gets the Linux expectation added, not `#[cfg(not(linux))]`-skipped; a test that genuinely can't run on Linux (Previewer, launchd) is gated with a comment saying why. Code with `cfg(target_os = "macos")`/`"windows"` branches and no Linux branch gets one (e.g. `deckle-core/src/devices.rs:204`, `previewer.rs` returns no candidates on Linux — that's correct).
- [ ] **Step 5:** All three matrix legs green, plus the press-smoke step. Commit `ci: build and test on Linux`.

### Task 2: Email-only papers without Previewer

`digest run --to device` with no Previewer only tolerates export folders, an e-reader or reMarkable (`crates/deckle-cli/src/lib.rs` ~2561); with `[email] enabled = true` as the only route it returns `PreviewerMissing`. Email sends the EPUB, so it needs no Previewer.

**Files:**
- Modify: `crates/deckle-cli/src/lib.rs` (the `previewer::locate` match in `DigestCommand::Run`)
- Test: `crates/deckle-cli/tests/it/cli.rs`

- [ ] **Step 1: Failing test** `email_only_paper_builds_without_previewer`: temp HOME, `digest.toml` with `sections = ["horoscope"]`, `[email] enabled = true`, `to = "x@kindle.com"`, `from`, `smtp_host = "127.0.0.1"`, `smtp_port = 1` (refused), `username`; env `DECKLE_SMTP_PASSWORD=x`, no Previewer on PATH. Run `--json digest run --date 2026-10-23`. Assert: stderr does not contain `Kindle Previewer is not installed`; `build.notes` contains `no Kindle copy`; the Morgue has `2026/2026-10-23.epub`. (The send itself fails — that's the existing "queued, reported" path, not this test's concern.)
- [ ] **Step 2:** Run `cargo nextest run -p deckle-cli email_only_paper_builds_without_previewer` → FAIL with the Previewer error.
- [ ] **Step 3:** Add `cfg.email.enabled` to the guard and `"email"` to the list that builds the `no Kindle copy: …; the paper went to … only` note. Long reads stay in the paper unless something else takes books (email doesn't).
- [ ] **Step 4:** Test passes; the existing `kobo.rs` no-Previewer test still passes.
- [ ] **Step 5:** Commit `fix(digest): email-only papers build without Kindle Previewer`.

### Task 3: What's due — the pure scheduling rule

**Files:**
- Create: `crates/deckle-digest/src/daemon.rs` (+ `pub mod daemon;` in `lib.rs`)
- Test: unit tests in the same file

**Interfaces:**
- Consumes: `schedule::Schedule` (slug, hour, minute, days), `config::Day`.
- Produces:
  ```rust
  pub const CATCH_UP_UNTIL: u8 = 12;          // local hour
  pub const RETRY_EVERY_MIN: i64 = 15;

  #[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq, Eq)]
  pub struct DaemonState {                     // /config/deckle/daemon.json
      pub printed: BTreeMap<String, NaiveDate>,              // slug -> last printed edition
      pub last_attempt: BTreeMap<String, DateTime<Utc>>,     // slug -> last failed try
  }
  impl DaemonState {
      pub fn load(path: &Path) -> DaemonState;               // missing/corrupt -> default, logged by caller
      pub fn save(&self, path: &Path) -> io::Result<()>;     // atomic write (temp + rename)
  }

  #[derive(Debug, PartialEq, Eq)]
  pub enum Decision { Run, Wait, Done, Skip(SkipWhy) }
  #[derive(Debug, PartialEq, Eq)]
  pub enum SkipWhy { NotToday, PastNoon, RetryLater }

  /// `exists`: today's edition is already on disk (Morgue sidecar), whatever printed it.
  pub fn decide(job: &Schedule, now: DateTime<Tz>, state: &DaemonState, exists: bool) -> Decision;
  ```
  Order of checks: not one of its days → `Skip(NotToday)`; `exists` or `state.printed[slug] == today` → `Done`; before `at` → `Wait`; at/after `at` and before 12:00 → `Run` unless a failed attempt is under 15 min old → `Skip(RetryLater)`; otherwise `Skip(PastNoon)`. A job whose `at` is itself ≥ 12:00 runs in its own minute through the next 15 minutes and is otherwise `PastNoon` (an afternoon paper is still caught up, briefly; the builder may widen that window — say so in the commit).

- [ ] **Step 1: Failing tests** (all with `Tz` = `America/New_York`, job `at 05:30`, every day unless stated):
  - `before_time_waits`: 05:29 → `Wait`.
  - `at_time_runs`: 05:30 → `Run`.
  - `restart_at_9_catches_up`: 09:00, nothing printed → `Run`.
  - `restart_after_noon_skips`: 12:00 → `Skip(PastNoon)`.
  - `printed_today_is_done`: state printed today, 05:31 → `Done`.
  - `existing_edition_is_done`: `exists = true`, nothing in state → `Done`.
  - `wrong_day_skips`: days `[Sat]`, a Monday → `Skip(NotToday)`.
  - `failed_run_retries_until_noon`: last attempt 05:30, now 05:40 → `Skip(RetryLater)`; now 05:46 → `Run`; now 12:05 → `Skip(PastNoon)`.
  - `daemon_restart_inside_the_minute_prints_once`: state printed today, 05:30:30 → `Done`.
  - `dst_fall_back_does_not_reprint`: 2026-11-01, job `at 01:30`, printed 2026-11-01 at the first 01:30 → at the second 01:30 (EST) → `Done`.
  - `state_round_trips` and `corrupt_state_loads_default` for `load`/`save`.
- [ ] **Step 2:** `cargo nextest run -p deckle-digest daemon::` → FAIL (module missing).
- [ ] **Step 3:** Implement as specified above.
- [ ] **Step 4:** Tests pass.
- [ ] **Step 5:** Commit `feat(digest): due rule for the daemon, with noon catch-up`.

### Task 4: `deckle digest daemon`

**Files:**
- Modify: `crates/deckle-cli/src/cli.rs` (new `DigestCommand::Daemon { tick: u64 }`, `#[arg(long, default_value_t = 30)]`, doc: "Stay running and print every paper on its schedule (containers and servers: no launchd, Task Scheduler or cron)")
- Create: `crates/deckle-cli/src/daemon.rs` (the loop; keeps `lib.rs` from growing)
- Modify: `crates/deckle-cli/src/lib.rs` (dispatch), `crates/deckle-digest/src/schedule.rs` (make `Schedule::args` `pub`)
- Modify: `crates/deckle-cli/Cargo.toml` (`signal-hook = "0.3"`)
- Test: `crates/deckle-cli/tests/it/daemon.rs` (+ `mod daemon;` in `tests/it/main.rs`)

**Interfaces:**
- Consumes: `daemon::{decide, DaemonState, Decision}`, `schedule::jobs_for`, `Schedule::args("run" | "watch")`, `Morgue::from_config(cfg).paths(date).1` (the sidecar is written last, so it means "the edition exists"), `tz::local_tz()`, `all_papers(&path)`.
- Produces: `deckle digest daemon [--tick N]`; env `DECKLE_DAEMON_FAKE_NOW` (RFC 3339, tests only, like `DECKLE_SCHEDULE_DRY_RUN`) and `DECKLE_DAEMON_ONCE=1` (one tick, then exit; tests and the image's smoke test); state at `<config dir>/daemon.json`.

Behaviour:
- Start: log version, config path, zone, and each paper's schedule (`main 05:30 every day`, `kid-courier 07:00 sat`). Run the path check (Task 5). Spawn `deckle digest [--config …] watch` as a child; if it exits, log and restart it after 30 s.
- Each tick: re-read papers (`all_papers`); a read error is logged once per distinct message and the last good set is kept. For each job, `decide`; on `Run`, spawn `exe` + `Schedule::args("run")`, one at a time (the existing `run_lock` serializes anyway), stream its stdout/stderr through prefixed with `[<slug>]`. Exit 0 → `printed[slug] = today`, clear `last_attempt`; non-zero → `last_attempt[slug] = now`, log the exit. Save state after each change. Log a `Skip` once per paper per day (`main: today's paper skipped — container started after 12:00`).
- SIGTERM/SIGINT: stop scheduling, send SIGTERM to children, wait up to 60 s for a running edition (Morgue writes are already atomic), then exit 0.

- [ ] **Step 1: Failing tests** in `tests/it/daemon.rs`, using the `kobo.rs` setup pattern (temp HOME, horoscope-only paper, export folder so no Previewer is needed), `DECKLE_DAEMON_ONCE=1`:
  - `due_paper_prints_once`: `[schedule] at = "05:30"`, fake now 06:00 local → exit 0, export folder has today's edition, `daemon.json` has `printed.main = today`. Run again with the same now → no new run (stdout says `main: printed`/no `[main]` run output), file mtime unchanged.
  - `after_noon_logs_skip`: fake now 13:00 → no edition; stderr contains `main` and `skipped` and `12:00`.
  - `config_changes_apply_next_tick`: tick once with no `at` (nothing runs), write `at = "05:30"` into the file, tick again → prints.
  - `unscheduled_paper_never_runs`: second paper file without `at` → never run.
- [ ] **Step 2:** Run `cargo nextest run -p deckle-cli daemon` → FAIL.
- [ ] **Step 3:** Implement per the behaviour above. With `DECKLE_DAEMON_ONCE`, skip spawning `watch`.
- [ ] **Step 4:** Tests pass; `cargo clippy --workspace --all-targets -- -D warnings` clean.
- [ ] **Step 5:** Manual check on Linux/WSL2: `deckle digest daemon --tick 5`, Ctrl-C mid-run → exits cleanly, no `*.tmp`/staged files left in the Morgue year folder.
- [ ] **Step 6:** Commit `feat(cli): digest daemon prints every paper on schedule`.

### Task 5: Path check at start, and `schedule` in daemon installs

**Files:**
- Create: `crates/deckle-digest/src/paths.rs` (+ `pub mod paths;`)
- Modify: `crates/deckle-cli/src/daemon.rs` (call it at start), `crates/deckle-cli/src/lib.rs` (`DigestCommand::Schedule`)
- Test: unit tests in `paths.rs`; `tests/it/daemon.rs`

**Interfaces:**
- Produces:
  ```rust
  pub struct PathProblem { pub paper: String, pub key: &'static str, pub path: PathBuf }
  /// Configured folders that don't exist and can't be created (parent missing too).
  pub fn check(papers: &[Config]) -> Vec<PathProblem>;
  ```
  Keys checked (as written in the TOML, so the log names what to edit): `export.dir` (each), `reading.inbox`, `morgue.folder`, `library` root if configured. A missing folder whose parent exists is fine (it's created on use, as today).
- `DECKLE_DAEMON=1` (set by the image) marks a daemon install. There, `digest schedule --at …` saves `at` and prints `saved; the daemon picks it up within a minute` instead of the Linux crontab text; `--off` clears `at` from the paper's file (main included) and touches no launchd/schtasks/cron.
- In a daemon install, commands that would store a password (`digest email password`, `digest subscribe <site>` with a password) don't try the keychain: they exit non-zero with `no keychain in the container: set DECKLE_SMTP_PASSWORD` (or the site's `DECKLE_SITE_PASSWORD_<SITE>` name, from the existing env-name helper). Reading passwords already falls back to env (`secret.rs`).

- [ ] **Step 1: Failing tests:** `check_names_missing_mac_export_dir` (export dir `/Users/someone/iCloud/Deckle` → one problem, key `export.dir`); `missing_folder_with_existing_parent_is_fine`; in `daemon.rs` tests, `start_lists_bad_paths_and_still_prints` (bad export dir + good second export → stderr names `export.dir` and the path; the good export still gets the paper); `schedule_off_in_daemon_install_clears_main_at` (`DECKLE_DAEMON=1 digest schedule --off` → `digest.toml` has no `at`); `email_password_in_daemon_install_names_env` (`DECKLE_DAEMON=1 digest email password --stdin` → non-zero, stderr contains `DECKLE_SMTP_PASSWORD`).
- [ ] **Step 2:** Run → FAIL.
- [ ] **Step 3:** Implement. Log format: `config: export.dir = /Users/… doesn't exist here (main paper). Point it at a folder under /data.`
- [ ] **Step 4:** Tests pass.
- [ ] **Step 5:** Commit `feat: daemon installs — path check at start, schedule edits config, passwords from env`.

### Task 6: Bookmarklet reachable from the LAN

**Files:**
- Modify: `crates/deckle-digest/src/bookmarklet.rs` (bind address, public origin)
- Test: unit tests in the same file

**Interfaces:**
- Produces: env `DECKLE_BOOKMARKLET_BIND` (default `127.0.0.1`; image sets `0.0.0.0`) and `DECKLE_BOOKMARKLET_URL` (default `http://127.0.0.1:47219`; e.g. `https://deckle.home.lan`). `fn origin() -> String` used by both the generated JavaScript (lines ~47 and ~64) and the origin check (~204). The token check is unchanged and stays mandatory.

- [ ] **Step 1: Failing tests:** `javascript_uses_public_url` (with the URL set, the bookmarklet contains `https://deckle.home.lan/save?token=`); `origin_check_accepts_public_url`; existing `127.0.0.1:47219` tests still pass with nothing set. Env-dependent tests take the value as a parameter of an inner fn rather than mutating process env.
- [ ] **Step 2:** Run → FAIL.
- [ ] **Step 3:** Implement; strip a trailing `/` from the URL.
- [ ] **Step 4:** Pass.
- [ ] **Step 5:** Commit `feat(digest): bookmarklet bind address and public URL for servers`.

### Task 7: The image

**Files:**
- Create: `docker/Dockerfile`, `docker/entrypoint.sh`, `docker/compose.example.yml`, `.dockerignore`
- Create: `tools/docker-smoke.sh`
- Modify: `.github/workflows/ci.yml` (a `docker` job: build amd64 image, run smoke)

**Interfaces:**
- Consumes: `deckle digest daemon`, `deckle digest init`, `DECKLE_DAEMON_ONCE`.
- Produces: image entrypoint contract: `PUID` (default 99), `PGID` (100), `TZ` (UTC), `XDG_CONFIG_HOME=/config`, `HOME=/data`, `DECKLE_DAEMON=1`, `DECKLE_BOOKMARKLET_BIND=0.0.0.0`, `EXPOSE 47219`, `VOLUME /config /data`, `CMD ["deckle","digest","daemon"]`.

- [ ] **Step 1:** Dockerfile: builder `rust:<toolchain from rust-toolchain.toml>-bookworm`, `cargo build --release -p deckle-cli --locked`; fetch PDFium for the target arch with `tools/fetch-pdfium.sh` and copy `libpdfium.so` next to the binary (set the env var `deckle-pdf` reads to its path). Runtime `debian:bookworm-slim` + `ca-certificates tzdata gosu fonts-dejavu-core` (add what the masthead/cover renderer needs — check `deckle-cover` for system font use; bundled fonts need nothing). No Tauri/webkit packages.
- [ ] **Step 2:** `entrypoint.sh` (`#!/bin/sh`, `set -eu`): create user/group with `PUID`/`PGID`; `chown` `/config` and `/data` only if their top-level owner differs (never recursive on every start); if `/config/deckle/digest.toml` is missing, run `deckle digest init` as that user and log `wrote a starter /config/deckle/digest.toml — edit it, then restart`; `exec gosu "$PUID:$PGID" "$@"`.
- [ ] **Step 3:** `tools/docker-smoke.sh IMAGE`: empty temp `/config` and `/data`; run once with `DECKLE_DAEMON_ONCE=1` → exit 0, `digest.toml` exists and is owned by 99:100; write a horoscope-only paper with an export to `/data/out` and `at` 1 minute in the past (TZ=UTC), run once → `/data/out` has today's EPUB.
- [ ] **Step 4:** CI `docker` job on `ubuntu-latest`: `docker build -f docker/Dockerfile -t deckle:ci .` then `tools/docker-smoke.sh deckle:ci`. Green.
- [ ] **Step 5:** `docker/compose.example.yml` with the two volumes, `TZ`, `PUID`/`PGID`, `DECKLE_SMTP_PASSWORD`, `restart: unless-stopped`, port 47219 commented with a note.
- [ ] **Step 6:** Commit `feat: Docker image for headless Deckle`.

### Task 8: Release — Linux binaries and multi-arch image

**Files:**
- Modify: `.github/workflows/release.yml`

- [ ] **Step 1:** New job `linux-cli`, matrix `ubuntu-latest` (`x86_64-unknown-linux-gnu`) and `ubuntu-24.04-arm` (`aarch64-unknown-linux-gnu`): build `deckle-cli` release, tar `deckle` + `libpdfium.so` + `LICENSE` as `deckle-<version>-<target>.tar.gz`, attach to the release the same way the existing jobs attach bundles (follow its dry-run vs real-release switch).
- [ ] **Step 2:** New job `image`: `permissions: packages: write`; `docker/setup-qemu-action`, `docker/setup-buildx-action`, `docker/login-action` to `ghcr.io` with `GITHUB_TOKEN`, `docker/build-push-action` with `platforms: linux/amd64,linux/arm64`, tags `ghcr.io/casavargas/deckle:<version>` and `:latest`; push only on a real release, build-only on dry run. Then run `tools/docker-smoke.sh` against the pushed amd64 image.
- [ ] **Step 3:** Trigger the dry run (`workflow_dispatch`) on the branch; both jobs green; download a Linux tarball and run `./deckle --version` on WSL2.
- [ ] **Step 4:** Commit `ci: release Linux binaries and the container image`.

### Task 9: Unraid template and docs

**Files:**
- Create: `docker/unraid/deckle.xml`, `docs/docker.md`
- Modify: `README.md` (the "Linux isn't supported" line in Status, the download line, Build from source's "Conversion needs macOS or Windows"), `CHANGELOG.md`

- [ ] **Step 1:** Unraid CA template: `Repository` `ghcr.io/casavargas/deckle:latest`, `Network` bridge, `WebUI` empty (Phase 2 adds it), paths `/config` → `/mnt/user/appdata/deckle`, `/data` → `/mnt/user/deckle`, variables `PUID` 99, `PGID` 100, `TZ`, `DECKLE_SMTP_PASSWORD` (`Mask="true"`), port 47219 (optional, bookmarklet), `Overview` stating what works and that Kindle book conversion doesn't. Validate it's well-formed XML (`xmllint --noout`).
- [ ] **Step 2:** `docs/docker.md`: what the container does and doesn't do (the Daily everywhere; Kindle copies by email only; no book conversion for Kindle — Previewer is Mac/Windows only); compose quick start; Unraid; first start and editing `digest.toml`; email to Kindle in the container (env password, approved sender list); bookmarklet over LAN and the https-page/http-server caveat (use a TLS reverse proxy and `DECKLE_BOOKMARKLET_URL`, or drop links in `/data/Deckle/Inbox`); catch-up behaviour in one paragraph; **Moving from the desktop app**: copy the config folder to `/config/deckle`, re-point paths at `/data/…` (the start-up log lists each one), passwords into env vars, turn off "Allow fresh-paper requests" on the desktop and stop its schedule (`deckle digest schedule --off`) so two installs never print or claim the same paper.
- [ ] **Step 3:** README: replace the Linux line with one sentence and a link to `docs/docker.md`; keep "Kindle book conversion needs macOS or Windows".
- [ ] **Step 4:** CHANGELOG entry under Unreleased.
- [ ] **Step 5:** Commit `docs: Docker and Unraid`.
