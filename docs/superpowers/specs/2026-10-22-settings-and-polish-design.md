# Settings page and app polish — design

Date: 2026-10-22 · Status: approved direction, pending document review

## Why

The settings slide-over (`apps/desktop/src/SettingsDrawer.tsx`) is a 420px panel
holding ten sections in one tall column: too much scrolling, everything crammed,
and most options carry a sentence or two of fine print. The rest of the desktop app
has the same symptoms on a smaller scale: finished book rows run to five lines of
jargon (EBOK, PDOC, LLM, counts), icons are text glyphs (⚙ ⬇ ⋯ ▾ ✕), the empty
state is three paragraphs, and `styles.css` defines several classes twice with
conflicting values. Separately, the empty state promises "paste an address" for web
articles, but there is no visible way to do it and pasting does not work in practice.

Success: settings fit on screen a page at a time with no long scroll; the Books
screen reads at a glance; adding a web article is obvious and works.

## Goals

1. Replace the slide-over with a full-window settings view with a left sidebar.
2. Make every setting a compact row: short label, optional one-line note, control on
   the right.
3. Polish the Books screen: shorter rows, plain-word status, real icons, quieter top
   bar and footer, shorter empty state.
4. Give web articles a visible, working entry point.
5. Clean up duplicated CSS, and carry shared icons/buttons/spacing to Daily and Kindle.

## Non-goals

- No change to what any setting does, its stored key, or its default. Layout and
  wording only (`Settings` type and `api.setSettings` stay as they are).
- No layout changes to the Daily and Kindle tabs beyond shared icons, buttons and
  spacing.
- No new settings, no settings search, no onboarding flow.

## Anti-goals

- Settings that still need a long scroll on any one page at the default window size.
- Losing information: anything removed from a row or a setting's note must still be
  reachable (Details view, About, or a tooltip).
- A new runtime dependency for icons or UI (keep the frontend dependency-light).

## Settings view (option A, chosen by Jonathan)

**Frame.** The gear button switches the window into the settings view. It replaces the
main content and the Books/Daily/Kindle tab bar; a sidebar on the left lists pages,
with "← Back" at its top returning to the tab you came from. Esc also goes back
(unless focus is in a text field that uses Esc itself). The footer's update marker
opens settings directly on About. The "Kindle Previewer not found" pill opens
settings on General.

Opening settings while a conversion or send is running is allowed; the running job
keeps going and its progress is visible again on return. (The `busy` guards on Update
and Restart stay.)

*(Claude's call)* The footer (device picker, Send, update marker) is hidden in the
settings view, so no send starts from Settings and the full 640px minimum window
height goes to the page. The `notice` banner stays visible above the settings page,
so errors from saving settings or device options, and a job finishing or failing in
the background, still surface there.

**Pages and contents.** Every current control keeps its place on exactly one page:

| Page | Contents |
|---|---|
| General | Kindle Previewer (found + version, or not found + download link; path field with Choose…); Library (folder, Change / Show / Stop using, "Add every book I send"); Default mode (Automatic / Cover first / Features first) |
| Books | Page numbers; Make a cover when missing; Match the store cover (Amazon edition via Open Library); Fill missing details; Tidy typography; Pop-up footnotes |
| X-Ray | X-Ray on/off; Wikipedia summaries; "Use an AI model" provider select, with model, endpoint (OpenAI-compatible only) and API key shown only when a provider is chosen |
| Word Wise | Word Wise on/off; hint level slider; dictionary note (one line) |
| Kindles | One block per connected Kindle: name, connection (USB/MTP), firmware, cover handling ("covers via PDOC" / "thumbnails", moved here from the footer), known issues, mode on this Kindle, real-ASIN X-Ray checkbox. Empty state: "Connect a Kindle to set per-device options." |
| About | Version, update status (existing `UpdateStatus` behaviour unchanged), what Deckle sends online, credits and licences |

**Rows.** Label of a few words; at most one short gray note line beneath; control
right-aligned. Toggles render as switches (styled checkbox, still an
`<input type="checkbox">` for accessibility). Network-using options get a small
"Online" tag next to the label instead of "(network)" in the sentence. Long
explanations currently under individual toggles move to About ("What Deckle sends
online") or are cut to one line; nothing that warns about privacy or network use is
dropped.

Rough copy targets (builder may tune wording, keep it short):

- "Add page numbers (approximate, like the store does)" → **Page numbers** · note "Approximate, like the store"
- "Fill missing title, author, series…" → **Fill missing details** · Online · note "Only empty fields; the book's own data wins"
- "Tidy typography: curly quotes…" → **Tidy typography** · note "Curly quotes, dashes, ellipses"
- "Match books to an Amazon edition…" → **Match the store cover** · Online · note "Features-first books keep Amazon's cover"

## Books screen polish

- **Top bar:** brand, segmented Books/Daily/Kindle control, spacer, "+ Add books",
  "Add article" (see below), gear icon. "Clear done" stays, shown as now.
- **Book rows:** title and byline as now. Below them, one line of small chips instead
  of stacked lines:
  - before sending *(Claude's call: one chip, from `plan` plus `settings.xray` /
    `settings.word_wise`; the plan effect gains those two settings as dependencies)*.
    Tooltip keeps the current `reason` text.

    | Today | Condition | Chip |
    |---|---|---|
    | Error | `mode === "error"` | "Can't convert" (fail colour) |
    | Ready | `mode === "ready"` | "Sent as is" |
    | Cover | PDOC, or no plan yet | "Keeps cover" |
    | Features | EBOK, no thumbnail | features that are on: "X-Ray + Word Wise", "X-Ray", "Word Wise", or "Kindle book" if both are off |
    | Everything | EBOK with thumbnail | the same feature text plus " + cover", e.g. "X-Ray + Word Wise + cover" |

  - running: progress bar plus stage label (unchanged behaviour).
  - done: "Sent · N pages" when delivered to a Kindle, "Exported · N pages" after
    Export to folder…; then chips for X-Ray / Word Wise if built, "N warnings" in
    warning colour, and "Show file".
  - failed: the error, one line, truncated with full text in Details.
- **Details** *(Claude's call: stays an inline panel under the row, closed by
  default, as `showReport` is today; no dialog)*: the "Show report" menu item becomes
  "Details" and shows everything removed from the row: mode and reason, ASIN, file
  path, delivered path, replaced copies, X-Ray counts and source, Word Wise hint
  count, level and dictionary, all warnings. It is opt-in per book, so default rows
  stay short.
- **Footer:** connection dot and Kindle name (or picker when several); version /
  update marker; Send button with its ▾ menu (Export to folder…). The "MTP · covers via
  PDOC" detail moves to the Kindles settings page.
- **Empty state:** headline "Drop EPUBs here", one line ("They become Kindle books
  with covers, page numbers, X-Ray and Word Wise."), then a muted line "Also .kfx,
  web links and saved pages", then "Choose files…" and "Add article".

## Web articles: visible entry point (bug fix)

Today, `App.tsx` listens for `paste` on `document` and only acts when focus is not in
a field. Nothing in the UI reveals this, and in practice it does not fire. Likely
cause (to confirm when building): WKWebView sends no paste event when no editable
element has focus. Fix:

- An "Add article" button in the top bar (Books tab) and in the empty state opens a
  small inline field: "Paste a web address", with Add and Cancel. Enter adds, Esc
  cancels. Several addresses pasted at once are all added (reuse `WEB_ADDRESS` and
  `addArticles`).
- Keep the document-level paste listener as a shortcut if it works; if the root
  cause turns out to be the webview, either make it work (e.g. ⌘V focuses the
  article field) or drop the empty-state promise. The copy must not claim a
  shortcut that doesn't work.
- Verify in the running app on macOS: paste one URL, several URLs, and junk text
  (shows "That isn't a web address").

## Shared polish

- **Icons:** a small in-repo `Icon` component with inline SVG paths for the handful
  needed (settings, plus, dots, chevron-down, x, arrow-left, check, link, download,
  device). No icon package.
- **CSS:** remove the duplicate `.primary`, `.link`, `.fail` and `.muted` definitions
  in `styles.css`, settling on one value each (keep the split Send button's
  left-only rounding as its own class). Delete `.drawer*` styles once unused.
- **Daily and Kindle tabs:** pick up the new icons, button styles and spacing; no
  structural change.

## Constraints

- Tauri 2 + React 19 + plain CSS (`styles.css` with `--bg/--panel/--fg/...`
  tokens, light and dark). Keep that system and its warm palette and serif titles.
- Must work on macOS and Windows window sizes the app already supports.
- Verify visually in the running app (light and dark) before calling it done.

## Left to the builder

- Exact component split (e.g. `SettingsView.tsx` with one component per page).
- Final copy within the targets above.
- Sidebar width, row spacing and switch styling within the existing palette.
- Whether the settings view remembers the last page opened during a session.
