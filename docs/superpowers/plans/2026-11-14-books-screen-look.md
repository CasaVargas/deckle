# Books Screen Look Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Restyle the desktop Books screen (and the shared type it uses) to the approved mockup, without changing its layout or behaviour.

**Architecture:** One pure function in `libraryView.ts` turns a series into ordered shelf slots (books, stubs, folded runs); `Library.tsx` renders them. Everything else is CSS in `styles.css` plus one markup change to the empty To send panel in `App.tsx`.

**Tech Stack:** React 19 + TypeScript, Vite, Vitest, Tauri 2 (desktop) and `deckle-server` (web build, used here only to check light mode with real data).

**Spec:** `docs/superpowers/specs/2026-11-14-books-screen-look-design.md`

## Global Constraints

- Work in the worktree `/Volumes/DATA/CasaVargas/deckle-books-look`, branch `feat/books-look`.
- No layout change, no new features, settings or network calls.
- Heading font stack, exactly: `"Iowan Old Style", "Palatino", ui-serif, Georgia, serif`. No bundled font.
- Terracotta (`--accent`) stays the only accent.
- `Library.tsx` and the To send panel are not on `t()`; new copy goes in as plain English (no locale files).
- Copy, exactly: series line `"<Series> · you have <owned> of <total>"`; fold tile `"<a>–<b>"` over `"not in your library"`; stub tooltip `"No. <n> isn't in your library"` (as today); empty To send `"Nothing to send yet"` over `"Pick covers on the left, or drop EPUBs, PDFs and Word files here"`.
- Checks before pushing: `cd apps/desktop && pnpm build && pnpm test`.

## Review Focus

- A series with only unnumbered books (no whole indices): no "you have" line count nonsense; show just the name.
- Fractional indices (2.5) and duplicates (two copies of vol. 2): books still all shown, in order; slots never drop a book.
- A run of exactly two missing (stubs, not a fold) and a run that starts at 1 vs. in the middle.
- Search and the "Not on Kindle" filter hide owned books but keep `missing`: the slot order must still be right with fewer books.
- The other tabs (Daily, Cookbook, Puzzles, Devices, Settings) share `h1`/`h2`/serif rules; the font swap must not break their spacing.

---

### Task 1: Shelf slots and the new series line

**Files:**
- Modify: `apps/desktop/src/libraryView.ts`
- Test: `apps/desktop/src/libraryView.test.ts`

**Interfaces:**
- Produces:
  - `export type ShelfSlot = { kind: "book"; book: CatalogEntry } | { kind: "stub"; n: number } | { kind: "fold"; from: number; to: number };`
  - `export function shelfSlots(g: SeriesGroup): ShelfSlot[]` — books in `g.books` order, each missing number placed before the first book whose index is greater than it (so in number order); unnumbered books stay last; consecutive missing runs of length ≥ 3 become one `fold`, shorter runs one `stub` each.
  - `seriesLine(g: SeriesGroup): string` now returns `"<name> · you have <owned> of <total>"` where `owned` = number of distinct whole indices among `g.books`, `total` = `owned + g.missing.length`; just `g.name` when `owned === 0`.

- [ ] **Step 1: Write the failing tests** in `libraryView.test.ts`:

```ts
const book = (i: number | null) => ({ title: `B${i}`, series: i == null ? null : { name: "S", index: i } }) as CatalogEntry;
const group = (idx: (number | null)[], missing: number[]): SeriesGroup =>
  ({ kind: "series", key: "s", name: "S", books: idx.map(book), missing });
const shape = (g: SeriesGroup) =>
  shelfSlots(g).map((s) => (s.kind === "book" ? `b${s.book.series?.index ?? "-"}` : s.kind === "stub" ? `s${s.n}` : `f${s.from}-${s.to}`));

describe("shelfSlots", () => {
  it("puts a lone gap where its book would be", () => expect(shape(group([2], [1]))).toEqual(["s1", "b2"]));
  it("keeps two in a row as stubs", () => expect(shape(group([1, 4], [2, 3]))).toEqual(["b1", "s2", "s3", "b4"]));
  it("folds three or more", () => expect(shape(group([6], [1, 2, 3, 4, 5]))).toEqual(["f1-5", "b6"]));
  it("handles separate runs", () => expect(shape(group([2, 8], [1, 3, 4, 5, 6, 7]))).toEqual(["s1", "b2", "f3-7", "b8"]));
  it("keeps every book: fractions, duplicates, unnumbered last", () =>
    expect(shape(group([1, 2, 2, 2.5, 4, null], [3]))).toEqual(["b1", "b2", "b2", "b2.5", "s3", "b4", "b-"]));
});
```

And replace the existing `seriesLine` expectation (line ~43) with `expect(seriesLine(g)).toBe("The Expanse · you have 5 of 7");`, plus `expect(seriesLine(group([null], []))).toBe("S");`.

- [ ] **Step 2: Run to see them fail**

Run: `cd apps/desktop && pnpm vitest run src/libraryView.test.ts`
Expected: FAIL (`shelfSlots` is not exported; seriesLine text differs).

- [ ] **Step 3: Implement `ShelfSlot`, `shelfSlots` and the new `seriesLine`** in `libraryView.ts` (`ranges` and `list` stay; `list` may become unused, then delete it).

- [ ] **Step 4: Run to see them pass**

Run: `cd apps/desktop && pnpm vitest run src/libraryView.test.ts`
Expected: PASS

- [ ] **Step 5: Commit**

```bash
git add apps/desktop/src/libraryView.ts apps/desktop/src/libraryView.test.ts
git commit -m "feat(books): order a series' gaps among its books, fold long runs"
```

### Task 2: Render the slots, restyle covers and the series line

**Files:**
- Modify: `apps/desktop/src/Library.tsx:196-209`
- Modify: `apps/desktop/src/styles.css` (the `.lib-*` rules, ~619-635)

**Interfaces:**
- Consumes: `shelfSlots`, `ShelfSlot`, `seriesLine` from Task 1.
- Produces: classes `.lib-stub` (one missing book) and `.lib-fold` (a run), used only here.

- [ ] **Step 1: Render** each series' grid from `shelfSlots(it)`: `book` → the existing `card(book)`; `stub` → `<div className="lib-stub" title={`No. ${n} isn't in your library`}><span>{n}</span></div>`; `fold` → `<div className="lib-fold" title={`Nos. ${from}–${to} aren't in your library`}><b>{`${from}–${to}`}</b><span>not in your library</span></div>`. Keys: `book.path`, `stub-${n}`, `fold-${from}`. Delete the old `.lib-gap` markup and CSS.

- [ ] **Step 2: Style**, values from the mockup:
  - `.lib-grid`: `grid-template-columns: repeat(auto-fill, minmax(136px, 1fr))`, `gap: 18px 14px`, `align-items: start`.
  - `.lib-cover`: `border-radius: 3px 4px 4px 3px`; shadow `inset 0 1px 0 rgb(255 255 255 / .08), 0 8px 18px rgb(0 0 0 / .22), 0 2px 4px rgb(0 0 0 / .2)` (dark mode overrides the two outer alphas to `.5`); a 3px spine via `::before` (`rgb(0 0 0 / .18)`, left edge, full height).
  - A series' grid becomes a wrapping row so a stub can be narrow: `.lib-series .lib-grid { display: flex; flex-wrap: wrap; align-items: flex-start; }` with `.lib-series .lib-card { width: 136px; }` (the loose-books grid stays a CSS grid). Add `--cover-h: calc((136px - 14px) * 1.5)` on `.lib-series` (card width minus the card's padding and border).
  - `.lib-stub`: `width: 16px; height: var(--cover-h); margin-top: 7px` (lines up with the cover inside a card), radius 3px, background `repeating-linear-gradient(180deg, var(--line) 0 2px, color-mix(in srgb, var(--line) 60%, var(--bg)) 2px 4px)`, position relative; its number absolutely at the foot, 9px, muted, centred.
  - `.lib-fold`: `width: 122px; height: var(--cover-h); margin: 7px`, `border: 1px solid var(--line)`, background `color-mix(in srgb, var(--line) 40%, var(--bg))`, centred; `b` in the heading serif 22px weight 500; `span` 10px muted.
  - `.lib-series-line`: weight 400, muted, 12px, letter-spacing .02em; the series name in `var(--fg)` weight 600 (wrap the name in `<b>` by splitting at `" · "` in the JSX).

- [ ] **Step 3: Check in the running app**

Run: `cd apps/desktop && pnpm tauri dev` (with `PATH=$HOME/.rustup/toolchains/stable-aarch64-apple-darwin/bin:$PATH` while `~/.cargo/bin/rustup` is missing). Capture the window by id (`screencapture -x -o -l<windowid>`).
Expected: Fixture Series shows a stub before vol. 2; Hokusai Manga shows one "1–5" tile before vol. 6; covers larger with shadows; nothing overlaps at 960×640 and full screen.

- [ ] **Step 4: Build and test**

Run: `cd apps/desktop && pnpm build && pnpm test`
Expected: build succeeds, all tests pass.

- [ ] **Step 5: Commit**

```bash
git add apps/desktop/src/Library.tsx apps/desktop/src/styles.css
git commit -m "feat(books): bigger covers with depth, stubs and folded gaps"
```

### Task 3: Heading type and palette touches

**Files:**
- Modify: `apps/desktop/src/styles.css`

**Interfaces:**
- Produces: CSS variable `--serif` on `:root`, used by every heading rule.

- [ ] **Step 1: Add** `--serif: "Iowan Old Style", "Palatino", ui-serif, Georgia, serif;` to `:root` and replace all 41 `ui-serif, Georgia, serif` stacks with `var(--serif)`.
- [ ] **Step 2: Heading weights:** `h1`, `.brand`, `.library-head h2`, `.tray-head h2` get `font-weight: 500; letter-spacing: -0.01em` (`.brand` keeps 600). `.library-head h2` goes 20px → 24px.
- [ ] **Step 3: Segmented control** (`.segmented`, ~693): no border; background `color-mix(in srgb, var(--line) 55%, var(--bg))`; `button.on` becomes `background: var(--panel); color: var(--fg); box-shadow: 0 1px 2px rgb(0 0 0 / .15)` (it was a black/white inverted pill). Dark mode: `button.on` background `color-mix(in srgb, var(--line) 80%, var(--panel))`.
- [ ] **Step 4: Check every tab in the running app** (Books, Daily, Cookbook, Puzzles, Devices, Settings), dark mode, window by id.
  Expected: headings in Iowan Old Style; no heading overlaps or clips; segmented controls read clearly on each tab.
- [ ] **Step 5: Build, test, commit**

Run: `cd apps/desktop && pnpm build && pnpm test` — expected PASS.

```bash
git add apps/desktop/src/styles.css
git commit -m "style: book serif for headings, softer segmented control"
```

### Task 4: The empty To send panel

**Files:**
- Modify: `apps/desktop/src/App.tsx:817-821` (the `dock-empty` block)
- Modify: `apps/desktop/src/styles.css` (`.dock-empty`, `.send-foot .device`)

- [ ] **Step 1: Markup.** Replace the two `<p>`s with:

```tsx
<div className="dock-empty">
  <div className="dock-empty-icon" aria-hidden="true"><Icon name="download" size={18} /></div>
  <p className="dock-empty-title">Nothing to send yet</p>
  <p className="muted small">Pick covers on the left, or drop EPUBs, PDFs and Word files here</p>
</div>
```

- [ ] **Step 2: Style.** `.dock-empty`: fills the tray scroll area (`height: 100%`, flex column, centred, gap 6px), `border: 1.5px dashed var(--line)`, `border-radius: 12px`, padding 16px, text centred. `.dock-empty-icon`: 34px square, radius 10px, background `color-mix(in srgb, var(--line) 60%, var(--panel))`, colour `var(--accent)`. `.dock-empty-title`: `var(--fg)`, weight 600, margin 0. When dragging (`.dragging .dock-empty`) the border becomes `var(--accent)`. `.send-foot .device` (the Kindle line): background `color-mix(in srgb, var(--line) 35%, var(--panel))`, no border.
- [ ] **Step 3: Check in the running app**, empty and with one book queued.
  Expected: the empty panel is a dashed drop area filling the column; queuing a book replaces it with the list as before; dropping a file still works.
- [ ] **Step 4: Build, test, commit**

Run: `cd apps/desktop && pnpm build && pnpm test` — expected PASS.

```bash
git add apps/desktop/src/App.tsx apps/desktop/src/styles.css
git commit -m "feat(books): an empty To send reads as a place to drop books"
```

### Task 5: Light mode, CHANGELOG, pull request

**Files:**
- Modify: `CHANGELOG.md` (Unreleased)

- [ ] **Step 1: Light mode with real data.** Build the web front end and run the server against this Mac's data:

Run: `cd apps/desktop && pnpm build:web && cd ../.. && DECKLE_WEB_AUTH=none DECKLE_WEB_PORT=8765 cargo run -p deckle-server` (toolchain `PATH` as above; if the server needs a dist path or roots, take them from `docs/docker.md`). Open `http://localhost:8765` in Playwright with `colorScheme: "light"` at 960×640 and full HD, screenshot the Books tab; repeat with `"dark"`.
Expected: both modes match the mockup's palette; nothing unreadable (check muted text on the fold tile and stubs in light mode).

- [ ] **Step 2: CHANGELOG** under *Unreleased*:

```markdown
- **The Books screen looks better.** Bigger covers with depth, headings in a book serif, a
  series' missing books as slim markers in their place (a long run as one "1–5" tile), and an
  empty To send that shows where to drop books.
```

- [ ] **Step 3: Full checks**

Run: `cd apps/desktop && pnpm build && pnpm test`
Expected: PASS.

- [ ] **Step 4: Commit, push, open the PR** (base `main`, head `CasaVargas:feat/books-look`), with the before/after screenshots in the body and the note that the README screenshot `docs/screenshots/readme/` of the Books tab is now out of date.

```bash
git add CHANGELOG.md
git commit -m "docs: changelog for the Books screen look"
git push origin feat/books-look
```
