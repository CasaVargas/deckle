// SPDX-License-Identifier: GPL-3.0-or-later
import { useState } from "react";
import type { BookState } from "./types";
import { STAGES, STAGE_LABEL } from "./types";

interface Props {
  book: BookState;
  busy: boolean;
  defaultCde: "pdoc" | "ebok";
  onOverride: (id: string, cde: "pdoc" | "ebok" | null) => void;
  onToggleReport: (id: string) => void;
  onRemove: (id: string) => void;
  onReveal: (path: string) => void;
}

function badge(book: BookState, effectiveCde: "pdoc" | "ebok") {
  if (book.mode === "error") return { cls: "badge error", text: "Error", title: book.reason };
  if (effectiveCde === "ebok") {
    return { cls: "badge features", text: "Features", title: "EBOK: X-Ray and Word Wise capable once M3 lands. Cover may show grey on MTP-only Kindles." };
  }
  return { cls: "badge cover", text: "Cover", title: book.reason };
}

export default function BookRowView({ book, busy, defaultCde, onOverride, onToggleReport, onRemove, onReveal }: Props) {
  const [menuOpen, setMenuOpen] = useState(false);
  const effectiveCde = book.cdeOverride ?? defaultCde;
  const b = badge(book, effectiveCde);
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
        Done{book.pages ? ` · ${book.pages} pages` : ""}
        {book.delivered && (
          <button className="link" onClick={() => onReveal(book.delivered!)}>
            Show file
          </button>
        )}
        {book.runWarnings.length > 0 && <span className="warn"> · {book.runWarnings.length} warning{book.runWarnings.length > 1 ? "s" : ""}</span>}
      </div>
    );
  } else if (book.status === "failed") {
    statusLine = <div className="result fail">{book.runError}</div>;
  } else if (book.status === "queued") {
    statusLine = <div className="result muted">Waiting…</div>;
  } else if (book.error) {
    statusLine = <div className="result fail">{book.error}</div>;
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
        </div>
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
            <button onClick={() => { onOverride(book.id, "ebok"); setMenuOpen(false); }} disabled={effectiveCde === "ebok" || book.mode === "error"}>
              Prefer features (EBOK)
            </button>
            <button onClick={() => { onOverride(book.id, "pdoc"); setMenuOpen(false); }} disabled={effectiveCde === "pdoc" || book.mode === "error"}>
              Prefer cover (PDOC)
            </button>
            {book.cdeOverride && (
              <button onClick={() => { onOverride(book.id, null); setMenuOpen(false); }}>Use default</button>
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
