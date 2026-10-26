// SPDX-License-Identifier: GPL-3.0-or-later
import { useState } from "react";
import { beforeChip, doneChips, statusLine, type Chip } from "./bookStatus";
import Icon from "./Icon";
import { PRESETS } from "./presets";
import type { BookState, PlanRow, Settings, StylePreset } from "./types";
import { STAGES, STAGE_LABEL } from "./types";

interface Props {
  book: BookState;
  busy: boolean;
  /** What the cover rule will do on the current target (undefined until planned). */
  plan?: PlanRow;
  /** Which features are on: the before-sending chip names them. */
  settings: Pick<Settings, "xray" | "word_wise">;
  preset?: StylePreset | null;
  onPreset?: (preset: StylePreset | null) => void;
  onOverride: (id: string, cde: "pdoc" | "ebok" | null) => void;
  onAsin: (id: string, asin: string | null) => void;
  onToggleReport: (id: string) => void;
  onRemove: (id: string) => void;
  onReveal: (path: string) => void;
  /** Opens Before you send. */
  onOpen?: (id: string) => void;
}

const MARK: Record<Chip["tone"], string> = { plain: "", ok: "✓", warn: "!", fail: "!" };

/** The row's status in one sentence, marked by its tone. */
function Line({ chips, children }: { chips: Chip[]; children?: React.ReactNode }) {
  const line = statusLine(chips);
  if (!line) return null;
  return (
    <div className={`status-line ${line.tone}`} title={line.title}>
      {MARK[line.tone] && <span className="mark" aria-hidden="true">{MARK[line.tone]}</span>}
      <span className="status-text">{line.text}</span>
      {children}
    </div>
  );
}

const SOURCE = { llm: "AI model", wikipedia: "Wikipedia", basic: "basic" } as const;

const ASIN = /^[A-Z0-9]{10}$/;

export default function BookRowView({ book, busy, plan, settings, preset, onPreset, onOverride, onAsin, onToggleReport, onRemove, onReveal, onOpen }: Props) {
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

  let status: React.ReactNode;
  if (book.status === "running") {
    status = (
      <div className="progress" aria-label={`${STAGE_LABEL[book.stage ?? ""] ?? book.stage}`}>
        <div className="progress-track">
          <div className="progress-fill" style={{ width: `${((stageIndex + 1) / STAGES.length) * 100}%` }} />
        </div>
        <span className="progress-label">{STAGE_LABEL[book.stage ?? ""] ?? book.stage}…</span>
      </div>
    );
  } else if (book.status === "done") {
    status = (
      <Line chips={doneChips(book)}>
        {book.delivered && (
          <button className="link" onClick={() => onReveal(book.delivered!)}>
            Show file
          </button>
        )}
      </Line>
    );
  } else if (book.status === "failed") {
    status = <Line chips={[{ text: book.runError ? `Failed: ${book.runError}` : "Failed", tone: "fail", title: book.runError ?? undefined }]} />;
  } else if (book.status === "queued") {
    status = <Line chips={[{ text: "Waiting…", tone: "plain" }]} />;
  } else {
    const notes = book.warnings.length;
    const chips: Chip[] = [before];
    if (preset) chips.push({ text: `${PRESETS[preset]} style`, tone: "plain", title: "Saved book style" });
    if (book.coverPick && book.coverPick.kind !== "own")
      chips.push({ text: book.coverPick.kind === "generated" ? "Typographic cover" : "Open Library cover", tone: "plain", title: "The cover picked in Before you send" });
    if (notes > 0) chips.push({ text: `${notes} note${notes > 1 ? "s" : ""}`, tone: "warn", title: book.warnings.join("\n") });
    if (book.error) chips.push({ text: book.error, tone: "fail", title: book.error });
    status = <Line chips={chips} />;
  }

  return (
    <li className={`row ${book.status}`}>
      <div className="cover-thumb" aria-hidden="true">
        {book.cover ? <img src={book.cover} alt="" /> : <div className="cover-placeholder">{book.title.slice(0, 1)}</div>}
      </div>
      <div className="meta">
        <div className="title-line">
          {onOpen && book.mode !== "error" && book.mode !== "ready" ? (
            <button className="title link-title" title={`Before you send: what Deckle will do to ${book.title}`} onClick={() => onOpen(book.id)}>
              {book.title}
            </button>
          ) : (
            <span className="title" title={book.path}>
              {book.title}
            </span>
          )}
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
        {status}
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
            {onPreset && <label className="preset-picker">Book style
              <select aria-label="Book style" value={preset ?? ""} disabled={busy || book.mode === "error" || book.mode === "ready"}
                onChange={(e) => onPreset((e.target.value || null) as StylePreset | null)}>
                <option value="">Original style / app settings</option>
                {Object.entries(PRESETS).map(([value, label]) => <option key={value} value={value}>{label}</option>)}
              </select>
            </label>}
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
            {onOpen && (
              <button onClick={() => { onOpen(book.id); setMenuOpen(false); }} disabled={book.mode === "error" || book.mode === "ready"}>
                Before you send…
              </button>
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
