// SPDX-License-Identifier: GPL-3.0-or-later
import { describe, expect, it } from "vitest";
import { PRODUCTS, STUDIO } from "./CasaVargas";

describe("the Casa Vargas card", () => {
  it("suggests one-time purchases only, never a subscription", () => {
    expect(PRODUCTS.length).toBeGreaterThan(0);
    for (const p of PRODUCTS) {
      expect(p.price).toMatch(/ once$/);
      expect(`${p.price} ${p.what} ${p.runs}`).not.toMatch(/month|year|subscri/i);
    }
  });

  it("links only to https pages", () => {
    for (const url of [STUDIO, ...PRODUCTS.map((p) => p.url)]) expect(url).toMatch(/^https:\/\//);
  });
});
