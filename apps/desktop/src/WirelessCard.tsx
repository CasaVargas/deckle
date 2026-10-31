// SPDX-License-Identifier: GPL-3.0-or-later
import { useEffect, useState } from "react";
import { api } from "./api";
import { DEFAULT_WIRELESS } from "./types";
import type { WirelessConfig, WirelessView } from "./types";

/**
 * KOReader's calibre connection (calibre bridge design §4.4): Deckle answers KOReader's built-in
 * Tools → calibre → Connect, so the paper and queued books go over Wi-Fi with no plugin. Its on
 * switch is on its row in Delivery; port and the paper save with the rest of the Daily
 * settings, and the watcher picks them up on its next tick. The password is write-only.
 */
export default function WirelessCard({
  wireless,
  onSave,
}: {
  wireless: WirelessConfig | undefined;
  onSave: (w: WirelessConfig, how?: "now" | "typing") => void;
}) {
  const current = wireless ?? DEFAULT_WIRELESS;
  const [view, setView] = useState<WirelessView | null>(null);
  const [password, setPassword] = useState("");
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    api.wirelessStatus().then(setView, () => setView(null));
  }, [current.port, current.enabled]);

  const setPw = async (p: string | null) => {
    setError(null);
    try {
      setView(await api.wirelessSetPassword(p));
      setPassword("");
    } catch (e) {
      setError(String(e));
    }
  };

  const connected = view?.readers.filter((r) => r.connected) ?? [];
  // The server says so in its log when calibre (or another program) holds the broadcast ports.
  const noBroadcast = !!view && !view.discovery && view.log.some((l) => l.includes("can't answer broadcasts"));

  return (
    <section className="reader-server" aria-label="KOReader's calibre connection">
      <p className="muted small">
        KOReader can take books from calibre over Wi-Fi. Deckle answers instead, so the paper and the books you queue (Settings →
        KOReader) reach any reader running KOReader with no cable and nothing to install.
      </p>
      <div className="export-row">
        <label>
          <span className="label">Port</span>
          <input
            type="number"
            min={1024}
            max={65535}
            value={current.port}
            onChange={(e) => onSave({ ...current, port: Number(e.target.value) || DEFAULT_WIRELESS.port }, "typing")}
          />
        </label>
        <div>
          <span className="label">Password (optional)</span>
          <form
            className="save-row"
            onSubmit={(e) => {
              e.preventDefault();
              if (password) setPw(password);
            }}
          >
            <input
              type="password"
              autoComplete="new-password"
              value={password}
              placeholder={view?.password_set ? "Set" : "None"}
              onChange={(e) => setPassword(e.target.value)}
            />
            <button className="ghost" type="submit" disabled={!password}>
              {view?.password_set ? "Replace" : "Save"}
            </button>
            {view?.password_set && (
              <button className="link" type="button" onClick={() => setPw(null)}>
                Clear
              </button>
            )}
          </form>
        </div>
      </div>
      <label className="inline">
        <input type="checkbox" checked={current.paper} onChange={(e) => onSave({ ...current, paper: e.target.checked })} />
        Send the newest paper when a reader connects (it keeps the last three)
      </label>
      {error && <p className="warn small">{error}</p>}
      {current.enabled && view && (
        <>
          <ol className="muted small wireless-steps">
            <li>The first time, on the reader: Tools → calibre → Wireless settings → choose an inbox folder{view.password_set ? ", and type the password" : ""}.</li>
            <li>Then each time: Tools → calibre → Connect. It finds Deckle on your network by itself.</li>
          </ol>
          {noBroadcast && <p className="warn small">Deckle can't answer readers looking for it (is calibre's own wireless server on?): type the address below into the reader.</p>}
          <dl className="reader-server-urls">
            <dt>If it doesn't find Deckle, the server address (Wireless settings → Server address)</dt>
            <dd>
              <code>{view.address}</code>
            </dd>
            {connected.length > 0 && (
              <>
                <dt>Connected now</dt>
                <dd>{connected.map((r) => r.name).join(", ")}</dd>
              </>
            )}
          </dl>
        </>
      )}
    </section>
  );
}
