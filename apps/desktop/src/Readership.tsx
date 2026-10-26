// SPDX-License-Identifier: GPL-3.0-or-later
import { useEffect, useState } from "react";
import { api } from "./api";
import { barTone, headline, howFar } from "./readershipView";
import type { Noticed, ReadershipSuggestion, ReadershipView } from "./types";

/** How far you read the paper, per section, with suggested edits (readership design §7). */
export default function Readership({ paper = null, onApplied }: { paper?: string | null; onApplied?: () => void }) {
  const [view, setView] = useState<ReadershipView | null>(null);
  const [busy, setBusy] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    api.readershipReport(paper).then(setView).catch((e) => setError(String(e)));
  }, [paper]);

  const act = (id: string, run: () => Promise<ReadershipView>) => {
    setBusy(id);
    setError(null);
    run()
      .then(setView)
      .catch((e) => setError(String(e)))
      .finally(() => setBusy(null));
  };

  if (!view) return error ? <section className="card readership"><p className="warn small">{error}</p></section> : null;
  const r = view.report;

  return (
    <section className="card readership" aria-label="How far you read">
      <div className="readership-head">
        <div>
          <h3>How far you read</h3>
          <p className="muted small">Each time the Kindle connects, Deckle notes how far you got in each edition, from the Kindle's own reading position. It stays on this Mac.</p>
        </div>
        {view.enabled && r.opened >= 3 && (
          <div className="readership-numbers">
            <div>
              <strong>{headline(r).replace("Opened ", "")}</strong>
              <span className="muted small">editions opened</span>
            </div>
            <div>
              <strong>{howFar(r).replace("About ", "")}</strong>
              <span className="muted small">usually{r.stopSection ? `, in ${r.stopSection}` : ""}</span>
            </div>
          </div>
        )}
      </div>

      {!view.enabled ? (
        <p className="muted small">
          Readership is off.{" "}
          <button className="link" onClick={() => act("on", () => api.readershipSet(true).then(() => api.readershipReport(paper)))} disabled={busy !== null}>
            Turn it on
          </button>
        </p>
      ) : r.opened < 3 ? (
        <p className="muted small">Plug your Kindle in after reading and this fills in.</p>
      ) : (
        <>
          <div className="readership-legend muted small">
            <span>Section, in paper order</span>
            <span>Editions where you got this far</span>
          </div>
          <ul className="readership-bars">
            {r.sections.map((s) => (
              <li key={s.key}>
                <span className="readership-name">{s.name}</span>
                <span className={`readership-bar ${barTone(s.share)}`} aria-hidden="true">
                  <span style={{ width: `${s.share ?? 0}%` }} />
                </span>
                <span className="readership-share">{s.share === null ? "new" : `${s.share}%`}</span>
              </li>
            ))}
          </ul>
          {r.suggestions.length > 0 && (
            <div className="readership-tips">
              <h4>Suggested edits</h4>
              {r.suggestions.map((s: ReadershipSuggestion) => (
                <div key={s.id} className="readership-tip">
                  <div className="grow">
                    <div className="readership-tip-title">{s.title}</div>
                    <div className="muted small">{s.why}</div>
                  </div>
                  {s.kind !== "stop" && (
                    <button
                      className="ghost"
                      disabled={busy !== null}
                      onClick={() => act(s.id, () => api.readershipApply(s.id, paper).finally(() => onApplied?.()))}
                    >
                      {busy === s.id ? "Saving…" : s.kind === "drop" ? "Drop it" : "Move up"}
                    </button>
                  )}
                  <button className="link" disabled={busy !== null} onClick={() => act(s.id, () => api.readershipDismiss(s.id, paper))}>
                    Dismiss
                  </button>
                </div>
              ))}
            </div>
          )}
        </>
      )}
      {view.noticed.length > 0 && (
        <div className="readership-tips noticed">
          <h4>Deckle noticed</h4>
          {view.noticed.map((n: Noticed) => (
            <div key={n.key} className="readership-tip">
              <div className="grow">
                <div className="readership-tip-title">{n.name}</div>
                <div className="muted small">{n.why}</div>
              </div>
              <button className="ghost" disabled={busy !== null} onClick={() => act(n.key, () => api.interestsAccept(n.key, paper).finally(() => onApplied?.()))}>
                {busy === n.key ? "Saving…" : "Add as interest"}
              </button>
              <button className="link" disabled={busy !== null} onClick={() => act(n.key, () => api.interestsDismiss(n.key, paper))}>
                Not interested
              </button>
            </div>
          ))}
        </div>
      )}
      {error && <p className="warn small">{error}</p>}
    </section>
  );
}
