// SPDX-License-Identifier: GPL-3.0-or-later
import { expect, test } from "vitest";
import type { WritebackChange, WritebackStatus } from "./types";
import { blocker, changeLine, noColumns, shown } from "./writebackView";

const change = (over: Partial<WritebackChange>): WritebackChange => ({
  uuid: "u",
  book_id: 1,
  title: "Dune",
  field: "progress",
  column: "progress",
  old: null,
  new: "62",
  ...over,
});

test("values read as calibre shows them", () => {
  expect(shown("progress", "62")).toBe("62%");
  expect(shown("read", "true")).toBe("Yes");
  expect(shown("read", "false")).toBe("No");
  expect(shown("rating", "8")).toBe("★★★★");
  expect(shown("rating", null)).toBe("empty");
  expect(shown("last_read", "2026-10-30T09:15:00Z")).toMatch(/2026/);
});

test("a change says the book, the field, and before and after", () => {
  expect(changeLine(change({ old: "30" }))).toBe("Dune: Progress 30% → 62%");
  expect(changeLine(change({ field: "rating", column: "stars", new: "8" }))).toBe("Dune: Stars empty → ★★★★");
  expect(changeLine(change({ field: "rating", column: "n", new: "4" }), false)).toBe("Dune: Stars empty → ★★★★");
});

test("what stops a write", () => {
  const cols = { progress: null, read: null, last_read: null, rating: null };
  const s = (over: Partial<WritebackStatus>): WritebackStatus => ({
    library: "/c",
    calibredb: "/bin/calibredb",
    writeback: { enabled: false, calibredb: null, server: null, username: null, columns: cols, confirmed: {}, last_run: null, last_result: [] },
    choices: {},
    plan: null,
    error: null,
    ...over,
  });
  expect(noColumns(cols)).toBe(true);
  expect(blocker(s({ library: null }))).toMatch(/No calibre library/);
  expect(blocker(s({ calibredb: null }))).toMatch(/calibredb/);
  expect(blocker(s({}))).toMatch(/Choose a column/);
  expect(blocker(s({ writeback: { ...s({}).writeback, columns: { ...cols, read: "read" } } }))).toBeNull();
});
