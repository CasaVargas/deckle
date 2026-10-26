// SPDX-License-Identifier: GPL-3.0-or-later
import { weekLabel } from "./pagePlan";
import type { WeekDay } from "./types";

function day(date: string): string {
  const [y, m, d] = date.split("-").map(Number);
  return new Date(Date.UTC(y, m - 1, d)).toLocaleDateString("en-US", { weekday: "short", timeZone: "UTC" });
}

/** The last six papers, read or not (tomorrow's paper design §7). "All editions" opens the Editions room. */
export default function ThisWeek({ week, stop, onAllEditions }: { week: WeekDay[]; stop?: string | null; onAllEditions: () => void }) {
  return (
    <section className="card this-week">
      <div className="card-head">
        <h3>This week</h3>
        <button className="link" onClick={onAllEditions}>
          All editions
        </button>
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
      {stop && (
        <p className="muted small">
          {stop}.{" "}
          <button className="link" onClick={onAllEditions}>
            How far you read
          </button>
        </p>
      )}
    </section>
  );
}
