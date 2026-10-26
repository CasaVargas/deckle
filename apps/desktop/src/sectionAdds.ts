// SPDX-License-Identifier: GPL-3.0-or-later
// + Add a section (Daily tab redesign §5): each choice as a pure edit of the config, with the id
// of the row it makes, so Sections can open it once the plan catches up.
import { addInterest } from "./interests";
import { statusKey, toggleGroup } from "./subscriptions";
import { TOPICS, toggleTopic, topicOn } from "./topics";
import type { DailyConfig, SectionKind } from "./types";

/** The back pages: sections that are one thing, switched on or off. */
export const BACK_PAGES: { kind: SectionKind; title: string; what: string }[] = [
  { kind: "weather", title: "Weather", what: "The forecast, the almanac and Your Day" },
  { kind: "shelf", title: "From Your Shelf", what: "A passage from a book on your Kindle" },
  { kind: "puzzle", title: "Puzzle", what: "A mini crossword from yesterday's headlines" },
  { kind: "lottery", title: "Lottery", what: "Last night's numbers" },
  { kind: "horoscope", title: "Horoscope", what: "Your signs, or all twelve" },
  { kind: "onthisday", title: "On This Day", what: "From Wikipedia" },
];

/** What an add made: the new config and the id of its row (as `rowId` names it). */
export interface Added {
  cfg: DailyConfig;
  row: string;
}

/** `kind` in `sections`, at the end, when it isn't there yet. */
export function withKind(cfg: DailyConfig, kind: SectionKind): DailyConfig {
  return cfg.sections.includes(kind) ? cfg : { ...cfg, sections: [...cfg.sections, kind] };
}

/** Something to follow, by name. Null when the name is blank or already followed. */
export function addFollow(cfg: DailyConfig, name: string): Added | null {
  const next = addInterest(cfg, name);
  if (next === cfg) return null;
  return { cfg: next, row: `interests:${(next.interests ?? []).length - 1}` };
}

/** The topics not in the paper yet. */
export function topicsLeft(cfg: DailyConfig): { slug: string; title: string }[] {
  return TOPICS.filter((t) => !topicOn(cfg.feeds, t.slug));
}

export function addTopic(cfg: DailyConfig, slug: string): Added | null {
  if (topicOn(cfg.feeds, slug)) return null;
  const feeds = toggleTopic(cfg.feeds, slug);
  return { cfg: withKind({ ...cfg, feeds }, "feeds"), row: `feeds:${feeds.length - 1}` };
}

/** A website's feed. The address is needed; the name defaults to the site's host. */
export function addFeed(cfg: DailyConfig, name: string, url: string): Added | null {
  const u = url.trim();
  if (!u) return null;
  const feeds = [...cfg.feeds, { name: name.trim() || hostOf(u), url: u, max_items: 8 }];
  return { cfg: withKind({ ...cfg, feeds }, "feeds"), row: `feeds:${feeds.length - 1}` };
}

/** Another town, after Local; its search starts as its name. */
export function addTown(cfg: DailyConfig, name: string): Added | null {
  const n = name.trim();
  if (!n) return null;
  const places = [...(cfg.local.places ?? []), { name: n, google_news: n, feeds: [] }];
  return { cfg: withKind({ ...cfg, local: { ...cfg.local, places } }, "local"), row: `town:${places.length - 1}` };
}

/** A paper you subscribe to, by its site: its account to fill in, and its News in the paper. */
export function addSubscription(cfg: DailyConfig, site: string): Added | null {
  const key = statusKey(site.trim());
  if (!key) return null;
  const subs = cfg.subscriptions ?? [];
  const withAccount = subs.some((s) => statusKey(s.site) === key) ? cfg : { ...cfg, subscriptions: [...subs, { site: key, username: "" }] };
  const grouped = (cfg.subscribed ?? []).some((g) => statusKey(g.site) === key) ? withAccount : toggleGroup(withAccount, key);
  return { cfg: withKind(grouped, "subscribed"), row: `subscribed:${key}:news` };
}

/** The back pages that are off, to switch on. */
export function backPagesOff(cfg: DailyConfig): typeof BACK_PAGES {
  return BACK_PAGES.filter((b) => !cfg.sections.includes(b.kind));
}

export function addBackPage(cfg: DailyConfig, kind: SectionKind): Added | null {
  if (cfg.sections.includes(kind)) return null;
  return { cfg: withKind(cfg, kind), row: kind };
}

function hostOf(url: string): string {
  try {
    return new URL(url).host.replace(/^www\./, "");
  } catch {
    return url;
  }
}
