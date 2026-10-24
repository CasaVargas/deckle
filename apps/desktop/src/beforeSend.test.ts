// SPDX-License-Identifier: GPL-3.0-or-later
import { expect, test } from "vitest";
import { kindlePage, plannedLines } from "./beforeSend";
import type { PreflightView } from "./types";

const view = (over: Partial<PreflightView> = {}): PreflightView => ({
  typography: { quotes: 120, dashes: 3, ellipses: 1, empty_paragraphs: 0 },
  notes: 12,
  notes_skipped: 1,
  cover: "own",
  cover_url: null,
  images_downscaled: 2,
  toc_synthesized: false,
  warnings: ["no dc:language"],
  before: null,
  after: null,
  ...over,
});
const on = { typography: true, popup_notes: true, xray: true, word_wise: true, generated_cover: true };

test("what Deckle will do, line by line", () => {
  const lines = plannedLines(view(), { id: "a", cde: "ebok", write_thumbnail: false, reason: "a store-style book" }, { cdeOverride: null }, on);
  const text = Object.fromEntries(lines.map((l) => [l.what, l.text]));
  expect(text.Typography).toBe("120 curly quotes, 3 dashes and 1 ellipsis fixed");
  expect(plannedLines(view({ typography: { quotes: 0, dashes: 0, ellipses: 0, empty_paragraphs: 2 } }), undefined, { cdeOverride: null }, on)[0].text).toBe("2 empty paragraphs removed");
  expect(text["Pop-up notes"]).toBe("12 notes become pop-up notes; 1 note-like link with no way back left as it is");
  expect(text.Mode).toBe("EBOK: a store-style book");
  expect(text["X-Ray and Word Wise"]).toBe("X-Ray and Word Wise, built for this book");
  expect(text.Pictures).toBe("2 pictures made smaller for the Kindle");
  expect(lines.at(-1)).toEqual({ what: "Note", text: "no dc:language", tone: "warn" });
});

test("settings that are off say so, and a PDOC gets no X-Ray", () => {
  const lines = plannedLines(view({ cover: "none" }), undefined, { cdeOverride: "pdoc" }, { ...on, typography: false, popup_notes: false, generated_cover: false });
  const by = Object.fromEntries(lines.map((l) => [l.what, l]));
  expect(by.Typography.tone).toBe("muted");
  expect(by.Cover.tone).toBe("warn");
  expect(by.Mode.text).toBe("PDOC, as you set it");
  expect(by["X-Ray and Word Wise"].text).toMatch(/^Not on a personal document/);
});

test("the page loads nothing and can't be broken out of by the book's CSS", () => {
  const page = kindlePage("<p>Hi</p>", "p { color: red } </style><script>alert(1)</script>");
  expect(page).toContain("default-src 'none'");
  expect(page).not.toContain("</style><script>");
  expect(page).toContain("<body><p>Hi</p></body>");
});

test("preset-disabled stages explain the per-book choice", () => {
  const lines = plannedLines(view({}), undefined, { cdeOverride: "ebok" }, { ...on, typography: false, popup_notes: false, xray: false, word_wise: false }, true);
  for (const what of ["Typography", "Pop-up notes", "X-Ray and Word Wise"]) {
    const line = lines.find((l) => l.what === what)!;
    expect(line.text).toContain("style preset");
    expect(line.text).not.toContain("Settings");
  }
});
