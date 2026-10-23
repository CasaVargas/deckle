// SPDX-License-Identifier: GPL-3.0-or-later
import { expect, test } from "vitest";
import { monthGrid, sizeLine } from "./morgueView";
import type { MorgueDay } from "./types";

const day = (date: string): MorgueDay => ({ date, paper: "The Deckle Daily", lead: "Mill sold" });

test("monthGrid starts on Monday", () => {
  // October 2026: the 1st is a Thursday, so three blanks come first.
  const grid = monthGrid(2026, 10, [day("2026-10-01")]);
  expect(grid.slice(0, 3)).toEqual(["blank", "blank", "blank"]);
  expect(grid[3]).toEqual(day("2026-10-01"));
  expect(grid.length % 7).toBe(0);
  expect(grid.length).toBe(35);
});

test("monthGrid marks days without an edition null", () => {
  const grid = monthGrid(2026, 10, [day("2026-10-01"), day("2026-10-21")]);
  const inMonth = grid.filter((c) => c !== "blank");
  expect(inMonth.length).toBe(31);
  expect(inMonth.filter((c) => c !== null).length).toBe(2);
  expect(grid[3 + 1]).toBeNull(); // the 2nd
  expect(grid[3 + 20]).toEqual(day("2026-10-21"));
});

test("a month that starts on Monday has no leading blanks", () => {
  // June 2026 starts on a Monday and has 30 days: 5 weeks, 5 trailing blanks.
  const grid = monthGrid(2026, 6, []);
  expect(grid[0]).toBeNull();
  expect(grid.length).toBe(35);
  expect(grid.slice(30)).toEqual(["blank", "blank", "blank", "blank", "blank"]);
});

test("sizeLine", () => {
  expect(sizeLine(1, 2_500_000)).toBe("Morgue: 1 edition, 2 MB");
  expect(sizeLine(214, 618_659_840)).toBe("Morgue: 214 editions, 590 MB");
  expect(sizeLine(0, 0)).toBe("Morgue: 0 editions, 0 MB");
});
