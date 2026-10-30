// SPDX-License-Identifier: GPL-3.0-or-later
// When the paper goes to press (Daily tab redesign §3): the press line, the switch, the time and
// the days, where it goes, and Send now / Preview / Skip the next paper.
import { useCapabilities } from "./capabilities";
import { useEffect, useState } from "react";
import type { ReactNode } from "react";
import { daysLine } from "./dailyRooms";
import { pressLine } from "./pagePlan";
import { DAYS } from "./papersView";
import type { Day, Press } from "./types";

const DAY_NAMES: Record<Day, string> = { mon: "Monday", tue: "Tuesday", wed: "Wednesday", thu: "Thursday", fri: "Friday", sat: "Saturday", sun: "Sunday" };

export default function PressCard({
  press,
  time,
  days,
  goes,
  running,
  onTime,
  onSchedule,
  onDay,
  onRestart,
  onStop,
  onRun,
  onDelivery,
  children,
}: {
  press: Press | null;
  time: string;
  days: Day[];
  /** Where the paper goes, in one line. */
  goes: string;
  running: null | "preview" | "send";
  onTime: (t: string) => void;
  onSchedule: (on: boolean, at?: string) => void;
  onDay: (d: Day) => void;
  onRestart: () => void;
  onStop: () => void;
  onRun: (send: boolean) => void;
  onDelivery: () => void;
  /** The last run's result, and the last-built line. */
  children?: ReactNode;
}) {
  const caps = useCapabilities();
  const [now, setNow] = useState(Date.now());
  useEffect(() => {
    const t = setInterval(() => setNow(Date.now()), 60_000);
    return () => clearInterval(t);
  }, []);
  const on = !!press?.at;
  return (
    <section className="card press-card" aria-label="When it goes to press">
      <div className="press-head" title={on ? (caps.schedule === "config" ? `The server builds it at ${press?.at}.` : `A Mac asleep at ${press?.at} builds it when it wakes.`) : undefined}>
        <span className={`press-dot ${press?.stopped ? "stopped" : on ? "on" : ""}`} aria-hidden="true" />
        <strong className="press-line">{press ? pressLine(press, now) : "…"}</strong>
        <input className="switch" type="checkbox" aria-label="Build it every morning" checked={on} onChange={(e) => onSchedule(e.target.checked)} />
      </div>
      <div className="press-when">
        <input
          type="time"
          aria-label="Press time"
          value={time}
          onChange={(e) => {
            onTime(e.target.value);
            if (on) onSchedule(true, e.target.value);
          }}
        />
        <div className="press-days" role="group" aria-label="Days">
          {DAYS.map((d) => (
            <button key={d} className={`day ${days.includes(d) ? "on" : ""}`} aria-pressed={days.includes(d)} aria-label={DAY_NAMES[d]} onClick={() => onDay(d)}>
              {d[0].toUpperCase()}
            </button>
          ))}
        </div>
        <span className="muted small">{daysLine(days)}</span>
      </div>
      <p className="muted small">
        {goes}.{" "}
        <button className="link" onClick={onDelivery}>
          Change
        </button>
      </p>
      <div className="paper-actions">
        <button className="primary" disabled={!!running} onClick={() => onRun(true)}>
          {running === "send" ? "Building and sending…" : "Send now"}
        </button>
        <button className="ghost" disabled={!!running} onClick={() => onRun(false)}>
          {running === "preview" ? "Building preview…" : "Preview"}
        </button>
        {press?.stopped ? (
          <button className="quiet" onClick={onRestart}>
            Restart the presses
          </button>
        ) : (
          press?.next && (
            <button className="quiet" disabled={!!running} onClick={onStop}>
              Skip the next paper
            </button>
          )
        )}
      </div>
      {running && <p className="muted small">Fetching every story and photo takes about a minute.</p>}
      {children}
    </section>
  );
}
