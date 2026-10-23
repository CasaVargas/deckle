// SPDX-License-Identifier: GPL-3.0-or-later
import { DEFAULT_REMARKABLE } from "./exportsView";
import { SCREEN_LABEL } from "./types";
import type { DailyConfig, NextPaper, ReadingEntry } from "./types";

/** The Daily tab's rooms: tonight's paper first, then the rest grouped by what it's for. */
export type Room = "today" | "editions" | "reading" | "delivery" | "paper";

export const ROOMS: { key: Room; title: string }[] = [
  { key: "today", title: "Today" },
  { key: "editions", title: "Editions" },
  { key: "reading", title: "Reading" },
  { key: "delivery", title: "Delivery" },
  { key: "paper", title: "The paper" },
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

/** The line under each room's name: what's in it at a glance. */
export function roomNotes(cfg: DailyConfig, next: NextPaper | null, reading: ReadingEntry[], dirty: boolean): Record<Room, string> {
  const press = next?.press;
  const read = next?.week.filter((d) => d.state.state === "read").length ?? 0;
  const saved = reading.filter((e) => e.state === "pending").length;
  const elsewhere = destinations(cfg).filter((p) => p.on && p.key !== "kindle" && p.key !== "readers").length;
  return {
    today: !press ? "" : press.stopped ? "Presses stopped" : press.at ? `Press at ${press.at}` : "No press time set",
    editions: next ? `${read} of the last ${next.week.length} read` : "",
    reading: saved ? `${saved} saved for tomorrow` : "Nothing saved",
    delivery: elsewhere ? `Kindle and ${elsewhere} more` : "Kindle",
    paper: dirty ? "Unsaved changes" : plural(cfg.sections.length, "section"),
  };
}
