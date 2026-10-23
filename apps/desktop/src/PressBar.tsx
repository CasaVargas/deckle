// SPDX-License-Identifier: GPL-3.0-or-later
import { useEffect, useState } from "react";
import { pressLine } from "./pagePlan";
import type { Press } from "./types";

/** When the next paper goes to press (tomorrow's paper design §4.1): the line, the switch and time, and Restart. */
export default function PressBar({
  press,
  time,
  onTime,
  onSchedule,
  onRestart,
}: {
  press: Press | null;
  time: string;
  onTime: (t: string) => void;
  onSchedule: (on: boolean, at?: string) => void;
  onRestart: () => void;
}) {
  const [now, setNow] = useState(Date.now());
  useEffect(() => {
    const t = setInterval(() => setNow(Date.now()), 60_000);
    return () => clearInterval(t);
  }, []);
  const on = !!press?.at;
  return (
    <section className="card press-bar" title={on ? `A Mac asleep at ${press?.at} builds it when it wakes.` : undefined}>
      <span className={`press-dot ${press?.stopped ? "stopped" : on ? "on" : ""}`} aria-hidden="true" />
      <span className="press-line">{press ? pressLine(press, now) : "…"}</span>
      <span className="grow" />
      {press?.stopped ? (
        <button className="ghost" onClick={onRestart}>
          Restart the presses
        </button>
      ) : (
        <>
          <input
            type="time"
            aria-label="Press time"
            value={time}
            onChange={(e) => {
              onTime(e.target.value);
              if (on) onSchedule(true, e.target.value);
            }}
          />
          <label className="check press-switch">
            <input className="switch" type="checkbox" checked={on} onChange={(e) => onSchedule(e.target.checked)} />
            {on ? "Every morning" : "Turn on"}
          </label>
        </>
      )}
    </section>
  );
}
