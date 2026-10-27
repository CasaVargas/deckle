// SPDX-License-Identifier: GPL-3.0-or-later
// The Cookbook tab's Delivery room (cookbook design §8.2, §9): "Where it goes", one row per Kindle
// Deckle knows, with where it stands and a switch that turns the cookbook on or off for that Kindle.
import { useState } from "react";
import { api } from "./api";
import { kindleRow, unbuiltLine, watcherNote } from "./cookbookRooms";
import type { CookbookBook, CookbookDelivery as Delivery } from "./types";

export default function CookbookDelivery({
  book,
  delivery,
  onChanged,
  onNotice,
  onDailyDelivery,
}: {
  book: CookbookBook | null;
  delivery: Delivery | null;
  onChanged: () => void | Promise<void>;
  onNotice: (m: string) => void;
  /** Go to the Daily tab's Delivery room, where the watcher is turned on. */
  onDailyDelivery: () => void;
}) {
  /** Switches just flipped, shown until the book is read again. */
  const [flipped, setFlipped] = useState<Record<string, boolean>>({});

  const flip = async (id: string, on: boolean) => {
    setFlipped((f) => ({ ...f, [id]: on }));
    try {
      await api.cookbookSetKindle(id, on);
    } catch (e) {
      onNotice(String(e));
    }
    // The switch holds where it was flipped until the rows are read again
    await onChanged();
    setFlipped((f) => {
      const rest = { ...f };
      delete rest[id];
      return rest;
    });
  };

  const note = delivery ? watcherNote(delivery.watcher_installed) : null;
  const unbuilt = book ? unbuiltLine(book.pending_build, book.build_error) : null;

  return (
    <div className="sections-tab delivery-tab">
      <div>
        <h2 className="daily-h2">Where it goes</h2>
        <p className="muted small">The cookbook goes to every Kindle that is on. One that isn't plugged in gets it the next time.</p>
      </div>
      {note && (
        <p className="muted small" role="note">
          {note}.{" "}
          <button className="link" onClick={onDailyDelivery}>
            Delivery on the Daily tab
          </button>
        </p>
      )}
      {unbuilt && (
        <p className="warn small" role="status">
          {unbuilt}
        </p>
      )}
      {delivery?.error && <p className="fail small">Can't tell where each Kindle stands: {delivery.error}</p>}
      {!delivery ? (
        <p className="muted">Looking for your Kindles…</p>
      ) : delivery.kindles.length === 0 ? (
        !delivery.error && <p className="muted">No Kindle yet. Plug one in and name it on the Kindle tab.</p>
      ) : (
        <ul className="sec-list">
          {delivery.kindles.map((k) => {
            const row = kindleRow(k);
            const on = flipped[k.id] ?? row.on;
            return (
              <li key={k.id} className="dl-row">
                <div className="dl-head cookbook-dl-head">
                  <span className={`press-dot ${on ? "on" : ""}`} aria-hidden="true" />
                  <span className="sec-open">
                    <strong>{row.name}</strong>
                  </span>
                  <span className="dl-status muted small">{row.text}</span>
                  <input className="switch" type="checkbox" aria-label={`${row.name}: cookbook on`} checked={on} disabled={k.id in flipped} onChange={() => flip(k.id, !on)} />
                </div>
              </li>
            );
          })}
        </ul>
      )}
    </div>
  );
}
