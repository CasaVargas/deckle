// SPDX-License-Identifier: GPL-3.0-or-later
import { expect, test } from "vitest";
import { addBackPage, addFeed, addFollow, addSubscription, addTopic, addTown, backPagesOff, topicsLeft } from "./sectionAdds";
import type { DailyConfig } from "./types";

const cfg = (p: Partial<DailyConfig> = {}): DailyConfig => ({
  title: "T",
  place: "Carthage, NC",
  latitude: null,
  longitude: null,
  units: "imperial",
  keep: 3,
  max_age_hours: 36,
  full_text: true,
  photos: true,
  max_photos: 4,
  sections: ["reading", "weather", "local"],
  local: { google_news: "", feeds: [], max_items: 12, max_age_hours: 72 },
  feeds: [],
  lottery: { games: [] },
  horoscope: { signs: [] },
  reading: { inbox: "", max_items: 10, long_read_words: 4000 },
  ...p,
});

test("something to follow becomes an interest, and its row is named", () => {
  const a = addFollow(cfg(), "  Carolina Panthers ");
  expect(a?.cfg.interests).toEqual([{ name: "Carolina Panthers", max_items: 5 }]);
  expect(a?.cfg.sections).toContain("interests");
  expect(a?.row).toBe("interests:0");
  expect(addFollow(cfg(), "  ")).toBeNull();
  expect(addFollow(a!.cfg, "carolina panthers")).toBeNull();
});

test("a topic is added once, and the paper gets its feeds", () => {
  const a = addTopic(cfg(), "SPORTS");
  expect(a?.cfg.feeds).toEqual([{ name: "Sports", url: "", topic: "SPORTS", max_items: 5 }]);
  expect(a?.cfg.sections).toContain("feeds");
  expect(a?.row).toBe("feeds:0");
  expect(addTopic(a!.cfg, "SPORTS")).toBeNull();
  expect(topicsLeft(a!.cfg).map((t) => t.slug)).not.toContain("SPORTS");
  expect(topicsLeft(cfg())).toHaveLength(8);
});

test("a feed needs its address, and takes the site's name when it has none", () => {
  expect(addFeed(cfg(), "Blog", " ")).toBeNull();
  const a = addFeed(cfg({ feeds: [{ name: "HN", url: "https://hnrss.org/frontpage", max_items: 10 }] }), "", "https://www.example.com/feed.xml");
  expect(a?.cfg.feeds[1]).toEqual({ name: "example.com", url: "https://www.example.com/feed.xml", max_items: 8 });
  expect(a?.row).toBe("feeds:1");
});

test("another town needs only its name: Google News is searched for it", () => {
  const a = addTown(cfg(), " Asheville, NC ");
  expect(a?.cfg.local.places).toEqual([{ name: "Asheville, NC", google_news: "", feeds: [] }]);
  expect(a?.row).toBe("town:0");
  expect(addTown(cfg(), "")).toBeNull();
});

test("a subscribed paper gets an account to fill in and its News in the paper, after Local", () => {
  const a = addSubscription(cfg({ sections: ["reading", "weather", "local", "lottery"] }), "https://www.ThePilot.com/");
  expect(a?.cfg.subscriptions).toEqual([{ site: "thepilot.com", username: "" }]);
  expect(a?.cfg.subscribed).toEqual([{ site: "thepilot.com", sections: ["news"] }]);
  expect(a?.cfg.sections).toEqual(["reading", "weather", "local", "subscribed", "lottery"]);
  expect(a?.row).toBe("subscribed:thepilot.com:news");
  // An account that's already there isn't added twice
  expect(addSubscription(a!.cfg, "thepilot.com")?.cfg.subscriptions).toHaveLength(1);
  expect(addSubscription(cfg(), "  ")).toBeNull();
});

test("the back pages that are off can be switched on", () => {
  expect(backPagesOff(cfg()).map((b) => b.kind)).toEqual(["shelf", "puzzle", "lottery", "horoscope", "onthisday"]);
  const a = addBackPage(cfg(), "puzzle");
  expect(a?.cfg.sections).toEqual(["reading", "weather", "local", "puzzle"]);
  expect(a?.row).toBe("puzzle");
  expect(addBackPage(a!.cfg, "puzzle")).toBeNull();
});
