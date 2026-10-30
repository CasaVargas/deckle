// SPDX-License-Identifier: GPL-3.0-or-later
import { useCallback, useEffect, useState } from "react";
import { platform } from "./platform";
import { api } from "./api";
import { DEFAULT_DAY, addressError, previewLine, withoutCalendar } from "./dayView";
import Icon from "./Icon";
import { Switch } from "./settings/Row";
import type { DayConfig, DayPreview } from "./types";

/**
 * Your Day on the weather page: the calendars it reads (almanac design §5). Each change
 * saves at once. Addresses are secrets, so the list shows the backend's masked labels.
 */
export default function YourDay({
  day,
  emailOn,
  onSave,
}: {
  day: DayConfig | undefined;
  emailOn: boolean;
  onSave: (day: DayConfig) => Promise<boolean>;
}) {
  const current = day ?? DEFAULT_DAY;
  const [preview, setPreview] = useState<DayPreview | null>(null);
  const [adding, setAdding] = useState(false);
  const [address, setAddress] = useState("");
  const [error, setError] = useState<string | null>(null);

  const refresh = useCallback((d: DayConfig) => {
    if (d.calendars.length === 0) {
      setPreview(null);
      return;
    }
    api.dailyDayPreview(d).then(setPreview).catch((e) => setError(String(e)));
  }, []);

  const key = current.calendars.join("\n");
  useEffect(() => {
    refresh(current);
    // Re-read only when the list itself changes.
  }, [key, refresh]);

  const change = async (next: DayConfig) => {
    setError(null);
    if (await onSave(next)) refresh(next);
  };

  const addFile = async () => {
    const picked = (await platform.pickFiles({ filterName: "Calendar", extensions: ["ics"] }))?.[0];
    if (picked !== undefined && !current.calendars.includes(picked)) {
      await change({ ...current, calendars: [...current.calendars, picked] });
    }
  };

  const addAddress = async () => {
    const why = addressError(address);
    if (why) {
      setError(why);
      return;
    }
    const a = address.trim();
    if (!current.calendars.includes(a)) await change({ ...current, calendars: [...current.calendars, a] });
    setAddress("");
    setAdding(false);
  };

  return (
    <section className="card yourday" aria-label="Your Day">
      <h3>Your Day</h3>
      <p className="muted small">
        Today's events on the weather page, from calendar files or a calendar's private address. They're read when the paper
        is built and go nowhere else.
      </p>

      {current.calendars.length > 0 && (
        <ul className="yourday-list">
          {current.calendars.map((c, i) => (
            <li key={c}>
              <span>{preview?.labels[i] ?? "Calendar"}</span>
              <button
                className="icon small"
                aria-label="Remove calendar"
                onClick={() => {
                  setPreview((p) => withoutCalendar(p, i));
                  change({ ...current, calendars: current.calendars.filter((x) => x !== c) });
                }}
              >
                <Icon name="x" size={14} />
              </button>
            </li>
          ))}
        </ul>
      )}

      <div className="yourday-actions">
        <button className="ghost" onClick={addFile}>
          Add file…
        </button>
        <button
          className="ghost"
          onClick={() => {
            setAdding((a) => !a);
            setAddress("");
            setError(null);
          }}
        >
          Add address
        </button>
      </div>
      {adding && (
        <form
          className="save-row"
          onSubmit={(e) => {
            e.preventDefault();
            addAddress();
          }}
        >
          <input
            placeholder="https://… or webcal://… (the calendar's private address)"
            value={address}
            onChange={(e) => setAddress(e.target.value)}
            spellCheck={false}
            autoFocus
          />
          <button className="primary" type="submit">
            Add
          </button>
        </form>
      )}
      {error && <p className="warn small">{error}</p>}

      <div className="yourday-row">
        <span>Mention rain on your events</span>
        <Switch
          label="Mention rain on your events"
          checked={current.weather_note}
          onChange={(on) => change({ ...current, weather_note: on })}
        />
      </div>

      {preview && <p className="small">{previewLine(preview)}</p>}
      {preview?.errors.map((e) => (
        <p key={e} className="warn small">
          {e}
        </p>
      ))}
      {emailOn && current.calendars.length > 0 && (
        <p className="muted small">Emailed papers include your agenda, so it passes through Amazon.</p>
      )}
    </section>
  );
}
