// SPDX-License-Identifier: GPL-3.0-or-later
import { expect, test } from "vitest";
import { ALL_SECTIONS, SECTION_LABEL } from "./types";

test("the puzzle is a section, before On This Day", () => {
  expect(ALL_SECTIONS).toContain("puzzle");
  expect(ALL_SECTIONS.indexOf("puzzle")).toBe(ALL_SECTIONS.indexOf("horoscope") + 1);
  expect(ALL_SECTIONS.indexOf("onthisday")).toBe(ALL_SECTIONS.indexOf("puzzle") + 1);
  expect(SECTION_LABEL.puzzle).toBe("Puzzle");
});

test("subscribed papers are a section right after your feeds", () => {
  expect(ALL_SECTIONS.indexOf("subscribed")).toBe(ALL_SECTIONS.indexOf("feeds") + 1);
  expect(SECTION_LABEL.subscribed).toBe("Subscribed papers");
});

test("the shelf is a section, between the subscribed papers and the lottery", () => {
  expect(ALL_SECTIONS.indexOf("shelf")).toBe(ALL_SECTIONS.indexOf("subscribed") + 1);
  expect(ALL_SECTIONS.indexOf("lottery")).toBe(ALL_SECTIONS.indexOf("shelf") + 1);
  expect(SECTION_LABEL.shelf).toBe("From Your Shelf");
});
