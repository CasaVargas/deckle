// SPDX-License-Identifier: GPL-3.0-or-later
import { useEffect, useState } from "react";
import { api } from "./api";
import { editorLine, followApp, isOwn, makeOwn } from "./dailyEditorView";
import type { SettingsPage } from "./settingsIndex";
import type { DailyEditorConfig, Settings } from "./types";

const DEFAULT: DailyEditorConfig = { enabled: false, provider: null, model: "", endpoint: "" };

/**
 * The paper's AI editor (local-model design §4): on or off, running where Settings → AI says
 * unless this paper is given its own service under Advanced.
 */
export default function DailyEditor({ value, onChange, onOpenSettings }: { value?: DailyEditorConfig; onChange: (value: DailyEditorConfig) => void; onOpenSettings?: (page: SettingsPage) => void }) {
  const config = value ?? DEFAULT;
  const [app, setApp] = useState<Settings | null>(null);
  useEffect(() => {
    api.getSettings().then(setApp).catch(() => setApp(null));
  }, [config.enabled]);
  const own = isOwn(config);
  const line = editorLine(config, app);
  const toAi = onOpenSettings && (
    <button className="link-action inline" onClick={() => onOpenSettings("ai")}>
      Settings → AI
    </button>
  );
  return <div className="daily-editor">
    <h3>AI editor</h3>
    <label className="check">
      <input type="checkbox" checked={config.enabled} onChange={(e) => onChange({ ...config, enabled: e.target.checked })} />
      Remove duplicate news, add section briefs, and write horoscopes
    </label>
    <p className="muted small">Off by default. When enabled, feed and local-news excerpts and horoscope signs go to the AI model. Saved reading, calendars, and Kindle highlights stay private. With interests, the editor also judges which stories are about them and names what Deckle noticed (from headlines only). If editing fails, the original paper still ships.</p>
    {config.enabled && <>
      <p className="muted small" role="status">
        {line.text} {line.needsChoice ? <>Choose one under {toAi ?? "Settings → AI"}.</> : own ? null : toAi && <>Change it under {toAi}.</>}
      </p>
      <details className="ai-advanced" open={own}>
        <summary>Advanced: a service just for this paper</summary>
        {own ? <>
          <div className="grid2">
            <label><span className="label">Provider</span>
              <select aria-label="Daily AI provider" value={config.provider ?? "openai"} onChange={(e) => onChange({ ...config, provider: e.target.value as NonNullable<DailyEditorConfig["provider"]> })}>
                <option value="ollama">Ollama, on this computer</option><option value="openai">OpenAI-compatible</option><option value="anthropic">Anthropic</option>
              </select>
            </label>
            <label><span className="label">Model</span><input value={config.model} placeholder="Provider default" onChange={(e) => onChange({ ...config, model: e.target.value })} /></label>
            <label><span className="label">{config.provider === "ollama" ? "Address" : "API base URL"}</span><input value={config.endpoint} placeholder={config.provider === "ollama" ? "http://localhost:11434" : "Provider default, or your local server"} spellCheck={false} onChange={(e) => onChange({ ...config, endpoint: e.target.value })} /></label>
          </div>
          {config.provider === "ollama"
            ? <p className="muted small">Nothing leaves this computer and no key is needed. Ollama must be running when the paper builds.</p>
            : <p className="muted small">Set {config.provider === "anthropic" ? "ANTHROPIC_API_KEY" : "OPENAI_API_KEY"} in the environment that runs Deckle. Local OpenAI-compatible servers can run without a key.</p>}
          <button className="ghost" onClick={() => onChange(followApp(config))}>Follow Settings → AI instead</button>
        </> : <>
          <p className="muted small">Every paper follows Settings → AI. One that should use a different service, or a key of its own from the environment, can name it here.</p>
          <button className="ghost" onClick={() => onChange(makeOwn(config, app))}>Use a service just for this paper</button>
        </>}
      </details>
    </>}
  </div>;
}
