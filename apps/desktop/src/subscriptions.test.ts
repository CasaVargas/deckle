// SPDX-License-Identifier: GPL-3.0-or-later
import { expect, test } from "vitest";
import { forgetTested, shownStatus, subscriptionLine } from "./subscriptions";
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
  expect(subscriptionLine({ ...s, state: { SignedIn: { service: "Pilot Plus" } } })).toBe("Signed in · Pilot Plus, active");
  expect(subscriptionLine({ ...s, state: { SignedIn: { service: null } } })).toBe("Signed in");
  expect(subscriptionLine({ ...s, state: { LoginFailed: "bad password" } })).toBe("Sign-in failed: bad password");
  expect(subscriptionLine({ ...s, state: { EntitlementUnknown: "couldn't read your plan: x" } })).toBe("Signed in, but couldn't read your plan: x");
  expect(subscriptionLine({ ...s, site: "thepilot.com", state: "NoAdapter" })).toBe("Deckle doesn't know how thepilot.com signs in yet");
  expect(subscriptionLine({ ...s, state: { NoPassword: "none saved" } })).toBe("No password: none saved");
});

test("a tested failure shows until the password changes, then the fresh status", () => {
  const fresh: SiteStatus = { ...s, site: "thepilot.com", has_password: true, state: "NotTried" };
  let tested = { "thepilot.com": { NoPassword: "none saved" } };
  expect(subscriptionLine(shownStatus([fresh], tested, "ThePilot.com")!)).toBe("No password: none saved");
  tested = forgetTested(tested, "https://www.thepilot.com/") as typeof tested;
  expect(subscriptionLine(shownStatus([fresh], tested, "thepilot.com")!)).toBe("Not signed in yet");
});
