// SPDX-License-Identifier: GPL-3.0-or-later
import { useEffect } from "react";
import { loadMastheadFonts } from "./mastheadFonts";
import { faceFamily, mastheadChoice } from "./mastheadView";
import type { NextPaper, Tile } from "./types";

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

/** The next edition's front page as it stands (tomorrow's paper design §5, Daily tab redesign §3). */
export default function TomorrowsPaper({ next, masthead, onSections }: { next: NextPaper | null; masthead?: string; onSections: () => void }) {
  useEffect(loadMastheadFonts, []);
  // The name in the paper's own face; a font file of the reader's shows in the default's place
  const face = mastheadChoice(masthead);
  return (
    <section className="tomorrow" id="tomorrows-paper" aria-label="Tomorrow's paper, so far">
      <div className="tomorrow-head">
        <h2 className="daily-h2">Tomorrow's paper, so far</h2>
        <span className="muted small" title="Headlines are checked when you open the tab and when Deckle comes to the front">
          {next?.checked ? `Headlines checked ${clock(next.checked)}` : "Headlines not checked yet"}
        </span>
      </div>
      {next && (
        <div className="front">
          <div className="ears">
            <span>NO. {next.number}</span>
            <span className="motto">Printed this morning</span>
            <span>{next.place.toUpperCase()}</span>
          </div>
          <div className="masthead-name" style={{ fontFamily: `${faceFamily(face === "file" ? "consent" : face)}, ui-serif, Georgia, serif` }}>
            {next.paper}
          </div>
          <div className="dateband">{longDate(next.date)}</div>
          {next.weather && <div className="weather-slot">Weather · fills in at press time</div>}
          {next.lead && <TileView t={next.lead} lead />}
          <div className="tiles">
            {next.tiles.map((t, i) => (
              <TileView key={`${t.key}-${i}`} t={t} />
            ))}
          </div>
        </div>
      )}
      <p className="muted small">
        It fills in as the day goes: a link you save drops in right away, and dashed slots fill at press time. What's in it is on{" "}
        <button className="link" onClick={onSections}>
          Sections
        </button>
        .
      </p>
    </section>
  );
}
