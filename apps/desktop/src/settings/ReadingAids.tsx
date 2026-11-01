// SPDX-License-Identifier: GPL-3.0-or-later
import type { Settings } from "../types";
import { Switch } from "./Row";
import { useCapabilities } from "../capabilities";

interface Props {
  settings: Settings;
  set: <K extends keyof Settings>(k: K, v: Settings[K]) => void;
}

/** Word Wise's example: two hard words, hinted when the level reaches them (Deckle's own sentence). */
function Example({ level, on }: { level: number; on: boolean }) {
  const hint = (word: string, gloss: string, from: number) => (on && level >= from ? <ruby>{word}<rt>{gloss}</rt></ruby> : word);
  return (
    <figure className="ww-example" aria-label="How Word Wise looks at this level">
      <p>
        The keeper's {hint("tenebrous", "dark, gloomy", 1)} halls ran on for miles, and every {hint("stalactite", "hanging stone point", 3)} dripped in time with the tide.
      </p>
      <figcaption>Example at this level</figcaption>
    </figure>
  );
}

/** The chapter recaps toggle's help (AI extras design §8). */
const RECAPS_HELP =
  "Adds a short AI-written recap at the end of each chapter. Uses your LLM setting: with a hosted provider the book's text is sent to it and billed to your key; with a local server (Ollama, LM Studio) nothing leaves this computer.";

/** Reading aids (Settings redesign §6): Word Wise, X-Ray and chapter recaps, help that appears while you read. */
export default function ReadingAids({ settings, set }: Props) {
  const caps = useCapabilities();
  const ai = settings.llm_provider !== "off";
  return (
    <div className="set-stack">
      <p className="overview-lead">Help that appears while you read. Word Wise and X-Ray need a book sent as a Kindle book; chapter recaps go to every e-reader.</p>

      <section className="set-card set-pad" aria-label="Word Wise">
        <div className="set-card-head">
          <h3 className="set-card-title">Word Wise</h3>
          <Switch label="Word Wise" checked={settings.word_wise} onChange={(on) => set("word_wise", on)} />
        </div>
        <div className="ww-body">
          <div className="ww-controls">
            <p className="muted">Short meanings above harder words, so you don't stop to look them up.</p>
            <label htmlFor="ww-level" className="tk-label">
              How many hints
            </label>
            <input id="ww-level" type="range" min={1} max={5} value={settings.word_wise_level} disabled={!settings.word_wise} onChange={(e) => set("word_wise_level", Number(e.target.value))} />
            <div className="range-ends">
              <span>Rarest words only</span>
              <span>Every hint</span>
            </div>
            <p className="muted small">At every hint, the Kindle's own slider decides how many show. The first book downloads a 12 MB dictionary.</p>
          </div>
          <Example level={settings.word_wise_level} on={settings.word_wise} />
        </div>
      </section>

      <section className="set-card set-pad" aria-label="X-Ray">
        <div className="set-card-head">
          <h3 className="set-card-title">X-Ray</h3>
          <Switch label="X-Ray" checked={settings.xray} onChange={(on) => set("xray", on)} />
        </div>
        <p className="muted">Who's who and what's what: tap a name in the book to see where it comes up.</p>
        <fieldset className="mode-cards">
          <legend>Who finds the people and places</legend>
          <div className="mode-grid">
            <label className={`mode-card ${!ai ? "on" : ""}`}>
              <span className="mode-head">
                <input type="radio" name="xray-by" checked={!ai} disabled={!settings.xray && !settings.chapter_recaps} onChange={() => set("llm_provider", "off")} />
                <strong>Deckle, on this Mac</strong>
              </span>
              <span className="muted">Good lists of names; nothing goes online.</span>
            </label>
            <label className={`mode-card ${ai ? "on" : ""}`}>
              <span className="mode-head">
                <input type="radio" name="xray-by" checked={ai} disabled={!settings.xray && !settings.chapter_recaps} onChange={() => set("llm_provider", "anthropic")} />
                <strong>An AI model</strong>
                <span className="tag">Online</span>
              </span>
              <span className="muted">Better character lists. Sends text from the book to the provider you pick.</span>
            </label>
          </div>
        </fieldset>
        {ai && (
          <div className="ai-fields">
            <label>
              <span className="tk-label">Provider</span>
              <select value={settings.llm_provider} onChange={(e) => set("llm_provider", e.target.value as Settings["llm_provider"])}>
                <option value="anthropic">Anthropic</option>
                <option value="openai">OpenAI-compatible (Ollama, LM Studio…)</option>
              </select>
            </label>
            <label>
              <span className="tk-label">Model</span>
              <input type="text" value={settings.llm_model} onChange={(e) => set("llm_model", e.target.value)} placeholder="claude-sonnet-5-5" />
            </label>
            {settings.llm_provider === "openai" && (
              <label>
                <span className="tk-label">Endpoint</span>
                <input type="text" value={settings.llm_endpoint} onChange={(e) => set("llm_endpoint", e.target.value)} placeholder="http://localhost:11434/v1" />
              </label>
            )}
            <label>
              <span className="tk-label">API key</span>
              <input type="password" value={settings.llm_api_key} onChange={(e) => set("llm_api_key", e.target.value)} placeholder="Optional" autoComplete="off" />
              <span className="muted small">Stored on {caps.web ? "the server" : "this computer"}. Empty uses ANTHROPIC_API_KEY / OPENAI_API_KEY.</span>
            </label>
          </div>
        )}
        <label className="tick-row">
          <input type="checkbox" checked={settings.wikipedia} disabled={!settings.xray} onChange={(e) => set("wikipedia", e.target.checked)} />
          <span>
            <strong>Add Wikipedia summaries</strong> <span className="muted">for real people and places</span> <span className="tag">Online</span>
          </span>
        </label>
      </section>

      <section className="set-card set-pad" aria-label="Chapter recaps">
        <div className="set-card-head">
          <h3 className="set-card-title">
            Chapter recaps <span className="tag">AI</span>
          </h3>
          <Switch label="Chapter recaps" checked={settings.chapter_recaps} onChange={(on) => set("chapter_recaps", on)} />
        </div>
        <p className="muted">{RECAPS_HELP}</p>
        {settings.chapter_recaps && !ai && <p className="muted small">Needs an AI model: choose "An AI model" under X-Ray above.</p>}
      </section>
    </div>
  );
}
