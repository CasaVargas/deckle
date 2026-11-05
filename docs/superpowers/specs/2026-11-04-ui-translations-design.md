# The app in other languages, French first — design

**Status:** foundation building · **Date:** 2026-11-04 · **Owner:** Jonathan (CasaVargas LLC) ·
**Issue:** #317 ("Is it possible to add translations like French? I can help if you want or AI
can make that?") · **Builds on:** `2026-11-01-welcome-and-no-unavailable-line-design.md` (the
welcome, migrated first).

Calls marked *(builder's call)* are mine, not the owner's; review them.

## 1. Intent

A French reader asked for Deckle in French and offered to help. Deckle is ~10.7k lines of desktop
TSX with every string written in English inline. The goal is a way to show the app's own words
in another language, start with French, and let the reporter review it.

**Success:** a Language setting lists English and Français; choosing Français turns the parts
already migrated to French at once, without a restart, and the choice survives one. Anything not
yet translated shows in English, never as a key or a blank.

## 2. Scope

- **In:** the app's own words (its "chrome"): labels, buttons, headings, help text.
- **Out, and staying English** *(builder's call, from the issue's plan)*: the generated paper,
  books, Word Wise (an English lexicon), the puzzle books, the command line, and messages that
  come from the Rust side (errors arrive as English strings). The paper's language is a separate
  question from the app's.
- **Out for now:** plurals beyond what a string can avoid, right-to-left languages, translating
  the README and docs.

## 3. How it works

- `apps/desktop/src/i18n.ts` holds the catalogs, the current language and `t(key, vars)`.
  - Catalogs are JSON: `src/locales/en.json` (the source of truth) and `src/locales/fr.json`.
    Keys are dotted by place (`welcome.paper.title`). Keys are typed from `en.json`, so a typo is
    a compile error.
  - `{name}` in a string is replaced by `vars.name`.
  - A key missing from the chosen language falls back to English *(the parity test keeps this
    from happening in a release, but a half-done catalog must never show a raw key)*.
  - `<b>…</b>` in a string becomes bold through `rich()`, so a sentence with a bold phrase stays
    one string a translator can reorder. No other markup.
- **Where the choice lives** *(builder's call)*: `localStorage` under `deckle.locale`, not the
  Rust-backed Settings. It's a display preference of this window, the web app gets it per
  browser (right for a container several people open), and it needs no Rust change. With no
  choice, the language comes from `navigator.language`: `fr…` gives French, anything else English.
- **Switching** is live: an `I18nProvider` at the root holds the language in state; components
  read `useT()`, so changing it re-renders them with nothing lost.
- **Dates** in migrated views use the language's own format (`fr-FR`, `en-GB`).

## 4. The Language setting

Settings → About gets a **Language** row with a select: English, Français. Search settings finds
it under "language langue french français translation". Names are written in their own language.

## 5. French

French is drafted by the builder (AI-assisted) and marked for review; the reporter on #317 offered
to read it. French typography: a narrow no-break space before `; ! ?` and a no-break space
before `:`, « guillemets » for quotes, "Kindle", "Kobo", "Deckle" and feature names that are
Amazon's own terms (X-Ray, Word Wise) left as they are.

## 6. Order of work

1. **Foundation (this PR):** `i18n.ts`, both catalogs, the provider, the Language row, and the
   welcome migrated as proof: `Welcome.tsx`, `onboarding.ts`'s lines and `BookmarkletInstall.tsx`.
2. The app frame: `App.tsx` (tabs, notices, footer).
3. Settings: `settings/*.tsx` and `settingsIndex.ts` (page titles, search entries).
4. The rest of the views, one area per PR (Daily, Books, Kindle, Puzzles, Cookbook), and the
   hard-coded `en-US` dates in `TomorrowsPaper.tsx`, `ThisWeek.tsx`, `shelfView.ts`, `kindleRooms.ts`.

## 7. Tests

- `t()` returns French when the language is French and falls back to English for a missing key;
  `{vars}` are filled.
- `en.json` and `fr.json` have exactly the same keys (parity).
- The language defaults from `navigator.language` and a choice round-trips through storage.
- `rich()` turns `<b>` into bold and leaves the rest as text.
- Grep: `Welcome.tsx` has no English text left in its JSX.
- `pnpm build` (tsc + vite) passes.
