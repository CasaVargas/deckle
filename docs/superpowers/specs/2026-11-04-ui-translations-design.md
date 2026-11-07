# The app in other languages, French first — design

**Status:** foundation built; German, Spanish, Italian and Portuguese drafted (§8) · **Date:** 2026-11-04 · **Owner:** Jonathan (CasaVargas LLC) ·
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
  - Catalogs are JSON: `src/locales/en.json` (the source of truth) and one file per language
    named by its BCP 47 tag (`fr.json`, `pt-BR.json`), found by `import.meta.glob`, so a new
    file needs no import. Keys are dotted by place (`welcome.paper.title`). Keys are typed from
    `en.json`, so a typo is a compile error.
  - `{name}` in a string is replaced by `vars.name`.
  - A key missing from the chosen language, or left `""`, falls back to English *(the parity test
    keeps this from happening for an offered language, but a half-done catalog must never show a
    raw key, and translation tools write `""` for a line not done yet)*.
  - `<b>…</b>` in a string becomes bold through `rich()`, so a sentence with a bold phrase stays
    one string a translator can reorder. No other markup.
- **Where the choice lives** *(builder's call)*: `localStorage` under `deckle.locale`, not the
  Rust-backed Settings. It's a display preference of this window, the web app gets it per
  browser (right for a container several people open), and it needs no Rust change. With no
  choice, the language comes from the browser's list (`navigator.languages`), in order: the first
  that Deckle offers exactly (`pt-BR`), else in any region (`es-MX` gives `es`, `pt-PT` gives
  `pt-BR`), else English.
- **Switching** is live: an `I18nProvider` at the root holds the language in state; components
  read `useT()`, so changing it re-renders them with nothing lost.
- **Dates** in migrated views use the language's own format (`fr-FR`, `en-GB`, `de-DE`).
- The page's `lang` follows the choice, so a screen reader speaks the language it's reading.

## 4. The Language setting

Settings → About gets a **Language** row with a select of the languages in `LOCALES`, English
first, each named in itself: English, Deutsch, Español, Français, Italiano, Português (Brasil).
Search settings finds it under the word for "language" in each, and each language's name. In any
language but English the note says the translation started as an AI draft that readers correct;
in every language it ends with a **Help translate** link to `docs/translating.md` (§8).

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
- Every language in `LOCALES` has exactly English's keys, none empty (parity).
- Every catalog in `locales/`, offered or not, is named by a BCP 47 tag, has no key English lacks,
  and keeps each line's `{vars}` and `<b>` pairs.
- The language defaults from the browser's list and a choice round-trips through storage.
- Each language's dateline.
- `rich()` turns `<b>` into bold and leaves the rest as text.
- Grep: `Welcome.tsx` has no English text left in its JSX.
- `pnpm build` (tsc + vite) passes.

## 8. More languages, corrected by readers

The owner's ask (2026-11-07): do other languages the way French was done, and let readers
correct them. French showed the loop works: an AI draft shipped, and a French reader fixed eight
lines in a pull request edited in GitHub's web editor (#322).

**Languages:** German, Spanish (one Spanish for Spain and Latin America), Italian and Portuguese
(Brazil), the owner's picks, all big Kindle stores. Drafted by the builder with the French
reviewer's choices carried over: tab, button and bookmarklet names the app still shows in English
stay English, and the watcher is "service (watcher)". *(Builder's calls:)* German says *Sie*,
like French's *vous*; Spanish, Italian and Portuguese use *tú*, *tu* and *você*, what apps in
those languages use.

**Offered vs. in progress.** `LOCALES` in `i18n.ts` is the list the Language row offers, each
with its own name and date format. A catalog in `locales/` that isn't listed loads but isn't
offered: a reader can start Dutch, merge it half done, and nobody sees it until it's complete,
read by a speaker, and added to the list. The tests hold an offered language to every key, and
any catalog to English's shape (§7), so a contributor's pull request can't break the app.

**When English gains a line**, the pull request drafts it in every offered language (the parity
test fails otherwise) and says the drafts are AI's. Readers correct them later.

**How a reader corrects a line**, easiest first (`docs/translating.md`):

1. **An issue form** (`.github/ISSUE_TEMPLATE/translation.yml`): language, where, what it says,
   what it should say. No files, any language.
2. **GitHub's web editor** on their language's file, which forks and opens a pull request, as
   #322 did. The guide has the rules for a line: keys, `{vars}`, `<b>`, names that stay.

**Next: a translation platform**, once more than a few lines a month come in. *(Builder's call:)*
[Hosted Weblate](https://hosted.weblate.org), free for libre projects and Deckle is GPL, edits
these flat JSON files as they are, shows each string beside its English with comments and a
glossary, takes suggestions from readers, and sends its changes back as pull requests, so the
checks above still run on them. Crowdin is the usual other choice (free for open source, not
itself open source). Setup is the owner's to do (it needs their GitHub and Weblate accounts):

1. Ask for libre hosting at hosted.weblate.org and add a project, *Deckle*, from
   `https://github.com/CasaVargas/deckle`, branch `main`.
2. One component, *App*: file format **JSON file**, file mask `apps/desktop/src/locales/*.json`,
   monolingual base file `apps/desktop/src/locales/en.json`, template for new translations the
   same, language code style **BCP style using hyphen as a separator** (so `pt-BR.json`, not
   `pt_BR.json`), and JSON indentation 2 so its diffs touch only the lines it changed.
3. Translation flags: `placeholders:r"\{\w+\}"` (a missing `{var}` is flagged), plus `xml-text`
   if Weblate's check of the `<b>` tags is wanted.
4. Workflow: turn on **reviews**. A line Weblate reads from the repository arrives as
   *Translated*, not *Approved*, which is exactly an AI draft waiting for a speaker; make a
   speaker of each language its reviewer (the reader from #322 for French).
5. Version control: push to a fork and open pull requests (GitHub integration), not straight to
   `main`, so CI checks every batch. Add the GitHub webhook Weblate gives so new English lines
   reach translators as soon as they merge.
6. Put the project's link at the top of *Fix a line* in `docs/translating.md`. The app's Help
   translate link goes to that page, so nothing in the app changes.

Check that Weblate's first pull request changes only the lines it should; if it rewrites whole
files, its JSON settings in step 2 need adjusting.

