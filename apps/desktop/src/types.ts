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
  /** Per-book override of the settings' cde. */
  cdeOverride: "pdoc" | "ebok" | null;
  showReport: boolean;
}

export interface DeviceRow {
  root: string;
  model_hint: string;
  thumbnails_writable: boolean;
}

export interface Settings {
  previewer_path: string | null;
  cde: "pdoc" | "ebok";
  pages_auto: boolean;
  llm_provider: "off" | "anthropic" | "openai";
  llm_model: string;
  llm_endpoint: string;
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
}

export type RunTarget = { kind: "device"; root: string } | { kind: "folder"; path: string };

export const STAGES = ["inspect", "normalize", "convert", "repackage", "deliver"] as const;
export const STAGE_LABEL: Record<string, string> = {
  inspect: "Reading",
  normalize: "Cleaning",
  convert: "Kindle Previewer",
  repackage: "Building KFX",
  deliver: "Copying",
};
