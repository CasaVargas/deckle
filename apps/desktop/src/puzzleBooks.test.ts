// SPDX-License-Identifier: GPL-3.0-or-later
import { describe, expect, it } from "vitest";
import { bookLine, makingLine, mb, ownListPrompt, problem, requestFor, sendLine, sizeChoices } from "./puzzleBooks";
import type { MadePuzzleBook, PuzzleKind } from "./types";

const cross: PuzzleKind = { key: "crossword", name: "crossword", title: "Crossword", blurb: "", sizes: [5, 15], default_size: 11, takes: "clued", graded: true, difficulties: ["a", "b", "c"] };
const sudoku: PuzzleKind = { ...cross, key: "sudoku", sizes: null, default_size: 9, takes: "none" };

describe("the Puzzles tab", () => {
  it("starts a kind from its defaults, keeping what carries over", () => {
    const r = requestFor(cross);
    expect([r.kind, r.count, r.size, r.difficulty, r.screen]).toEqual(["crossword", 10, 11, "medium", "kindle-scribe"]);
    const next = requestFor(sudoku, { ...r, count: 25, screen: "remarkable-2", title: "Mine", words: "x" });
    expect([next.kind, next.count, next.size, next.screen, next.title, next.words]).toEqual(["sudoku", 25, 9, "remarkable-2", "", ""]);
  });
  it("offers each size with the usual one marked", () => {
    const sizes = sizeChoices(cross);
    expect(sizes.length).toBe(11);
    expect(sizes[0].label).toBe("5 × 5 (the Mini)");
    expect(sizes[6].label).toBe("11 × 11 (usual)");
    expect(sizeChoices(sudoku)).toEqual([]);
  });
  it("asks for the right own list", () => {
    expect(ownListPrompt(cross)?.label).toBe("Your own words and clues");
    expect(ownListPrompt({ ...cross, takes: "quotes" })?.label).toBe("Your own quotes");
    expect(ownListPrompt(sudoku)).toBeNull();
  });
  it("names what's wrong with a request", () => {
    const r = requestFor(cross);
    expect(problem(r, cross)).toBeNull();
    expect(problem({ ...r, count: 0 }, cross)).toMatch(/1 to 200/);
    expect(problem({ ...r, size: 16 }, cross)).toMatch(/5 to 15/);
    expect(problem({ ...r, date: "yesterday" }, cross)).toMatch(/YYYY/);
    expect(problem({ ...r, size: 99 }, sudoku)).toBeNull();
  });
  it("writes the lines", () => {
    const b = { subtitle: "11×11 · medium · 10 puzzles", request: { ...requestFor(cross), screen: "remarkable-2" } } as MadePuzzleBook;
    expect(bookLine(b)).toBe("11×11 · medium · 10 puzzles · for the reMarkable 2");
    expect(mb(2_140_000)).toBe("2.1 MB");
    expect(makingLine("crossword", 3, 10)).toBe("Making puzzle 4 of 10…");
    expect(makingLine("ladder", 0, 0)).toBe("Starting…");
    expect(sendLine("Scribe")).toBe("Copies it to Scribe, into Documents.");
    expect(sendLine(null)).toMatch(/Plug a Kindle in/);
  });
});
