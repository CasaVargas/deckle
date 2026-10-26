// SPDX-License-Identifier: GPL-3.0-or-later
import type { SiteStatus } from "./types";

/** One line on where a subscription's session stands. */
export function subscriptionLine(s: SiteStatus): string {
  const st = s.state;
  if (st === "NotTried") return "Not signed in yet";
  if (st === "NoAdapter") return `Deckle doesn't know how ${s.site} signs in yet`;
  if ("SignedIn" in st) return st.SignedIn.service ? `Signed in · ${st.SignedIn.service}, active` : "Signed in";
  if ("LoginFailed" in st) return `Sign-in failed: ${st.LoginFailed}`;
  return `No password: ${st.NoPassword}`;
}
