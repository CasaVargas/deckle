// SPDX-License-Identifier: GPL-3.0-or-later
// The AI page (local-model design §3): which of the three choices is on, and the "On this
// computer" checklist as the page shows it. Pure, so it's tested without a backend.
import type { LlmCheck, LlmPullEvent, LlmRecommendation, Settings } from "./types";

export type AiChoice = "off" | "local" | "hosted";

export function aiChoice(s: Pick<Settings, "llm_provider">): AiChoice {
  return s.llm_provider === "off" ? "off" : s.llm_provider === "ollama" ? "local" : "hosted";
}

/** The settings a choice sets: local clears the hosted fields, so no key rides along to Ollama. */
export function choose(s: Settings, choice: AiChoice): Settings {
  if (choice === "off") return { ...s, llm_provider: "off" };
  if (choice === "local") return { ...s, llm_provider: "ollama", llm_model: "", llm_endpoint: "", llm_api_key: "" };
  return s.llm_provider === "anthropic" || s.llm_provider === "openai" ? s : { ...s, llm_provider: "anthropic", llm_model: "", llm_endpoint: "" };
}

/** "Anthropic", "Ollama on this Mac", …: the provider as the other pages name it. */
export function providerName(s: Pick<Settings, "llm_provider" | "llm_endpoint">): string {
  switch (s.llm_provider) {
    case "anthropic":
      return "Anthropic";
    case "openai":
      return s.llm_endpoint.trim() && !s.llm_endpoint.includes("api.openai.com") ? "your own server" : "OpenAI";
    case "ollama":
      return s.llm_endpoint.trim() && !/localhost|127\.0\.0\.1/.test(s.llm_endpoint) ? "Ollama on your network" : "Ollama on this Mac";
    default:
      return "the provider you pick";
  }
}

/** Whether the choice sends text off this computer. */
export function sendsOnline(s: Pick<Settings, "llm_provider" | "llm_endpoint">): boolean {
  if (s.llm_provider === "off") return false;
  if (s.llm_provider === "anthropic") return true;
  if (s.llm_provider === "ollama") return false;
  const e = s.llm_endpoint.trim();
  return !e || e.includes("api.openai.com");
}

export const gb = (bytes: number) => `${(bytes / 1e9).toFixed(1)} GB`;
const gib = (bytes: number) => `${Math.round(bytes / 2 ** 30)} GB`;

/** Why this model: the machine's memory, when known. `here` is "this Mac" or "the server". */
export function whyThisModel(rec: LlmRecommendation, here = "this Mac"): string {
  const size = `a ${gb(rec.download)} download`;
  return rec.memory ? `${size}, picked for the ${gib(rec.memory)} of memory on ${here}` : size;
}

/** What to expect, so a 20-minute book isn't a surprise. */
export const expectation = (here = "this Mac") => `A novel's recaps take about 20 minutes on ${here} and happen in the background; the morning paper's editor takes a minute or two.`;

export interface Pull {
  model: string;
  status: string;
  completed: number;
  total: number;
}

/** The download's line: "pulling… 42% of 2.6 GB". */
export function pullLine(p: Pull): string {
  if (p.total > 0) return `Downloading ${p.model}: ${Math.floor((p.completed * 100) / p.total)}% of ${gb(p.total)}`;
  return p.status === "pulling manifest" ? `Asking Ollama for ${p.model}…` : p.status === "success" ? `${p.model} is installed` : `${p.status}…`;
}

/** The next pull state after an event; `null` once it's over. */
export function afterPull(prev: Pull | null, e: LlmPullEvent): Pull | null {
  if (e.done) return null;
  // a later layer's progress replaces an earlier one's; a status without bytes keeps the last bytes seen for that status
  return { model: e.model, status: e.status, completed: e.completed, total: e.total || (prev?.status === e.status ? prev.total : 0) };
}

export interface CheckRow {
  key: "ollama" | "model" | "try";
  mark: "ok" | "todo" | "wait";
  title: string;
  state: string;
  /** The button that fixes it. */
  fix?: "get" | "download" | "stop" | "try";
}

/**
 * The three rows. `check` null: not run yet. A row after the first that isn't ok waits.
 * `pull` set: the model is downloading. `tried` false: the test prompt hasn't been asked.
 */
export function checklist(a: { check: LlmCheck | null; checking: boolean; rec: LlmRecommendation | null; pull: Pull | null; pullError: string | null; model: string; here?: string }): CheckRow[] {
  const model = a.model || a.rec?.model || a.check?.model || "the model";
  const rows: CheckRow[] = [];
  if (!a.check) {
    rows.push({ key: "ollama", mark: "wait", title: "Ollama", state: a.checking ? "looking…" : "not checked yet" });
    rows.push({ key: "model", mark: "wait", title: model, state: "" });
    rows.push({ key: "try", mark: "wait", title: "Try it", state: "" });
    return rows;
  }
  if ("Err" in a.check.server) {
    rows.push({ key: "ollama", mark: "todo", title: "Ollama", state: a.check.server.Err.includes("not like Ollama") ? "something else answers at that address" : "not running. Get it, open it, and this ticks itself.", fix: "get" });
    rows.push({ key: "model", mark: "wait", title: model, state: "" });
    rows.push({ key: "try", mark: "wait", title: "Try it", state: "" });
    return rows;
  }
  rows.push({ key: "ollama", mark: "ok", title: "Ollama", state: `${a.check.server.Ok.version} is running` });
  if (a.pull) rows.push({ key: "model", mark: "wait", title: model, state: pullLine(a.pull), fix: "stop" });
  else if (!a.check.installed)
    rows.push({
      key: "model",
      mark: "todo",
      title: model,
      state: a.pullError ? `the download failed: ${a.pullError}` : `not installed${a.rec && model === a.rec.model ? `. ${whyThisModel(a.rec, a.here).replace(/^a /, "A ")}.` : "."}`,
      fix: "download",
    });
  else rows.push({ key: "model", mark: "ok", title: model, state: "installed" });
  const tried = a.check.answered;
  if (!a.check.installed || a.pull) rows.push({ key: "try", mark: "wait", title: "Try it", state: "" });
  else if (!tried) rows.push({ key: "try", mark: "wait", title: "Try it", state: a.checking ? "asking…" : "", fix: "try" });
  else if ("Ok" in tried) rows.push({ key: "try", mark: "ok", title: "Try it", state: `answered in ${(tried.Ok / 1000).toFixed(1)} s`, fix: "try" });
  else rows.push({ key: "try", mark: "todo", title: "Try it", state: `didn't answer: ${tried.Err}`, fix: "try" });
  return rows;
}

/** The page's one-line state for Overview and the sidebar. */
export function aiSummary(s: Pick<Settings, "llm_provider" | "llm_endpoint" | "llm_model">, rec?: string): string {
  const c = aiChoice(s);
  if (c === "off") return "Off";
  if (c === "local") return `On this Mac: ${s.llm_model.trim() || rec || "the recommended model"} through Ollama`;
  return `Online: ${providerName(s)}${s.llm_model.trim() ? `, ${s.llm_model.trim()}` : ""}`;
}
