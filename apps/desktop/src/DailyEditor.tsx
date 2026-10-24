// SPDX-License-Identifier: GPL-3.0-or-later
import { useRef, useState } from "react";
import { api } from "./api";
import type { DailyEditorConfig } from "./types";

const DEFAULT: DailyEditorConfig = { enabled: false, provider: "openai", model: "", endpoint: "" };
export default function DailyEditor({ value, onChange }: { value?: DailyEditorConfig; onChange: (value: DailyEditorConfig) => void }) {
  const config = value ?? DEFAULT;
  const latest = useRef(config);
  latest.current = config;
  const [note, setNote] = useState<string | null>(null);
  const copyProvider = async () => {
    try {
      const settings = await api.getSettings();
      if (settings.llm_provider !== "openai" && settings.llm_provider !== "anthropic") {
        setNote("Choose an AI provider in Settings → X-Ray first.");
        return;
      }
      onChange({ ...latest.current, provider: settings.llm_provider, model: settings.llm_model, endpoint: settings.llm_endpoint });
      setNote("Provider copied. Daily reads its API key from the environment; saved book API keys are not copied.");
    } catch (e) { setNote(String(e)); }
  };
  return <div className="daily-editor">
    <h3>AI editor</h3>
    <label className="check">
      <input type="checkbox" checked={config.enabled} onChange={(e) => onChange({ ...config, enabled: e.target.checked })} />
      Remove duplicate news, add section briefs, and write horoscopes
    </label>
    <p className="muted small">Off by default. When enabled, feed and local-news excerpts and horoscope signs go to your provider. Saved reading, calendars, and Kindle highlights stay private. If editing fails, the original paper still ships.</p>
    {config.enabled && <>
      <div className="grid2">
        <label><span className="label">Provider</span>
          <select aria-label="Daily AI provider" value={config.provider} onChange={(e) => onChange({ ...config, provider: e.target.value as DailyEditorConfig["provider"] })}>
            <option value="openai">OpenAI-compatible</option><option value="anthropic">Anthropic</option>
          </select>
        </label>
        <label><span className="label">Model</span><input value={config.model} placeholder="Provider default" onChange={(e) => onChange({ ...config, model: e.target.value })} /></label>
        <label><span className="label">API base URL</span><input value={config.endpoint} placeholder="Provider default, or your local server" spellCheck={false} onChange={(e) => onChange({ ...config, endpoint: e.target.value })} /></label>
      </div>
      <button className="ghost" onClick={copyProvider}>Use Books provider</button>
      <p className="muted small">Set {config.provider === "anthropic" ? "ANTHROPIC_API_KEY" : "OPENAI_API_KEY"} in the environment that runs Deckle. Local OpenAI-compatible servers can run without a key. Save above to apply.</p>
    </>}
    {note && <p className="muted small" role="status">{note}</p>}
  </div>;
}
