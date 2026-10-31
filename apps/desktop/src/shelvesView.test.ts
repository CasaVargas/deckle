// SPDX-License-Identifier: GPL-3.0-or-later
import { expect, test } from "vitest";
import { diffLine, makeRule, needsYes, ruleLine, targetName } from "./shelvesView";

test("rules read as sentences", () => {
  expect(ruleLine({ name: "Sci-Fi", from: "tag:Sci-Fi", readers: ["all"] })).toBe("Tag Sci-Fi → Sci-Fi");
  expect(ruleLine({ name: "", from: "series:*", readers: ["all"] })).toBe("Each series, a shelf of its own");
  expect(ruleLine({ name: "Bed", from: "column:#shelf=Bedside", readers: ["kobo/N1"] })).toBe("Column #shelf=Bedside → Bed (only kobo/N1)");
});

test("the add form makes a rule or says why not", () => {
  expect(makeRule("tag", " Sci-Fi ", "Science fiction")).toEqual({ name: "Science fiction", from: "tag:Sci-Fi", readers: ["all"] });
  expect(makeRule("series", "*", "")).toEqual({ name: "", from: "series:*", readers: ["all"] });
  expect(makeRule("tag", "", "x")).toBe("Give a value.");
  expect(makeRule("tag", "x", "")).toBe("Give the shelf a name.");
  expect(makeRule("column", "#shelf", "x")).toBe("A column rule is #label=value.");
});

test("a diff in a line, and the Kobo's yes", () => {
  const d = { create: ["A", "B"], delete: [], add: { A: ["x", "y"], B: ["z"] }, remove: { C: ["w"] } };
  expect(diffLine(d)).toBe("2 new shelves, 3 books added, 1 taken off");
  expect(diffLine({ create: [], delete: [], add: {}, remove: {} })).toBe("Up to date");
  const t = { kind: "kobo" as const, key: "kobo/N1", diff: d, applied: false, note: null };
  expect(targetName(t)).toBe("Kobo shelves");
  expect(needsYes(t, [])).toBe(true);
  expect(needsYes(t, ["kobo/N1"])).toBe(false);
  expect(needsYes({ ...t, kind: "koreader" }, [])).toBe(false);
});
