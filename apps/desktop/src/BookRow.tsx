// SPDX-License-Identifier: GPL-3.0-or-later
import { useState } from "react";
import type { DrawerTab } from "./BookDrawer";
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
  onToggleReport: (id: string) => void;
  onRemove: (id: string) => void;
  /** Missing where files can't be shown (the web app). */
  onReveal?: (path: string) => void;
  /** Opens the book's drawer (Books tab redesign §5) on a tab. */
  onOpen?: (id: string, tab: DrawerTab) => void;
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

export default function BookRowView({ book, busy, plan, settings, preset, onPreset, onOverride, onToggleReport, onRemove, onReveal, onOpen }: Props) {
  const [menuOpen, setMenuOpen] = useState(false);
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
        {book.delivered && onReveal && (
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
      chips.push({ text: book.coverPick.kind === "generated" ? "Typographic cover" : "Open Library cover", tone: "plain", title: "The cover picked in the book’s Cover tab" });
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
          {onOpen ? (
            <button className="title link-title" title={`What Deckle will do to ${book.title}`} onClick={() => onOpen(book.id, "kindle")}>
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
            {onOpen && (
              <button onClick={() => { onOpen(book.id, "details"); setMenuOpen(false); }}>
                Edit details…
              </button>
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
