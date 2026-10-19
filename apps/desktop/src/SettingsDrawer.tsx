// SPDX-License-Identifier: GPL-3.0-or-later
import type { ReactNode } from "react";
import { api } from "./api";
import type { DeviceRow, PreviewerStatus, Settings } from "./types";

const REPO = "https://github.com/prjoni99/deckle";

/** An external link: opens in the default browser (a plain anchor would do nothing in the app). */
function Link({ href, children }: { href: string; children: ReactNode }) {
  return (
    <a
      href={href}
      onClick={(e) => {
        e.preventDefault();
        api.openLink(href).catch(() => {});
      }}
    >
      {children}
    </a>
  );
}

interface Props {
  open: boolean;
  version: string;
  settings: Settings;
  previewer: PreviewerStatus | null;
  devices: DeviceRow[];
  onChange: (s: Settings) => void;
  onDeviceOptions: (key: string, cde: "pdoc" | "ebok" | null, verified: boolean) => void;
  onClose: () => void;
  onPickPreviewer: () => void;
}

export default function SettingsDrawer({ open, version, settings, previewer, devices, onChange, onDeviceOptions, onClose, onPickPreviewer }: Props) {
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
              Not found. {previewer?.download_url ? <Link href={previewer.download_url}>Download Kindle Previewer</Link> : "Download Kindle Previewer"} (free, from Amazon) and
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

      <section>
        <h3>
          X-Ray
        </h3>
        <label className="field check">
          <input type="checkbox" checked={settings.xray} onChange={(e) => set("xray", e.target.checked)} />
          <span>Build an X-Ray for every book that ships with features (EBOK)</span>
        </label>
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
            <label className="field">
              <span>API key</span>
              <input type="password" value={settings.llm_api_key} onChange={(e) => set("llm_api_key", e.target.value)} placeholder="optional; else ANTHROPIC_API_KEY / OPENAI_API_KEY" autoComplete="off" />
            </label>
            <p className="hint">Stored in Deckle's settings file on this computer. Leave empty to use the environment variable, or for local servers that need no key.</p>
          </>
        )}
        <label className="field check">
          <input type="checkbox" checked={settings.wikipedia} onChange={(e) => set("wikipedia", e.target.checked)} />
          <span>Wikipedia summaries for X-Ray entities (network)</span>
        </label>
      </section>

      <section>
        <h3>Word Wise</h3>
        <label className="field check">
          <input type="checkbox" checked={settings.word_wise} onChange={(e) => set("word_wise", e.target.checked)} />
          <span>Add Word Wise hints to every book that ships with features (EBOK)</span>
        </label>
        <label className="field">
          <span>
            Hint level: {settings.word_wise_level} ({settings.word_wise_level === 5 ? "all hints; the Kindle's slider decides" : settings.word_wise_level === 1 ? "rarest words only" : "fewer hints"})
          </span>
          <input type="range" min={1} max={5} value={settings.word_wise_level} disabled={!settings.word_wise} onChange={(e) => set("word_wise_level", Number(e.target.value))} />
        </label>
        <p className="hint">
          The first book downloads Deckle's dictionary (about 12 MB, from Wiktionary). Kindles that already have Amazon's English Word Wise dictionary use it;
          others get Deckle's, shown when the Kindle's Word Wise language is set to 中文 (Chinese). The hints are English either way.
        </p>
      </section>

      <section>
        <h3>Covers on MTP Kindles</h3>
        <label className="field check">
          <input type="checkbox" checked={settings.amazon_match} onChange={(e) => set("amazon_match", e.target.checked)} />
          <span>Match books to an Amazon edition (Open Library lookup, network) so EBOK can keep the store cover</span>
        </label>
      </section>
      <section className="about">
        <h3>About</h3>
        <p className="hint">
          Deckle {version} · GPL-3.0-or-later · <Link href={`${REPO}/releases`}>Check for a newer version</Link> ·{" "}
          <Link href={REPO}>Source</Link>
        </p>
        <p className="hint">
          Deckle never checks for updates or sends anything about you. It goes online only for: the Word Wise dictionary
          (downloaded once from GitHub), and, when you turn them on, an LLM for X-Ray, Wikipedia summaries, and Amazon edition
          matching (Open Library). A Kindle itself may ask Amazon about books sent with features (EBOK).
        </p>
        <p className="hint">
          Word Wise definitions come from <Link href="https://en.wiktionary.org/">Wiktionary</Link> via{" "}
          <Link href="https://kaikki.org/">kaikki.org</Link> (
          <Link href="https://creativecommons.org/licenses/by-sa/4.0/">CC BY-SA 4.0</Link>), word frequencies from{" "}
          <Link href="https://github.com/rspeer/wordfreq">wordfreq</Link> (CC BY-SA 4.0). Built on the research and code of
          jhowell (KFX Input, kfxlib), xxyzz (WordDumb, Proficiency), jloutsch (kfxgen) and seapy (kindle-cli). Kindle
          Previewer is Amazon's and is never bundled. Kindle is a trademark of Amazon; Deckle is not affiliated with Amazon.
        </p>
      </section>
    </aside>
  );
}
