// SPDX-License-Identifier: GPL-3.0-or-later
import type { DailyConfig, SessionState, SiteStatus, SubscribedConfig } from "./types";

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

/** BLOX's section slugs and their display names (publisher subscriptions design §3). */
export const SUBSCRIBED_SLUGS: { slug: string; name: string }[] = [
  { slug: "news", name: "News" },
  { slug: "sports", name: "Sports" },
  { slug: "opinion", name: "Opinion" },
  { slug: "obituaries", name: "Obituaries" },
  { slug: "business", name: "Business" },
  { slug: "lifestyles", name: "Lifestyles" },
];

/** The draft's `[[subscribed]]` group for a site, found by normalized site. */
export function groupFor(cfg: DailyConfig, site: string): SubscribedConfig | undefined {
  const key = statusKey(site);
  return (cfg.subscribed ?? []).find((g) => statusKey(g.site) === key);
}

/** `cfg` with `subscribed` kept in `sections` exactly while a group exists: after `local`, else last. */
function syncSections(cfg: DailyConfig): DailyConfig {
  const has = cfg.sections.includes("subscribed");
  const want = (cfg.subscribed ?? []).length > 0;
  if (want && !has) {
    const at = cfg.sections.indexOf("local");
    const sections = [...cfg.sections];
    sections.splice(at < 0 ? sections.length : at + 1, 0, "subscribed");
    return { ...cfg, sections };
  }
  if (!want && has) return { ...cfg, sections: cfg.sections.filter((s) => s !== "subscribed") };
  return cfg;
}

const withGroups = (cfg: DailyConfig, subscribed: SubscribedConfig[]): DailyConfig => syncSections({ ...cfg, subscribed });

/** "Print this paper": adds the site's group (News only) or removes it. */
export function toggleGroup(cfg: DailyConfig, site: string): DailyConfig {
  const key = statusKey(site);
  const groups = cfg.subscribed ?? [];
  if (groupFor(cfg, site)) return withGroups(cfg, groups.filter((g) => statusKey(g.site) !== key));
  return withGroups(cfg, [...groups, { site: key, sections: ["news"] }]);
}

/** Ticks or unticks one slug; unticking the last one removes the group. */
export function toggleSlug(cfg: DailyConfig, site: string, slug: string): DailyConfig {
  const g = groupFor(cfg, site);
  if (!g) return cfg;
  const sections = g.sections.includes(slug) ? g.sections.filter((s) => s !== slug) : [...g.sections, slug];
  if (sections.length === 0) return toggleGroup(cfg, site);
  return withGroups(cfg, (cfg.subscribed ?? []).map((x) => (x === g ? { ...x, sections } : x)));
}

/** Sets the group's stories-per-section; an empty or invalid value unsets it. */
export function setMaxItems(cfg: DailyConfig, site: string, value: string): DailyConfig {
  const g = groupFor(cfg, site);
  if (!g) return cfg;
  const n = Math.floor(Number(value));
  const { max_items: _drop, ...rest } = g;
  const next: SubscribedConfig = value.trim() !== "" && n >= 1 ? { ...rest, max_items: n } : rest;
  return withGroups(cfg, (cfg.subscribed ?? []).map((x) => (x === g ? next : x)));
}
