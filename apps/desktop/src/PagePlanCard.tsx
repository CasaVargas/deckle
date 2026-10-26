// SPDX-License-Identifier: GPL-3.0-or-later
import { useState } from "react";
import Icon from "./Icon";
import { step } from "./pagePlan";
import type { NextPaper, PagePlanRow } from "./types";

/** The sections in order with their budgets (tomorrow's paper design §6). Edits save at once. */
export default function PagePlanCard({
  next,
  locked,
  lockedWhy,
  onMove,
  onBudget,
  onToggle,
}: {
  next: NextPaper | null;
  /** A save is in flight, or The paper has unsaved changes. */
  locked: boolean;
  lockedWhy?: string;
  onMove: (from: number, to: number) => void;
  onBudget: (row: PagePlanRow, n: number) => void;
  onToggle: (row: PagePlanRow, on: boolean) => void;
}) {
  const [dragging, setDragging] = useState<number | null>(null);
  const [over, setOver] = useState<number | null>(null);
  if (!next) return null;
  const rows = next.plan;
  return (
    <section className="card page-plan">
      <div className="card-head">
        <h3>Page plan</h3>
        <span className="muted small">Drag to reorder · about {next.pages} Kindle pages</span>
      </div>
      {lockedWhy && <p className="warn small">{lockedWhy}</p>}
      <ol className={`plan-rows ${locked ? "locked" : ""}`} aria-disabled={locked}>
        {rows.map((r, i) => (
          <li
            key={`${r.key}-${i}`}
            className={`plan-row ${r.place !== null ? "place" : ""} ${over === i && dragging !== i ? "over" : ""} ${dragging === i ? "dragging" : ""}`}
            draggable={!locked && r.place === null}
            onDragStart={(e) => {
              setDragging(i);
              e.dataTransfer.effectAllowed = "move";
              e.dataTransfer.setData("text/plain", String(i));
            }}
            onDragOver={(e) => {
              e.preventDefault();
              setOver(i);
            }}
            onDragLeave={() => setOver((o) => (o === i ? null : o))}
            onDrop={(e) => {
              e.preventDefault();
              const from = Number(e.dataTransfer.getData("text/plain"));
              setDragging(null);
              setOver(null);
              if (!locked && !Number.isNaN(from) && from !== i) onMove(from, i);
            }}
            onDragEnd={() => {
              setDragging(null);
              setOver(null);
            }}
          >
            <span className="handle" aria-hidden="true">{r.place === null ? "⋮⋮" : ""}</span>
            {r.place === null ? (
              <label className="check" title={r.kind === "feeds" ? "Switches all your feeds" : r.kind === "interests" ? "Switches all your interests" : undefined}>
                <input type="checkbox" checked disabled={locked} onChange={() => onToggle(r, false)} />
              </label>
            ) : (
              <span className="place-mark" aria-hidden="true" title="Follows Local">
                ↳
              </span>
            )}
            <span className="plan-name">
              <strong>{r.name}</strong>
              <span className="muted small">{r.detail}</span>
            </span>
            {r.items !== null ? (
              <select aria-label={`${r.name} budget`} disabled={locked} value={r.items} onChange={(e) => onBudget(r, Number(e.target.value))}>
                {Array.from({ length: 20 }, (_, n) => n + 1).map((n) => (
                  <option key={n} value={n}>
                    {r.kind === "reading" ? `up to ${n}` : `${n} ${n === 1 ? "story" : "stories"}`}
                  </option>
                ))}
              </select>
            ) : (
              <span className="budget muted small">{r.budget}</span>
            )}
            {r.place === null ? (
              <span className="arrows">
                <button className="icon small" aria-label="Move up" disabled={locked || step(rows, i, -1) === null} onClick={() => onMove(i, step(rows, i, -1)!)}>
                  <Icon name="arrow-up" size={14} />
                </button>
                <button className="icon small" aria-label="Move down" disabled={locked || step(rows, i, 1) === null} onClick={() => onMove(i, step(rows, i, 1)!)}>
                  <Icon name="arrow-down" size={14} />
                </button>
              </span>
            ) : (
              <span className="arrows" />
            )}
          </li>
        ))}
        {next.off.map((r, i) => (
          <li key={`off-${r.key}-${i}`} className="plan-row off">
            <span className="handle" aria-hidden="true" />
            <label className="check" title={r.kind === "feeds" ? "Switches all your feeds" : r.kind === "interests" ? "Switches all your interests" : undefined}>
              <input type="checkbox" checked={false} disabled={locked} onChange={() => onToggle(r, true)} />
            </label>
            <span className="plan-name">
              <strong>{r.name}</strong>
              <span className="muted small">{r.detail}</span>
            </span>
            <span className="budget muted small">off</span>
            <span className="arrows" />
          </li>
        ))}
      </ol>
    </section>
  );
}
