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

/** Kinds whose rows come from a list in the config and stay one block in the paper. */
const GROUPED: SectionKind[] = ["feeds", "interests"];
const isFeed = (r: PagePlanRow) => r.kind === "feeds";
const isInterest = (r: PagePlanRow) => r.kind === "interests";
const grouped = (r: PagePlanRow) => GROUPED.includes(r.kind);

/** The rows were made from this config: each feed, interest (and place) row names the one at its index. */
function matches(cfg: DailyConfig, rows: PagePlanRow[]): boolean {
  return (
    rows.filter(isFeed).every((r) => r.feed !== null && cfg.feeds[r.feed]?.name === r.name) &&
    rows.filter(isInterest).every((r) => r.interest !== null && cfg.interests?.[r.interest]?.name === r.name) &&
    rows.every((r) => r.place === null || cfg.local.places?.[r.place]?.name === r.name)
  );
}

/**
 * Moves row `from` to `to` and writes the result into `sections`, `[[interests]]` and `[[feeds]]`
 * (spec §6.2). Each grouped kind stays one block: a section dropped inside a block, or a grouped
 * row dropped outside its own block, goes to the block's nearest end.
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
  const order = rows.slice();
  const [moved] = order.splice(from, 1);
  order.splice(Math.max(0, Math.min(to, order.length)), 0, moved);
  const block = (kind: SectionKind) => order.map((r, i) => (r.kind === kind && r !== moved ? i : -1)).filter((i) => i >= 0);
  const snap = (kind: SectionKind, end: "start" | "end") => {
    order.splice(order.indexOf(moved), 1);
    const b = block(kind);
    order.splice(end === "start" ? b[0] : b[b.length - 1] + 1, 0, moved);
  };
  for (const kind of GROUPED) {
    const others = block(kind);
    if (others.length === 0) continue;
    const at = order.indexOf(moved);
    const first = others[0];
    const last = others[others.length - 1];
    const nearest = (): "start" | "end" => (at - first < last - at ? "start" : "end");
    // a row of another kind must not split this block
    if (moved.kind !== kind && at > first && at < last) snap(kind, nearest());
    // a row of this kind must touch its block: next to another, or between them
    if (moved.kind === kind && !(at >= first - 1 && at <= last + 1)) snap(kind, at < first ? "start" : "end");
  }
  const sections: SectionKind[] = [];
  for (const r of order) {
    const kind = r.kind;
    if (!sections.includes(kind)) sections.push(kind);
  }
  // sections not shown as rows (none today) keep their place at the end
  for (const s of cfg.sections) if (!sections.includes(s)) sections.push(s);
  const feedRows = order.filter(isFeed);
  const feeds = feedRows.length > 0 ? feedRows.map((r) => ({ ...cfg.feeds[r.feed ?? 0] })) : cfg.feeds.slice();
  const interestRows = order.filter(isInterest);
  const interests =
    interestRows.length > 0 && cfg.interests ? interestRows.map((r) => ({ ...cfg.interests![r.interest ?? 0] })) : cfg.interests;
  return interests === undefined ? { ...cfg, sections, feeds } : { ...cfg, sections, feeds, interests };
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
    case "interests":
      return { ...cfg, interests: (cfg.interests ?? []).map((it, i) => (i === row.interest ? { ...it, max_items: v } : it)) };
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
    if (rows[j].place === null) return j;
  }
  return null;
}
