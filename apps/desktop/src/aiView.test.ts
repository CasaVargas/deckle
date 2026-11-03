// SPDX-License-Identifier: GPL-3.0-or-later
import { describe, expect, it } from "vitest";
import { afterPull, aiChoice, aiSummary, checklist, choose, providerName, pullLine, sendsOnline, whyThisModel } from "./aiView";
import type { LlmCheck, LlmRecommendation, Settings } from "./types";

const base = { llm_provider: "off", llm_model: "", llm_endpoint: "", llm_api_key: "sk-x" } as Settings;
const rec: LlmRecommendation = { model: "qwen3:4b", download: 2_600_000_000, memory: 8 * 2 ** 30, ollama_url: "https://ollama.com/download" };
const up: LlmCheck = { endpoint: "http://localhost:11434", model: "qwen3:4b", server: { Ok: { version: "0.12.3", models: [{ name: "qwen3:4b", size: 1 }] } }, installed: true, answered: null };

describe("the three choices", () => {
  it("map the provider setting", () => {
    expect(aiChoice({ llm_provider: "off" })).toBe("off");
    expect(aiChoice({ llm_provider: "ollama" })).toBe("local");
    expect(aiChoice({ llm_provider: "anthropic" })).toBe("hosted");
    expect(aiChoice({ llm_provider: "openai" })).toBe("hosted");
  });
  it("choosing this computer clears the hosted fields, so no key rides along", () => {
    const s = choose({ ...base, llm_provider: "anthropic", llm_model: "claude-x" }, "local");
    expect([s.llm_provider, s.llm_model, s.llm_endpoint, s.llm_api_key]).toEqual(["ollama", "", "", ""]);
    expect(choose(s, "hosted").llm_provider).toBe("anthropic");
    const kept = choose({ ...base, llm_provider: "openai", llm_endpoint: "http://localhost:1234/v1" }, "hosted");
    expect(kept.llm_endpoint).toBe("http://localhost:1234/v1");
    expect(choose(kept, "off").llm_provider).toBe("off");
  });
  it("names the provider and knows what goes online", () => {
    expect(providerName({ llm_provider: "ollama", llm_endpoint: "" })).toBe("Ollama on this Mac");
    expect(providerName({ llm_provider: "ollama", llm_endpoint: "http://nas.local:11434" })).toBe("Ollama on your network");
    expect(providerName({ llm_provider: "openai", llm_endpoint: "http://localhost:1234/v1" })).toBe("your own server");
    expect(providerName({ llm_provider: "openai", llm_endpoint: "" })).toBe("OpenAI");
    expect(sendsOnline({ llm_provider: "ollama", llm_endpoint: "" })).toBe(false);
    expect(sendsOnline({ llm_provider: "openai", llm_endpoint: "http://localhost:1234/v1" })).toBe(false);
    expect(sendsOnline({ llm_provider: "openai", llm_endpoint: "" })).toBe(true);
    expect(sendsOnline({ llm_provider: "anthropic", llm_endpoint: "" })).toBe(true);
  });
  it("summarises the page in a line", () => {
    expect(aiSummary(base)).toBe("Off");
    expect(aiSummary({ ...base, llm_provider: "ollama" }, "qwen3:4b")).toBe("On this Mac: qwen3:4b through Ollama");
    expect(aiSummary({ ...base, llm_provider: "ollama", llm_model: "gemma3:4b" })).toBe("On this Mac: gemma3:4b through Ollama");
    expect(aiSummary({ ...base, llm_provider: "anthropic", llm_model: "claude-haiku-5-5" })).toBe("Online: Anthropic, claude-haiku-5-5");
  });
});

describe("the checklist", () => {
  const rows = (a: Partial<Parameters<typeof checklist>[0]>) => checklist({ check: null, checking: false, rec, pull: null, pullError: null, model: "", ...a }).map((r) => [r.title, r.mark, r.state, r.fix ?? ""]);
  it("waits before the first check", () => {
    expect(rows({ checking: true })[0]).toEqual(["Ollama", "wait", "looking…", ""]);
  });
  it("stops at Ollama when it isn't running, with the way to get it", () => {
    const down: LlmCheck = { ...up, server: { Err: "Ollama isn't answering at http://localhost:11434 (refused)" }, installed: null };
    expect(rows({ check: down })).toEqual([
      ["Ollama", "todo", "not running. Get it, open it, and this ticks itself.", "get"],
      ["qwen3:4b", "wait", "", ""],
      ["Try it", "wait", "", ""],
    ]);
  });
  it("offers the download with its size and why this model, then the test prompt", () => {
    const missing: LlmCheck = { ...up, server: { Ok: { version: "0.12.3", models: [] } }, installed: false };
    expect(rows({ check: missing })).toEqual([
      ["Ollama", "ok", "0.12.3 is running", ""],
      ["qwen3:4b", "todo", "not installed. A 2.6 GB download, picked for the 8 GB of memory on this Mac.", "download"],
      ["Try it", "wait", "", ""],
    ]);
    expect(rows({ check: missing, pullError: "no space" })[1][2]).toBe("the download failed: no space");
    expect(rows({ check: missing, pull: { model: "qwen3:4b", status: "pulling abc", completed: 1_300_000_000, total: 2_600_000_000 } })[1]).toEqual(["qwen3:4b", "wait", "Downloading qwen3:4b: 50% of 2.6 GB", "stop"]);
    expect(rows({ check: up })[2]).toEqual(["Try it", "wait", "", "try"]);
    expect(rows({ check: { ...up, answered: { Ok: 1400 } } })[2]).toEqual(["Try it", "ok", "answered in 1.4 s", "try"]);
    expect(rows({ check: { ...up, answered: { Err: "timed out" } } })[2]).toEqual(["Try it", "todo", "didn't answer: timed out", "try"]);
  });
  it("names the model chosen under Advanced", () => {
    expect(rows({ check: { ...up, model: "gemma3:4b", installed: false }, model: "gemma3:4b" })[1]).toEqual(["gemma3:4b", "todo", "not installed.", "download"]);
  });
  it("explains the model", () => {
    expect(whyThisModel(rec)).toBe("a 2.6 GB download, picked for the 8 GB of memory on this Mac");
    expect(whyThisModel({ ...rec, memory: null })).toBe("a 2.6 GB download");
    expect(whyThisModel(rec, "the server")).toBe("a 2.6 GB download, picked for the 8 GB of memory on the server");
  });
});

describe("the download's progress", () => {
  it("reads the events and ends on done", () => {
    const p1 = afterPull(null, { model: "m", status: "pulling manifest", completed: 0, total: 0, done: false, error: null });
    expect(pullLine(p1!)).toBe("Asking Ollama for m…");
    const p2 = afterPull(p1, { model: "m", status: "pulling abc", completed: 10, total: 100, done: false, error: null });
    expect(pullLine(p2!)).toBe("Downloading m: 10% of 0.0 GB");
    const p3 = afterPull(p2, { model: "m", status: "pulling abc", completed: 20, total: 0, done: false, error: null });
    expect(p3!.total).toBe(100);
    expect(afterPull(p3, { model: "m", status: "success", completed: 0, total: 0, done: true, error: null })).toBeNull();
    expect(pullLine({ model: "m", status: "verifying sha256 digest", completed: 0, total: 0 })).toBe("verifying sha256 digest…");
  });
});
