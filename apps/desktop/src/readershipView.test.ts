// SPDX-License-Identifier: GPL-3.0-or-later
import { describe, expect, test } from "vitest";
import { barTone, headline, howFar } from "./readershipView";
import type { ReadershipReport } from "./types";

const report = (r: Partial<ReadershipReport>): ReadershipReport => ({
  window: 14,
  opened: 0,
  medianPercent: null,
  stopSection: null,
  sections: [],
  suggestions: [],
  ...r,
});

describe("readership view", () => {
  test("bars under 30% are muted, missing shares have none", () => {
    expect(barTone(64)).toBe("strong");
    expect(barTone(30)).toBe("strong");
    expect(barTone(21)).toBe("muted");
    expect(barTone(null)).toBe("none");
  });

  test("headline", () => {
    expect(headline(report({ opened: 12, window: 14 }))).toBe("Opened 12 of 14");
  });

  test("how far in", () => {
    expect(howFar(report({ medianPercent: 55 }))).toBe("About 55% in");
    expect(howFar(report({}))).toBe("—");
  });
});
