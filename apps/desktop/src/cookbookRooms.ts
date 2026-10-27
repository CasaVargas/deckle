// SPDX-License-Identifier: GPL-3.0-or-later
// The Cookbook tab's pure parts (cookbook design §8.2): its tabs and the lines of copy that are
// worked out from the book. Beside the tab, as dailyRooms.ts is beside the Daily.
import type { CookbookChapter, CookbookChapterRows, CookbookDelivery, CookbookIllustrated, CookbookIllustrateEvent, CookbookKindle, CookbookPatch, CookbookPicture, CookbookRecipe } from "./types";

/** The Cookbook tab's tabs: add a recipe, see what's in the book, see where it goes. */
export type CookbookRoom = "add" | "recipes" | "delivery";

export const COOKBOOK_ROOMS: { key: CookbookRoom; title: string }[] = [
  { key: "add", title: "Add" },
  { key: "recipes", title: "Recipes" },
  { key: "delivery", title: "Delivery" },
];

const plural = (n: number, one: string, many = `${one}s`) => `${n} ${n === 1 ? one : many}`;

/** "the Scribe"; a name that already owns itself ("Maya's Kindle") stands alone. */
const the = (name: string) => (/['’]s\b/.test(name) ? name : `the ${name}`);
const The = (name: string) => the(name).replace(/^the /, "The ");

/** One line on where the book stands with the Kindles, from Delivery's rows. */
export function deliveryLine(kindles: CookbookKindle[]): string {
  const on = kindles.filter((k) => k.state !== "off");
  if (kindles.length === 0) return "Not sent yet";
  if (on.length === 0) return "Delivery is off for every Kindle";
  const behind = on.filter((k) => k.state === "behind");
  if (behind.length > 0) {
    const known = behind.filter((k) => k.behind != null);
    if (known.length < behind.length) {
      return behind.length === 1 ? `${The(behind[0].name)} is behind and gets the book on next plug-in` : `${behind.length} Kindles are behind and get the book on next plug-in`;
    }
    const n = known.reduce((sum, k) => sum + (k.behind ?? 0), 0);
    const to = behind.length === 1 ? the(behind[0].name) : `${behind.length} Kindles`;
    return `${plural(n, "change")} ${n === 1 ? "goes" : "go"} to ${to} on next plug-in`;
  }
  const never = on.filter((k) => k.state === "never_sent");
  if (never.length === on.length) return "Not sent yet";
  if (never.length > 0) return `Not sent to ${never.map((k) => the(k.name)).join(" or ")} yet`;
  if (on.length === 1) return `${The(on[0].name)} is up to date`;
  return on.length === 2 ? "Both Kindles up to date" : "Every Kindle up to date";
}

/** Under the Add card once the recipe is filed. */
export function addResultLine(stored: "added" | "updated", chapterName: string): string {
  return stored === "added" ? `Added to ${chapterName}` : "Updated";
}

/** Whether the recipe came from the page's recipe data or only from its text. */
export function sourceLine(r: { structured: boolean }): string {
  return r.structured ? "Read from recipe data" : "Read from the page text, not recipe data";
}

const minutes = (m: number) => (m >= 60 ? (m % 60 === 0 ? `${m / 60} h` : `${Math.floor(m / 60)} h ${m % 60} min`) : `${m} min`);

/** "12 ingredients · 6 steps · Serves 4 · 45 min": the preview card's facts. */
export function factsLine(p: { ingredients: number; steps: number; yield: string | null; total: number | null; prep: number | null; cook: number | null }): string {
  const time = p.total ?? (p.prep != null || p.cook != null ? (p.prep ?? 0) + (p.cook ?? 0) : null);
  return [plural(p.ingredients, "ingredient"), plural(p.steps, "step"), p.yield, time ? minutes(time) : null].filter(Boolean).join(" · ");
}

/** The next address to fetch from those waiting, and the ones left behind it. */
export function takeNext(queue: string[]): { next: string | null; rest: string[] } {
  return queue.length === 0 ? { next: null, rest: [] } : { next: queue[0], rest: queue.slice(1) };
}

/** "2 more waiting"; null when none are. */
export function waitingLine(n: number): string | null {
  return n > 0 ? `${n} more waiting` : null;
}

/** Whether an add that has finished should move on: nothing newer was fetched while it ran. */
export function addIsCurrent(startedAt: number, now: number): boolean {
  return startedAt === now;
}

/** "23 recipes", beside the title on the book menu. */
export function recipeCount(n: number): string {
  return plural(n, "recipe");
}

/** "23 recipes in 5 chapters", on the book card. */
export function bookLine(recipes: number, chapters: number): string {
  return recipes === 0 ? "No recipes yet" : `${plural(recipes, "recipe")} in ${plural(chapters, "chapter")}`;
}

/** "Last added · Tomato soup"; null before there is a recipe. */
export function lastAddedLine(newest: { title: string } | null): string | null {
  return newest ? `Last added · ${newest.title}` : null;
}

/** The address if it's one a link may open in the browser (`http` or `https`), else null. */
export function webUrl(url: string): string | null {
  try {
    const u = new URL(url);
    return u.protocol === "https:" || u.protocol === "http:" ? u.href : null;
  } catch {
    return null;
  }
}

/** The chip that shows as picked: an untouched card follows the book's default picture. */
export function chipPicture(pick: CookbookPicture, bookDefault: Exclude<CookbookPicture, "default">): Exclude<CookbookPicture, "default"> {
  return pick === "default" ? bookDefault : pick;
}

/** After an add: an illustration is wanted when it was chosen, illustrations are on, and none is drawn. */
export function wantsDrawing(pick: CookbookPicture, bookDefault: Exclude<CookbookPicture, "default">, illustrationsOn: boolean, hasIllustration: boolean): boolean {
  return illustrationsOn && !hasIllustration && chipPicture(pick, bookDefault) === "illustration";
}

// Recipes (§8.2)

/** "Example Kitchen · Oct 26", with the year once it isn't this year's, and "· from the page text" when unstructured. */
export function recipeDetail(r: Pick<CookbookRecipe, "site_name" | "added" | "structured">, now: Date = new Date()): string {
  const d = new Date(r.added);
  const when = Number.isNaN(d.getTime())
    ? null
    : d.toLocaleDateString("en-US", d.getFullYear() === now.getFullYear() ? { month: "short", day: "numeric" } : { month: "short", day: "numeric", year: "numeric" });
  return [r.site_name.trim() || null, when, r.structured ? null : "from the page text"].filter(Boolean).join(" · ");
}

export interface RecipeGroup {
  id: string;
  name: string;
  count: number;
  /** "Nothing yet" for a chapter with no recipes (it is left out of the book). */
  empty: string | null;
  recipes: CookbookRecipe[];
}

const byTitle = (a: CookbookRecipe, b: CookbookRecipe) => a.title.localeCompare(b.title, undefined, { sensitivity: "base" });

/**
 * The Recipes list: chapters in book order, recipes alphabetical within one. A search keeps the
 * recipes whose title or an ingredient has the words in it, and drops the chapters left with none.
 */
export function groupByChapter(chapters: CookbookChapterRows[], query = ""): RecipeGroup[] {
  const q = query.trim().toLowerCase();
  const hit = (r: CookbookRecipe) => r.title.toLowerCase().includes(q) || r.ingredients.some((i) => i.toLowerCase().includes(q));
  return chapters
    .map((c) => {
      const recipes = [...(q ? c.recipes.filter(hit) : c.recipes)].sort(byTitle);
      return { id: c.id, name: c.name, count: recipes.length, empty: recipes.length === 0 ? "Nothing yet" : null, recipes };
    })
    .filter((g) => !q || g.count > 0);
}

/** The draft of a recipe's row: what its fields show. An empty title shows the site's. */
export interface RecipeDraft {
  title: string;
  chapter: string;
  picture: CookbookPicture;
  note: string;
}

export function recipeDraft(r: CookbookRecipe): RecipeDraft {
  return { title: r.title_override ?? "", chapter: r.chapter, picture: r.picture, note: r.note ?? "" };
}

/** Only what changed from `base`; a cleared title or note is sent empty. Null when nothing did. */
export function recipePatch(draft: RecipeDraft, base: RecipeDraft): CookbookPatch | null {
  const patch: CookbookPatch = {};
  if (draft.title.trim() !== base.title.trim()) patch.title_override = draft.title.trim();
  if (draft.chapter !== base.chapter) patch.chapter = draft.chapter;
  if (draft.picture !== base.picture) patch.picture = draft.picture;
  if (draft.note.trim() !== base.note.trim()) patch.note = draft.note.trim();
  return Object.keys(patch).length === 0 ? null : patch;
}

/** One open row: what is typed, what is on disk, whether a save runs, and why the last one failed. */
export interface RowState {
  draft: RecipeDraft;
  base: RecipeDraft;
  error: string | null;
  saving: boolean;
}

export const edited = (row: RowState, change: Partial<RecipeDraft>): RowState => ({ ...row, draft: { ...row.draft, ...change } });

/** A save landed: `base` is what's on disk now. Anything typed since is kept, and still to save. */
export const savedRow = (row: RowState, base: RecipeDraft): RowState => ({ ...row, base, error: null, saving: recipePatch(row.draft, base) !== null });

/** A save failed: the reason shows on the row, and the typed text stays. */
export const failedSave = (row: RowState, reason: string): RowState => ({ ...row, error: reason, saving: false });

/** The line on a row: the reason a save failed, else "Saving…" while one runs. */
export function rowSaveLine(row: { error: string | null; saving: boolean }): string | null {
  return row.error ?? (row.saving ? "Saving…" : null);
}

// Delivery (§8.2)

/** A Kindle's row: its name, where it stands, and whether delivery is on. */
export function kindleRow(k: CookbookKindle): { name: string; text: string; on: boolean } {
  const text =
    k.state === "up_to_date"
      ? "Up to date"
      : k.state === "behind"
        ? k.behind == null
          ? "Behind"
          : `${plural(k.behind, "change")} behind`
        : k.state === "never_sent"
          ? "Never sent"
          : "Off";
  return { name: k.name, text, on: k.state !== "off" };
}

/** Above the rows while the watcher isn't installed; null once it is. */
export function watcherNote(installed: boolean): string | null {
  return installed ? null : "Updates go over when you press Send now, or turn on delivery when a Kindle connects";
}

/** What the CLI prints for changes not built yet; null when none are waiting. */
export function unbuiltLine(pending: boolean, error: string | null): string | null {
  return pending ? `Changes not built yet: ${error ?? "waiting for the next build"}` : null;
}

// Book settings (§8.2, §6.2)

/** The catch-all chapter's id: it can be renamed but not removed. */
export const OTHER_CHAPTER = "other";

export const canRemoveChapter = (id: string): boolean => id !== OTHER_CHAPTER;

/** `chapters` with `id` one place up (-1) or down (1); null at either end or for an unknown id. */
export function moveChapter(chapters: CookbookChapter[], id: string, by: -1 | 1): CookbookChapter[] | null {
  const from = chapters.findIndex((c) => c.id === id);
  const to = from + by;
  if (from < 0 || to < 0 || to >= chapters.length) return null;
  const out = [...chapters];
  [out[from], out[to]] = [out[to], out[from]];
  return out;
}

/** What removing a chapter needs to say first (§6.2); null for an empty one, which just goes. */
export function removeChapterWarning(chapter: Pick<CookbookChapter, "name">, count: number, otherName = "Everything else"): string | null {
  if (count === 0) return null;
  return `${plural(count, "recipe")} in ${chapter.name} will print under ${otherName} until you ${count === 1 ? "move it" : "move them"}.`;
}

/** A chapter id from its name: lowercase words with dashes, and never one already taken. */
export function chapterSlug(name: string, taken: string[]): string {
  const base =
    name
      .normalize("NFD")
      .replace(/[\u0300-\u036f]/g, "")
      .toLowerCase()
      .replace(/['’]/g, "")
      .replace(/[^a-z0-9]+/g, "-")
      .replace(/^-+|-+$/g, "") || "chapter";
  if (!taken.includes(base)) return base;
  let n = 2;
  while (taken.includes(`${base}-${n}`)) n++;
  return `${base}-${n}`;
}

/** A new, empty chapter: before Everything else when that is last, else at the end. */
export function addChapter(chapters: CookbookChapter[], name: string): CookbookChapter[] {
  const fresh: CookbookChapter = { id: chapterSlug(name, chapters.map((c) => c.id)), name, match: [] };
  const last = chapters[chapters.length - 1];
  return last && last.id === OTHER_CHAPTER ? [...chapters.slice(0, -1), fresh, last] : [...chapters, fresh];
}

/** The line under Illustrate all as it runs. */
export function illustrateProgress(e: CookbookIllustrateEvent): string {
  if (e.result === "start") return `Drawing ${plural(e.total, "recipe")}`;
  return `${e.n} of ${e.total}: ${e.title}${e.result === "kept_photo" ? " (kept the photo)" : ""}`;
}

/** How a run of Illustrate all ended. */
export function illustrateSummary(r: CookbookIllustrated): string {
  const parts = [
    r.stopped ? "Stopped." : r.drawn + r.kept === 0 ? "Nothing to draw." : null,
    r.drawn > 0 ? `Drew ${r.drawn}.` : null,
    r.kept > 0 ? `Kept the photo for ${r.kept}.` : null,
    r.already > 0 ? `${r.already} already had one.` : null,
  ];
  return parts.filter(Boolean).join(" ");
}

/** `chapters` with the one at `from` moved to `to` (a drag); null when that moves nothing. */
export function reorderChapters(chapters: CookbookChapter[], from: number, to: number): CookbookChapter[] | null {
  if (from === to || from < 0 || to < 0 || from >= chapters.length || to >= chapters.length) return null;
  const out = [...chapters];
  const [moved] = out.splice(from, 1);
  out.splice(to, 0, moved);
  return out;
}

/** The line under the Illustrations switch (§7.2): what is sent, and where. */
export function illustrationSends(endpoint: string): string {
  const e = endpoint.trim();
  let where = "api.openai.com";
  if (e) {
    try {
      where = new URL(e).host || e;
    } catch {
      where = e;
    }
  }
  return `Each drawing sends the recipe's title, its chapter's name and up to eight ingredient names to ${where}. Nothing else goes: no web address, description, note or photo.`;
}

/** After a save: draw at once only for an Illustration just chosen, with illustrations on, none drawn, and no Illustrate all going. */
export function shouldAutoDraw(
  patch: CookbookPatch,
  pick: CookbookPicture,
  bookDefault: Exclude<CookbookPicture, "default">,
  illustrationsOn: boolean,
  hasIllustration: boolean,
  drawingAll: boolean,
): boolean {
  return patch.picture !== undefined && !drawingAll && wantsDrawing(pick, bookDefault, illustrationsOn, hasIllustration);
}

/** Delivery when the Kindles couldn't be listed: no rows, and the reason to show. */
export function failedDelivery(reason: string): CookbookDelivery {
  return { kindles: [], watcher_installed: true, error: reason };
}
