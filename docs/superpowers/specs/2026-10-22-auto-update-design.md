# In-app updates — design

**Status:** settled by owner (2026-10-22) · **Date:** 2026-10-22 · **Owner:** Jonathan (CasaVargas LLC)
**Prior art:** DebridDownloader (`CasaVargas/DebridDownloader`), which ships the same Tauri
updater from GitHub Releases.

Deckle finds out when a newer version is out and installs it from inside the app. Nobody has to
visit the Releases page, download an installer and run it by hand.

## 1. Intent and why

Today the About section links to the Releases page and the user does the rest. Most people never
look, so fixes (X-Ray, Word Wise, Kindle quirks) reach almost nobody. DebridDownloader already
updates itself this way; Deckle should behave the same.

Success: 0.3.1 is published. The next time someone opens 0.3.0, the footer says an update is
available. They click it, click **Update**, watch a progress bar, click **Restart to finish**,
and they're on 0.3.1. They never open a browser.

**Builder's call — the first updater release:** this ships in **0.3.0**. 0.2.1 and earlier have
no updater and can never update themselves, so existing users install 0.3.0 by hand once. The
0.3.0 CHANGELOG entry and release body say so ("install this one manually; later versions update
from inside Deckle"). The owner's end-to-end test is 0.3.0 → 0.3.1 (or 0.3.0 → a local build
pointed at a hand-made `latest.json`).

## 2. Goals, non-goals, anti-goals

**Goals**

1. **Automatic check.** Once per launch, in the background, Deckle asks GitHub whether a newer
   version exists.
2. **Visible but quiet.** When there is one, the footer's `Deckle <version>` gains an "Update
   available" marker. Clicking it opens Settings at About.
3. **One-click install.** About shows the new version, a link to its release notes and an
   **Update** button. Clicking it downloads the update and checks its signature, with progress
   shown, then shows **Restart to finish**.
4. **Signed updates.** Every update is signed with a Deckle-only updater key. The app refuses
   anything that doesn't verify.
5. **Release pipeline produces it.** Tagging a release builds the updater bundles, their
   signatures and `latest.json`, all attached to the draft release. Publishing the draft (the
   owner's step, unchanged) makes it live.
6. **Honest privacy text.** The About note and the README's Network table say Deckle checks
   GitHub for updates at launch.

**Non-goals**

- Updating the `deckle` CLI. It is distributed separately and stays manual.
- Update channels (beta/stable), staged rollouts, or skipping versions.
- A setting that turns the check off. The owner chose an automatic check. If one is added later,
  it belongs in Settings with the other network toggles.
- Delta or background-silent updates.

**Anti-goals** (failures even if it works)

- **Restarting by itself.** Deckle never relaunches without the click. A restart during a
  conversion or a USB/MTP copy can leave a half-written book on the Kindle.
- **Installing without the click.** Downloading and installing both wait for **Update**.
- **Nagging.** No modal, no popup on launch, no badge that comes back after it's dismissed
  within the same session. The footer marker and the About section are the whole surface.
- **Noise when offline.** A failed check (offline, GitHub down, rate-limited) shows nothing and
  doesn't retry until the next launch.
- **Sending anything about the user.** The check is a plain GET of `latest.json`. It carries no
  identifiers, settings or library data.

## 3. Decided by the owner

- **Normal releases.** Releases are published as normal (not pre-release) from now on. The
  updater's endpoint, `releases/latest/download/latest.json`, skips pre-releases.
- **Word Wise releases never become "Latest".** The same Releases page holds the Word Wise
  database (`wordwise-vN`, see `data/wordwise/README.md`). GitHub's "Latest" is what the updater
  reads, so a Word Wise release marked Latest would 404 every client's check until the next app
  release. Word Wise releases are published with **Set as latest** off
  (`gh release create … --latest=false`), and a CI guard enforces it (§4, "Latest guard").
- **No more `.msi`.** The per-user NSIS `-setup.exe` is the only Windows installer. It's the one
  the updater installs, with no UAC prompt, and having only one rules out an MSI install and an
  NSIS update ending up side by side. Remove `msi` from `bundle.targets` and the MSI mentions in
  `README.md` (install section), `docs/release.md`, `release.yml` comments and
  `windows/sign.ps1`'s header. 0.2.1 MSI users uninstall it and run the 0.3.0 `-setup.exe`; the
  0.3.0 release notes say so.
- **Automatic check at launch,** with the privacy note rewritten to say so.
- **Same approach as DebridDownloader:** `tauri-plugin-updater` reading `latest.json` from GitHub
  Releases.

## 4. How it works

### App

- Add `tauri-plugin-updater` (Rust) and `@tauri-apps/plugin-updater` (JS), plus
  `tauri-plugin-process` for the relaunch. Register both in `src-tauri/src/lib.rs` the way
  DebridDownloader does. Add `updater:default` and `process:allow-restart` to
  `capabilities/default.json`.
- `tauri.conf.json`:
  - `bundle.createUpdaterArtifacts: true`
  - `plugins.updater.pubkey`: Deckle's own public key (§5)
  - `plugins.updater.endpoints`:
    `["https://github.com/CasaVargas/deckle/releases/latest/download/latest.json"]`
  - Windows: `plugins.updater.windows.installMode: "passive"`. NSIS is per-user
    (`installMode: currentUser`), so no UAC prompt.
- **Check:** in `App.tsx`, on mount, call `check()` once. Keep the resulting `Update` object (or
  `null`) in app state and pass it to the footer and `SettingsDrawer`. Swallow errors, but log
  them to the console.
- **Footer** (`App.tsx`, `.version`): when an update exists, render
  `Deckle 0.2.1 · Update available` as a button that opens Settings scrolled to About.
- **About** (`SettingsDrawer.tsx`) replaces the "Check for a newer version" link:
  - no update: `Deckle 0.2.1 · up to date` (or `· couldn't check` after a failed check, with the
    Releases link as a fallback)
  - update: `Deckle 0.3.0 is available` + "What's new" (the release page for that tag) +
    **Update**
  - downloading: progress bar (bytes from `downloadAndInstall`'s progress events)
  - ready: **Restart to finish**, which calls `relaunch()`
  - error: a one-line message + **Try again** + the Releases link
- **macOS install:** the plugin replaces the `.app` bundle in place. If Deckle is running from
  somewhere it can't write (a read-only DMG mount, or `/Applications` without permission), the
  error state says so and links to the Releases page.

### Release workflow (`.github/workflows/release.yml`)

- `prerelease: false` (keep `releaseDraft: true`).
- Pass `TAURI_SIGNING_PRIVATE_KEY` (and `TAURI_SIGNING_PRIVATE_KEY_PASSWORD`, which can be empty)
  from secrets to `tauri-action`. With `createUpdaterArtifacts`, tauri-action uploads the
  `.app.tar.gz`, NSIS `.exe` and AppImage, each with a `.sig`, and builds a single `latest.json`
  covering `darwin-aarch64`, `darwin-x86_64`, `windows-x86_64` and `linux-x86_64` across the
  matrix jobs. `includeUpdaterJson` defaults to true.
- If the signing key secret is missing, fail the job with a readable `::error::`. An unsigned
  updater build would publish a `latest.json` that no client can verify.
- Release body: drop the "pre-release" framing if there is any. Keep the rest.

### Latest guard (`.github/workflows/latest-guard.yml`)

- Runs on `release: [published, edited, deleted]` and by hand (`workflow_dispatch`).
- Reads `GET /repos/{repo}/releases/latest`. If its tag isn't a `v*` app tag, or the release has
  no `latest.json` asset, it finds the newest published `v*` release that does and sets it as
  Latest (`PATCH … make_latest=true`). Then it **fails** with a `::error::` naming the release
  that was wrongly Latest, so the owner gets the failure email even though the repair is already
  done. *(Builder's call: repair as well as fail, because every hour spent wrong is an hour no
  client can update.)*
- `data/wordwise/README.md` gets the `--latest=false` rule next to its publish command.

### Linux

The AppImage updates in place. **Checked (builder):** `tauri-plugin-updater` 2.10 also updates
`.deb` installs. It reads the bundle type baked into the binary, looks for a
`linux-x86_64-deb` key in `latest.json`, and installs with `dpkg -i` through `pkexec` (a
system password prompt). No Download-link fallback is needed. The release workflow fails if
`latest.json` lacks that key, so a `.deb` install never gets the AppImage by mistake.

## 5. Updater signing key

- A new minisign keypair just for Deckle (`pnpm tauri signer generate`). Not DebridDownloader's,
  so a leak of one doesn't compromise the other.
- **Owner steps:** keep the private key in the password manager, and add it to the repository as
  the `TAURI_SIGNING_PRIVATE_KEY` secret. Whoever generates the key hands it over; it is never
  committed.
- The public key goes in `tauri.conf.json`.
- Losing the private key means installed copies can never update again; they'd need a manual
  reinstall. `docs/release.md` says this in bold.

## 6. Privacy text

**About** (`SettingsDrawer.tsx`), replacing "Deckle never checks for updates or sends anything
about you. It goes online only for: …":

> Deckle checks GitHub for a newer version when it starts; the request carries nothing about you
> or your books. Otherwise it goes online only for: …

(the rest of the existing sentence unchanged)

**README → Network table:** add a first row:

| What | When | Default |
|---|---|---|
| Update check (`latest.json` from this repo's Releases); the update itself only when you click Update | each launch | on |

## 7. Docs

- `docs/release.md`: a one-time section on the updater key (generate, secret, back up, what
  losing it means). In "Every release", note that publishing the draft is what ships the update,
  so test the installers first. Note that releases are normal, not pre-release.
- `CHANGELOG.md`: entry for the version that ships this.

## 8. Testing

- Unit test the front-end update state (idle / available / downloading / ready / error) if the
  repo's front-end tests cover similar components. Otherwise keep it small and typed.
- `cargo build` and `pnpm build` pass; the app starts with the plugin registered.
- Visually check the footer marker and every About state in the running app. A dev-only way to
  force a fake "available" state is fine; it must not ship enabled.
- **End-to-end (owner, once):** publish a test release with a higher version (or point a local
  build's endpoint at a hand-made `latest.json`), open an older signed build, and update it on
  macOS and Windows.

## 9. Left to the builder

- Exact component structure and naming, matching `App.tsx`/`SettingsDrawer.tsx`.
- Wording of the error states, short and plain.
- macOS updater bundles are already named per-arch (v0.2.1 shipped `Deckle_aarch64.app.tar.gz`
  and `Deckle_x64.app.tar.gz`), so DebridDownloader's rename step isn't needed. Confirm
  `latest.json` points each `darwin-*` key at the right one.
- Plugin versions compatible with the pinned `tauri` / `@tauri-apps/api`.
