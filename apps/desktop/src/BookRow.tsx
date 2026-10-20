// SPDX-License-Identifier: GPL-3.0-or-later
import { useState } from "react";
import type { BookState, PlanRow } from "./types";
import { STAGES, STAGE_LABEL } from "./types";

interface Props {
  book: BookState;
  busy: boolean;
  /** What the cover rule will do on the current target (undefined until planned). */
  plan?: PlanRow;
  onOverride: (id: string, cde: "pdoc" | "ebok" | null) => void;
  onAsin: (id: string, asin: string | null) => void;
  onToggleReport: (id: string) => void;
  onRemove: (id: string) => void;
  onReveal: (path: string) => void;
}

function badge(book: BookState, plan: PlanRow | undefined) {
  if (book.mode === "error") return { cls: "badge error", text: "Error", title: book.reason };
  if (book.mode === "ready") return { cls: "badge ready", text: "Ready", title: book.reason };
  if (!plan) return { cls: "badge cover", text: "Cover", title: book.reason };
  if (plan.cde === "ebok") {
    return { cls: "badge features", text: plan.write_thumbnail ? "Everything" : "Features", title: plan.reason };
  }
  return { cls: "badge cover", text: "Cover", title: plan.reason };
}

const ASIN = /^[A-Z0-9]{10}$/;

export default function BookRowView({ book, busy, plan, onOverride, onAsin, onToggleReport, onRemove, onReveal }: Props) {
  const [menuOpen, setMenuOpen] = useState(false);
  /** The ASIN field's text while it is open; `null` when closed. */
  const [asinDraft, setAsinDraft] = useState<string | null>(null);
  const draft = (asinDraft ?? "").trim().toUpperCase();
  const draftValid = ASIN.test(draft);
  const saveAsin = () => {
    if (!draftValid) return;
    onAsin(book.id, draft);
    setAsinDraft(null);
  };
  const effectiveCde = book.cdeOverride ?? plan?.cde ?? "pdoc";
  const b = badge(book, plan);
  const stageIndex = book.stage ? STAGES.indexOf(book.stage as (typeof STAGES)[number]) : -1;

  let statusLine: React.ReactNode = null;
  if (book.status === "running") {
    statusLine = (
      <div className="progress" aria-label={`${STAGE_LABEL[book.stage ?? ""] ?? book.stage}`}>
        <div className="progress-track">
          <div className="progress-fill" style={{ width: `${((stageIndex + 1) / STAGES.length) * 100}%` }} />
        </div>
        <span className="progress-label">{STAGE_LABEL[book.stage ?? ""] ?? book.stage}…</span>
      </div>
    );
  } else if (book.status === "done") {
    statusLine = (
      <div className="result ok">
        Done{book.mode === "ready" ? " · sent as is" : ""}
        {book.pages ? ` · ${book.pages} pages` : ""}
        {book.delivered && (
          <button className="link" onClick={() => onReveal(book.delivered!)}>
            Show file
          </button>
        )}
        {book.runWarnings.length > 0 && <span className="warn"> · {book.runWarnings.length} warning{book.runWarnings.length > 1 ? "s" : ""}</span>}
        {book.shipped && (
          <div className="shipped" title={book.shipped.reason}>
            {book.shipped.cde} · {book.shipped.reason}
          </div>
        )}
        {(book.xray || book.wordWise) && (
          <div className="shipped">
            {book.xray && (
              <>
                X-Ray: {book.xray.people} people, {book.xray.terms} terms (
                {book.xray.source === "llm" ? "LLM" : book.xray.source === "wikipedia" ? "Wikipedia" : "basic"})
              </>
            )}
            {book.xray && book.wordWise && " · "}
            {book.wordWise && (
              <>
                Word Wise: {book.wordWise.hints.toLocaleString()} hints (level {book.wordWise.level},{" "}
                {book.wordWise.dictionary === "device" ? "Kindle's dictionary" : "Deckle's dictionary"})
              </>
            )}
          </div>
        )}
      </div>
    );
  } else if (book.status === "failed") {
    statusLine = <div className="result fail">{book.runError}</div>;
  } else if (book.status === "queued") {
    statusLine = <div className="result muted">Waiting…</div>;
  } else if (book.error) {
    statusLine = <div className="result fail">{book.error}</div>;
  } else if (book.mode === "ready") {
    statusLine = <div className="result muted">Already a Kindle book: sent as is, no conversion.</div>;
  } else if (book.warnings.length > 0) {
    statusLine = <div className="result muted">{book.warnings.join("; ")}</div>;
  }

  return (
    <li className={`row ${book.status}`}>
      <div className="cover-thumb" aria-hidden="true">
        {book.cover ? <img src={book.cover} alt="" /> : <div className="cover-placeholder">{book.title.slice(0, 1)}</div>}
      </div>
      <div className="meta">
        <div className="title-line">
          <span className="title" title={book.path}>
            {book.title}
          </span>
          <span className={b.cls} title={b.title}>
            {b.text}
          </span>
        </div>
        <div className="byline">
          {book.authors || <em>Unknown author</em>}
          {book.series && <span className="series"> · {book.series}</span>}
          {book.asin && <span className="series"> · ASIN {book.asin}</span>}
        </div>
        {asinDraft !== null && (
          <form
            className="asin-edit"
            onSubmit={(e) => {
              e.preventDefault();
              saveAsin();
            }}
          >
            <input
              type="text"
              autoFocus
              value={asinDraft}
              onChange={(e) => setAsinDraft(e.target.value)}
              onKeyDown={(e) => e.key === "Escape" && setAsinDraft(null)}
              placeholder="B0… from the book's Amazon page"
              aria-label="Amazon ASIN"
              spellCheck={false}
              maxLength={12}
            />
            <button type="submit" className="ghost" disabled={!draftValid}>
              Save
            </button>
            <button type="button" className="ghost" onClick={() => setAsinDraft(null)}>
              Cancel
            </button>
            <div className="asin-hint">
              {asinDraft.trim() && !draftValid
                ? "An ASIN is 10 letters or digits."
                : "With Prefer features (EBOK), the Kindle fetches Amazon's cover for this edition."}
            </div>
          </form>
        )}
        {statusLine}
        {book.showReport && (
          <div className="report">
            <div>
              <b>Mode:</b> {effectiveCde.toUpperCase()} — {b.title}
            </div>
            <div>
              <b>File:</b> {book.path}
            </div>
            {book.delivered && (
              <div>
                <b>Delivered:</b> {book.delivered}
              </div>
            )}
            {[...book.warnings, ...book.runWarnings].map((w, i) => (
              <div key={i} className="warn">
                {w}
              </div>
            ))}
            {book.runError && <div className="fail">{book.runError}</div>}
          </div>
        )}
      </div>
      <div className="row-menu">
        <button className="icon" aria-label="Book options" onClick={() => setMenuOpen((o) => !o)} disabled={busy}>
          ⋯
        </button>
        {menuOpen && (
          <div className="menu" onMouseLeave={() => setMenuOpen(false)}>
            <button onClick={() => { onOverride(book.id, "ebok"); setMenuOpen(false); }} disabled={effectiveCde === "ebok" || book.mode === "error" || book.mode === "ready"}>
              Prefer features (EBOK)
            </button>
            <button onClick={() => { onOverride(book.id, "pdoc"); setMenuOpen(false); }} disabled={effectiveCde === "pdoc" || book.mode === "error" || book.mode === "ready"}>
              Prefer cover (PDOC)
            </button>
            {book.cdeOverride && (
              <button onClick={() => { onOverride(book.id, null); setMenuOpen(false); }}>Use default</button>
            )}
            <button onClick={() => { setAsinDraft(book.asin ?? ""); setMenuOpen(false); }} disabled={book.mode === "error" || book.mode === "ready"}>
              {book.asin ? "Change Amazon ASIN…" : "Set Amazon ASIN…"}
            </button>
            {book.asin && (
              <button onClick={() => { onAsin(book.id, null); setMenuOpen(false); }}>Clear ASIN</button>
            )}
            <button onClick={() => { onToggleReport(book.id); setMenuOpen(false); }}>
              {book.showReport ? "Hide report" : "Show report"}
            </button>
            <button className="danger" onClick={() => { onRemove(book.id); setMenuOpen(false); }}>
              Remove
            </button>
          </div>
        )}
      </div>
    </li>
  );
}
