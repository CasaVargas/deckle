// SPDX-License-Identifier: GPL-3.0-or-later
import { describe, expect, it } from "vitest";
import { writtenSummary } from "./highlightSummary";

describe("writtenSummary", () => {
  it("counts new highlights and files", () => {
    expect(writtenSummary({ written: [{ file: "Walden.md", new: 2, created: true }, { file: "Dune.md", new: 1, created: false }], failed: [] })).toBe("3 new highlights in 2 files.");
    expect(writtenSummary({ written: [{ file: "Walden.md", new: 1, created: false }], failed: [] })).toBe("1 new highlight in Walden.md.");
  });
  it("says when nothing was new", () => {
    expect(writtenSummary({ written: [], failed: [] })).toBe("Nothing new since the last time.");
  });
  it("names failures", () => {
    expect(writtenSummary({ written: [], failed: [["Walden.md", "/x/Walden.md: Permission denied"]] })).toBe("Couldn't write /x/Walden.md: Permission denied");
  });
});
