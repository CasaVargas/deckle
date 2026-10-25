// SPDX-License-Identifier: GPL-3.0-or-later
import { describe, expect, it } from "vitest";
import { deviceLabel } from "./deviceLabel";

describe("deviceLabel", () => {
  it("leads with the owner's name", () => {
    expect(deviceLabel({ name: "Mom's Kindle", model_hint: "Kindle Paperwhite" })).toBe("Mom's Kindle — Kindle Paperwhite");
  });
  it("falls back to the model", () => {
    expect(deviceLabel({ name: null, model_hint: "Kindle Scribe" })).toBe("Kindle Scribe");
    expect(deviceLabel({ name: "  ", model_hint: "Kindle Scribe" })).toBe("Kindle Scribe");
  });
});
