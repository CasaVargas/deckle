// SPDX-License-Identifier: GPL-3.0-or-later
export type Mode = "full" | "cover" | "error";
export type RunStatus = "idle" | "queued" | "running" | "done" | "failed";

export interface BookRow {
  id: string;
  path: string;
  title: string;
  authors: string;
  series: string | null;
  cover: string | null;
  mode: Mode;
  reason: string;
  warnings: string[];
  error: string | null;
}

export interface BookState extends BookRow {
  status: RunStatus;
  stage: string | null;
  delivered: string | null;
  pages: number | null;
  runWarnings: string[];
  runError: string | null;
  /** What shipped (from the final progress event). */
  shipped: { cde: string; asin: string; reason: string; matched: string | null } | null;
  /** X-Ray sidecar summary (from the final progress event). */
  xray: { people: number; terms: number; source: "llm" | "wikipedia" | "basic" } | null;
  /** Word Wise sidecar summary (from the final progress event). */
  wordWise: WordWiseSummary | null;
  /** Per-book override of the settings' cde. */
  cdeOverride: "pdoc" | "ebok" | null;
  /** The book's Amazon ASIN, typed by the owner: EBOK then gets the store cover. */
  asin: string | null;
  showReport: boolean;
}

export interface WordWiseSummary {
  hints: number;
  level: number;
  /** `device`: the Kindle's own English dictionary; `deckle`: Deckle's, in the 中文 slot. */
  dictionary: "device" | "deckle";
  downloaded: boolean;
}

export interface DeviceRow {
  key: string;
  kind: "mass_storage" | "mtp";
  root: string | null;
  bus: number;
  address: number;
  model_hint: string;
  thumbnails_writable: boolean;
  cde_override: "pdoc" | "ebok" | null;
  verified_real_asin: boolean;
}

export interface PlanRow {
  id: string;
  cde: "pdoc" | "ebok";
  write_thumbnail: boolean;
  reason: string;
}

export interface Settings {
  previewer_path: string | null;
  cde: "auto" | "pdoc" | "ebok";
  pages_auto: boolean;
  llm_provider: "off" | "anthropic" | "openai";
  llm_model: string;
  llm_endpoint: string;
  llm_api_key: string;
  xray: boolean;
  word_wise: boolean;
  word_wise_level: number;
  wikipedia: boolean;
  amazon_match: boolean;
}

export interface PreviewerStatus {
  found: boolean;
  path: string | null;
  version: string | null;
  download_url: string;
}

export interface ProgressEvent {
  id: string;
  status: "running" | "done" | "failed";
  stage: string | null;
  delivered: string | null;
  pages: number | null;
  warnings: string[];
  error: string | null;
  cde?: "PDOC" | "EBOK" | null;
  asin?: string | null;
  reason?: string | null;
  matched?: string | null;
  xray?: { people: number; terms: number; source: "llm" | "wikipedia" | "basic" } | null;
  word_wise?: WordWiseSummary | null;
}

export type RunTarget =
  | { kind: "device"; root: string }
  | { kind: "mtp"; bus: number; address: number }
  | { kind: "folder"; path: string };

export const STAGES = ["inspect", "normalize", "convert", "repackage", "enrich", "deliver"] as const;
export const STAGE_LABEL: Record<string, string> = {
  inspect: "Reading",
  normalize: "Cleaning",
  convert: "Kindle Previewer",
  repackage: "Building KFX",
  enrich: "X-Ray & Word Wise",
  deliver: "Copying",
  match: "Matching edition",
};
