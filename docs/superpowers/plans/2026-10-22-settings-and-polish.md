# Settings Page and App Polish Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Replace the settings slide-over with a full-window sidebar view, tighten the Books screen, and give web articles a visible, working entry point.

**Architecture:** Pure display logic (status chips, address parsing) moves into small tested modules. A new `SettingsView` takes over the window when `settingsPage` state in `App.tsx` is non-null; `SettingsDrawer.tsx` is deleted. An in-repo `Icon` component replaces text glyphs across all tabs.

**Tech Stack:** Tauri 2, React 19, TypeScript 5.9, Vite 8, plain CSS (`apps/desktop/src/styles.css`). Adds `vitest` as a dev dependency only.

**Spec:** `docs/superpowers/specs/2026-10-22-settings-and-polish-design.md`

## Global Constraints

- No change to any setting's behaviour, stored key or default; `Settings` type and `api.setSettings` stay as they are.
- No new runtime dependency (icons are inline SVG in-repo). `vitest` is dev-only.
- Keep the existing CSS tokens (`--bg --panel --fg --muted --line --accent --ok --warn --fail`), warm palette and serif titles; light and dark must both work.
- Default window 960×640, minimum 640×420 (`src-tauri/tauri.conf.json`). At 960×640 no settings page may need a long scroll.
- Nothing removed from a row or a setting note may become unreachable (Details, About, or a tooltip). No privacy or network warning is dropped.
- Copy: short labels, at most one gray note line per setting, network options get an "Online" tag.
- Every UI task ends with a visual check in the running app (`pnpm tauri dev` in `apps/desktop`), light and dark. "It compiles" is not done.
- Commit per task, conventional style, ending with `Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>`.

## Review Focus

1. Esc pressed inside a text field in Settings (API key, model, endpoint, Previewer path) must not leave Settings: it stays in the field. Owned by Task 4.
2. Pasted text with no address, an address wrapped in punctuation (`<https://x.com/a>`, `(https://x.com/a).`), or the same address twice: junk gets "That isn't a web address", wrappers are trimmed, duplicates are added once. Owned by Task 2.
3. The minimum window (640×420): the sidebar stays put and only the page column scrolls; nothing overflows horizontally. Owned by Task 4.
4. A job that finishes or fails while Settings is open: the notice bar shows it, and Back shows the finished rows. Owned by Task 4.
5. Both X-Ray and Word Wise off with an EBOK plan: the chip says "Kindle book", never a feature that won't be built. Owned by Task 1.

---

### Task 1: Status chip logic

**Files:**
- Modify: `apps/desktop/package.json` (add `"vitest": "3.2.4"` to devDependencies and `"test": "vitest run"` to scripts)
- Create: `apps/desktop/src/bookStatus.ts`
- Test: `apps/desktop/src/bookStatus.test.ts`
- Modify: `apps/desktop/src/types.ts` (add `exported: boolean` to `BookState`)

**Interfaces:**
- Produces:
  - `type Chip = { text: string; tone: "plain" | "ok" | "warn" | "fail"; title?: string }`
  - `beforeChip(book: Pick<BookState, "mode" | "reason">, plan: PlanRow | undefined, settings: Pick<Settings, "xray" | "word_wise">): Chip`
  - `doneChips(book: BookState): Chip[]`
  - `BookState.exported: boolean` (true when the last run targeted a folder)

- [ ] **Step 1: Install vitest**

Run: `cd apps/desktop && pnpm add -D vitest@3.2.4`
Expected: lockfile updated, no runtime deps added.

- [ ] **Step 2: Write the failing tests** in `bookStatus.test.ts`:

```ts
const on = { xray: true, word_wise: true };
const ebok = (thumb: boolean): PlanRow => ({ id: "a", cde: "ebok", write_thumbnail: thumb, reason: "r" });
const pdoc: PlanRow = { id: "a", cde: "pdoc", write_thumbnail: false, reason: "r" };
const b = (mode: Mode) => ({ mode, reason: "why" });

test("error", () => expect(beforeChip(b("error"), undefined, on)).toEqual({ text: "Can't convert", tone: "fail", title: "why" }));
test("ready", () => expect(beforeChip(b("ready"), undefined, on).text).toBe("Sent as is"));
test("no plan yet", () => expect(beforeChip(b("full"), undefined, on).text).toBe("Keeps cover"));
test("pdoc", () => expect(beforeChip(b("full"), pdoc, on)).toMatchObject({ text: "Keeps cover", title: "r" }));
test("ebok both", () => expect(beforeChip(b("full"), ebok(false), on).text).toBe("X-Ray + Word Wise"));
test("ebok xray only", () => expect(beforeChip(b("full"), ebok(false), { xray: true, word_wise: false }).text).toBe("X-Ray"));
test("ebok word wise only", () => expect(beforeChip(b("full"), ebok(false), { xray: false, word_wise: true }).text).toBe("Word Wise"));
test("ebok neither", () => expect(beforeChip(b("full"), ebok(false), { xray: false, word_wise: false }).text).toBe("Kindle book"));
test("ebok + thumbnail", () => expect(beforeChip(b("full"), ebok(true), on).text).toBe("X-Ray + Word Wise + cover"));
test("ebok neither + thumbnail", () => expect(beforeChip(b("full"), ebok(true), { xray: false, word_wise: false }).text).toBe("Kindle book + cover"));

// doneChips: base is a done BookState with pages 496, no xray/wordWise, no warnings, exported false
test("sent", () => expect(doneChips(done()).map((c) => c.text)).toEqual(["Sent · 496 pages"]));
test("exported", () => expect(doneChips(done({ exported: true }))[0].text).toBe("Exported · 496 pages"));
test("ready sent", () => expect(doneChips(done({ mode: "ready", pages: null }))[0].text).toBe("Sent as is"));
test("features and warnings", () =>
  expect(doneChips(done({ xray: { people: 1, terms: 1, source: "basic" }, wordWise: ww, runWarnings: ["a", "b"] })).map((c) => [c.text, c.tone]))
    .toEqual([["Sent · 496 pages", "ok"], ["X-Ray", "plain"], ["Word Wise", "plain"], ["2 warnings", "warn"]]));
test("one warning", () => expect(doneChips(done({ runWarnings: ["a"] })).at(-1)!.text).toBe("1 warning"));
```

Define `done(overrides)` and `ww` as local fixtures in the test file.

- [ ] **Step 3: Run to verify they fail**

Run: `cd apps/desktop && pnpm test`
Expected: FAIL, cannot resolve `./bookStatus`.

- [ ] **Step 4: Implement `beforeChip` and `doneChips` in `bookStatus.ts`**

`title` on the before-chip is `plan?.reason ?? book.reason`. Without pages, the sent/exported chip drops " · N pages". Add `exported: false` to `toState` in `App.tsx`, and in `start(target)` set `exported: target.kind === "folder"` alongside the other reset fields.

Note: the chip is computed at render time from `settings`, so the plan `useEffect` does **not** need new dependencies (refines the spec's note; the backend plan doesn't depend on `xray` / `word_wise`).

- [ ] **Step 5: Run tests and type-check**

Run: `cd apps/desktop && pnpm test && pnpm exec tsc --noEmit`
Expected: all pass, no type errors.

- [ ] **Step 6: Commit**

```bash
git add apps/desktop/package.json apps/desktop/pnpm-lock.yaml apps/desktop/src/bookStatus.ts apps/desktop/src/bookStatus.test.ts apps/desktop/src/types.ts apps/desktop/src/App.tsx
git commit -m "feat(desktop): plain-word status chips for book rows"
```

---

### Task 2: Web address parsing

**Files:**
- Create: `apps/desktop/src/articles.ts`
- Test: `apps/desktop/src/articles.test.ts`
- Modify: `apps/desktop/src/App.tsx:34` (remove `WEB_ADDRESS`, import from `articles.ts`)

**Interfaces:**
- Produces: `parseAddresses(text: string): string[]`: unique http(s) addresses in order of first appearance, with trailing `.,;:!?)]}>'"` and leading `<(` trimmed.

- [ ] **Step 1: Write the failing tests**

```ts
test("one", () => expect(parseAddresses("https://a.com/x")).toEqual(["https://a.com/x"]));
test("several, deduped", () => expect(parseAddresses("https://a.com/x\nhttps://b.org https://a.com/x")).toEqual(["https://a.com/x", "https://b.org"]));
test("wrapped", () => expect(parseAddresses("see (https://a.com/x). and <https://b.org/y>")).toEqual(["https://a.com/x", "https://b.org/y"]));
test("junk", () => expect(parseAddresses("hello world")).toEqual([]));
test("not web", () => expect(parseAddresses("ftp://a.com file:///x")).toEqual([]));
```

- [ ] **Step 2: Run to verify they fail**

Run: `cd apps/desktop && pnpm test articles`
Expected: FAIL, cannot resolve `./articles`.

- [ ] **Step 3: Implement `parseAddresses`**, then switch the paste handler in `App.tsx` to `const urls = parseAddresses(ev.clipboardData?.getData("text") ?? "")`.

- [ ] **Step 4: Run tests**

Run: `cd apps/desktop && pnpm test`
Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add apps/desktop/src/articles.ts apps/desktop/src/articles.test.ts apps/desktop/src/App.tsx
git commit -m "refactor(desktop): parse pasted web addresses in one tested place"
```

---

### Task 3: Icons and CSS cleanup

**Files:**
- Create: `apps/desktop/src/Icon.tsx`
- Modify: `apps/desktop/src/styles.css`, `App.tsx`, `BookRow.tsx`, `KindleView.tsx`, `DailyView.tsx`

**Interfaces:**
- Produces: `type IconName = "settings" | "plus" | "dots" | "chevron-down" | "x" | "arrow-left" | "arrow-up" | "arrow-down" | "check" | "link" | "download"`; `default function Icon({ name, size = 16 }: { name: IconName; size?: number }): JSX.Element`. The component renders a 24×24 viewBox stroke SVG (`stroke="currentColor"`, `fill="none"`, `stroke-width` 1.75, `aria-hidden`), with path data drawn or adapted from an MIT-licensed outline set (Tabler or Lucide). Credit it in About in Task 4.

- [ ] **Step 1: Create `Icon.tsx`** with the names above.

- [ ] **Step 2: Replace glyphs.** ⚙ → `settings` (App top bar), ⬇ → `download` (empty state), ⋯ → `dots` (BookRow, KindleView), ▾ → `chevron-down` (Send split), ✕ → `x` (DailyView ×2), ↑/↓ → `arrow-up`/`arrow-down` (DailyView). Keep every existing `aria-label`.

- [ ] **Step 3: De-duplicate `styles.css`.** Keep one each of `.primary` (radius 8px, `padding: 8px 16px`, weight 600), `.link`, `.fail`, `.muted`. Add `.primary.joined { border-radius: 8px 0 0 8px; }` for the split Send button and use it in `App.tsx`. Then delete `.primary.solo` and switch its users (`SettingsDrawer` update buttons, which Task 4 moves) to plain `.primary`. Give `.icon` `display: inline-grid; place-items: center` so the SVGs center.

- [ ] **Step 4: Type-check and build**

Run: `cd apps/desktop && pnpm build`
Expected: succeeds.

- [ ] **Step 5: Visual check.** Run `pnpm tauri dev`. In light and dark, check that the Books, Daily and Kindle tabs show the icons centred and that the Send button keeps its joined ▾. Screenshot each.

- [ ] **Step 6: Commit**

```bash
git add apps/desktop/src
git commit -m "style(desktop): real icons and one definition per shared class"
```

---

### Task 4: Full-window settings view

**Files:**
- Create: `apps/desktop/src/SettingsView.tsx` (frame and sidebar), `apps/desktop/src/settings/` with `General.tsx`, `Books.tsx`, `XRay.tsx`, `WordWise.tsx`, `Kindles.tsx`, `About.tsx`, and `Row.tsx`
- Delete: `apps/desktop/src/SettingsDrawer.tsx`
- Modify: `apps/desktop/src/App.tsx`, `apps/desktop/src/styles.css` (remove `.drawer*`, `.field*`, `.device-card*`, and add settings styles)

**Interfaces:**
- Consumes: `Icon` (Task 3).
- Produces:
  - `type SettingsPage = "general" | "books" | "xray" | "wordwise" | "kindles" | "about"`
  - `SettingsView` props: everything `SettingsDrawer` took except `open`, `focusAbout` and `onClose`, plus `page: SettingsPage`, `onPage: (p: SettingsPage) => void`, `onBack: () => void`.
  - `Row({ label, note, online, children })`: label left, optional note line, optional "Online" tag, control right.
  - `Switch({ checked, onChange, disabled, label })`: an `<input type="checkbox" role="switch">` styled as a switch.

- [ ] **Step 1: App wiring.** Replace `settingsOpen` / `aboutFocus` with `const [settingsPage, setSettingsPage] = useState<SettingsPage | null>(null)`. The gear opens `"general"`, or the last page opened this session if any (keep it in a ref). The footer update marker opens `"about"`, and the Previewer pill and Previewer-missing `start()` path open `"general"`. While `settingsPage !== null`, render `SettingsView` in place of the top bar, tab content and footer. The `notice` bar still renders, above the settings page.

- [ ] **Step 2: Esc.** In `SettingsView`, a `keydown` listener on `document` calls `onBack` on Escape, unless `e.target` is inside `input, textarea, select` (Review Focus 1). Remove it on unmount.

- [ ] **Step 3: Pages.** Move every control from `SettingsDrawer.tsx` onto its page per the spec's table, using `Row` and `Switch`. Copy rules:
  - General: Previewer row note "Found · {version}", or "Not found" + Download link; the path field sits below it, full width. The Library row keeps Change / Show / Stop using and the "Add every book I send" switch. Default mode is a select labelled Automatic / Cover first / Features first.
  - Books: Page numbers ("Approximate, like the store"); Make a cover when missing ("Title and author, never replaces a real cover"); Match the store cover, Online ("Features-first books keep Amazon's cover"); Fill missing details, Online ("Only empty fields; the book's own data wins"); Tidy typography ("Curly quotes, dashes, ellipses"); Pop-up footnotes ("Tap a note number to read it").
  - X-Ray: X-Ray switch ("For books sent with features"); Wikipedia summaries, Online; a "Use an AI model" select (Off / Anthropic / OpenAI-compatible) with model, endpoint and API key indented below when not Off, and the key note cut to "Stored on this computer. Empty uses ANTHROPIC_API_KEY / OPENAI_API_KEY."
  - Word Wise: switch ("For books sent with features"); hint level slider with its current label; one-line note "First book downloads a 12 MB dictionary from Wiktionary." The Chinese-slot explanation moves to About.
  - Kindles: one block per device: name with a USB/MTP tag, a note "Firmware {x} · covers via PDOC|thumbnails" (moved from the footer), compat warnings, a mode select, and the real-ASIN switch ("Keeps Deckle's X-Ray for real ASINs").
  - About: version and licence and Source, `UpdateStatus` (moved as is), a "What Deckle sends online" paragraph (existing text plus the Word Wise Chinese-slot note), credits (existing text plus the icon set's licence line).

- [ ] **Step 4: Styles.** Use a grid with a 200px sidebar and a `minmax(0,1fr)` page. The sidebar stays fixed; only the page column scrolls (`overflow: auto`). Page max width is 640px. Rows get `padding: 12px 0` and a 1px `--line` divider. The switch is 32×18 with `--accent` when on and `--line` when off, and the "Online" tag reuses `.tag`.

- [ ] **Step 5: Build**

Run: `cd apps/desktop && pnpm build && pnpm test`
Expected: succeeds, and `SettingsDrawer` is not referenced anywhere (`grep -r SettingsDrawer src` prints nothing).

- [ ] **Step 6: Visual and behaviour check** in `pnpm tauri dev`, in light and dark:
  - At 960×640, every page fits without scrolling. X-Ray with Anthropic selected is allowed a short scroll.
  - At 640×420, the sidebar stays put and the page scrolls with no horizontal overflow (Review Focus 3).
  - Esc in the API key field stays in the field; Esc elsewhere goes back (Review Focus 1).
  - Start a conversion, open Settings and wait: the notice shows if it fails, and Back shows the finished rows (Review Focus 4).
  - The update marker opens About, and the Previewer pill opens General.
  - Toggle one setting on each page, restart the app, and confirm it persisted.

  Screenshot each page.

- [ ] **Step 7: Commit**

```bash
git add -A apps/desktop/src
git commit -m "feat(desktop): full-window settings with a sidebar, replacing the drawer"
```

---

### Task 5: Books screen polish

**Files:**
- Modify: `apps/desktop/src/BookRow.tsx`, `apps/desktop/src/App.tsx`, `apps/desktop/src/styles.css`

**Interfaces:**
- Consumes: `beforeChip`, `doneChips`, `Chip` (Task 1); `Icon` (Task 3).
- `BookRowView` gains the prop `settings: Pick<Settings, "xray" | "word_wise">`.

- [ ] **Step 1: Rows.** Replace the badge and the stacked `statusLine` lines with one chip line:
  - idle shows `beforeChip`;
  - running keeps the progress bar;
  - done shows `doneChips` followed by "Show file" (when `delivered`);
  - failed shows the one-line error, truncated with CSS.

  Remove the `.shipped` lines from the row. Style a `.chip` with tone classes `ok/warn/fail` from the tokens.

- [ ] **Step 2: Details.** Rename the menu item to "Details" / "Hide details". The inline panel (`book.showReport`) lists mode and reason, ASIN, file, delivered path, replaced copies, X-Ray people/terms/source, Word Wise hints/level/dictionary, warnings and error. Everything removed from the row in Step 1 must appear here.

- [ ] **Step 3: Top bar and footer.**
  - Top bar: "+ Add books" (`Icon plus`), then the "Add article" slot that Task 6 fills, then the gear.
  - Footer: remove the " · MTP · covers via PDOC" span (now on the Kindles page) and keep the dot, name/picker, version marker and Send split.

- [ ] **Step 4: Empty state.** Keep the "Drop EPUBs here" headline. Under it go one line, "They become Kindle books with covers, page numbers, X-Ray and Word Wise.", then the muted line "Also .kfx, web links and saved pages", then the buttons "Choose files…" and "Add article" (wired in Task 6).

- [ ] **Step 5: Build and test**

Run: `cd apps/desktop && pnpm build && pnpm test`
Expected: succeeds.

- [ ] **Step 6: Visual check** in light and dark. Using fixtures in `fixtures/`, add an EPUB, a .kfx and a broken file. Check the before chips; switch X-Ray off in Settings and check that the chip changes (Review Focus 5). Export to a folder and check for "Exported · N pages"; Details shows the counts. Screenshot each state.

- [ ] **Step 7: Commit**

```bash
git add apps/desktop/src
git commit -m "feat(desktop): compact book rows with plain-word chips and a Details panel"
```

---

### Task 6: Add article entry point (bug fix)

**Files:**
- Create: `apps/desktop/src/ArticleField.tsx`
- Modify: `apps/desktop/src/App.tsx`, `apps/desktop/src/styles.css`

**Interfaces:**
- Consumes: `parseAddresses` (Task 2); `addArticles(targets: string[])` (existing in `App.tsx`).
- Produces: `ArticleField({ onAdd, onCancel }: { onAdd: (urls: string[]) => void; onCancel: () => void })`

- [ ] **Step 1: Confirm the root cause first (superpowers:systematic-debugging).** In `pnpm tauri dev`, on the Books tab with nothing focused, add a temporary `console.log` in `onPaste` and press ⌘V with a URL on the clipboard. Record whether the event fires and whether Edit ▸ Paste is enabled in the menu. Remove the log.

- [ ] **Step 2: Implement `ArticleField`.** It's an inline row under the top bar: an autofocused text input with placeholder "Paste a web address", plus Add and Cancel buttons.
  - Enter or Add runs `parseAddresses`. If that returns nothing, show "That isn't a web address" in `--fail` below the field and keep it open; editing the text clears the message. Otherwise call `onAdd` and close.
  - Esc or Cancel calls `onCancel`.

- [ ] **Step 3: Wire it.** The "Add article" button sits in the top bar (Books tab only, disabled while `busy`) and in the empty state. Both set `articleOpen`.

  Then act on Step 1's finding:
  - If the document paste never fires, make ⌘V on the Books tab (with focus outside a field) open `ArticleField` prefilled from `navigator.clipboard.readText()`. If clipboard read isn't permitted in the webview, remove the document listener.
  - If it does fire, keep it.

  Either way, the empty-state copy promises only what works.

- [ ] **Step 4: Build**

Run: `cd apps/desktop && pnpm build && pnpm test`
Expected: succeeds.

- [ ] **Step 5: Verify in the running app.** Paste one URL, then two URLs at once, then junk text, then the same URL twice. Check that the articles are fetched and appear as rows and that junk shows the message (Review Focus 2). If Step 3 kept or replaced ⌘V, try it too.

- [ ] **Step 6: Commit**

```bash
git add apps/desktop/src
git commit -m "fix(desktop): visible Add article field; pasting an address actually works"
```

---

### Task 7: Whole-app pass

- [ ] **Step 1:** Run `cd apps/desktop && pnpm build && pnpm test` and `cargo build -p deckle-desktop`. Both must pass.
- [ ] **Step 2:** In `pnpm tauri dev`, take light and dark screenshots of Books (empty, with rows), Daily, Kindle, and each Settings page. Compare them against the spec's anti-goals: no long scroll at 960×640, nothing lost, no network warning dropped.
- [ ] **Step 3:** Windows can't be checked on this Mac. Note it for the release checklist (`docs/release.md`) rather than claiming it.
- [ ] **Step 4:** Add a line to `CHANGELOG.md` under a new `## Unreleased` heading above 0.3.0: "Settings now open full-window with a sidebar; tidier book rows; a visible Add article field." Commit with `docs: changelog for settings and polish`.
