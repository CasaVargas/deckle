// SPDX-License-Identifier: GPL-3.0-or-later
import { expect, test } from "vitest";
import { addInterest, listText, removeInterest, setList } from "./interests";
import type { DailyConfig } from "./types";

const base = (over: Partial<DailyConfig>): DailyConfig => ({
  title: "T",
  place: "",
  latitude: null,
  longitude: null,
  units: "imperial",
  keep: 3,
  max_age_hours: 36,
  full_text: true,
  photos: true,
  max_photos: 4,
  sections: ["local", "feeds"],
  local: { google_news: "", feeds: [], max_items: 12, max_age_hours: 72 },
  feeds: [],
  lottery: { games: [] },
  horoscope: { signs: [] },
  reading: { inbox: "", max_items: 10, long_read_words: 4000 },
  ...over,
});

test("addInterest adds the kind before feeds once and ignores duplicates", () => {
  const cfg = base({ sections: ["local", "feeds"] });
  const a = addInterest(cfg, " Rust ");
  expect(a.interests).toEqual([{ name: "Rust", max_items: 5 }]);
  expect(a.sections).toEqual(["local", "interests", "feeds"]);
  expect(addInterest(a, "rust")).toBe(a);
  expect(addInterest(a, "  ")).toBe(a);
  expect(cfg.interests).toBeUndefined();
  const b = addInterest(base({ sections: ["horoscope"] }), "Cargo");
  expect(b.sections).toEqual(["horoscope", "interests"]);
  const c = addInterest(a, "Cargo");
  expect(c.interests!.map((i) => i.name)).toEqual(["Rust", "Cargo"]);
  expect(c.sections).toEqual(["local", "interests", "feeds"]);
});

test("removeInterest drops one by index", () => {
  const cfg = addInterest(addInterest(base({}), "Rust"), "Cargo");
  expect(removeInterest(cfg, 0).interests!.map((i) => i.name)).toEqual(["Cargo"]);
  expect(removeInterest(cfg, 5)).toBe(cfg);
});

test("setList parses comma lists and drops an emptied key", () => {
  const cfg = addInterest(base({}), "Rust");
  const w = setList(cfg, 0, "not", " Rust Belt, corrosion ,, ");
  expect(w.interests![0].not).toEqual(["Rust Belt", "corrosion"]);
  expect(listText(w.interests![0].not)).toBe("Rust Belt, corrosion");
  expect(listText(undefined)).toBe("");
  expect(setList(w, 0, "not", "").interests![0].not).toBeUndefined();
  expect(setList(w, 0, "also", "Cargo").interests![0].also).toEqual(["Cargo"]);
  expect(setList(w, 3, "also", "x")).toBe(w);
});
