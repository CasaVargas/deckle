// SPDX-License-Identifier: GPL-3.0-or-later
// Tomorrow's paper, so far (tomorrow's paper design §4, §6, §7): the press bar's copy and the
// page plan's edits, as pure functions over the config the Daily tab saves.
import type { DailyConfig, PagePlanRow, Press, SectionKind, WeekDay } from "./types";

/** "in 14 h 22 m", "in 38 m", or "now"; minutes round down (as `press::countdown`). */
export function countdown(nextIso: string, nowMs: number): string {
  const secs = Math.floor((Date.parse(nextIso) - nowMs) / 1000);
  if (secs <= 0) return "now";
  const mins = Math.floor(secs / 60);
  const h = Math.floor(mins / 60);
  return h === 0 ? `in ${mins % 60} m` : `in ${h} h ${mins % 60} m`;
}

/** The weekday of a YYYY-MM-DD date, whatever the machine's zone. */
function weekday(date: string): string {
  const [y, m, d] = date.split("-").map(Number);
  return new Date(Date.UTC(y, m - 1, d)).toLocaleDateString("en-US", { weekday: "long", timeZone: "UTC" });
}

/** The press bar's line (as `press::bar_line`). */
export function pressLine(p: Press, nowMs: number): string {
  if (!p.at || !p.next) return "No press time set";
  if (p.stopped) return `Presses stopped: no paper ${weekday(p.stopped)} · next at ${p.at} ${weekday(p.edition)}`;
  return `Goes to press at ${p.at} · ${countdown(p.next, nowMs)}`;
}

const isFeed = (r: PagePlanRow) => r.kind === "feeds";
const isSub = (r: PagePlanRow) => r.kind === "subscribed";

/** The rows were made from this config: each feed (and place) row names the one at its index. */
function matches(cfg: DailyConfig, rows: PagePlanRow[]): boolean {
  return (
    rows.filter(isFeed).every((r) => r.feed !== null && cfg.feeds[r.feed]?.name === r.name) &&
    rows
      .filter(isSub)
      .every((r) => r.subscribed && cfg.subscribed?.some((g) => g.site === r.subscribed!.site && g.sections.includes(r.subscribed!.slug))) &&
    rows.every((r) => r.place === null || cfg.local.places?.[r.place]?.name === r.name)
  );
}

/**
 * Moves row `from` to `to` and writes the result into `sections` and `[[feeds]]` (spec §6.2).
 * The feeds stay one block: a section dropped inside it, or a feed dropped outside it, goes to
 * the block's nearest end.
 */
export function reorder(cfg: DailyConfig, all: PagePlanRow[], from: number, to: number): DailyConfig {
  if (!matches(cfg, all)) return cfg;
  // Other places follow Local: they aren't moved on their own, and a drop onto one is Local's spot
  const moving = all[from];
  if (!moving || moving.place !== null) return cfg;
  const rows = all.filter((r) => r.place === null);
  const target = all[to];
  const local = rows.findIndex((r) => r.kind === "local" && r.place === null);
  from = rows.indexOf(moving);
  to = target && target.place !== null ? local : rows.indexOf(target ?? rows[rows.length - 1]);
  if (to < 0) to = rows.length - 1;
  if (isSub(moving)) return moveSubscribed(cfg, rows, from, to);
  const order = rows.slice();
  const [moved] = order.splice(from, 1);
  order.splice(Math.max(0, Math.min(to, order.length)), 0, moved);
  // a section or feed never lands inside a subscribed paper's block
  const subs = () => order.map((r, i) => (isSub(r) ? i : -1)).filter((i) => i >= 0);
  if (subs().length > 0) {
    const at0 = order.indexOf(moved);
    const [a, b] = [subs()[0], subs()[subs().length - 1]];
    if (at0 > a && at0 < b) {
      order.splice(at0, 1);
      const block = subs();
      order.splice(at0 - a < b - at0 ? block[0] : block[block.length - 1] + 1, 0, moved);
    }
  }
  const at = order.indexOf(moved);
  const others = order.map((r, i) => (isFeed(r) && r !== moved ? i : -1)).filter((i) => i >= 0);
  if (others.length > 0) {
    const first = others[0];
    const last = others[others.length - 1];
    const snap = (end: "start" | "end") => {
      order.splice(order.indexOf(moved), 1);
      const block = order.map((r, i) => (isFeed(r) && r !== moved ? i : -1)).filter((i) => i >= 0);
      order.splice(end === "start" ? block[0] : block[block.length - 1] + 1, 0, moved);
    };
    const nearest = (): "start" | "end" => (at - first < last - at ? "start" : "end");
    if (!isFeed(moved) && at > first && at < last) snap(nearest());
    // a feed must touch the block: next to another feed, or between them
    if (isFeed(moved) && !(at >= first - 1 && at <= last + 1)) snap(at < first ? "start" : "end");
  }
  const sections = sectionsOf(cfg, order);
  const feedRows = order.filter(isFeed);
  const feeds = feedRows.length > 0 ? feedRows.map((r) => ({ ...cfg.feeds[r.feed ?? 0] })) : cfg.feeds.slice();
  return { ...cfg, sections, feeds };
}

/** The kinds of `order`, each where it first appears; kinds with no row keep their place at the end. */
function sectionsOf(cfg: DailyConfig, order: PagePlanRow[]): SectionKind[] {
  const sections: SectionKind[] = [];
  for (const r of order) {
    if (!sections.includes(r.kind)) sections.push(r.kind);
  }
  for (const s of cfg.sections) if (!sections.includes(s)) sections.push(s);
  return sections;
}

/** The subscribed rows are one block: moving one moves them all, to the row at `to` (never into
 * the feeds block). Their order inside the block is the file's, so a drop on one is a no-op. */
function moveSubscribed(cfg: DailyConfig, rows: PagePlanRow[], from: number, to: number): DailyConfig {
  if (isSub(rows[to])) return cfg;
  const rest = rows.filter((r) => !isSub(r));
  const block = rows.filter(isSub);
  const target = rows[to];
  let at = rest.indexOf(target) + (to > from ? 1 : 0);
  const feeds = rest.map((r, i) => (isFeed(r) ? i : -1)).filter((i) => i >= 0);
  if (feeds.length > 0 && at > feeds[0] && at <= feeds[feeds.length - 1]) {
    at = at - feeds[0] < feeds[feeds.length - 1] + 1 - at ? feeds[0] : feeds[feeds.length - 1] + 1;
  }
  const order = rest.slice();
  order.splice(at, 0, ...block);
  return { ...cfg, sections: sectionsOf(cfg, order) };
}

/** Sets a row's story budget, 1–20, on its own key. */
export function setBudget(cfg: DailyConfig, row: PagePlanRow, n: number): DailyConfig {
  if (!matches(cfg, [row])) return cfg;
  const v = Math.max(1, Math.min(20, Math.round(n)));
  switch (row.kind) {
    case "reading":
      return { ...cfg, reading: { ...cfg.reading, max_items: v } };
    case "local":
      if (row.place !== null) {
        const places = (cfg.local.places ?? []).map((p, i) => (i === row.place ? { ...p, max_items: v } : p));
        return { ...cfg, local: { ...cfg.local, places } };
      }
      return { ...cfg, local: { ...cfg.local, max_items: v } };
    case "feeds":
      return { ...cfg, feeds: cfg.feeds.map((f, i) => (i === row.feed ? { ...f, max_items: v } : f)) };
    case "subscribed": {
      const ref = row.subscribed;
      return { ...cfg, subscribed: (cfg.subscribed ?? []).map((g) => (g.site === ref?.site ? { ...g, max_items: v } : g)) };
    }
    default:
      return cfg;
  }
}

/** "read 90%", "not opened", "sent", "no paper". */
export function weekLabel(d: WeekDay): string {
  switch (d.state.state) {
    case "read":
      return `read ${d.state.percent}%`;
    case "notOpened":
      return "not opened";
    case "sent":
      return "sent";
    case "noPaper":
      return "no paper";
  }
}

/** The row an up (-1) or down (+1) arrow moves row `i` to: the next one that isn't another
 * place (places follow Local), or null at the ends. */
export function step(rows: PagePlanRow[], i: number, dir: -1 | 1): number | null {
  for (let j = i + dir; j >= 0 && j < rows.length; j += dir) {
    // a subscribed paper moves as a block, past its own rows
    if (rows[j].place === null && !(isSub(rows[i]) && isSub(rows[j]))) return j;
  }
  return null;
}
