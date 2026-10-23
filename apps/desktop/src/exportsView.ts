// SPDX-License-Identifier: GPL-3.0-or-later
import type { ExportConfig, ExportReport, RemarkableConfig } from "./types";

/** A new export's settings: color photos for a tablet, a week of papers. */
export const DEFAULT_EXPORT: Omit<ExportConfig, "name" | "dir"> = {
  format: "epub",
  photos: "color",
  screen: "remarkable-2",
  keep: 7,
  books: false,
};

/** `[remarkable]` before it's turned on. */
export const DEFAULT_REMARKABLE: RemarkableConfig = {
  enabled: false,
  screen: "remarkable-2",
  books: false,
};

/** The folder's own name, for either path separator. */
export function folderName(dir: string): string {
  const parts = dir.split(/[\\/]+/).filter(Boolean);
  return parts[parts.length - 1] ?? "";
}

/** An export for `dir`, named after the folder and unique among `existing` ("Deckle Daily 2"). */
export function newExport(dir: string, existing: ExportConfig[]): ExportConfig {
  const base = folderName(dir) || "Folder";
  const taken = new Set(existing.map((e) => e.name.trim()));
  let name = base;
  for (let n = 2; taken.has(name); n++) name = `${base} ${n}`;
  return { ...DEFAULT_EXPORT, name, dir };
}

/** Whether `dir` is already an export folder. */
export function hasFolder(existing: ExportConfig[], dir: string): boolean {
  const norm = (d: string) => d.replace(/[\\/]+$/, "");
  return existing.some((e) => norm(e.dir) === norm(dir));
}

/** A run's export folders, one sentence each, for the Daily tab's result line. */
export function exportParts(reports: ExportReport[] | undefined): string[] {
  return (reports ?? []).map((r) => (r.error ? `Not put in ${r.name}: ${r.error}.` : `Put in ${r.name}.`));
}

/** What a run did for a reMarkable, as a sentence (none when it isn't on). */
export function remarkablePart(report: { remarkable_queued?: boolean; remarkable?: { uploaded: string[]; errors: string[] } } | undefined): string | null {
  const r = report?.remarkable;
  if (r?.uploaded.length) return "Sent to your reMarkable.";
  if (r?.errors.length) return `reMarkable: ${r.errors[0]}`;
  if (report?.remarkable_queued) return "Waiting for your reMarkable: plug it in with its USB web interface on.";
  return null;
}

/** The iCloud Drive folder on a Mac, under `home`. */
export function iCloudDrive(home: string): string {
  return `${home.replace(/\/+$/, "")}/Library/Mobile Documents/com~apple~CloudDocs`;
}
