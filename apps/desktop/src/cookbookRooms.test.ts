// SPDX-License-Identifier: GPL-3.0-or-later
import { expect, test } from "vitest";
import { COOKBOOK_ROOMS, addResultLine, bookLine, chipPicture, recipeCount, takeNext, waitingLine, addIsCurrent, deliveryLine, factsLine, lastAddedLine, sourceLine, webUrl, wantsDrawing } from "./cookbookRooms";
import type { CookbookKindle } from "./types";

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
