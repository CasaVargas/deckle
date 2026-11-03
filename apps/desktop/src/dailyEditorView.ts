// SPDX-License-Identifier: GPL-3.0-or-later
// The paper's AI editor section (local-model design §4): a paper follows Settings → AI unless
// its own [llm] names a provider, model or endpoint. Pure, so it's tested without a backend.
import { providerName } from "./aiView";
import type { DailyEditorConfig, Settings } from "./types";

/** Whether the paper names its own provider, model or endpoint (the `[llm]` table's). */
export function isOwn(c: DailyEditorConfig): boolean {
  return !!c.provider || !!c.model.trim() || !!c.endpoint.trim();
}

/** The config that follows Settings → AI again. */
export function followApp(c: DailyEditorConfig): DailyEditorConfig {
  return { ...c, provider: null, model: "", endpoint: "" };
}

/** The config made its own, starting from what the app has chosen. */
export function makeOwn(c: DailyEditorConfig, app: Pick<Settings, "llm_provider" | "llm_model" | "llm_endpoint"> | null): DailyEditorConfig {
  const provider = app && app.llm_provider !== "off" ? app.llm_provider : "ollama";
  return { ...c, provider, model: app?.llm_model ?? "", endpoint: app?.llm_endpoint ?? "" };
}

/**
 * The line under the switch: where this paper's editor runs. `app` null: the app's settings
 * couldn't be read (the command line's config, say).
 */
export function editorLine(c: DailyEditorConfig, app: Pick<Settings, "llm_provider" | "llm_model" | "llm_endpoint"> | null): { text: string; needsChoice: boolean } {
  if (isOwn(c)) {
    const p = c.provider ?? "openai";
    return { text: `This paper uses its own: ${providerName({ llm_provider: p, llm_endpoint: c.endpoint })}${c.model.trim() ? `, ${c.model.trim()}` : ""}.`, needsChoice: false };
  }
  if (!app) return { text: "Runs where Settings → AI says.", needsChoice: false };
  if (app.llm_provider === "off") return { text: "No AI model is chosen yet, so the paper goes out unedited.", needsChoice: true };
  return { text: `Runs where Settings → AI says: ${providerName(app)}${app.llm_model.trim() ? `, ${app.llm_model.trim()}` : ""}.`, needsChoice: false };
}
