// SPDX-License-Identifier: GPL-3.0-or-later
import { describe, expect, test } from "vitest";
import { countdown, pressLine, reorder, setBudget, step, weekLabel } from "./pagePlan";
import type { DailyConfig, PagePlanRow, Press, SectionKind } from "./types";

const at = "2026-10-22T15:08:00-04:00";
const nowMs = Date.parse(at);

describe("press copy", () => {
  test("countdown", () => {
    const later = (s: number) => new Date(nowMs + s * 1000).toISOString();
    expect(countdown(later(14 * 3600 + 22 * 60 + 59), nowMs)).toBe("in 14 h 22 m");
    expect(countdown(later(38 * 60), nowMs)).toBe("in 38 m");
    expect(countdown(later(-1), nowMs)).toBe("now");
  });

  const press = (p: Partial<Press>): Press => ({
    at: "05:30",
    next: "2026-10-23T05:30:00-04:00",
    edition: "2026-10-23",
    stopped: null,
    since: "2026-10-22T05:30:00-04:00",
    ...p,
  });

  test("the three bar states", () => {
    expect(pressLine(press({}), nowMs)).toBe("Goes to press at 05:30 · in 14 h 22 m");
    expect(
      pressLine(press({ stopped: "2026-10-23", next: "2026-10-24T05:30:00-04:00", edition: "2026-10-24" }), nowMs),
    ).toBe("Presses stopped: no paper Friday · next at 05:30 Saturday");
    expect(pressLine(press({ at: null, next: null }), nowMs)).toBe("No press time set");
  });
});

const cfg = (): DailyConfig => ({
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
  sections: ["reading", "weather", "local", "feeds", "horoscope"],
  local: {
    google_news: "",
    feeds: [],
    max_items: 12,
    max_age_hours: 72,
    places: [{ name: "Asheville, NC", google_news: "Asheville NC", feeds: [] }],
  },
  feeds: [
    { name: "A", url: "https://a.example/feed", max_items: 10 },
    { name: "B", url: "https://b.example/feed", max_items: 10 },
  ],
  lottery: { games: [] },
  horoscope: { signs: [] },
  reading: { inbox: "", max_items: 10, long_read_words: 4000 },
});

const row = (
  key: string,
  kind: SectionKind,
  feed: number | null = null,
  items: number | null = null,
  place: number | null = null,
  interest: number | null = null,
): PagePlanRow => ({
  key,
  kind,
  feed,
  place,
  interest,
  name: key.replace(/^(feeds|interests):/, ""),
  detail: "",
  budget: "",
  items,
});

const rows = (): PagePlanRow[] => [
  row("reading", "reading", null, 10),
  row("weather", "weather"),
  row("local", "local", null, 12),
  row("feeds:A", "feeds", 0, 10),
  row("feeds:B", "feeds", 1, 10),
  row("horoscope", "horoscope"),
];

const subRows = (): PagePlanRow[] => [
  row("reading", "reading", null, 10),
  row("feeds:A", "feeds", 0, 10),
  { ...row("subscribed:p.com:news", "subscribed", null, 12), subscribed: { site: "p.com", slug: "news" } },
  { ...row("subscribed:p.com:sports", "subscribed", null, 12), subscribed: { site: "p.com", slug: "sports" } },
  row("horoscope", "horoscope"),
];
const subCfg = (): DailyConfig => ({
  ...cfg(),
  sections: ["reading", "feeds", "subscribed", "horoscope"],
  feeds: [cfg().feeds[0]],
  subscribed: [{ site: "p.com", sections: ["news", "sports"] }],
});

describe("subscribed papers", () => {
  test("a budget lands on the group's max_items", () => {
    const r = subRows();
    const c = setBudget(subCfg(), r[3], 7);
    expect(c.subscribed?.[0].max_items).toBe(7);
    expect(setBudget(subCfg(), r[3], 99).subscribed?.[0].max_items).toBe(20);
  });

  test("the rows move as one block", () => {
    const c = reorder(subCfg(), subRows(), 3, 0);
    expect(c.sections).toEqual(["subscribed", "reading", "feeds", "horoscope"]);
    const d = reorder(subCfg(), subRows(), 2, 4);
    expect(d.sections).toEqual(["reading", "feeds", "horoscope", "subscribed"]);
    // a drop on its own rows changes nothing
    expect(reorder(subCfg(), subRows(), 3, 2).sections).toEqual(subCfg().sections);
  });

  test("a section dropped inside the block snaps to its edge; step skips the block", () => {
    const c = reorder(subCfg(), subRows(), 4, 3);
    expect(c.sections).toEqual(["reading", "feeds", "subscribed", "horoscope"]);
    expect(step(subRows(), 2, 1)).toBe(4);
    expect(step(subRows(), 3, -1)).toBe(1);
  });

  test("a row for a paper no longer in the config is left alone", () => {
    const c = { ...subCfg(), subscribed: [] };
    expect(setBudget(c, subRows()[2], 3)).toBe(c);
  });
});

const names = (c: DailyConfig) => c.feeds.map((f) => f.name);

describe("reorder", () => {
  test("a section moves within sections", () => {
    const c = reorder(cfg(), rows(), 5, 1);
    expect(c.sections).toEqual(["reading", "horoscope", "weather", "local", "feeds"]);
    expect(names(c)).toEqual(["A", "B"]);
  });

  test("a section dropped on a feed snaps out of the block", () => {
    expect(reorder(cfg(), rows(), 1, 4).sections).toEqual(["reading", "local", "feeds", "weather", "horoscope"]);
    // between A and B: to the nearest end, never splitting the feeds
    const c = reorder(cfg(), rows(), 5, 4);
    expect(c.sections.filter((s) => s === "feeds")).toHaveLength(1);
    expect(names(c)).toEqual(["A", "B"]);
  });

  test("a feed moves within [[feeds]]", () => {
    const c = reorder(cfg(), rows(), 4, 3);
    expect(names(c)).toEqual(["B", "A"]);
    expect(c.sections).toEqual(cfg().sections);
  });

  test("a feed dropped outside the block snaps to its nearest end", () => {
    const c = reorder(cfg(), rows(), 3, 0);
    expect(names(c)).toEqual(["A", "B"]);
    expect(c.sections).toEqual(cfg().sections);
  });

  test("two feeds with the same name are both kept", () => {
    const base = cfg();
    base.feeds[1].name = "A";
    const r = rows();
    r[4].name = "A";
    const c = reorder(base, r, 4, 3);
    expect(c.feeds.map((f) => f.url)).toEqual(["https://b.example/feed", "https://a.example/feed"]);
  });

  test("the original config is left alone", () => {
    const base = cfg();
    reorder(base, rows(), 5, 1);
    expect(base.sections).toEqual(["reading", "weather", "local", "feeds", "horoscope"]);
  });
});

describe("interest rows", () => {
  const withInterests = (): DailyConfig => ({
    ...cfg(),
    sections: ["local", "interests", "feeds", "horoscope"],
    interests: [
      { name: "Rust", max_items: 5 },
      { name: "Cargo", max_items: 5 },
    ],
  });
  const irows = (): PagePlanRow[] => [
    row("local", "local", null, 12),
    row("interests:Rust", "interests", null, 5, null, 0),
    row("interests:Cargo", "interests", null, 5, null, 1),
    row("feeds:A", "feeds", 0, 10),
    row("feeds:B", "feeds", 1, 10),
    row("horoscope", "horoscope"),
  ];
  const inames = (c: DailyConfig) => (c.interests ?? []).map((i) => i.name);

  test("setBudget edits an interest's max_items", () => {
    const c = setBudget(withInterests(), irows()[2], 7);
    expect(c.interests![1].max_items).toBe(7);
    expect(c.interests![0].max_items).toBe(5);
  });

  test("an interest moves within [[interests]]", () => {
    const c = reorder(withInterests(), irows(), 2, 1);
    expect(inames(c)).toEqual(["Cargo", "Rust"]);
    expect(names(c)).toEqual(["A", "B"]);
    expect(c.sections).toEqual(withInterests().sections);
  });

  test("a section dropped between interests snaps to the block's nearest end", () => {
    const c = reorder(withInterests(), irows(), 5, 2);
    expect(c.sections.filter((s) => s === "interests")).toHaveLength(1);
    expect(inames(c)).toEqual(["Rust", "Cargo"]);
    expect(c.sections.indexOf("horoscope")).toBeLessThan(c.sections.indexOf("feeds"));
  });

  test("an interest dropped outside its block snaps back to it", () => {
    const c = reorder(withInterests(), irows(), 1, 5);
    expect(inames(c)).toEqual(["Cargo", "Rust"]);
    expect(c.sections).toEqual(withInterests().sections);
  });

  test("rows naming interests the config no longer has change nothing", () => {
    const c = withInterests();
    c.interests = [{ name: "Other", max_items: 5 }];
    expect(reorder(c, irows(), 2, 1)).toBe(c);
  });
});

describe("budgets", () => {
  test("each lands on its own key, clamped to 1–20", () => {
    const r = rows();
    expect(setBudget(cfg(), r[0], 4).reading.max_items).toBe(4);
    expect(setBudget(cfg(), r[2], 0).local.max_items).toBe(1);
    expect(setBudget(cfg(), r[4], 25).feeds[1].max_items).toBe(20);
    expect(setBudget(cfg(), r[4], 25).feeds[0].max_items).toBe(10);
  });
});

test("week labels", () => {
  expect(weekLabel({ date: "2026-10-17", state: { state: "read", percent: 90 } })).toBe("read 90%");
  expect(weekLabel({ date: "2026-10-18", state: { state: "notOpened" } })).toBe("not opened");
  expect(weekLabel({ date: "2026-10-19", state: { state: "sent" } })).toBe("sent");
  expect(weekLabel({ date: "2026-10-20", state: { state: "noPaper" } })).toBe("no paper");
});

describe("a plan out of step with the config", () => {
  test("rows naming feeds the config no longer has change nothing", () => {
    const base = cfg();
    base.feeds = [base.feeds[1]]; // A removed, not saved yet
    expect(reorder(base, rows(), 5, 1)).toBe(base);
    expect(setBudget(base, rows()[4], 3)).toBe(base);
  });
});


describe("other places' rows", () => {
  const withPlace = (): PagePlanRow[] => {
    const r = rows();
    r.splice(3, 0, { ...row("local:Asheville, NC", "local", null, 12, 0), name: "Asheville, NC" });
    return r; // reading, weather, local, Asheville, A, B, horoscope
  };

  test("a place's budget lands on the place", () => {
    const c = setBudget(cfg(), withPlace()[3], 4);
    expect(c.local.places?.[0].max_items).toBe(4);
    expect(c.local.max_items).toBe(12);
  });

  test("a place row isn't dragged on its own", () => {
    const base = cfg();
    expect(reorder(base, withPlace(), 3, 0)).toBe(base);
  });

  test("the arrows step over places", () => {
    const r = withPlace(); // reading, weather, local, Asheville, A, B, horoscope
    expect(step(r, 2, 1)).toBe(4); // Local down: past its places, below A
    expect(reorder(cfg(), r, 2, step(r, 2, 1)!).sections).toEqual(["reading", "weather", "feeds", "local", "horoscope"]);
    expect(step(r, 4, -1)).toBe(2); // A up: above Local
    expect(step(r, 0, -1)).toBeNull();
    expect(step(r, 6, 1)).toBeNull();
  });

  test("places follow Local, and dropping onto one means Local's spot", () => {
    const c = reorder(cfg(), withPlace(), 6, 3); // horoscope onto Asheville
    expect(c.sections).toEqual(["reading", "weather", "horoscope", "local", "feeds"]);
    const d = reorder(cfg(), withPlace(), 2, 0); // Local to the top
    expect(d.sections).toEqual(["local", "reading", "weather", "feeds", "horoscope"]);
    expect(d.local.places).toEqual(cfg().local.places);
  });
});
