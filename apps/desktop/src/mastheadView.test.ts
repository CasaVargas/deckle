// SPDX-License-Identifier: GPL-3.0-or-later
import { describe, expect, it } from "vitest";
import { FACES, fontFileName, mastheadChoice, mastheadValue } from "./mastheadView";

describe("mastheadView", () => {
  it("reads the saved setting", () => {
    expect(mastheadChoice(undefined)).toBe("consent");
    expect(mastheadChoice("")).toBe("consent");
    expect(mastheadChoice("blackletter")).toBe("unifraktur");
    expect(mastheadChoice("Unifraktur")).toBe("unifraktur");
    expect(mastheadChoice("Serif")).toBe("serif");
    expect(mastheadChoice("bodoni")).toBe("bodoni");
    expect(mastheadChoice("/Users/me/Fonts/Fraktur.ttf")).toBe("file");
    expect(mastheadChoice("C:\\Fonts\\Fraktur.ttf")).toBe("file");
  });

  it("has ten faces, five of them blackletter, each read back as itself", () => {
    expect(FACES).toHaveLength(10);
    expect(FACES.filter((f) => f.group === "blackletter")).toHaveLength(5);
    for (const f of FACES) expect(mastheadChoice(f.key)).toBe(f.key);
  });

  it("names a file by its last part", () => {
    expect(fontFileName("/Users/me/Fonts/Fraktur.ttf")).toBe("Fraktur.ttf");
    expect(fontFileName("C:\\Fonts\\Fraktur.ttf")).toBe("Fraktur.ttf");
  });

  it("stores the default as nothing", () => {
    expect(mastheadValue("consent")).toBeUndefined();
    expect(mastheadValue("unifraktur")).toBe("unifraktur");
    expect(mastheadValue("sans")).toBe("sans");
  });
});
