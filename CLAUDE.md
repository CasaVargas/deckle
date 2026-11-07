# Working on Deckle

## Keep the README current

The README is the project's front page, and people decide from it whether to install Deckle.
Every pull request that changes what someone sees or can do updates it in the same pull request:

- **The section for that area** (The Daily, Books, Your Kindle, KOReader and calibre, What each
  reader gets, What goes online, Command line, In your language…): a new feature gets its line,
  a renamed tab, button or setting gets its new name, and something removed comes out.
- **The intro and its bullets**, when the change alters what Deckle is or which readers it
  serves.
- **Screenshot alt text** under `docs/screenshots/readme/` when what it describes is renamed, and
  a note in the pull request when the picture itself is out of date.
- **Status**, when a device or feature is verified on real hardware (details go in
  `docs/devices.md`).
- **What goes online**, whenever a change adds or removes a network call.

Match the README's voice: plain, short, what the reader gets, no marketing words. Depth belongs in
`docs/` (`docs/daily.md`, `docs/translating.md`, …) with a link from the README.

A change nobody would notice (a refactor, a test, an internal fix) leaves the README alone. When
unsure, update it.

## Also in the same pull request

- **CHANGELOG.md**, an entry under *Unreleased*, in the style of the ones there.
- **The app's words.** In a view already moved to `t()` (UI translations design,
  `docs/superpowers/specs/2026-11-04-ui-translations-design.md`), new text goes in
  `apps/desktop/src/locales/en.json` and is drafted in every language in `LOCALES`; say in the pull
  request that those drafts are AI's. `docs/translating.md` has the rules.
- **A spec** in `docs/superpowers/specs/` for anything new enough to need one.

## Checks before pushing

- Frontend: `cd apps/desktop && pnpm build && pnpm test`.
- Rust: `cargo fmt --check`, `cargo clippy --workspace`, `cargo nextest run --workspace` (the
  `.githooks` pre-push hook runs fmt and clippy).
