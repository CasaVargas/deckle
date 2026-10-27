// SPDX-License-Identifier: GPL-3.0-or-later
import { expect, test } from "vitest";
import { forgetTested, keepUnfinished, savable, shownStatus, subscriptionLine } from "./subscriptions";
import type { SiteStatus } from "./types";

const s: SiteStatus = {
  site: "example.com",
  username: "me",
  kind: null,
  has_password: true,
  state: "NotTried",
  jar_note: null,
};

test("subscription lines", () => {
  expect(subscriptionLine({ ...s, state: "NotTried" })).toBe("Not signed in yet");
  expect(subscriptionLine({ ...s, state: { SignedIn: { service: "Gazette Plus" } } })).toBe("Signed in · Gazette Plus, active");
  expect(subscriptionLine({ ...s, state: { SignedIn: { service: null } } })).toBe("Signed in");
  expect(subscriptionLine({ ...s, state: { LoginFailed: "bad password" } })).toBe("Sign-in failed: bad password");
  expect(subscriptionLine({ ...s, state: { EntitlementUnknown: "couldn't read your plan: x" } })).toBe("Signed in, but couldn't read your plan: x");
  expect(subscriptionLine({ ...s, site: "gazette.example", state: "NoAdapter" })).toBe("Deckle doesn't know how gazette.example signs in yet");
  expect(subscriptionLine({ ...s, state: { NoPassword: "none saved" } })).toBe("No password: none saved");
});

test("a tested failure shows until the password changes, then the fresh status", () => {
  const fresh: SiteStatus = { ...s, site: "gazette.example", has_password: true, state: "NotTried" };
  let tested = { "gazette.example": { NoPassword: "none saved" } };
  expect(subscriptionLine(shownStatus([fresh], tested, "Gazette.example")!)).toBe("No password: none saved");
  tested = forgetTested(tested, "https://www.gazette.example/") as typeof tested;
  expect(subscriptionLine(shownStatus([fresh], tested, "gazette.example")!)).toBe("Not signed in yet");
});

import { groupFor, removeGroup, renameGroup, setMaxItems, toggleGroup, toggleSlug } from "./subscriptions";
import type { DailyConfig } from "./types";

const cfgOf = (sections: string[], subscribed?: unknown[]) => ({ sections, subscribed }) as unknown as DailyConfig;

test("toggling a paper on adds news and slots subscribed after local", () => {
  const c = toggleGroup(cfgOf(["reading", "local", "feeds"]), "gazette.example");
  expect(c.subscribed).toEqual([{ site: "gazette.example", sections: ["news"] }]);
  expect(c.sections).toEqual(["reading", "local", "subscribed", "feeds"]);
});

test("subscribed goes at the end without local, and is not duplicated", () => {
  expect(toggleGroup(cfgOf(["reading", "feeds"]), "a.com").sections).toEqual(["reading", "feeds", "subscribed"]);
  const two = toggleGroup(toggleGroup(cfgOf(["local"]), "a.com"), "b.com");
  expect(two.sections).toEqual(["local", "subscribed"]);
  expect(two.subscribed).toHaveLength(2);
});

test("toggling off the last group removes subscribed from sections", () => {
  const on = toggleGroup(toggleGroup(cfgOf(["local"]), "a.com"), "b.com");
  const one = toggleGroup(on, "a.com");
  expect(one.sections).toEqual(["local", "subscribed"]);
  const none = toggleGroup(one, "b.com");
  expect(none.sections).toEqual(["local"]);
  expect(none.subscribed).toEqual([]);
});

test("unticking the last slug removes the group; other slugs toggle freely", () => {
  let c = toggleGroup(cfgOf(["local"]), "a.com");
  c = toggleSlug(c, "a.com", "sports");
  expect(groupFor(c, "a.com")?.sections).toEqual(["news", "sports"]);
  c = toggleSlug(c, "a.com", "news");
  expect(groupFor(c, "a.com")?.sections).toEqual(["sports"]);
  c = toggleSlug(c, "a.com", "sports");
  expect(groupFor(c, "a.com")).toBeUndefined();
  expect(c.sections).toEqual(["local"]);
});

test("groupFor matches by normalized site", () => {
  const c = cfgOf(["subscribed"], [{ site: "gazette.example", sections: ["news"] }]);
  expect(groupFor(c, "https://www.Gazette.example/")?.site).toBe("gazette.example");
  expect(groupFor(c, "other.com")).toBeUndefined();
});

test("stories per section edits max_items; empty unsets", () => {
  let c = toggleGroup(cfgOf(["local"]), "a.com");
  c = setMaxItems(c, "a.com", "6");
  expect(groupFor(c, "a.com")?.max_items).toBe(6);
  c = setMaxItems(c, "a.com", "");
  expect(groupFor(c, "a.com")).not.toHaveProperty("max_items");
});

test("removing an account drops its group and, with the last, the kind", () => {
  const two = toggleGroup(toggleGroup(cfgOf(["local"]), "a.com"), "b.com");
  const one = removeGroup(two, "A.com");
  expect(one.subscribed?.map((g) => g.site)).toEqual(["b.com"]);
  expect(one.sections).toEqual(["local", "subscribed"]);
  const none = removeGroup(one, "b.com");
  expect(none.subscribed).toEqual([]);
  expect(none.sections).toEqual(["local"]);
});

test("renaming a site moves its group", () => {
  let c = toggleSlug(toggleGroup(cfgOf(["local"]), "a.com"), "a.com", "sports");
  c = renameGroup(c, "a.com", "https://www.B.com/");
  expect(groupFor(c, "a.com")).toBeUndefined();
  expect(groupFor(c, "b.com")?.sections).toEqual(["news", "sports"]);
  expect(c.sections).toEqual(["local", "subscribed"]);
});

test("a group stays while another row still has its site", () => {
  const c = toggleSlug(toggleGroup(cfgOf(["local"]), "a.com"), "a.com", "sports");
  // two rows for a.com: removing or renaming one leaves the other's group alone
  expect(removeGroup(c, "a.com", ["https://www.A.com/"])).toBe(c);
  expect(renameGroup(c, "a.com", "b.com", ["a.com"])).toBe(c);
  // no other row has it: the group goes, or moves
  expect(removeGroup(c, "a.com", ["b.com"]).subscribed).toEqual([]);
  expect(groupFor(renameGroup(c, "a.com", "b.com", ["c.com"]), "b.com")?.sections).toEqual(["news", "sports"]);
});

test("renaming onto a site that already has a group keeps that one", () => {
  let c = toggleGroup(toggleGroup(cfgOf(["local"]), "a.com"), "b.com");
  c = toggleSlug(c, "b.com", "opinion");
  c = renameGroup(c, "a.com", "b.com");
  expect(c.subscribed).toHaveLength(1);
  expect(groupFor(c, "b.com")?.sections).toEqual(["news", "opinion"]);
});

test("an account still being set up stays out of the save and in the draft", () => {
  const cfg = {
    sections: ["local", "subscribed"],
    subscriptions: [
      { site: "a.com", username: "me" },
      { site: "gazette.example", username: "" },
      { site: "", username: "" },
      { site: "B.com", username: " you " },
    ],
  } as unknown as DailyConfig;
  expect(savable(cfg).subscriptions).toEqual([
    { site: "a.com", username: "me" },
    { site: "B.com", username: " you " },
  ]);
  // The backend normalizes what it saved; the unfinished rows go back where they were.
  const saved = { ...cfg, subscriptions: [{ site: "a.com", username: "me" }, { site: "b.com", username: "you" }] };
  expect(keepUnfinished(saved, cfg).subscriptions).toEqual([
    { site: "a.com", username: "me" },
    { site: "gazette.example", username: "" },
    { site: "", username: "" },
    { site: "b.com", username: "you" },
  ]);
  const done = { ...cfg, subscriptions: [{ site: "a.com", username: "me" }] };
  expect(savable(done)).toBe(done);
  expect(keepUnfinished(saved, done)).toBe(saved);
});
