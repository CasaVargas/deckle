// SPDX-License-Identifier: GPL-3.0-or-later
import { DEFAULT_REMARKABLE } from "./exportsView";
import { Switch } from "./settings/Row";
import { SCREEN_LABEL } from "./types";
import type { RemarkableConfig, Screen } from "./types";

/**
 * A reMarkable (other readers design §9): the paper as a PDF laid out for its screen, sent over
 * its USB web interface when it's plugged in. Each change saves at once.
 */
export default function RemarkableCard({
  remarkable,
  onSave,
}: {
  remarkable: RemarkableConfig | undefined;
  onSave: (r: RemarkableConfig) => Promise<boolean>;
}) {
  const current = remarkable ?? DEFAULT_REMARKABLE;
  const change = (p: Partial<RemarkableConfig>) => onSave({ ...current, ...p });
  return (
    <section className="card remarkable" aria-label="reMarkable">
      <h3>reMarkable</h3>
      <p className="muted small">
        The paper as a PDF laid out for your reMarkable, crossword and all, so you can write on it. Turn on Settings → Storage →
        USB web interface on the tablet; the paper goes over the next time it's plugged in. Deckle can't delete from a reMarkable,
        so older papers stay until you remove them.
      </p>
      <div className="yourday-row">
        <span>Send the paper to my reMarkable</span>
        <Switch label="Send the paper to my reMarkable" checked={current.enabled} onChange={(on) => change({ enabled: on })} />
      </div>
      {current.enabled && (
        <>
          <div className="export-row">
            <label>
              <span className="label">Model</span>
              <select value={current.screen} onChange={(ev) => change({ screen: ev.target.value as Screen })}>
                {(["remarkable-2", "remarkable-paper-pro", "remarkable-paper-pro-move"] as Screen[]).map((s) => (
                  <option key={s} value={s}>
                    {SCREEN_LABEL[s]}
                  </option>
                ))}
              </select>
            </label>
          </div>
          <div className="yourday-row">
            <span>Long reads too, as EPUBs</span>
            <Switch label="Long reads to the reMarkable" checked={current.books} onChange={(on) => change({ books: on })} />
          </div>
        </>
      )}
    </section>
  );
}
