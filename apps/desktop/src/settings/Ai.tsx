// SPDX-License-Identifier: GPL-3.0-or-later
import { useCallback, useEffect, useRef, useState } from "react";
import { api } from "../api";
import { aiChoice, afterPull, checklist, choose, expectation, gb, providerName, whyThisModel } from "../aiView";
import type { CheckRow, Pull } from "../aiView";
import { useCapabilities } from "../capabilities";
import type { LlmCheck, LlmRecommendation, Settings } from "../types";
import { Link } from "./Row";

interface Props {
  settings: Settings;
  onChange: (s: Settings) => void;
}

const MARK = { ok: "✓", todo: "!", wait: "…" } as const;
/** How often the page asks Ollama again while it isn't answering: the tick appears when the app opens. */
const POLL_MS = 4000;

/**
 * AI (local-model design §3): off, a model on this computer that sets itself up, or a service
 * online. One choice for X-Ray, chapter recaps and the Daily editor.
 */
export default function Ai({ settings, onChange }: Props) {
  const caps = useCapabilities();
  const choice = aiChoice(settings);
  const latest = useRef(settings);
  latest.current = settings;
  const patch = (p: Partial<Settings>) => onChange({ ...latest.current, ...p });

  const [rec, setRec] = useState<LlmRecommendation | null>(null);
  const [check, setCheck] = useState<LlmCheck | null>(null);
  const [checking, setChecking] = useState(false);
  const [pull, setPull] = useState<Pull | null>(null);
  const [pullError, setPullError] = useState<string | null>(null);
  const [advanced, setAdvanced] = useState(false);

  useEffect(() => {
    api.llmRecommended().then(setRec).catch(() => setRec(null));
  }, []);

  const runCheck = useCallback(async () => {
    setChecking(true);
    try {
      setCheck(await api.llmCheck(latest.current.llm_endpoint, latest.current.llm_model));
    } catch {
      setCheck(null);
    } finally {
      setChecking(false);
    }
  }, []);

  // On this computer: check now, and keep asking while Ollama isn't there (a fresh install
  // takes a minute to open), or when the address or model changes.
  const endpoint = settings.llm_endpoint;
  const model = settings.llm_model;
  useEffect(() => {
    if (choice !== "local") return;
    setCheck(null);
    runCheck();
  }, [choice, endpoint, model, runCheck]);
  const waitingForOllama = choice === "local" && (!check || "Err" in check.server);
  useEffect(() => {
    if (!waitingForOllama) return;
    const t = setInterval(() => {
      if (!checking) runCheck();
    }, POLL_MS);
    return () => clearInterval(t);
  }, [waitingForOllama, checking, runCheck]);

  // The download's progress, and a fresh check when it's over.
  useEffect(() => {
    const off = api.onLlmPull((e) => {
      setPull((prev) => afterPull(prev, e));
      if (e.done) {
        setPullError(e.error && e.status === "failed" ? e.error : null);
        runCheck();
      }
    });
    return () => {
      off.then((f) => f());
    };
  }, [runCheck]);

  const download = () => {
    const m = settings.llm_model.trim() || rec?.model || "";
    setPullError(null);
    setPull({ model: m, status: "pulling manifest", completed: 0, total: 0 });
    api.llmPull(settings.llm_endpoint, m).catch((e) => {
      setPull(null);
      setPullError(String(e));
    });
  };
  const fix = (row: CheckRow) => {
    if (row.fix === "get" && rec) api.openLink(rec.ollama_url).catch(() => {});
    else if (row.fix === "download") download();
    else if (row.fix === "stop") api.llmPullStop().catch(() => {});
    else if (row.fix === "try") runCheck();
  };
  const FIX_LABEL: Record<NonNullable<CheckRow["fix"]>, string> = {
    get: "Get Ollama",
    download: rec && (settings.llm_model.trim() || rec.model) === rec.model ? `Download (${gb(rec.download)})` : "Download",
    stop: "Stop",
    try: check?.answered ? "Try again" : "Try it",
  };
  const here = caps.web ? "the server" : "this Mac";
  const rows = checklist({ check, checking, rec, pull, pullError, model: settings.llm_model.trim(), here });
  const installed = check && "Ok" in check.server ? check.server.Ok.models : [];
  const modelChoices = Array.from(new Set([rec?.model ?? "", ...installed.map((m) => m.name)].filter(Boolean)));

  return (
    <div className="set-stack">
      <p className="overview-lead">One choice for X-Ray's descriptions, chapter recaps and the Daily's editor. Off by default; each of those stays off until you turn it on.</p>

      <fieldset className="mode-cards">
        <legend>Where the AI runs</legend>
        <div className="mode-grid ai-grid">
          <label className={`mode-card ${choice === "off" ? "on" : ""}`}>
            <span className="mode-head">
              <input type="radio" name="ai-choice" checked={choice === "off"} onChange={() => onChange(choose(settings, "off"))} />
              <strong>Off</strong>
            </span>
            <span className="muted">X-Ray uses Deckle's own lists of names. No recaps, no editor.</span>
          </label>
          <label className={`mode-card ${choice === "local" ? "on" : ""}`}>
            <span className="mode-head">
              <input type="radio" name="ai-choice" checked={choice === "local"} onChange={() => onChange(choose(settings, "local"))} />
              <strong>On {here}</strong>
            </span>
            <span className="muted">Free, private, slower. A small model through Ollama; nothing leaves {here}.</span>
          </label>
          <label className={`mode-card ${choice === "hosted" ? "on" : ""}`}>
            <span className="mode-head">
              <input type="radio" name="ai-choice" checked={choice === "hosted"} onChange={() => onChange(choose(settings, "hosted"))} />
              <strong>A service online</strong>
              <span className="tag">Online</span>
            </span>
            <span className="muted">Anthropic or an OpenAI-compatible service. Better prose, costs cents a book, sends text to them.</span>
          </label>
        </div>
      </fieldset>

      {choice === "local" && (
        <section className="set-card set-pad" aria-label="On this computer">
          <div className="set-card-head">
            <h3 className="set-card-title">Setting itself up</h3>
          </div>
          <div className="setup-list ai-checks" role="list">
            {rows.map((r) => (
              <div key={r.key} className="setup-row" role="listitem">
                <span className={`setup-mark ${r.mark === "wait" ? "optional" : r.mark}`} aria-label={r.mark === "ok" ? "Done" : r.mark === "todo" ? "Needs doing" : "Waiting"}>
                  {MARK[r.mark]}
                </span>
                <span className="setup-text">
                  <strong>{r.title}</strong> <span className="muted">{r.state}</span>
                </span>
                {r.fix && (
                  <button className="ghost" onClick={() => fix(r)} disabled={checking && r.fix === "try"}>
                    {FIX_LABEL[r.fix]}
                  </button>
                )}
              </div>
            ))}
          </div>
          {pull && pull.total > 0 && (
            <div className="progress" aria-label="Download progress">
              <div className="progress-track">
                <div className="progress-fill" style={{ width: `${Math.min(100, (pull.completed * 100) / pull.total)}%` }} />
              </div>
            </div>
          )}
          {rec && <p className="muted small">{rec.model.replace(/^./, (c) => c.toUpperCase())}: {whyThisModel(rec, here)}. {expectation(here)}</p>}
          <details className="ai-advanced" open={advanced} onToggle={(e) => setAdvanced((e.target as HTMLDetailsElement).open)}>
            <summary>Advanced</summary>
            <div className="ai-fields">
              <label>
                <span className="tk-label">Model</span>
                <select value={settings.llm_model.trim() && !modelChoices.includes(settings.llm_model.trim()) ? "__other" : settings.llm_model.trim()} onChange={(e) => patch({ llm_model: e.target.value === "__other" ? settings.llm_model : e.target.value })}>
                  <option value="">{rec ? `${rec.model} (recommended)` : "Recommended"}</option>
                  {modelChoices
                    .filter((m) => m !== rec?.model)
                    .map((m) => (
                      <option key={m} value={m}>
                        {m}
                      </option>
                    ))}
                  {settings.llm_model.trim() && !modelChoices.includes(settings.llm_model.trim()) && <option value="__other">{settings.llm_model.trim()}</option>}
                </select>
                <span className="muted small">Every model Ollama has, from `ollama pull`.</span>
              </label>
              <label>
                <span className="tk-label">Address</span>
                <input type="text" value={settings.llm_endpoint} onChange={(e) => patch({ llm_endpoint: e.target.value })} placeholder="http://localhost:11434" />
                <span className="muted small">Ollama on another computer on your network.</span>
              </label>
            </div>
          </details>
        </section>
      )}

      {choice === "hosted" && (
        <section className="set-card set-pad" aria-label="A service online">
          <div className="set-card-head">
            <h3 className="set-card-title">{providerName(settings)}</h3>
            <span className="tag">Online</span>
          </div>
          <p className="muted">Text from your books and the paper's news excerpts go to this service, billed to your key. Saved reading, calendars and highlights never do.</p>
          <div className="ai-fields">
            <label>
              <span className="tk-label">Provider</span>
              <select value={settings.llm_provider} onChange={(e) => patch({ llm_provider: e.target.value as Settings["llm_provider"], llm_model: "", llm_endpoint: "" })}>
                <option value="anthropic">Anthropic</option>
                <option value="openai">OpenAI-compatible (OpenAI, LM Studio…)</option>
              </select>
            </label>
            <label>
              <span className="tk-label">Model</span>
              <input type="text" value={settings.llm_model} onChange={(e) => patch({ llm_model: e.target.value })} placeholder={settings.llm_provider === "openai" ? "gpt-5-mini" : "claude-sonnet-5-5"} />
            </label>
            {settings.llm_provider === "openai" && (
              <label>
                <span className="tk-label">Endpoint</span>
                <input type="text" value={settings.llm_endpoint} onChange={(e) => patch({ llm_endpoint: e.target.value })} placeholder="https://api.openai.com/v1" />
                <span className="muted small">LM Studio on this Mac: http://localhost:1234/v1, no key.</span>
              </label>
            )}
            <label>
              <span className="tk-label">API key</span>
              <input type="password" value={settings.llm_api_key} onChange={(e) => patch({ llm_api_key: e.target.value })} placeholder="Optional" autoComplete="off" />
              <span className="muted small">Stored on {here}. Empty uses ANTHROPIC_API_KEY / OPENAI_API_KEY.</span>
            </label>
          </div>
          <table className="ai-prices">
            <caption className="muted small">About, for a 100,000-word novel's recaps (the paper's editor is a fraction of that a day)</caption>
            <tbody>
              <tr>
                <td>Anthropic, Claude Sonnet 5.5</td>
                <td>≈ $0.30 a book</td>
              </tr>
              <tr>
                <td>Anthropic, Claude Haiku 5.5</td>
                <td>≈ $0.02 a book, plainer prose</td>
              </tr>
              <tr>
                <td>On {here}, through Ollama</td>
                <td>free, about 20 minutes a book</td>
              </tr>
            </tbody>
          </table>
          <p className="muted small">
            Keys: <Link href="https://console.anthropic.com/">console.anthropic.com</Link> or <Link href="https://platform.openai.com/">platform.openai.com</Link>. A book is never recapped twice.
          </p>
        </section>
      )}
    </div>
  );
}
