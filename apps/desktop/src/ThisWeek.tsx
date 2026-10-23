// SPDX-License-Identifier: GPL-3.0-or-later
import { useEffect, useState } from "react";
import { weekLabel } from "./pagePlan";
import type { WeekDay } from "./types";

function day(date: string): string {
  const [y, m, d] = date.split("-").map(Number);
  return new Date(Date.UTC(y, m - 1, d)).toLocaleDateString("en-US", { weekday: "short", timeZone: "UTC" });
}

/** The last six papers, read or not (tomorrow's paper design §7). */
export default function ThisWeek({ week }: { week: WeekDay[] }) {
  // "All editions" links to the Morgue's card once it exists (#109), never to nothing
  const [morgue, setMorgue] = useState(false);
  useEffect(() => setMorgue(!!document.getElementById("every-edition")), [week]);
  return (
    <section className="card this-week">
      <div className="card-head">
        <h3>This week</h3>
        {morgue && (
          <button className="link" onClick={() => document.getElementById("every-edition")?.scrollIntoView({ behavior: "smooth" })}>
            All editions
          </button>
        )}
      </div>
      <ol className="week">
        {week.map((d) => (
          <li key={d.date} className={`week-day ${d.state.state}`}>
            <span className="week-name">{day(d.date)}</span>
            <span className="week-label small">{weekLabel(d)}</span>
            {d.state.state === "read" && (
              <span className="readership-bar strong" aria-hidden="true">
                <span style={{ width: `${d.state.percent}%` }} />
              </span>
            )}
          </li>
        ))}
      </ol>
    </section>
  );
}
