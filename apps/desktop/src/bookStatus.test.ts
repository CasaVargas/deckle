// SPDX-License-Identifier: GPL-3.0-or-later
import { expect, test } from "vitest";
import { beforeChip, doneChips, runSummary, statusLine } from "./bookStatus";
import type { BookState, Mode, PlanRow, WordWiseSummary } from "./types";

const on = { xray: true, word_wise: true };
const ebok = (thumb: boolean): PlanRow => ({ id: "a", cde: "ebok", write_thumbnail: thumb, reason: "r" });
const pdoc: PlanRow = { id: "a", cde: "pdoc", write_thumbnail: false, reason: "r" };
const b = (mode: Mode) => ({ mode, reason: "why" });
const ww: WordWiseSummary = { hints: 3210, level: 5, dictionary: "device", downloaded: false };

function done(overrides: Partial<BookState> = {}): BookState {
  return {
    id: "a", path: "/b.epub", title: "T", authors: "A", series: null, cover: null, mode: "full", reason: "", warnings: [], error: null,
    status: "done", stage: null, delivered: "/k/b.kfx", pages: 496, runWarnings: [], runError: null, shipped: null, xray: null, wordWise: null,
    cdeOverride: null, asin: null, showReport: false, replaced: [], exported: false, upgradeOf: null, coverPick: null,
    ...overrides,
  };
}

test("error", () => expect(beforeChip(b("error"), undefined, on)).toEqual({ text: "Can't convert", tone: "fail", title: "why" }));
test("ready", () => expect(beforeChip(b("ready"), undefined, on).text).toBe("Sent as is"));
test("no plan yet", () => expect(beforeChip(b("full"), undefined, on).text).toBe("Keeps cover"));
test("pdoc", () => expect(beforeChip(b("full"), pdoc, on)).toMatchObject({ text: "Keeps cover", title: "r" }));
test("ebok both", () => expect(beforeChip(b("full"), ebok(false), on).text).toBe("X-Ray + Word Wise"));
test("ebok xray only", () => expect(beforeChip(b("full"), ebok(false), { xray: true, word_wise: false }).text).toBe("X-Ray"));
test("ebok word wise only", () => expect(beforeChip(b("full"), ebok(false), { xray: false, word_wise: true }).text).toBe("Word Wise"));
test("ebok neither", () => expect(beforeChip(b("full"), ebok(false), { xray: false, word_wise: false }).text).toBe("Kindle book"));
test("ebok + thumbnail", () => expect(beforeChip(b("full"), ebok(true), on).text).toBe("X-Ray + Word Wise + cover"));
test("ebok neither + thumbnail", () => expect(beforeChip(b("full"), ebok(true), { xray: false, word_wise: false }).text).toBe("Kindle book + cover"));

test("sent", () => expect(doneChips(done()).map((c) => c.text)).toEqual(["Sent · 496 pages"]));
test("exported", () => expect(doneChips(done({ exported: true }))[0].text).toBe("Exported · 496 pages"));
test("ready sent", () => expect(doneChips(done({ mode: "ready", pages: null }))[0].text).toBe("Sent as is"));
test("features and warnings", () =>
  expect(doneChips(done({ xray: { people: 1, terms: 1, source: "basic" }, wordWise: ww, runWarnings: ["a", "b"] })).map((c) => [c.text, c.tone])).toEqual([
    ["Sent · 496 pages", "ok"],
    ["X-Ray", "plain"],
    ["Word Wise", "plain"],
    ["2 warnings", "warn"],
  ]));
test("one warning", () => expect(doneChips(done({ runWarnings: ["a"] })).at(-1)!.text).toBe("1 warning"));

test("run summary: sent and failed", () =>
  expect(runSummary([done(), done(), done({ status: "failed", runError: "x" }), done({ status: "idle" })])).toBe("Done: 2 sent, 1 failed"));
test("run summary: exported", () => expect(runSummary([done({ exported: true })])).toBe("Done: 1 exported"));
test("run summary: nothing ran", () => expect(runSummary([done({ status: "idle" })])).toBeNull());

test("line: nothing", () => expect(statusLine([])).toBeNull());
test("line: one plain chip", () => expect(statusLine([{ text: "Keeps cover", tone: "plain", title: "r" }])).toEqual({ text: "Keeps cover", tone: "plain", title: "r" }));
test("line: joined, in the worst tone", () =>
  expect(
    statusLine([
      { text: "Sent · 496 pages", tone: "ok" },
      { text: "X-Ray", tone: "plain" },
      { text: "2 warnings", tone: "warn", title: "a\nb" },
    ]),
  ).toEqual({ text: "Sent · 496 pages · X-Ray · 2 warnings", tone: "warn", title: "a\nb" }));
test("line: a failure wins", () => expect(statusLine([{ text: "1 note", tone: "warn" }, { text: "Can't convert", tone: "fail" }])?.tone).toBe("fail"));
