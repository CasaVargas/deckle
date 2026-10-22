// SPDX-License-Identifier: GPL-3.0-or-later
import { useState } from "react";
import { beforeChip, doneChips, type Chip } from "./bookStatus";
import Icon from "./Icon";
import type { BookState, PlanRow, Settings } from "./types";
import { STAGES, STAGE_LABEL } from "./types";

interface Props {
  book: BookState;
  busy: boolean;
  /** What the cover rule will do on the current target (undefined until planned). */
  plan?: PlanRow;
  /** Which features are on: the before-sending chip names them. */
  settings: Pick<Settings, "xray" | "word_wise">;
  onOverride: (id: string, cde: "pdoc" | "ebok" | null) => void;
  onAsin: (id: string, asin: string | null) => void;
  onToggleReport: (id: string) => void;
  onRemove: (id: string) => void;
  onReveal: (path: string) => void;
}

function Chips({ chips }: { chips: Chip[] }) {
  return (
    <>
      {chips.map((c) => (
        <span key={c.text} className={`chip ${c.tone}`} title={c.title}>
          {c.text}
        </span>
      ))}
    </>
  );
}

const SOURCE = { llm: "AI model", wikipedia: "Wikipedia", basic: "basic" } as const;

const ASIN = /^[A-Z0-9]{10}$/;

export default function BookRowView({ book, busy, plan, settings, onOverride, onAsin, onToggleReport, onRemove, onReveal }: Props) {
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
  const before = beforeChip(book, plan, settings);
  const stageIndex = book.stage ? STAGES.indexOf(book.stage as (typeof STAGES)[number]) : -1;

  let statusLine: React.ReactNode;
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
      <div className="chips">
        <Chips chips={doneChips(book)} />
        {book.delivered && (
          <button className="link" onClick={() => onReveal(book.delivered!)}>
            Show file
          </button>
        )}
      </div>
    );
  } else if (book.status === "failed") {
    statusLine = (
      <div className="chips">
        <span className="chip fail">Failed</span>
        <span className="chip-text fail" title={book.runError ?? undefined}>
          {book.runError}
        </span>
      </div>
    );
  } else if (book.status === "queued") {
    statusLine = (
      <div className="chips">
        <span className="chip plain">Waiting…</span>
      </div>
    );
  } else {
    const notes = book.warnings.length;
    statusLine = (
      <div className="chips">
        <Chips chips={[before]} />
        {book.error && (
          <span className="chip-text fail" title={book.error}>
            {book.error}
          </span>
        )}
        {notes > 0 && (
          <span className="chip warn" title={book.warnings.join("\n")}>
            {notes} note{notes > 1 ? "s" : ""}
          </span>
        )}
      </div>
    );
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
          <dl className="report">
            <dt>Mode</dt>
            <dd>
              {(book.shipped?.cde ?? effectiveCde).toUpperCase()} · {book.shipped?.reason ?? plan?.reason ?? book.reason}
            </dd>
            {book.asin && (
              <>
                <dt>ASIN</dt>
                <dd>{book.asin}</dd>
              </>
            )}
            <dt>File</dt>
            <dd>{book.path}</dd>
            {book.delivered && (
              <>
                <dt>{book.exported ? "Exported" : "Delivered"}</dt>
                <dd>{book.delivered}</dd>
              </>
            )}
            {book.replaced.length > 0 && (
              <>
                <dt>Replaced</dt>
                <dd>{book.replaced.join(", ")}</dd>
              </>
            )}
            {book.xray && (
              <>
                <dt>X-Ray</dt>
                <dd>
                  {book.xray.people} people, {book.xray.terms} terms ({SOURCE[book.xray.source]})
                </dd>
              </>
            )}
            {book.wordWise && (
              <>
                <dt>Word Wise</dt>
                <dd>
                  {book.wordWise.hints.toLocaleString()} hints, level {book.wordWise.level}, {book.wordWise.dictionary === "device" ? "Kindle's dictionary" : "Deckle's dictionary"}
                </dd>
              </>
            )}
            {[...book.warnings, ...book.runWarnings].map((w, i) => (
              <dd key={i} className="warn wide">
                {w}
              </dd>
            ))}
            {(book.runError || book.error) && <dd className="fail wide">{book.runError ?? book.error}</dd>}
          </dl>
        )}
      </div>
      <div className="row-menu">
        <button className="icon" aria-label="Book options" onClick={() => setMenuOpen((o) => !o)} disabled={busy}>
          <Icon name="dots" size={18} />
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
              {book.showReport ? "Hide details" : "Details"}
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
