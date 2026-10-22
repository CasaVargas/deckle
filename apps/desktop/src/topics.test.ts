// SPDX-License-Identifier: GPL-3.0-or-later
import { describe, expect, test } from "vitest";
import { TOPICS, toggleTopic, topicOn } from "./topics";
import type { FeedConfig } from "./types";

const hand = (name: string, url = "https://x/feed"): FeedConfig => ({ name, url, max_items: 8 });

describe("topics picker", () => {
  test("the eight topics in spec order", () => {
    expect(TOPICS.map((t) => t.slug)).toEqual(["WORLD", "NATION", "BUSINESS", "TECHNOLOGY", "ENTERTAINMENT", "SPORTS", "SCIENCE", "HEALTH"]);
  });

  test("ticking_adds_a_topic_feed", () => {
    const out = toggleTopic([], "SPORTS");
    expect(out).toEqual([{ name: "Sports", url: "", topic: "SPORTS", max_items: 5 }]);
    expect(topicOn(out, "SPORTS")).toBe(true);
    expect(topicOn(out, "WORLD")).toBe(false);
  });

  test("unticking_removes_only_topic_feeds", () => {
    const feeds = [hand("Blog"), { name: "Sports", url: "", topic: "SPORTS", max_items: 5 }];
    const out = toggleTopic(feeds, "SPORTS");
    expect(out).toEqual([hand("Blog")]);
    expect(feeds).toHaveLength(2);
  });

  test("a_handwritten_feed_with_the_same_name_survives_unticking", () => {
    const on = toggleTopic([hand("Sports")], "SPORTS");
    const off = toggleTopic(on, "SPORTS");
    expect(off).toEqual([hand("Sports")]);
  });

  test("name_clash_gets_the_topic_suffix", () => {
    const out = toggleTopic([hand("Sports")], "SPORTS");
    expect(out[1].name).toBe("Sports (topic)");
  });

  test("url_feed_with_no_topic_is_untouched", () => {
    const feeds = [hand("A"), hand("B")];
    const out = toggleTopic(toggleTopic(feeds, "WORLD"), "WORLD");
    expect(out).toEqual(feeds);
  });

  test("case_insensitive_topic_match", () => {
    const feeds: FeedConfig[] = [{ name: "x", url: "", topic: "sports", max_items: 5 }];
    expect(topicOn(feeds, "SPORTS")).toBe(true);
    expect(toggleTopic(feeds, "SPORTS")).toEqual([]);
  });
});
