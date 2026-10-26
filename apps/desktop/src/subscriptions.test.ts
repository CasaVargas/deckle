// SPDX-License-Identifier: GPL-3.0-or-later
import { expect, test } from "vitest";
import { subscriptionLine } from "./subscriptions";
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
  expect(subscriptionLine({ ...s, site: "thepilot.com", state: "NoAdapter" })).toBe("Deckle doesn't know how thepilot.com signs in yet");
  expect(subscriptionLine({ ...s, state: { NoPassword: "none saved" } })).toBe("No password: none saved");
});
