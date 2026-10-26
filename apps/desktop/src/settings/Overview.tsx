// SPDX-License-Identifier: GPL-3.0-or-later
import type { Check, SettingsPage } from "../settingsIndex";
import { PAGES, onlineLine, setupHeading } from "../settingsIndex";

interface Props {
  checks: Check[];
  online: string[];
  summary: (page: SettingsPage) => string;
  onFix: (check: Check) => void;
  onPage: (page: SettingsPage) => void;
}

const MARK = { ok: "✓", todo: "!", optional: "–" } as const;

/** Overview (Settings redesign §4): is everything set up, what goes online, and a card per page. */
export default function Overview({ checks, online, summary, onFix, onPage }: Props) {
  return (
    <div className="overview">
      <p className="overview-lead">{setupHeading(checks)}</p>
      <section className="set-card setup-list" aria-label="Setup">
        {checks.map((c) => (
          <div key={c.key} className="setup-row">
            <span className={`setup-mark ${c.mark}`} aria-label={c.mark === "ok" ? "Set up" : c.mark === "todo" ? "Needs doing" : "Off"}>
              {MARK[c.mark]}
            </span>
            <span className="setup-text">
              <strong>{c.title}</strong> <span className="muted">{c.state}</span>
            </span>
            {c.fix && (
              <button className="ghost" onClick={() => onFix(c)}>
                {c.fix}
              </button>
            )}
          </div>
        ))}
      </section>
      <section className="online-line" aria-label="Going online">
        <span>{onlineLine(online)}</span>
        <button className="link-action" onClick={() => onPage("online")}>
          What each one sends →
        </button>
      </section>
      <section className="page-cards" aria-label="All settings">
        {PAGES.filter((p) => p.id !== "overview").map((p) => (
          <button key={p.id} className="page-card" onClick={() => onPage(p.id)}>
            <span className="page-card-title">{p.title}</span>
            <span className="muted">{summary(p.id)}</span>
          </button>
        ))}
      </section>
    </div>
  );
}
