// SPDX-License-Identifier: GPL-3.0-or-later
import { describe, expect, it } from "vitest";
import { editorLine, followApp, isOwn, makeOwn } from "./dailyEditorView";
import type { DailyEditorConfig } from "./types";

const bare: DailyEditorConfig = { enabled: true, provider: null, model: "", endpoint: "" };
const app = { llm_provider: "ollama", llm_model: "", llm_endpoint: "" } as const;

describe("the paper's editor", () => {
  it("follows Settings → AI unless the paper names its own", () => {
    expect(isOwn(bare)).toBe(false);
    expect(isOwn({ ...bare, provider: "anthropic" })).toBe(true);
    expect(isOwn({ ...bare, endpoint: "http://localhost:11434/v1" })).toBe(true);
    expect(editorLine(bare, app)).toEqual({ text: "Runs where Settings → AI says: Ollama on this Mac.", needsChoice: false });
    expect(editorLine(bare, { llm_provider: "anthropic", llm_model: "claude-haiku-5-5", llm_endpoint: "" })).toEqual({ text: "Runs where Settings → AI says: Anthropic, claude-haiku-5-5.", needsChoice: false });
    expect(editorLine(bare, { llm_provider: "off", llm_model: "", llm_endpoint: "" })).toEqual({ text: "No AI model is chosen yet, so the paper goes out unedited.", needsChoice: true });
    expect(editorLine(bare, null).text).toBe("Runs where Settings → AI says.");
    expect(editorLine({ ...bare, provider: "openai", endpoint: "http://localhost:1234/v1", model: "x" }, app).text).toBe("This paper uses its own: your own server, x.");
    expect(editorLine({ ...bare, endpoint: "http://localhost:11434/v1" }, app).text).toBe("This paper uses its own: your own server.");
  });
  it("switches between its own and the app's", () => {
    const own = makeOwn(bare, { llm_provider: "anthropic", llm_model: "m", llm_endpoint: "" });
    expect([own.provider, own.model]).toEqual(["anthropic", "m"]);
    expect(makeOwn(bare, { llm_provider: "off", llm_model: "", llm_endpoint: "" }).provider).toBe("ollama");
    expect(makeOwn(bare, null).provider).toBe("ollama");
    expect(followApp(own)).toEqual(bare);
  });
});
