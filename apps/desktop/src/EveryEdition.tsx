// SPDX-License-Identifier: GPL-3.0-or-later
import { open as openDialog } from "@tauri-apps/plugin-dialog";
import { useEffect, useMemo, useState } from "react";
import { api } from "./api";
import { monthGrid, sizeLine } from "./morgueView";
import type { MorgueDay, MorgueDetail, MorgueHit, MorgueMonth } from "./types";

const WEEKDAYS = ["Mon", "Tue", "Wed", "Thu", "Fri", "Sat", "Sun"];
const MONTH_NAMES = ["January", "February", "March", "April", "May", "June", "July", "August", "September", "October", "November", "December"];

/** Every edition kept: a month of front pages, search, and sending one to the Kindle again (Morgue design §6). */
export default function EveryEdition({ onFolder, disabled }: { onFolder: (folder: string) => Promise<void> | void; disabled?: boolean }) {
  const today = new Date();
  const [year, setYear] = useState(today.getFullYear());
  const [month, setMonth] = useState(today.getMonth() + 1);
  const [view, setView] = useState<MorgueMonth | null>(null);
  const [query, setQuery] = useState("");
  const [hits, setHits] = useState<MorgueHit[]>([]);
  const [selected, setSelected] = useState<string | null>(null);
  const [detail, setDetail] = useState<MorgueDetail | null>(null);
  const [note, setNote] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);

  const load = () => api.morgueMonth(year, month).then(setView).catch((e) => setError(String(e)));
  useEffect(() => {
    load();
  }, [year, month]);

  // Debounced: a search scans every kept edition.
  useEffect(() => {
    if (!query.trim()) {
      setHits([]);
      return;
    }
    const t = setTimeout(() => api.morgueSearch(query).then(setHits).catch((e) => setError(String(e))), 250);
    return () => clearTimeout(t);
  }, [query]);

  useEffect(() => {
    setNote(null);
    if (!selected) {
      setDetail(null);
      return;
    }
    api.morgueShow(selected).then(setDetail).catch((e) => setError(String(e)));
  }, [selected]);

  const hitDates = useMemo(() => new Set(hits.map((h) => h.date)), [hits]);
  const searching = query.trim().length > 0;
  const grid = useMemo(() => monthGrid(year, month, view?.days ?? []), [year, month, view]);

  const step = (by: number) => {
    const d = new Date(year, month - 1 + by, 1);
    setYear(d.getFullYear());
    setMonth(d.getMonth() + 1);
  };
  const pick = (date: string) => {
    setSelected(date);
    const [y, m] = date.split("-").map(Number);
    if (y !== year || m !== month) {
      setYear(y);
      setMonth(m);
    }
  };
  const sendAgain = async () => {
    if (!selected) return;
    setBusy(true);
    setError(null);
    try {
      const name = await api.morgueSend(selected);
      setNote(`Queued ${name}. It goes over the next time a Kindle connects and stays a week.`);
    } catch (e) {
      setError(String(e));
    } finally {
      setBusy(false);
    }
  };
  const changeFolder = async () => {
    const dir = await openDialog({ directory: true, multiple: false });
    if (typeof dir === "string") {
      await onFolder(dir);
      await load();
    }
  };

  const cell = (c: MorgueDay | null | "blank", i: number) => {
    if (c === "blank") return <div key={i} className="morgue-cell blank" aria-hidden="true" />;
    const dayNumber = c ? Number(c.date.slice(8)) : i - grid.findIndex((x) => x !== "blank") + 1;
    if (c === null) {
      const date = `${year}-${String(month).padStart(2, "0")}-${String(dayNumber).padStart(2, "0")}`;
      return (
        <div key={i} className="morgue-cell empty">
          <span className="morgue-day">{dayNumber}</span>
          {view?.nextBuildDate === date && view.nextBuild && <span className="morgue-next">{view.nextBuild}</span>}
        </div>
      );
    }
    const state = searching ? (hitDates.has(c.date) ? " hit" : " dim") : "";
    return (
      <button key={i} className={`morgue-cell has${state}${selected === c.date ? " on" : ""}`} onClick={() => pick(c.date)} aria-label={`${c.date}: ${c.lead ?? c.paper}`}>
        <span className="morgue-day">{dayNumber}</span>
        <span className="morgue-paper">{c.paper}</span>
        {c.lead && <span className="morgue-lead">{c.lead}</span>}
      </button>
    );
  };

  return (
    <section className="card morgue" aria-label="Every edition">
      <div className="card-head">
        <h3>Every edition</h3>
        <div className="morgue-nav">
          <button className="ghost" onClick={() => step(-1)} aria-label="Previous month">
            ‹
          </button>
          <span className="morgue-month">
            {MONTH_NAMES[month - 1]} {year}
          </span>
          <button className="ghost" onClick={() => step(1)} aria-label="Next month">
            ›
          </button>
        </div>
      </div>
      <input className="morgue-search" type="search" placeholder="Search every story…" value={query} onChange={(e) => setQuery(e.target.value)} spellCheck={false} aria-label="Search every edition" />
      <div className="morgue-grid">
        {WEEKDAYS.map((w) => (
          <div key={w} className="morgue-wd">
            {w}
          </div>
        ))}
        {grid.map(cell)}
      </div>
      {searching && (
        <div className="morgue-hits" aria-live="polite">
          {hits.length === 0 ? (
            <p className="muted small">Nothing found.</p>
          ) : (
            hits.map((h) => (
              <div key={h.date} className="morgue-hit">
                <button className="link" onClick={() => pick(h.date)}>
                  {h.date}
                </button>
                <ul>
                  {h.stories.map(([section, title], i) => (
                    <li key={i}>
                      <span className="muted small">{section} · </span>
                      {title}
                    </li>
                  ))}
                </ul>
              </div>
            ))
          )}
        </div>
      )}
      {detail && selected && (
        <div className="morgue-detail">
          <h4>{detail.heading}</h4>
          {detail.weather && <p className="muted small">{detail.weather}</p>}
          {detail.lead && (
            <p>
              <strong>{detail.lead.title}</strong>
              {detail.lead.teaser && <span className="muted"> {detail.lead.teaser}</span>}
            </p>
          )}
          {detail.read && <p className="small">{detail.read}</p>}
          {detail.highlights && <p className="small">{detail.highlights}</p>}
          {detail.onKindle && <p className="small">{detail.onKindle}</p>}
          <div className="morgue-actions">
            <button className="primary" disabled={busy} onClick={sendAgain}>
              Send again
            </button>
            <button className="ghost" onClick={() => api.morgueOpen(selected).catch((e) => setError(String(e)))}>
              Open
            </button>
            <button className="ghost" onClick={() => api.reveal(detail.epub).catch((e) => setError(String(e)))}>
              Show in Finder
            </button>
          </div>
          {note && <p className="muted small">{note}</p>}
        </div>
      )}
      {error && <p className="warn small">{error}</p>}
      <div className="morgue-foot muted small">
        <span>{view ? sizeLine(view.editions, view.bytes) : ""}</span>
        <span className="morgue-folder" title={view?.folder}>
          {view?.folder}
        </span>
        <button className="link" disabled={disabled} title={disabled ? "Save The paper's changes first." : undefined} onClick={changeFolder}>
          Change…
        </button>
      </div>
    </section>
  );
}
