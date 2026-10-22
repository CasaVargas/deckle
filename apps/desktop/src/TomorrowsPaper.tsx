// SPDX-License-Identifier: GPL-3.0-or-later
import type { ReactNode } from "react";
import type { EditionPreview, NextPaper, Tile } from "./types";

function longDate(date: string): string {
  const [y, m, d] = date.split("-").map(Number);
  return new Date(Date.UTC(y, m - 1, d))
    .toLocaleDateString("en-US", { weekday: "long", month: "long", day: "numeric", year: "numeric", timeZone: "UTC" })
    .toUpperCase();
}

function clock(iso: string): string {
  const t = new Date(iso);
  return `${String(t.getHours()).padStart(2, "0")}:${String(t.getMinutes()).padStart(2, "0")}`;
}

function TileView({ t, lead }: { t: Tile; lead?: boolean }) {
  return (
    <div className={`tile ${t.dashed ? "dashed" : ""} ${lead ? "lead" : ""}`}>
      <div className="kicker">{t.kicker}</div>
      {t.title && <div className="tile-title">{t.title}</div>}
      {t.reason && <div className="reason">{t.reason}</div>}
    </div>
  );
}

/** The next edition's front page as it stands (tomorrow's paper design §5). */
export default function TomorrowsPaper({
  next,
  preview,
  running,
  onRun,
  onStop,
  onShowFile,
  children,
}: {
  next: NextPaper | null;
  preview: EditionPreview | null;
  running: null | "preview" | "send";
  onRun: (send: boolean) => void;
  onStop: () => void;
  onShowFile: () => void;
  /** The last run's result. */
  children?: ReactNode;
}) {
  return (
    <section className="card tomorrow" id="tomorrows-paper">
      <div className="card-head">
        <h3>Tomorrow's paper, so far</h3>
        <span className="live" title={next?.checked ? `Headlines checked ${clock(next.checked)}` : "Headlines not checked yet"}>
          <span className="badge live-badge">Live</span>
          <span className="muted small">{next?.checked ? `Headlines checked ${clock(next.checked)}` : "Headlines not checked yet"}</span>
        </span>
      </div>
      {next && (
        <div className="front">
          <div className="ears">
            <span>NO. {next.number}</span>
            <span className="motto">All the news that fits</span>
            <span>{next.place.toUpperCase()}</span>
          </div>
          <div className="masthead-name">{next.paper}</div>
          <div className="dateband">{longDate(next.date)}</div>
          {next.weather && <div className="weather-slot">Weather · fills in at press time</div>}
          {next.lead && <TileView t={next.lead} lead />}
          <div className="tiles">
            {next.tiles.map((t, i) => (
              <TileView key={`${t.key}-${i}`} t={t} />
            ))}
          </div>
          <p className="muted small">
            The page lays itself out as the day goes on. Each link you save drops into a slot right away, and dashed slots fill at
            press time.
          </p>
        </div>
      )}
      <div className="paper-actions">
        <button className="primary" disabled={!!running} onClick={() => onRun(true)}>
          {running === "send" ? "Building and sending…" : "Send now"}
        </button>
        <button className="ghost" disabled={!!running} onClick={() => onRun(false)}>
          {running === "preview" ? "Building preview…" : "Preview"}
        </button>
        {next?.press.next && !next.press.stopped && (
          <button className="ghost" disabled={!!running} onClick={onStop}>
            Stop the presses
          </button>
        )}
      </div>
      {running && <p className="muted small">Fetching every story and photo takes about a minute.</p>}
      {children}
      <p className="muted small">
        {preview ? `Last built ${preview.title.replace(/^.*— /, "")}` : "No edition built yet."}
        {preview && preview.outbox > 0 && ` · ${preview.outbox} waiting for your Kindle`}
        {preview && (
          <>
            {" · "}
            <button className="link" onClick={onShowFile}>
              Show file
            </button>
          </>
        )}
      </p>
    </section>
  );
}
