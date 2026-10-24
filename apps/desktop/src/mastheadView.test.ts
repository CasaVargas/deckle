// SPDX-License-Identifier: GPL-3.0-or-later
import { describe, expect, it } from "vitest";
import { fontFileName, mastheadChoice, mastheadValue } from "./mastheadView";

describe("mastheadView", () => {
  it("reads the saved setting", () => {
    expect(mastheadChoice(undefined)).toBe("blackletter");
    expect(mastheadChoice("")).toBe("blackletter");
    expect(mastheadChoice("Serif")).toBe("serif");
    expect(mastheadChoice("sans")).toBe("sans");
    expect(mastheadChoice("/Users/me/Fonts/Fraktur.ttf")).toBe("file");
    expect(mastheadChoice("C:\\Fonts\\Fraktur.ttf")).toBe("file");
  });

  it("names a file by its last part", () => {
    expect(fontFileName("/Users/me/Fonts/Fraktur.ttf")).toBe("Fraktur.ttf");
    expect(fontFileName("C:\\Fonts\\Fraktur.ttf")).toBe("Fraktur.ttf");
  });

  it("stores the default as nothing", () => {
    expect(mastheadValue("blackletter")).toBeUndefined();
    expect(mastheadValue("sans")).toBe("sans");
  });
});
