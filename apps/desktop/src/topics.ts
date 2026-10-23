// SPDX-License-Identifier: GPL-3.0-or-later
import type { FeedConfig } from "./types";

/** Keep in sync with `Topic` in crates/deckle-digest/src/config.rs. */
export const TOPICS: { slug: string; title: string }[] = [
  { slug: "WORLD", title: "World" },
  { slug: "NATION", title: "Nation" },
  { slug: "BUSINESS", title: "Business" },
  { slug: "TECHNOLOGY", title: "Technology" },
  { slug: "ENTERTAINMENT", title: "Entertainment" },
  { slug: "SPORTS", title: "Sports" },
  { slug: "SCIENCE", title: "Science" },
  { slug: "HEALTH", title: "Health" },
];

const matches = (f: FeedConfig, slug: string) => (f.topic ?? "").toUpperCase() === slug.toUpperCase();

export function topicTitle(slug: string): string {
  return TOPICS.find((t) => t.slug === slug.toUpperCase())?.title ?? slug;
}

export function topicOn(feeds: FeedConfig[], slug: string): boolean {
  return feeds.some((f) => matches(f, slug));
}

/**
 * Tick/untick a topic. The config is saved straight to Rust without
 * `Config::parse` normalisation, so new entries carry explicit `url` and
 * `max_items`. Unticking removes only feeds whose `topic` matches.
 */
export function toggleTopic(feeds: FeedConfig[], slug: string): FeedConfig[] {
  if (topicOn(feeds, slug)) return feeds.filter((f) => !matches(f, slug));
  const title = topicTitle(slug);
  const name = feeds.some((f) => f.name === title) ? `${title} (topic)` : title;
  return [...feeds, { name, url: "", topic: slug, max_items: 5 }];
}
