// SPDX-License-Identifier: GPL-3.0-or-later
import { expect, test } from "vitest";
import { draftFrom } from "./detailsDraft";
import { suggestionsFor, applyAll } from "./suggest";
import type { Suggested } from "./types";

const blank = draftFrom(null, { title: "T", authors: "", asin: null });
const found: Suggested = {
  authors: ["J.R.R. Tolkien"], series: { name: "The Lord of the Rings", index: 1 }, description: "A blurb", tags: ["Elves", "Hobbits"],
  publisher: "Mariner Books", published: "1994", language: "en",
};

test("one suggestion per field the form leaves empty, in form order", () =>
  expect(suggestionsFor(blank, found).map((s) => [s.field, s.text])).toEqual([
    ["authors", "J.R.R. Tolkien"],
    ["series", "The Lord of the Rings, 1"],
    ["description", "A blurb"],
    ["publisher", "Mariner Books"],
    ["published", "1994"],
    ["language", "en"],
    ["tags", "Elves, Hobbits"],
  ]));
test("a field the person filled since gets none", () =>
  expect(suggestionsFor({ ...blank, publisher: "Mine", tags: ["x"], series: "S" }, found).map((s) => s.field)).toEqual(["authors", "description", "published", "language"]));
test("nothing found, nothing suggested", () =>
  expect(suggestionsFor(blank, { authors: [], series: null, description: " ", tags: [], publisher: null, published: null, language: null })).toEqual([]));
test("a series without a number", () => expect(suggestionsFor(blank, { ...found, series: { name: "S", index: null } })[1]).toMatchObject({ text: "S", patch: { series: "S", index: "" } }));
test("apply one", () => expect({ ...blank, ...suggestionsFor(blank, found)[1].patch }).toMatchObject({ series: "The Lord of the Rings", index: "1" }));
test("apply all", () =>
  expect(applyAll(blank, suggestionsFor(blank, found))).toMatchObject({
    authors: ["J.R.R. Tolkien"], series: "The Lord of the Rings", index: "1", description: "A blurb", tags: ["Elves", "Hobbits"], publisher: "Mariner Books", published: "1994", language: "en",
  }));
