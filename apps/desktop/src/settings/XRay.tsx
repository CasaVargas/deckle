// SPDX-License-Identifier: GPL-3.0-or-later
import type { Settings } from "../types";
import { Row, Switch } from "./Row";

interface Props {
  settings: Settings;
  set: <K extends keyof Settings>(k: K, v: Settings[K]) => void;
}

export default function XRay({ settings, set }: Props) {
  const llm = settings.llm_provider !== "off";
  return (
    <>
      <Row label="X-Ray" note="For books sent with features">
        <Switch label="X-Ray" checked={settings.xray} onChange={(on) => set("xray", on)} />
      </Row>
      <Row label="Wikipedia summaries" note="Short descriptions for people and places" online>
        <Switch label="Wikipedia summaries" checked={settings.wikipedia} onChange={(on) => set("wikipedia", on)} />
      </Row>
      <Row label="Use an AI model" note="Better character lists than the built-in method" online={llm}>
        <select value={settings.llm_provider} onChange={(e) => set("llm_provider", e.target.value as Settings["llm_provider"])} aria-label="AI model provider">
          <option value="off">Off</option>
          <option value="anthropic">Anthropic</option>
          <option value="openai">OpenAI-compatible</option>
        </select>
      </Row>
      {llm && (
        <div className="set-sub">
          <Row label="Model">
            <input type="text" value={settings.llm_model} onChange={(e) => set("llm_model", e.target.value)} placeholder="claude-sonnet-5-5" aria-label="Model" />
          </Row>
          {settings.llm_provider === "openai" && (
            <Row label="Endpoint" note="Ollama, LM Studio…">
              <input type="text" value={settings.llm_endpoint} onChange={(e) => set("llm_endpoint", e.target.value)} placeholder="http://localhost:11434/v1" aria-label="Endpoint" />
            </Row>
          )}
          <Row label="API key" note="Stored on this computer. Empty uses ANTHROPIC_API_KEY / OPENAI_API_KEY.">
            <input type="password" value={settings.llm_api_key} onChange={(e) => set("llm_api_key", e.target.value)} placeholder="Optional" autoComplete="off" aria-label="API key" />
          </Row>
        </div>
      )}
    </>
  );
}
