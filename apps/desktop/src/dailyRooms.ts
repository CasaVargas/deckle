// SPDX-License-Identifier: GPL-3.0-or-later
import { DEFAULT_REMARKABLE } from "./exportsView";
import { SCREEN_LABEL } from "./types";
import { DAYS } from "./papersView";
import type { DailyConfig, Day, ReadershipView } from "./types";

/** The Daily tab's tabs (Daily tab redesign §2): tomorrow's paper first, then what's in it. */
export type Room = "today" | "sections" | "reading" | "editions" | "delivery";

export const ROOMS: { key: Room; title: string }[] = [
  { key: "today", title: "Today" },
  { key: "sections", title: "Sections" },
  { key: "reading", title: "Reading" },
  { key: "editions", title: "Editions" },
  { key: "delivery", title: "Delivery" },
];

/** Somewhere the paper goes, as one row of the Delivery room. `open` rows have settings. */
export interface Place {
  key: "kindle" | "email" | "remarkable" | "exports" | "readers";
  name: string;
  how: string;
  status: string;
  on: boolean;
  open: boolean;
}

const plural = (n: number, one: string, many = `${one}s`) => `${n} ${n === 1 ? one : many}`;

/** Where the paper goes, from the config: the Kindle always, the rest when they're set up. */
export function destinations(cfg: DailyConfig): Place[] {
  const email = cfg.email;
  const rm = cfg.remarkable ?? DEFAULT_REMARKABLE;
  const exports = cfg.export ?? [];
  return [
    { key: "kindle", name: "Kindle", how: "Over USB when it's plugged in", status: `Keeps ${plural(cfg.keep, "edition")}`, on: true, open: false },
    { key: "email", name: "Email to Kindle", how: "Over Wi-Fi, from your own mail account", status: email?.enabled ? email.to || "On" : "Off", on: !!email?.enabled, open: true },
    { key: "remarkable", name: "reMarkable", how: "A PDF over its USB web interface", status: rm.enabled ? SCREEN_LABEL[rm.screen] : "Off", on: rm.enabled, open: true },
    {
      key: "exports",
      name: "Export folders",
      how: "iCloud Drive for an iPad, Dropbox for a Boox",
      status: exports.length === 0 ? "None" : exports.length === 1 ? exports[0].name : plural(exports.length, "folder"),
      on: exports.length > 0,
      open: true,
    },
    { key: "readers", name: "Kobo and PocketBook", how: "Found when one is plugged in, nothing to set up", status: "Automatic", on: true, open: false },
  ];
}

/** The press days with `d` switched, in week order; none on means every day. */
export function toggleDay(days: Day[], d: Day): Day[] {
  const on = days.includes(d) ? days.filter((x) => x !== d) : [...days, d];
  return DAYS.filter((x) => on.includes(x));
}

/** "Every day", "Weekdays", or "Mon, Wed and Fri", under the day toggles. */
export function daysLine(days: Day[]): string {
  const ds = DAYS.filter((d) => days.includes(d));
  if (ds.length === 0 || ds.length === 7) return "Every day";
  if (ds.join() === "mon,tue,wed,thu,fri") return "Weekdays";
  if (ds.join() === "sat,sun") return "Weekends";
  const names = ds.map((d) => d[0].toUpperCase() + d.slice(1));
  return names.length === 1 ? names[0] : `${names.slice(0, -1).join(", ")} and ${names[names.length - 1]}`;
}

/** Where the paper goes, in one line under the press card (§3): "To your Kindle by email and over USB". */
export function goesLine(cfg: DailyConfig): string {
  const email = cfg.email?.enabled ? cfg.email : null;
  const kindle = !email ? "over USB" : email.usb_too ? "by email and over USB" : "by email";
  const others = destinations(cfg)
    .filter((p) => p.on && (p.key === "remarkable" || p.key === "exports"))
    .map((p) => (p.key === "remarkable" ? "your reMarkable" : p.status));
  return others.length ? `To your Kindle ${kindle}, and to ${join(others)}` : `To your Kindle ${kindle}`;
}

const join = (xs: string[]) => (xs.length < 2 ? (xs[0] ?? "") : `${xs.slice(0, -1).join(", ")} and ${xs[xs.length - 1]}`);

/** "You usually stop in Sports", once Readership knows; else null. */
export function stopLine(view: ReadershipView | null): string | null {
  const r = view?.enabled ? view.report : null;
  return r && r.opened >= 3 && r.stopSection ? `You usually stop in ${r.stopSection}` : null;
}
