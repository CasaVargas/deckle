// SPDX-License-Identifier: GPL-3.0-or-later
import { describe, expect, it } from "vitest";
import { SUNDAY_DEFAULT, bookLine, ipadLine, makingLine, mb, ownListPrompt, problem, requestFor, sendLine, sizeChoices, solvedLine, sundayLine } from "./puzzleBooks";
import type { MadePuzzleBook, PuzzleKind } from "./types";

const cross: PuzzleKind = { key: "crossword", name: "crossword", title: "Crossword", blurb: "", sizes: [5, 15], default_size: 11, takes: "clued", graded: true, difficulties: ["a", "b", "c"] };
const sudoku: PuzzleKind = { ...cross, key: "sudoku", name: "sudoku", sizes: null, default_size: 9, takes: "none" };

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
  it("says where a book went for the iPad, and how much was solved there", () => {
    const targets = [{ name: "iPad", dir: "/Users/jon/Library/Mobile Documents/Deckle" }];
    const none = { bundles: [] } as unknown as MadePuzzleBook;
    expect(ipadLine(none, targets)).toBeNull();
    expect(solvedLine(none)).toBeNull();
    const sent = { bundles: ["/Users/jon/Library/Mobile Documents/Deckle/deckle-puzzles-mazes-2026-11-02-9f3a.decklepuzzles"], solved_ipad: [2, 10] } as unknown as MadePuzzleBook;
    expect(ipadLine(sent, targets)).toBe("On iPad's folder, for the iPad");
    expect(solvedLine(sent)).toBe("2 of 10 solved on the iPad");
    const elsewhere = { bundles: ["/Volumes/Share/deckle-puzzles-x.decklepuzzles", ...sent.bundles] } as unknown as MadePuzzleBook;
    expect(ipadLine(elsewhere, targets)).toBe("On /Volumes/Share and iPad's folders, for the iPad");
    expect(ipadLine({ bundles: ["C:\\iPad\\book.decklepuzzles"] } as unknown as MadePuzzleBook, [{ name: "Mia", dir: "C:\\iPad" }])).toBe("On Mia's folder, for the iPad");
  });
  it("tells Sunday's recipe", () => {
    expect(sundayLine(SUNDAY_DEFAULT, [cross])).toBe("10 crosswords from the week's paper, beside Sunday's paper");
    expect(sundayLine({ ...SUNDAY_DEFAULT, kind: "sudoku", count: 1, theme: "vocab" }, [cross, sudoku])).toBe("1 sudoku, beside Sunday's paper");
    expect(sundayLine({ ...SUNDAY_DEFAULT, kind: "ladder", count: 3 }, [cross])).toBe("3 pages of word ladders, beside Sunday's paper");
  });
});

import { CLUE_STYLES, clueLine, interestRows, themeChips } from "./puzzleBooks";
import type { PuzzleSources } from "./types";

describe("themes and clues", () => {
  const sources: PuzzleSources = { sources: { interests: [["The Gazette", "Rust", 41], ["The Gazette", "Panthers", 9]], week: 120, vocab: 0, cookbook: 23 }, ai: "Ollama on this computer" };
  it("offers the chips a kind can use, with counts and reasons", () => {
    const chips = themeChips(cross, sources);
    expect(chips.map((c) => c.key)).toEqual(["none", "own", "interests", "week", "vocab", "cookbook", "subject"]);
    expect(chips[2].tag).toBe("50 words");
    expect(chips[4].off).toBe("no Kindle has shared its vocabulary yet");
    expect(chips[6]).toEqual({ key: "subject", label: "A subject", off: null, tag: "AI" });
    expect(themeChips(cross, { ...sources, ai: null })[6].off).toBe("needs the AI: Settings → AI");
    expect(themeChips(sudoku, sources)).toEqual([]);
    expect(themeChips(cross, null)[2].off).toBeNull();
  });
  it("lists interests, naming the paper only when there are several", () => {
    expect(interestRows(sources).map((r) => r.label)).toEqual(["Rust", "Panthers"]);
    const two = { ...sources, sources: { ...sources.sources, interests: [["A", "Rust", 1], ["B", "Dinos", 2]] as [string, string, number][] } };
    expect(interestRows(two).map((r) => r.label)).toEqual(["Rust (A)", "Dinos (B)"]);
    expect(interestRows(null)).toEqual([]);
  });
  it("explains the clue styles", () => {
    expect(CLUE_STYLES.filter((s) => s.ai).map((s) => s.key)).toEqual(["trivia", "cryptic", "kid"]);
    expect(clueLine("plain", null)).toMatch(/Wiktionary/);
    expect(clueLine("cryptic", null)).toMatch(/Settings → AI/);
    expect(clueLine("kid", "Ollama on this computer")).toMatch(/on Ollama on this computer/);
  });
});
