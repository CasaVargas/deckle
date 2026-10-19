# Releasing Deckle

## Every release

1. Bump the version in `Cargo.toml` (workspace), `apps/desktop/package.json` and
   `apps/desktop/src-tauri/tauri.conf.json` (a test fails if they differ). Add a `CHANGELOG.md`
   entry.
2. Merge to `main`, then tag and push: `git tag -a v0.1.0 -m "Deckle 0.1.0" && git push origin v0.1.0`.
3. The `release` workflow builds macOS arm64 + x86_64 (`.dmg`, `.app.tar.gz`), Windows x64 (NSIS
   `.exe`, `.msi`) and Linux (`.deb`, `.AppImage`) and attaches them to a **draft** release.
4. Download one installer per platform, install, send a book. Then publish the draft.

Rebuild an existing tag: *Actions → release → Run workflow* with the tag name.

## macOS signing and notarization (owner, one time)

Without these secrets the `.app` is ad-hoc signed: Gatekeeper blocks a double-click and users
must right-click → Open once (README says so). With them, the workflow signs with Developer ID
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

Unsigned in v1 (design §12): SmartScreen shows "Windows protected your PC" → *More info → Run
anyway*. A code-signing certificate is a purchase and waits for the owner's go-ahead; signing then means
`bundle.windows.certificateThumbprint` (or a `signCommand`) plus importing the certificate in the
workflow.

## Word Wise database

Separate from app releases: see `data/wordwise/README.md` (tag `wordwise-vN`, the app pins the
SHA-256).
