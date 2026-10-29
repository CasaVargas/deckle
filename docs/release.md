# Releasing Deckle

## Every release

1. Bump the version in `Cargo.toml` (workspace), `apps/desktop/package.json` and
   `apps/desktop/src-tauri/tauri.conf.json` (a test fails if they differ). Add a `CHANGELOG.md`
   entry.
2. Merge to `main`, then tag and push: `git tag -a v0.1.0 -m "Deckle 0.1.0" && git push origin v0.1.0`.
3. The `release` workflow builds macOS arm64 + x86_64 (`.dmg`, `.app.tar.gz`), Windows x64 (NSIS
   `-setup.exe`), each updater bundle with its `.sig`, plus
   `latest.json`, and attaches them to a **draft** release. `check-updater-json` fails the run
   if `latest.json` lacks a platform.
4. Download one installer per platform, install, send a book, look over Settings, and paste a
   web address on the Books tab (⌘V / Ctrl+V). Then publish the draft, as a
   normal release (not a pre-release). **Publishing ships the update:** every installed copy
   offers it at its next launch, so test first.

Rebuild an existing tag: *Actions → release → Run workflow* with the tag name.

**Dry run.** *Actions → release → Run workflow* on a branch, with **dry_run** ticked and no tag:
it builds, signs and notarizes every platform exactly as a release does, checks the bundled
PDFium (the `.app` passes `codesign --verify --deep --strict` and `spctl`, the Windows installer
puts `pdfium.dll` beside `Deckle.exe`, and each built app reads `fixtures/pdf/book.pdf` through
its own copy), checks the Mac app carries the Liquid Glass icon (`Assets.car` and
`CFBundleIconName`), and keeps the `.dmg` and `-setup.exe` as workflow artifacts for a week. No release
or `latest.json` is created, so nothing reaches installed copies. Use it before tagging whenever
the bundle changes.

## Updater key (owner, one time)

In-app updates (`docs/superpowers/specs/2026-10-22-auto-update-design.md`) are verified against
the public key in `tauri.conf.json` (`plugins.updater.pubkey`). The matching private key signs
each release's updater bundles.

1. The key pair was generated with `pnpm tauri signer generate --ci -p "" -w ~/.tauri/deckle-updater.key`
   (no password). It is Deckle's own, not DebridDownloader's.
2. Store `~/.tauri/deckle-updater.key` in the password manager.
3. Repository secret `TAURI_SIGNING_PRIVATE_KEY` = the file's contents. The release workflow
   stops with an error without it.
4. **Losing the private key strands every install:** copies in the field only accept updates
   signed with it, so users would have to reinstall by hand. Rotating it is the same:
   ship the new public key in a release signed with the old key, then switch the secret.

A local `pnpm tauri build` needs the key in the environment too:
`TAURI_SIGNING_PRIVATE_KEY=$(cat ~/.tauri/deckle-updater.key) pnpm tauri build`.

### "Latest" must stay an app release

Installs read `releases/latest/download/latest.json`. The `latest-guard` workflow runs whenever a
release is published, edited or deleted. If Latest isn't the newest app release with a
`latest.json` (for example a `wordwise-vN` data release), it moves Latest back and fails so you
get the email. Publish data releases with `--latest=false`.

## macOS signing and notarization (owner, one time)

Without these secrets the `.app` is ad-hoc signed: Gatekeeper blocks a double-click and users
must right-click → Open once. With them, the workflow signs with Developer ID
and notarizes.

1. Apple Developer account → Certificates → **Developer ID Application** (Xcode → Settings →
   Accounts → Manage Certificates → + also works). This Mac has only an *Apple Development*
   identity today (`security find-identity -v -p codesigning`).
2. Export it from Keychain Access as a `.p12` with a password; `base64 -i cert.p12 | pbcopy`.
3. appleid.apple.com → App-Specific Passwords → create one for "Deckle notarization".
4. Repository secrets (Settings → Secrets and variables → Actions):
   - `APPLE_CERTIFICATE` — the base64 `.p12`
   - `APPLE_CERTIFICATE_PASSWORD` — its password
   - `APPLE_SIGNING_IDENTITY` — `Developer ID Application: <Name> (<TEAMID>)`
   - `APPLE_ID` — the Apple ID email
   - `APPLE_PASSWORD` — the app-specific password
   - `APPLE_TEAM_ID` — the 10-character team id
5. Re-run the workflow for the tag; the log says "Signing with the Developer ID certificate".
   Check locally: `spctl -a -vv Deckle.app` → `source=Notarized Developer ID`.

## Windows

Signed with **Azure Artifact Signing**: account `casavargas`, certificate profile
`casavargas-public-trust`, endpoint `https://eus.codesigning.azure.net`, the same setup as Beltr
and DebridDownloader. The pieces:

- `apps/desktop/src-tauri/windows/tauri.signing.conf.json` sets `bundle.windows.signCommand` to
  `windows/sign.ps1`. CI passes it with `--config` only when signing is configured, so local and
  pull-request builds stay unsigned.
- `sign.ps1` runs signtool with the Artifact Signing dlib on each file Tauri hands it: the app
  `.exe` and the NSIS installer and uninstaller. Timestamped by
  `http://timestamp.acs.microsoft.com` (the certificates live for days).
- The release workflow's "Azure Artifact Signing" step downloads the pinned, hash-checked
  `Microsoft.ArtifactSigning.Client` package, finds a Windows SDK signtool (10.0.22621+), checks
  for a .NET 8+ runtime, and signs a throwaway exe first, so a bad secret or a missing role fails
  there, readably. After the build, "Verify Windows signatures" checks the installer and the
  app exe inside it.

Repository secrets (Settings → Secrets and variables → Actions), from the Entra app registration
that has the **Artifact Signing Certificate Profile Signer** role on the `casavargas` account:
`AZURE_TENANT_ID`, `AZURE_CLIENT_ID`, `AZURE_CLIENT_SECRET`. Without them the Windows build is
unsigned and the log warns. A new certificate may still trigger SmartScreen until it has
reputation.

## Word Wise database

Separate from app releases: see `data/wordwise/README.md` (tag `wordwise-vN`, the app pins the
SHA-256).
