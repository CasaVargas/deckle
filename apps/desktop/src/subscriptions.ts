// SPDX-License-Identifier: GPL-3.0-or-later
import type { SessionState, SiteStatus } from "./types";

/** One line on where a subscription's session stands. */
export function subscriptionLine(s: SiteStatus): string {
  const st = s.state;
  if (st === "NotTried") return "Not signed in yet";
  if (st === "NoAdapter") return `Deckle doesn't know how ${s.site} signs in yet`;
  if ("SignedIn" in st) return st.SignedIn.service ? `Signed in · ${st.SignedIn.service}, active` : "Signed in";
  if ("LoginFailed" in st) return `Sign-in failed: ${st.LoginFailed}`;
  if ("EntitlementUnknown" in st) return `Signed in, but ${st.EntitlementUnknown}`;
  return `No password: ${st.NoPassword}`;
}

/** The site as the backend files it: a bare lowercase host. */
export const statusKey = (site: string) =>
  site.trim().toLowerCase().replace(/^https?:\/\//, "").replace(/^www\./, "").replace(/\/$/, "");

/** A test's answer by site: a fresh status only knows what this process has tried. */
export type Tested = Record<string, SessionState>;

/** The status the row shows: the site's, with a test's answer over its state when there is one. */
export function shownStatus(statuses: SiteStatus[], tested: Tested, site: string): SiteStatus | undefined {
  const key = statusKey(site);
  const st = statuses.find((t) => t.site === key);
  return st && { ...st, state: tested[key] ?? st.state };
}

/** `tested` without `site`'s answer, once something it was about has changed. */
export function forgetTested(tested: Tested, site: string): Tested {
  const { [statusKey(site)]: _gone, ...rest } = tested;
  return rest;
}
