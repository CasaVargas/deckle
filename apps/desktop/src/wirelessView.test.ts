// SPDX-License-Identifier: GPL-3.0-or-later
import { expect, test } from "vitest";
import type { CatalogEntry, WirelessJob, WirelessReader } from "./types";
import { bookLine, freeLine, jobLine, jobReader, jobsFor, pickable, seenLine } from "./wirelessView";

const reader = (over: Partial<WirelessReader> = {}): WirelessReader => ({
  id: "u-1",
  name: "KOReader (Kobo_frost)",
  kind: "Kobo_frost",
  connected: false,
  last_seen: "2026-10-30T12:00:00Z",
  free: null,
  books: [],
  ...over,
});

const job = (over: Partial<WirelessJob>): WirelessJob => ({ id: "j", reader: "u-1", added: "2026-10-31T00:00:00Z", ...over });

const entry = (title: string, authors: string[]): CatalogEntry => ({
  path: `/b/${title}.epub`,
  from: "library",
  title,
  authors,
  series: null,
  isbn: null,
  library_dir: null,
  sent_to: [],
  added: null,
  on_kindle: false,
});

test("free space and when it was seen", () => {
  expect(freeLine(null)).toBeNull();
  expect(freeLine(2_100_000_000)).toBe("2.1 GB free");
  expect(freeLine(640_000_000)).toBe("640 MB free");
  expect(seenLine(reader({ connected: true }))).toBe("Connected");
  expect(seenLine(reader())).toMatch(/^Last seen /);
  expect(seenLine(reader({ last_seen: "" }))).toBe("Not connected");
});

test("jobs name their reader and what they do", () => {
  const rs = [reader()];
  expect(jobReader(job({ reader: "any" }), rs)).toBe("the next reader to connect");
  expect(jobReader(job({ reader: "u-1" }), rs)).toBe("KOReader (Kobo_frost)");
  expect(jobReader(job({ reader: "koreader (kobo_frost)" }), rs)).toBe("KOReader (Kobo_frost)");
  expect(jobReader(job({ reader: "Elsewhere" }), rs)).toBe("Elsewhere");
  expect(jobLine(job({ send: "/Users/me/Books/Dune.epub" }))).toBe("Send Dune.epub");
  expect(jobLine(job({ send: "C:\\Books\\Dune.epub" }))).toBe("Send Dune.epub");
  expect(jobLine(job({ remove: "A/B - A.epub" }))).toBe("Take off A/B - A.epub");
  const q = [job({ id: "1", reader: "u-1" }), job({ id: "2", reader: "any" }), job({ id: "3", reader: "other" })];
  expect(jobsFor(reader(), q).map((j) => j.id)).toEqual(["1", "2"]);
});

test("the picker matches title or author and stops at the limit", () => {
  const all = [entry("Dune", ["Frank Herbert"]), entry("Emma", ["Jane Austen"]), entry("Persuasion", ["Jane Austen"])];
  expect(pickable(all, "").length).toBe(3);
  expect(pickable(all, "austen").map((e) => e.title)).toEqual(["Emma", "Persuasion"]);
  expect(pickable(all, "dun").map((e) => e.title)).toEqual(["Dune"]);
  expect(pickable(all, "", 2).length).toBe(2);
  expect(bookLine({ lpath: "x.epub", title: null, authors: [] })).toBe("x.epub");
  expect(bookLine({ lpath: "x.epub", title: "Dune", authors: ["Frank Herbert"] })).toBe("Dune — Frank Herbert");
});
