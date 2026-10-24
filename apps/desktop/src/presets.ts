// SPDX-License-Identifier: GPL-3.0-or-later
import type { Settings } from "./types";
export const PRESETS = { academic: "Academic paper", novel: "Novel", manual: "Manual" } as const;
/** Display the same stage defaults as the core preset model. */
export function presetSettings(settings: Settings, path: string): Settings {
  const preset = settings.book_presets[path];
  return preset ? { ...settings, typography: preset !== "manual", popup_notes: preset !== "manual", pages_auto: true, xray: preset === "novel", word_wise: false } : settings;
}
