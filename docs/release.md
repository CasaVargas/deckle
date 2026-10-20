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
  `.exe`, the NSIS installer and uninstaller, and the `.msi`. Timestamped by
  `http://timestamp.acs.microsoft.com` (the certificates live for days).
- The release workflow's "Azure Artifact Signing" step downloads the pinned, hash-checked
  `Microsoft.ArtifactSigning.Client` package, finds a Windows SDK signtool (10.0.22621+), checks
  for a .NET 8+ runtime, and signs a throwaway exe first, so a bad secret or a missing role fails
  there, readably. After the build, "Verify Windows signatures" checks both installers and the
  app exe inside the NSIS installer.

Repository secrets (Settings → Secrets and variables → Actions), from the Entra app registration
that has the **Artifact Signing Certificate Profile Signer** role on the `casavargas` account:
`AZURE_TENANT_ID`, `AZURE_CLIENT_ID`, `AZURE_CLIENT_SECRET`. Without them the Windows build is
unsigned and the log warns. A new certificate may still trigger SmartScreen until it has
reputation.

## Word Wise database

Separate from app releases: see `data/wordwise/README.md` (tag `wordwise-vN`, the app pins the
SHA-256).
