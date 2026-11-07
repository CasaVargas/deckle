# Translating Deckle

Deckle's own words (its labels, buttons and help text) come in English, Deutsch, Español,
Français, Italiano and Português (Brasil). Every language starts as an AI draft, and the people
who read Deckle in it correct it. French was the first: drafted for
[#317](https://github.com/CasaVargas/deckle/issues/317), then corrected by a French reader in
[#322](https://github.com/CasaVargas/deckle/pull/322).

Only the app is translated. The paper, your books and Word Wise keep their own language. So far
the welcome and the Language setting are translated; the rest of the app moves over one area at a
time and shows in English until it does.

The language is under **Settings → About → Language**.

## Fix a line

You don't need to be a programmer. Pick whichever is easier:

- **Tell us.** Open a [translation fix](https://github.com/CasaVargas/deckle/issues/new?template=translation.yml):
  what it says now, where you saw it, and what it should say. Write in English or in your own
  language. Someone makes the change.
- **Change it yourself on GitHub.** Open your language's file, click the pencil (*Edit this file*),
  change the words, and click *Commit changes…* then *Propose changes*. GitHub makes your own copy
  and opens a pull request. That's how the French was corrected.

  [Deutsch](../apps/desktop/src/locales/de.json) ·
  [Español](../apps/desktop/src/locales/es.json) ·
  [Français](../apps/desktop/src/locales/fr.json) ·
  [Italiano](../apps/desktop/src/locales/it.json) ·
  [Português (Brasil)](../apps/desktop/src/locales/pt-BR.json) ·
  [English, the original](../apps/desktop/src/locales/en.json)

### The rules for a line

Each line is `"key": "text"`. Find the line by searching for words you saw in the app.

- **Change only the text on the right.** The key on the left (`"welcome.skip"`) is how the app
  finds the line.
- **Keep every `{word}` in braces** exactly as it is, `{time}` or `{name}`, and move it wherever
  your sentence needs it. Deckle fills it in.
- **`<b>…</b>` makes words bold.** Keep as many pairs as the English has, around whatever words
  fit your sentence.
- **Names stay as they are:** Deckle, Kindle, Kindle Scribe, Kobo, PocketBook, reMarkable, Ollama,
  X-Ray, Word Wise, EPUB, PDF. So do the names of things the app still shows in English: the tabs
  (Daily, Books, Cookbook, Puzzles, Kindle), Delivery, the Send button, and the bookmarklets
  *Save to paper* and *Save recipe*.

If a brace or tag goes missing, the checks on the pull request say which line.

### Each language's voice

| Language | Speaks to the reader as | Quotes | Notes |
|---|---|---|---|
| Deutsch | Sie | „…“ | KI for AI; *Morgendruck* for the morning schedule |
| Español | tú | «…» | One Spanish for Spain and Latin America: *equipo* for computer, *pronóstico del tiempo* for the weather |
| Français | vous | « … » | A narrow no-break space before `; ! ?`, a no-break space before `:` and inside « » |
| Italiano | tu | «…» | |
| Português (Brasil) | você | “…” | Brazilian usage; *matéria* for a news story |

These were the first draft's choices. If they read wrong to you, say so in an issue.

## Add a language

1. Open an issue first, so two people don't start the same one.
2. Copy `apps/desktop/src/locales/en.json` to a file named by the language's
   [BCP 47 tag](https://www.w3.org/International/articles/language-tags/): `nl.json`,
   `pt-PT.json`, `zh-Hans.json`. Translate the right-hand side. A file that's partly done can be
   merged: a line left out or left `""` shows in English.
3. When every line is done and someone who speaks it has read it, add it to `LOCALES` in
   `apps/desktop/src/i18n.ts` with its own name and its date format. That's when it shows up in
   Settings → About → Language.

## For developers: a new line of English

- Write it in `apps/desktop/src/locales/en.json`, the source of truth. Keys are dotted by where
  the line appears (`settings.language.help`) and typed from `en.json`, so a typo won't compile.
- Every language in `LOCALES` needs the line too, or `i18n.test.ts` fails. Draft it (AI is fine;
  that's how every language started) and say so in the pull request, so readers know it's
  waiting for them.
- In code: `t("key", { name })`, and `rich(t("key"))` for a line with `<b>`. No other markup.
  Keep a sentence in one key, never stitched from pieces, so a translator can reorder it.

Design: [docs/superpowers/specs/2026-11-04-ui-translations-design.md](superpowers/specs/2026-11-04-ui-translations-design.md).
