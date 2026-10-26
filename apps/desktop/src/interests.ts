// SPDX-License-Identifier: GPL-3.0-or-later
// Your interests (interests design §3, §5.4): pure edits to the config the Daily tab saves.
import type { DailyConfig, InterestConfig } from "./types";

/** Puts "interests" into `sections` right before "feeds", or at the end. */
function withSection(cfg: DailyConfig): DailyConfig {
  if (cfg.sections.includes("interests")) return cfg;
  const at = cfg.sections.indexOf("feeds");
  const sections = cfg.sections.slice();
  sections.splice(at < 0 ? sections.length : at, 0, "interests");
  return { ...cfg, sections };
}

/**
 * Adds an interest by name (trimmed). A blank name or one already there (ignoring case) changes
 * nothing. New entries carry an explicit `max_items`, as the Topics picker does, because the
 * config is saved without `Config::parse` normalisation.
 */
export function addInterest(cfg: DailyConfig, name: string): DailyConfig {
  const trimmed = name.trim();
  if (!trimmed) return cfg;
  const have = cfg.interests ?? [];
  if (have.some((i) => i.name.trim().toLowerCase() === trimmed.toLowerCase())) return cfg;
  const entry: InterestConfig = { name: trimmed, max_items: 5 };
  return withSection({ ...cfg, interests: [...have, entry] });
}

export function removeInterest(cfg: DailyConfig, index: number): DailyConfig {
  const have = cfg.interests ?? [];
  if (index < 0 || index >= have.length) return cfg;
  return { ...cfg, interests: have.filter((_, i) => i !== index) };
}

/** A comma-separated field as the list it stands for; an emptied field removes the key. */
export function setList(cfg: DailyConfig, index: number, key: "also" | "not", text: string): DailyConfig {
  const have = cfg.interests ?? [];
  if (index < 0 || index >= have.length) return cfg;
  const list = text
    .split(",")
    .map((s) => s.trim())
    .filter((s) => s.length > 0);
  const interests = have.map((it, i) => {
    if (i !== index) return it;
    const next = { ...it };
    if (list.length === 0) delete next[key];
    else next[key] = list;
    return next;
  });
  return { ...cfg, interests };
}

export function listText(v: string[] | undefined): string {
  return (v ?? []).join(", ");
}
