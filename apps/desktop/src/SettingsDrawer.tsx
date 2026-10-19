// SPDX-License-Identifier: GPL-3.0-or-later
import type { DeviceRow, PreviewerStatus, Settings } from "./types";

interface Props {
  open: boolean;
  settings: Settings;
  previewer: PreviewerStatus | null;
  devices: DeviceRow[];
  onChange: (s: Settings) => void;
  onDeviceOptions: (key: string, cde: "pdoc" | "ebok" | null, verified: boolean) => void;
  onClose: () => void;
  onPickPreviewer: () => void;
}

export default function SettingsDrawer({ open, settings, previewer, devices, onChange, onDeviceOptions, onClose, onPickPreviewer }: Props) {
  if (!open) return null;
  const set = <K extends keyof Settings>(k: K, v: Settings[K]) => onChange({ ...settings, [k]: v });
  return (
    <aside className="drawer" role="dialog" aria-label="Settings">
      <header>
        <h2>Settings</h2>
        <button className="icon" aria-label="Close settings" onClick={onClose}>
          ✕
        </button>
      </header>

      <section>
        <h3>Kindle Previewer</h3>
        <p className="hint">
          {previewer?.found ? (
            <>
              Found version {previewer.version} at <code>{previewer.path}</code>
            </>
          ) : (
            <>
              Not found. <a href={previewer?.download_url} target="_blank" rel="noreferrer">Download Kindle Previewer</a> (free, from Amazon) and
              restart Deckle, or choose the command line binary below.
            </>
          )}
        </p>
        <div className="field-row">
          <input
            type="text"
            value={settings.previewer_path ?? ""}
            placeholder="Auto-detect"
            onChange={(e) => set("previewer_path", e.target.value || null)}
          />
          <button onClick={onPickPreviewer}>Choose…</button>
        </div>
      </section>

      <section>
        <h3>Output</h3>
        <label className="field">
          <span>Default mode</span>
          <select value={settings.cde} onChange={(e) => set("cde", e.target.value as Settings["cde"])}>
            <option value="auto">Automatic — the cover rule decides per Kindle</option>
            <option value="pdoc">Cover first (PDOC) — covers on every Kindle</option>
            <option value="ebok">Features first (EBOK) — X-Ray / Word Wise capable</option>
          </select>
        </label>
        <label className="field check">
          <input type="checkbox" checked={settings.pages_auto} onChange={(e) => set("pages_auto", e.target.checked)} />
          <span>Add page numbers (approximate, like the store does)</span>
        </label>
      </section>

      <section>
        <h3>Kindles</h3>
        {devices.length === 0 && <p className="hint">Connect a Kindle to set per-device options.</p>}
        {devices.map((d) => (
          <div key={d.key} className="device-card">
            <div className="device-name">
              {d.model_hint} <span className="tag">{d.kind === "mtp" ? "MTP" : "USB"}</span>
            </div>
            <label className="field">
              <span>Mode on this Kindle</span>
              <select
                value={d.cde_override ?? "auto"}
                onChange={(e) => onDeviceOptions(d.key, e.target.value === "auto" ? null : (e.target.value as "pdoc" | "ebok"), d.verified_real_asin)}
              >
                <option value="auto">Automatic</option>
                <option value="pdoc">Cover first (PDOC)</option>
                <option value="ebok">Features first (EBOK)</option>
              </select>
            </label>
            <label className="field check">
              <input type="checkbox" checked={d.verified_real_asin} onChange={(e) => onDeviceOptions(d.key, d.cde_override, e.target.checked)} />
              <span>Keeps Deckle's X-Ray for real ASINs (spike 2 verified on this Kindle)</span>
            </label>
          </div>
        ))}
      </section>

      <section className="coming">
        <h3>
          X-Ray <span className="tag">M3</span>
        </h3>
        <label className="field">
          <span>LLM provider</span>
          <select value={settings.llm_provider} onChange={(e) => set("llm_provider", e.target.value as Settings["llm_provider"])}>
            <option value="off">Off (heuristic X-Ray)</option>
            <option value="anthropic">Anthropic API</option>
            <option value="openai">OpenAI-compatible endpoint (Ollama, LM Studio…)</option>
          </select>
        </label>
        {settings.llm_provider !== "off" && (
          <>
            <label className="field">
              <span>Model</span>
              <input type="text" value={settings.llm_model} onChange={(e) => set("llm_model", e.target.value)} placeholder="claude-sonnet-5-5" />
            </label>
            {settings.llm_provider === "openai" && (
              <label className="field">
                <span>Endpoint</span>
                <input type="text" value={settings.llm_endpoint} onChange={(e) => set("llm_endpoint", e.target.value)} placeholder="http://localhost:11434/v1" />
              </label>
            )}
            <p className="hint">API keys are read from the environment (ANTHROPIC_API_KEY / OPENAI_API_KEY) when M3 lands; nothing is stored here.</p>
          </>
        )}
        <label className="field check">
          <input type="checkbox" checked={settings.wikipedia} onChange={(e) => set("wikipedia", e.target.checked)} />
          <span>Wikipedia summaries for X-Ray entities (network)</span>
        </label>
      </section>

      <section className="coming">
        <h3>
          Word Wise <span className="tag">M4</span>
        </h3>
        <label className="field">
          <span>Hint level: {settings.word_wise_level}</span>
          <input type="range" min={1} max={5} value={settings.word_wise_level} onChange={(e) => set("word_wise_level", Number(e.target.value))} />
        </label>
      </section>

      <section>
        <h3>Covers on MTP Kindles</h3>
        <label className="field check">
          <input type="checkbox" checked={settings.amazon_match} onChange={(e) => set("amazon_match", e.target.checked)} />
          <span>Match books to an Amazon edition (Open Library lookup, network) so EBOK can keep the store cover</span>
        </label>
      </section>
    </aside>
  );
}
