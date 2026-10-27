// SPDX-License-Identifier: GPL-3.0-or-later
import { expect, test } from "vitest";
import { rowId } from "./SectionsTab";
import type { PagePlanRow } from "./types";

const row = (p: Partial<PagePlanRow>): PagePlanRow => ({
  key: "k",
  kind: "local",
  feed: null,
  place: null,
  interest: null,
  name: "",
  detail: "",
  budget: "",
  items: null,
  ...p,
});

test("a row keeps its identity when it's renamed", () => {
  expect(rowId(row({ kind: "feeds", feed: 2, key: "feeds:Hacker News", name: "Hacker News" }))).toBe(
    rowId(row({ kind: "feeds", feed: 2, key: "feeds:HN", name: "HN" })),
  );
  expect(rowId(row({ kind: "interests", interest: 0 }))).toBe("interests:0");
  expect(rowId(row({ kind: "local", place: 1 }))).toBe("town:1");
  expect(rowId(row({ kind: "local" }))).toBe("local");
  expect(rowId(row({ kind: "subscribed", subscribed: { site: "gazette.example", slug: "sports" } }))).toBe("subscribed:gazette.example:sports");
});
