# In-app updates Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Deckle checks GitHub for a newer version at launch and installs it in place when the user clicks.

**Architecture:** `tauri-plugin-updater` 2.14 reads `latest.json` from the Latest GitHub release.
The front end calls `check()` once on mount, keeps the `Update` in `App` state, and drives a
small state machine shown in Settings → About plus a footer marker. The release workflow signs
the updater bundles and publishes `latest.json`. A guard workflow keeps "Latest" on an app
release.

**Tech Stack:** Tauri 2.12, tauri-plugin-updater 2.14, tauri-plugin-process 2.4, React 19, GitHub Actions (tauri-action v0).

**Spec:** `docs/superpowers/specs/2026-10-22-auto-update-design.md`

## Global Constraints

- Endpoint: `https://github.com/CasaVargas/deckle/releases/latest/download/latest.json`
- Never download, install or relaunch without a click. No modal, no launch popup.
- A failed check shows nothing in the footer and doesn't retry until the next launch.
- `msi` is gone from `bundle.targets`; NSIS (`currentUser`) is the only Windows installer.
- Releases: `releaseDraft: true`, `prerelease: false`.
- About text, verbatim: "Deckle checks GitHub for a newer version when it starts; the request carries nothing about you or your books. Otherwise it goes online only for: …" (rest of the existing sentence unchanged).
- The private updater key is never committed.

## Review Focus

- **Restart while a conversion or send is running:** **Restart to finish** is disabled while `busy`, with a hint "Finish the current job first." (Task 2.)
- **Check races the drawer opening:** About renders "checking…" until the check settles; it never flashes "up to date" first. (Task 2.)
- **Download fails midway (network drop, bad signature):** the error state shows the plugin's message, **Try again** and the Releases link. Retrying calls `downloadAndInstall` on the same `Update` again. (Task 2.)
- **`latest.json` missing a platform key** (for example `linux-x86_64-deb`): CI fails before the draft is published. (Task 3.)
- **Dev builds:** `pnpm tauri dev` must not throw on the check. Errors are swallowed and the state becomes `failed`. (Task 2.)

---

### Task 1: Updater plugin wired in, config guarded

**Files:**
- Modify: `apps/desktop/src-tauri/Cargo.toml`, `apps/desktop/package.json` (+ lockfiles)
- Modify: `apps/desktop/src-tauri/src/lib.rs:25-27`
- Modify: `apps/desktop/src-tauri/capabilities/default.json`
- Modify: `apps/desktop/src-tauri/tauri.conf.json`
- Test: `apps/desktop/src-tauri/tests/config.rs`

**Interfaces:**
- Produces: JS `@tauri-apps/plugin-updater` (`check`, `Update`), `@tauri-apps/plugin-process` (`relaunch`) usable from the front end.

- [ ] **Step 1: Replace `no_updater_plugin` and the `msi` assertion in `tests/config.rs`**

  - `bundle_targets_…`: the loop checks `["app", "dmg", "nsis", "appimage", "deb"]`, plus `assert!(!targets.contains(&"msi"))`.
  - New `updater_points_at_latest_release_and_is_signed`: `plugins.updater.endpoints == [<endpoint>]`; `pubkey` is a non-empty string; `bundle.createUpdaterArtifacts == true`; `plugins.updater.windows.installMode == "passive"`; Cargo.toml contains `tauri-plugin-updater` and `tauri-plugin-process`.
- [ ] **Step 2: Run** `cargo test -p deckle-desktop --test config`. Expected: FAIL on the new asserts.
- [ ] **Step 3: Generate the key:** `pnpm tauri signer generate --ci -p "" -w ~/.tauri/deckle-updater.key`. The private key stays outside the repo. Put the `.pub` contents in `pubkey`.
- [ ] **Step 4: Add the dependencies:** `tauri-plugin-updater = "2.10"` and `tauri-plugin-process = "2.3"` in Cargo, `@tauri-apps/plugin-updater` ^2.10 and `@tauri-apps/plugin-process` ^2.3 in pnpm. Register `tauri_plugin_updater::Builder::new().build()` and `tauri_plugin_process::init()` in `lib.rs`. Add `"updater:default"` and `"process:allow-restart"` to the capabilities. Update `tauri.conf.json` per the spec §4, and drop `msi`.
- [ ] **Step 5: Run** `cargo test -p deckle-desktop --test config` and `cargo clippy -p deckle-desktop --all-targets -- -D warnings`. Expected: PASS.
- [ ] **Step 6: Commit** `feat(desktop): register the Tauri updater and drop the MSI target`

### Task 2: Update UI (footer marker + About states)

**Files:**
- Create: `apps/desktop/src/updates.ts`
- Modify: `apps/desktop/src/App.tsx` (state, footer `.version`), `apps/desktop/src/SettingsDrawer.tsx:245-255`, `apps/desktop/src/styles.css`

**Interfaces:**
- Consumes: `check`, `Update` from `@tauri-apps/plugin-updater`; `relaunch` from `@tauri-apps/plugin-process`.
- Produces, in `updates.ts`:
  ```ts
  export type UpdateState =
    | { kind: "checking" }
    | { kind: "current" }
    | { kind: "failed" }                                    // check failed
    | { kind: "available"; update: Update }
    | { kind: "downloading"; update: Update; done: number; total: number | null }
    | { kind: "ready"; update: Update }
    | { kind: "error"; update: Update; message: string };   // download/install failed
  export function checkForUpdate(): Promise<UpdateState>;   // never rejects
  export function install(update: Update, onState: (s: UpdateState) => void): Promise<void>; // never rejects
  ```
  `SettingsDrawer` gets new props: `update: UpdateState`, `busy: boolean`, `onUpdateState: (s: UpdateState) => void`, and `focusAbout: boolean` (scrolls About into view when it opens).

- [ ] **Step 1: `updates.ts`.** `checkForUpdate` wraps `check()`, mapping `null` → `current` and a throw → `failed` (`console.warn`). `install` emits `downloading` from the `Started`/`Progress` events (`contentLength`, summed `chunkLength`), then `ready`, or `error` with `String(e)`.
- [ ] **Step 2: `App.tsx`.** `useState<UpdateState>({ kind: "checking" })`, and `checkForUpdate().then(setUpdate)` in the existing mount effect. Footer: if `update.kind` is `available`, `downloading` or `ready`, the `.version` div becomes a `button.link` reading `Deckle {version} · Update available` (`· Restart to update` when ready). It opens the drawer with `focusAbout`.
- [ ] **Step 3: About section.** Replace the "Check for a newer version" link with one line per state:
  - checking: `· checking for updates…`
  - current: `· up to date`
  - failed: `· couldn't check for updates (<Link>Releases</Link>)`
  - available: `Deckle {v} is available · <Link>What's new</Link>` + `button.primary.solo` **Update**. What's new goes to `${REPO}/releases/tag/v{v}`.
  - downloading: the existing `.progress` markup, with the label `Downloading… {MB of MB | MB}`
  - ready: **Restart to finish** (`relaunch()`), disabled when `busy`, with the hint "Finish the current job first."
  - error: `{message}` + **Try again** + Releases link

  Swap in the privacy sentence (Global Constraints).
- [ ] **Step 4: Verify** `pnpm build` (tsc + vite) passes. Run `pnpm tauri dev` and screenshot the footer and About for `failed` (dev, real) and for `available`/`downloading`/`ready`/`error`. Get those by temporarily seeding the state in `App.tsx`; revert the seed before committing.
- [ ] **Step 5: Commit** `feat(desktop): check for updates at launch and install from About`

### Task 3: Release pipeline + Latest guard

**Files:**
- Modify: `.github/workflows/release.yml`
- Create: `.github/workflows/latest-guard.yml`

- [ ] **Step 1: `release.yml`.**
  - `prerelease: false`.
  - A step before tauri-action fails with `::error::TAURI_SIGNING_PRIVATE_KEY is not set: updater bundles can't be signed (docs/release.md)` when the secret is empty.
  - Pass `TAURI_SIGNING_PRIVATE_KEY` and `TAURI_SIGNING_PRIVATE_KEY_PASSWORD: ""` to tauri-action.
  - Drop the MSI mentions in comments. The verify glob can stay.
  - Release body: add "Updating from 0.2.x? Install this version once by hand; later versions update from inside Deckle (Settings → About). Windows: if you used the .msi, uninstall it first."
- [ ] **Step 2: New `check-updater-json` job** (`needs: bundle`, ubuntu, runs only when `TAG` is set). `gh release download "$TAG" -p latest.json`. Assert with `jq` that `.platforms` has `darwin-aarch64`, `darwin-x86_64`, `windows-x86_64`, `linux-x86_64` and `linux-x86_64-deb`, each with a non-empty `signature` and a `url` containing `/download/$TAG/`. A missing key → `::error::latest.json lacks <key>`.
- [ ] **Step 3: `latest-guard.yml`.** Triggers: `release: [published, edited, deleted]` and `workflow_dispatch`, with `permissions: contents: write`. Read `gh api repos/$REPO/releases/latest`. OK when the tag matches `^v[0-9]` and the assets include `latest.json`. Otherwise:
  - pick the newest non-draft, non-prerelease `v*` release that has `latest.json`
  - `gh api -X PATCH repos/$REPO/releases/<id> -f make_latest=true`
  - `::error::<bad tag> was Latest; restored <good tag>. Publish data releases with --latest=false.` and `exit 1`

  If no app release has `latest.json` yet (true until 0.3.0 ships), print a notice and exit 0.
- [ ] **Step 4: Verify** with `actionlint` if available. Otherwise `python3 -c 'import yaml,sys;[yaml.safe_load(open(f)) for f in sys.argv[1:]]' .github/workflows/*.yml`. Dry-run the guard's jq/shell logic locally against `gh api repos/CasaVargas/deckle/releases/latest`. Today it should take the "no `latest.json` yet" branch.
- [ ] **Step 5: Commit** `ci(release): sign updater bundles, check latest.json, guard Latest`

### Task 4: Docs

**Files:** `README.md` (install section line 33, Network table), `docs/release.md`, `data/wordwise/README.md`, `docs/superpowers/specs/2026-10-18-deckle-design.md:305,330`, `CHANGELOG.md`, `apps/desktop/src-tauri/windows/sign.ps1:6` (comment)

- [ ] **Step 1:** README: Windows install line becomes `-setup.exe` only. Add the Network table's first row (spec §6), and a short "Updates" note in the install section.
- [ ] **Step 2:** `docs/release.md`:
  - new "Updater key (owner, one time)" section: generate, store in the password manager, the `TAURI_SIGNING_PRIVATE_KEY` secret, and in bold that **losing it strands every install**
  - "Every release": publishing the draft ships the update; normal release, not pre-release
  - artifact list without `.msi`
  - local `tauri build` needs `TAURI_SIGNING_PRIVATE_KEY` exported
- [ ] **Step 3:** Wordwise README: publish with `gh release create wordwise-vN … --latest=false`, with the reason. Deckle design §12 and decisions log: note that "No auto-updater" was superseded on 2026-10-22, with a pointer to the new spec.
- [ ] **Step 4:** CHANGELOG `Unreleased`: in-app updates, MSI dropped, the manual one-time install note.
- [ ] **Step 5: Commit** `docs: in-app updates, updater key, Word Wise --latest=false`

### Task 5: Whole-branch verification

- [ ] `cargo fmt --all --check && cargo clippy --workspace --all-targets -- -D warnings && cargo test --workspace`
- [ ] `cd apps/desktop && pnpm build`
- [ ] `TAURI_SIGNING_PRIVATE_KEY=$(cat ~/.tauri/deckle-updater.key) pnpm tauri build --bundles app`. Confirm `Deckle.app.tar.gz` and `.sig` are produced.
