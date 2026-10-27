// SPDX-License-Identifier: GPL-3.0-or-later
// The Cookbook tab's pure parts (cookbook design §8.2): its tabs and the lines of copy that are
// worked out from the book. Beside the tab, as dailyRooms.ts is beside the Daily.
import type { CookbookKindle, CookbookPicture } from "./types";

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
