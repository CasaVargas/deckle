// SPDX-License-Identifier: GPL-3.0-or-later
import { expect, test } from "vitest";
import { bindable, move } from "./bindList";

test("move shifts an item and stops at the ends", () => {
  expect(move(["a", "b", "c"], 2, -1)).toEqual(["a", "c", "b"]);
  expect(move(["a", "b", "c"], 0, 1)).toEqual(["b", "a", "c"]);
  const same = ["a", "b"];
  expect(move(same, 0, -1)).toBe(same);
  expect(move(same, 1, 1)).toBe(same);
});

test("a finished Kindle book or an error row can't be a part", () => {
  expect(bindable("/x/a.epub", "cover")).toBe(true);
  expect(bindable("/x/notes.DOCX", "cover")).toBe(true);
  expect(bindable("/x/done.kfx", "ready")).toBe(false);
  expect(bindable("/x/bad.epub", "error")).toBe(false);
});
