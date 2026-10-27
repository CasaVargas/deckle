// SPDX-License-Identifier: GPL-3.0-or-later
import { expect, test } from "vitest";
import { COOKBOOK_ROOMS, addResultLine, bookLine, chipPicture, recipeCount, takeNext, waitingLine, addIsCurrent, deliveryLine, factsLine, lastAddedLine, sourceLine, webUrl, wantsDrawing } from "./cookbookRooms";
import {
  addChapter,
  canRemoveChapter,
  chapterSlug,
  edited,
  failedSave,
  groupByChapter,
  illustrateSummary,
  illustrateProgress,
  illustrationSends,
  reorderChapters,
  kindleRow,
  moveChapter,
  recipeDetail,
  recipeDraft,
  recipePatch,
  removeChapterWarning,
  rowSaveLine,
  savedRow,
  unbuiltLine,
  watcherNote,
} from "./cookbookRooms";
import type { RowState } from "./cookbookRooms";
import type { CookbookChapter, CookbookChapterRows, CookbookKindle, CookbookRecipe } from "./types";

const kindle = (name: string, state: CookbookKindle["state"], behind: number | null = null): CookbookKindle => ({
  name,
  id: name,
  status: "",
  state,
  behind,
});

test("the tab has Add, Recipes and Delivery, in that order", () => {
  expect(COOKBOOK_ROOMS.map((r) => r.key)).toEqual(["add", "recipes", "delivery"]);
  expect(COOKBOOK_ROOMS.map((r) => r.title)).toEqual(["Add", "Recipes", "Delivery"]);
});

test("delivery says nothing went yet when no Kindle is known", () => {
  expect(deliveryLine([])).toBe("Not sent yet");
});

test("delivery names the Kindles that are up to date", () => {
  expect(deliveryLine([kindle("Scribe", "up_to_date")])).toBe("The Scribe is up to date");
  expect(deliveryLine([kindle("Scribe", "up_to_date"), kindle("Paperwhite", "up_to_date")])).toBe("Both Kindles up to date");
  expect(deliveryLine([kindle("A", "up_to_date"), kindle("B", "up_to_date"), kindle("C", "up_to_date")])).toBe("Every Kindle up to date");
});

test("delivery counts the changes a Kindle is behind", () => {
  expect(deliveryLine([kindle("Scribe", "behind", 3)])).toBe("3 changes go to the Scribe on next plug-in");
  expect(deliveryLine([kindle("Scribe", "behind", 1)])).toBe("1 change goes to the Scribe on next plug-in");
  expect(deliveryLine([kindle("Scribe", "behind", 3), kindle("Paperwhite", "up_to_date")])).toBe("3 changes go to the Scribe on next plug-in");
});

test("delivery with several Kindles behind adds them up", () => {
  expect(deliveryLine([kindle("Scribe", "behind", 3), kindle("Paperwhite", "behind", 2)])).toBe("5 changes go to 2 Kindles on next plug-in");
});

test("a Kindle that is behind by an unknown amount just says it is behind", () => {
  expect(deliveryLine([kindle("Scribe", "behind", null)])).toBe("The Scribe is behind and gets the book on next plug-in");
});

test("delivery says not sent while every Kindle is waiting for its first send", () => {
  expect(deliveryLine([kindle("Scribe", "never_sent")])).toBe("Not sent yet");
  expect(deliveryLine([kindle("Scribe", "up_to_date"), kindle("Paperwhite", "never_sent")])).toBe("Not sent to the Paperwhite yet");
});

test("Kindles switched off are left out of the line", () => {
  expect(deliveryLine([kindle("Scribe", "off")])).toBe("Delivery is off for every Kindle");
  expect(deliveryLine([kindle("Scribe", "up_to_date"), kindle("Old", "off")])).toBe("The Scribe is up to date");
});

test("adding says which chapter, updating says updated", () => {
  expect(addResultLine("added", "Soups and stews")).toBe("Added to Soups and stews");
  expect(addResultLine("updated", "Soups and stews")).toBe("Updated");
});

test("the source line says where the recipe was read from", () => {
  expect(sourceLine({ structured: true })).toBe("Read from recipe data");
  expect(sourceLine({ structured: false })).toBe("Read from the page text, not recipe data");
});

test("the facts line counts ingredients and steps, then the yield and time", () => {
  expect(factsLine({ ingredients: 12, steps: 6, yield: "Serves 4", total: 45, prep: null, cook: null })).toBe("12 ingredients · 6 steps · Serves 4 · 45 min");
  expect(factsLine({ ingredients: 1, steps: 1, yield: null, total: null, prep: null, cook: null })).toBe("1 ingredient · 1 step");
  expect(factsLine({ ingredients: 8, steps: 4, yield: null, total: null, prep: 15, cook: 75 })).toBe("8 ingredients · 4 steps · 1 h 30 min");
  expect(factsLine({ ingredients: 8, steps: 4, yield: null, total: 120, prep: null, cook: null })).toBe("8 ingredients · 4 steps · 2 h");
});

test("the book line counts recipes and chapters", () => {
  expect(bookLine(0, 0)).toBe("No recipes yet");
  expect(bookLine(1, 1)).toBe("1 recipe in 1 chapter");
  expect(bookLine(23, 5)).toBe("23 recipes in 5 chapters");
});

test("the menu counts recipes", () => {
  expect(recipeCount(0)).toBe("0 recipes");
  expect(recipeCount(1)).toBe("1 recipe");
  expect(recipeCount(23)).toBe("23 recipes");
});

test("the next waiting address comes off the front of the queue", () => {
  expect(takeNext([])).toEqual({ next: null, rest: [] });
  expect(takeNext(["a"])).toEqual({ next: "a", rest: [] });
  expect(takeNext(["a", "b", "c"])).toEqual({ next: "a", rest: ["b", "c"] });
});

test("the waiting line counts what is left", () => {
  expect(waitingLine(0)).toBeNull();
  expect(waitingLine(1)).toBe("1 more waiting");
  expect(waitingLine(3)).toBe("3 more waiting");
});

test("an add moves on only if nothing newer was fetched while it ran", () => {
  expect(addIsCurrent(4, 4)).toBe(true);
  expect(addIsCurrent(4, 5)).toBe(false);
});

test("last added names the newest recipe", () => {
  expect(lastAddedLine(null)).toBeNull();
  expect(lastAddedLine({ title: "Tomato soup" })).toBe("Last added · Tomato soup");
});

test("only http and https addresses open", () => {
  expect(webUrl("https://example.com/soup")).toBe("https://example.com/soup");
  expect(webUrl("http://example.com/soup")).toBe("http://example.com/soup");
  expect(webUrl("javascript:alert(1)")).toBeNull();
  expect(webUrl("file:///etc/passwd")).toBeNull();
  expect(webUrl("not a url")).toBeNull();
});

test("an untouched picture chip follows the book's default", () => {
  expect(chipPicture("default", "site")).toBe("site");
  expect(chipPicture("default", "illustration")).toBe("illustration");
  expect(chipPicture("none", "illustration")).toBe("none");
});

test("a drawing is wanted for an illustration with none yet, while illustrations are on", () => {
  expect(wantsDrawing("illustration", "site", true, false)).toBe(true);
  expect(wantsDrawing("default", "illustration", true, false)).toBe(true);
  expect(wantsDrawing("illustration", "site", false, false)).toBe(false);
  expect(wantsDrawing("illustration", "site", true, true)).toBe(false);
  expect(wantsDrawing("site", "illustration", true, false)).toBe(false);
});

const recipe = (p: Partial<CookbookRecipe> = {}): CookbookRecipe => ({
  id: "r",
  title: "Roasted squash soup",
  site_title: "Roasted squash soup",
  title_override: null,
  chapter: "soups",
  home: "soups",
  home_name: "Soups and stews",
  added: "2026-10-26T12:00:00Z",
  is_new: false,
  structured: true,
  site_name: "Example Kitchen",
  url: "https://example.com/soup",
  note: null,
  picture: "default",
  printed: "site",
  has_illustration: false,
  thumb: null,
  ingredients: ["butternut squash", "onion"],
  ...p,
});

test("a recipe's detail is its site and the day it was added", () => {
  expect(recipeDetail(recipe(), new Date("2026-11-01T00:00:00Z"))).toBe("Example Kitchen · Oct 26");
});

test("a recipe from the page text says so", () => {
  expect(recipeDetail(recipe({ structured: false }), new Date("2026-11-01T00:00:00Z"))).toBe("Example Kitchen · Oct 26 · from the page text");
});

test("a recipe added in an earlier year shows the year; a missing site is left out", () => {
  expect(recipeDetail(recipe({ added: "2025-03-02T12:00:00Z", site_name: "" }), new Date("2026-11-01T00:00:00Z"))).toBe("Mar 2, 2025");
});

const rows = (id: string, name: string, titles: string[]): CookbookChapterRows => ({
  id,
  name,
  match: [],
  recipes: titles.map((t, i) => recipe({ id: `${id}${i}`, title: t, chapter: id, home: id })),
});

test("chapters stay in book order and recipes go alphabetically within one", () => {
  const groups = groupByChapter([rows("soups", "Soups", ["Minestrone", "apple soup", "Bisque"]), rows("salads", "Salads", ["Cobb"])]);
  expect(groups.map((g) => g.name)).toEqual(["Soups", "Salads"]);
  expect(groups[0].recipes.map((r) => r.title)).toEqual(["apple soup", "Bisque", "Minestrone"]);
  expect(groups[0].count).toBe(3);
  expect(groups[0].empty).toBeNull();
});

test("an empty chapter is a heading that says Nothing yet", () => {
  const groups = groupByChapter([rows("soups", "Soups", []), rows("other", "Everything else", ["Toast"])]);
  expect(groups[0].empty).toBe("Nothing yet");
  expect(groups[0].count).toBe(0);
  expect(groups[1].empty).toBeNull();
});

test("a search keeps recipes by title or ingredient and drops chapters left with none", () => {
  const soups: CookbookChapterRows = {
    ...rows("soups", "Soups", []),
    recipes: [recipe({ id: "a", title: "Squash soup", ingredients: ["butternut squash"] }), recipe({ id: "b", title: "Chili", ingredients: ["beans", "Squash"] }), recipe({ id: "c", title: "Pho", ingredients: ["noodles"] })],
  };
  const salads: CookbookChapterRows = { ...rows("salads", "Salads", []), recipes: [recipe({ id: "s", title: "Cobb", ingredients: ["lettuce"] })] };
  const groups = groupByChapter([soups, salads], "  SQUASH ");
  expect(groups.map((g) => g.name)).toEqual(["Soups"]);
  expect(groups[0].recipes.map((r) => r.id)).toEqual(["b", "a"]);
  expect(groups[0].count).toBe(2);
  expect(groupByChapter([soups], "nothing like it")).toEqual([]);
});

test("a Kindle row says where it stands", () => {
  expect(kindleRow(kindle("Scribe", "up_to_date"))).toEqual({ name: "Scribe", text: "Up to date", on: true });
  expect(kindleRow(kindle("Scribe", "behind", 3)).text).toBe("3 changes behind");
  expect(kindleRow(kindle("Scribe", "behind", 1)).text).toBe("1 change behind");
  expect(kindleRow(kindle("Scribe", "behind", null)).text).toBe("Behind");
  expect(kindleRow(kindle("Scribe", "never_sent")).text).toBe("Never sent");
  expect(kindleRow(kindle("Scribe", "off"))).toEqual({ name: "Scribe", text: "Off", on: false });
});

test("the watcher note appears only while the watcher isn't installed", () => {
  expect(watcherNote(true)).toBeNull();
  expect(watcherNote(false)).toBe("Updates go over when you press Send now, or turn on delivery when a Kindle connects");
});

test("what is not built yet reads like the CLI's line", () => {
  expect(unbuiltLine(false, null)).toBeNull();
  expect(unbuiltLine(true, null)).toBe("Changes not built yet: waiting for the next build");
  expect(unbuiltLine(true, "no cover font")).toBe("Changes not built yet: no cover font");
});

const ch = (id: string, name = id): CookbookChapter => ({ id, name, match: [] });

test("Alt+arrow moves a chapter one place", () => {
  const list = [ch("a"), ch("b"), ch("c")];
  expect(moveChapter(list, "b", -1)?.map((c) => c.id)).toEqual(["b", "a", "c"]);
  expect(moveChapter(list, "b", 1)?.map((c) => c.id)).toEqual(["a", "c", "b"]);
  expect(list.map((c) => c.id)).toEqual(["a", "b", "c"]);
});

test("a chapter can't move past either end, and an unknown one doesn't move", () => {
  const list = [ch("a"), ch("b")];
  expect(moveChapter(list, "a", -1)).toBeNull();
  expect(moveChapter(list, "b", 1)).toBeNull();
  expect(moveChapter(list, "zzz", 1)).toBeNull();
});

test("Everything else can't be removed", () => {
  expect(canRemoveChapter("other")).toBe(false);
  expect(canRemoveChapter("soups")).toBe(true);
});

test("removing a chapter with recipes says where they go", () => {
  expect(removeChapterWarning(ch("soups", "Soups and stews"), 3)).toBe("3 recipes in Soups and stews will print under Everything else until you move them.");
  expect(removeChapterWarning(ch("soups", "Soups"), 1, "The rest")).toBe("1 recipe in Soups will print under The rest until you move it.");
});

test("removing an empty chapter needs no warning", () => {
  expect(removeChapterWarning(ch("soups", "Soups"), 0)).toBeNull();
});

test("a new chapter's id is a slug of its name, and never one that is taken", () => {
  expect(chapterSlug("Grandma's Pies!", [])).toBe("grandmas-pies");
  expect(chapterSlug("Soups", ["soups"])).toBe("soups-2");
  expect(chapterSlug("Soups", ["soups", "soups-2"])).toBe("soups-3");
  expect(chapterSlug("   ", [])).toBe("chapter");
  expect(chapterSlug("Crème brûlée", [])).toBe("creme-brulee");
});

test("a new chapter goes in before Everything else when that is last", () => {
  const out = addChapter([ch("soups"), ch("other", "Everything else")], "Breads");
  expect(out.map((c) => c.id)).toEqual(["soups", "breads", "other"]);
  expect(out[1]).toEqual({ id: "breads", name: "Breads", match: [] });
  expect(addChapter([ch("soups")], "Breads").map((c) => c.id)).toEqual(["soups", "breads"]);
});

test("a recipe's draft is what its row shows", () => {
  expect(recipeDraft(recipe())).toEqual({ title: "", chapter: "soups", picture: "default", note: "" });
  expect(recipeDraft(recipe({ title_override: "Squash", note: "Less salt", picture: "none" }))).toEqual({ title: "Squash", chapter: "soups", picture: "none", note: "Less salt" });
});

test("only what changed is sent, and a cleared field is sent empty", () => {
  const base = recipeDraft(recipe({ title_override: "Squash", note: "Less salt" }));
  expect(recipePatch(base, base)).toBeNull();
  expect(recipePatch({ ...base, chapter: "mains" }, base)).toEqual({ chapter: "mains" });
  expect(recipePatch({ ...base, picture: "illustration" }, base)).toEqual({ picture: "illustration" });
  expect(recipePatch({ ...base, title: "", note: "" }, base)).toEqual({ title_override: "", note: "" });
  expect(recipePatch({ ...base, title: "  Squash  " }, base)).toBeNull();
});

test("a save that works clears the reason and keeps what was typed since", () => {
  const base = recipeDraft(recipe());
  let row: RowState = { draft: base, base, error: "Couldn't save", saving: true };
  row = edited(row, { note: "Less salt" });
  row = savedRow(row, { ...base, note: "Less" });
  expect(row.error).toBeNull();
  expect(row.base.note).toBe("Less");
  expect(row.draft.note).toBe("Less salt");
  // what was typed since is still to be saved, so the row is still saving
  expect(row.saving).toBe(true);
  expect(savedRow(row, { ...base, note: "Less salt" }).saving).toBe(false);
});

test("a failed save keeps the typed text and shows the reason on the row", () => {
  const base = recipeDraft(recipe());
  let row: RowState = { draft: base, base, error: null, saving: false };
  row = edited(row, { note: "Less salt", title: "Squash" });
  row = failedSave(row, "Disk is full");
  expect(row.draft).toEqual({ ...base, note: "Less salt", title: "Squash" });
  expect(row.base).toEqual(base);
  expect(row.saving).toBe(false);
  expect(rowSaveLine(row)).toBe("Disk is full");
});

test("a row says Saving while a save runs and nothing when it rests", () => {
  const base = recipeDraft(recipe());
  expect(rowSaveLine({ error: null, saving: true })).toBe("Saving…");
  expect(rowSaveLine({ error: null, saving: false })).toBeNull();
  expect(rowSaveLine({ error: "No", saving: true })).toBe("No");
});

test("Illustrate all reports its progress", () => {
  expect(illustrateProgress({ n: 0, total: 12, id: "", title: "", result: "start" })).toBe("Drawing 12 recipes");
  expect(illustrateProgress({ n: 3, total: 12, id: "a", title: "Squash soup", result: "drawn" })).toBe("3 of 12: Squash soup");
  expect(illustrateProgress({ n: 4, total: 12, id: "b", title: "Chili", result: "kept_photo" })).toBe("4 of 12: Chili (kept the photo)");
});

test("Illustrate all ends with a summary", () => {
  expect(illustrateSummary({ total: 10, drawn: 7, kept: 1, already: 2, stopped: false })).toBe("Drew 7. Kept the photo for 1. 2 already had one.");
  expect(illustrateSummary({ total: 10, drawn: 3, kept: 0, already: 0, stopped: true })).toBe("Stopped. Drew 3.");
  expect(illustrateSummary({ total: 0, drawn: 0, kept: 0, already: 4, stopped: false })).toBe("Nothing to draw. 4 already had one.");
});

test("dragging a chapter puts it where it was dropped", () => {
  const list = [ch("a"), ch("b"), ch("c")];
  expect(reorderChapters(list, 0, 2)?.map((c) => c.id)).toEqual(["b", "c", "a"]);
  expect(reorderChapters(list, 2, 0)?.map((c) => c.id)).toEqual(["c", "a", "b"]);
  expect(reorderChapters(list, 1, 1)).toBeNull();
  expect(reorderChapters(list, 5, 0)).toBeNull();
  expect(reorderChapters(list, 0, 3)).toBeNull();
});

test("the illustrations switch says exactly what is sent and where", () => {
  const sent = "the recipe's title, its chapter's name and up to eight ingredient names";
  expect(illustrationSends("")).toBe(`Each drawing sends ${sent} to api.openai.com. Nothing else goes: no web address, description, note or photo.`);
  expect(illustrationSends("http://localhost:11434/v1")).toBe(`Each drawing sends ${sent} to localhost:11434. Nothing else goes: no web address, description, note or photo.`);
  expect(illustrationSends("my image box")).toBe(`Each drawing sends ${sent} to my image box. Nothing else goes: no web address, description, note or photo.`);
});
