// SPDX-License-Identifier: GPL-3.0-or-later
import { useEffect, useState } from "react";
import { api } from "../api";
import type { CatalogEntry, WirelessReader, WirelessView } from "../types";
import { bookLine, freeLine, jobLine, jobReader, jobsFor, pickable, seenLine } from "../wirelessView";

/**
 * Settings → KOReader, over Wi-Fi (calibre bridge design §4.4): each reader that has connected
 * through KOReader's calibre connection, what it holds, what's waiting for it, and a picker over
 * the Books tab's library (Deckle's, calibre's, your folders') to send more. Sends go when the
 * reader connects, or within seconds while it is.
 */
export default function KoreaderWireless() {
  const [view, setView] = useState<WirelessView | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [picking, setPicking] = useState<string | null>(null);
  const [catalog, setCatalog] = useState<CatalogEntry[] | null>(null);
  const [text, setText] = useState("");
  const [chosen, setChosen] = useState<string[]>([]);

  const load = () => api.wirelessStatus().then(setView, (e) => setError(String(e)));
  useEffect(() => {
    load();
    // While a reader is connected, sends land within seconds: keep the page current.
    const t = setInterval(load, 5000);
    return () => clearInterval(t);
  }, []);

  const act = async (f: () => Promise<WirelessView>) => {
    setError(null);
    try {
      setView(await f());
    } catch (e) {
      setError(String(e));
    }
  };

  const openPicker = async (reader: string) => {
    setPicking(reader);
    setChosen([]);
    if (!catalog) {
      try {
        setCatalog((await api.libraryCatalog(null)).entries);
      } catch (e) {
        setError(String(e));
      }
    }
  };

  const send = async () => {
    if (!picking || chosen.length === 0) return;
    await act(() => api.wirelessQueue(picking, chosen));
    setPicking(null);
  };

  const readers = view?.readers ?? [];
  const picker = (
    <div className="store-readme">
      <form className="save-row" onSubmit={(e) => e.preventDefault()}>
        <input value={text} placeholder="Title or author" aria-label="Find a book" spellCheck={false} onChange={(e) => setText(e.target.value)} />
        <button className="ghost" disabled={chosen.length === 0} onClick={send}>
          Send {chosen.length || ""}
        </button>
        <button className="link" type="button" onClick={() => setPicking(null)}>
          Cancel
        </button>
      </form>
      {!catalog && <p className="muted small">Reading the library…</p>}
      {catalog && catalog.length === 0 && <p className="muted small">No books yet: set up the library on the Books tab, or calibre's in Settings → Books.</p>}
      <ul className="store-list">
        {catalog &&
          pickable(catalog, text).map((e) => (
            <li key={e.path} className="store-item">
              <label className="inline">
                <input
                  type="checkbox"
                  checked={chosen.includes(e.path)}
                  onChange={(ev) => setChosen((c) => (ev.target.checked ? [...c, e.path] : c.filter((p) => p !== e.path)))}
                />
                {bookLine({ lpath: e.path, title: e.title, authors: e.authors })} <span className="muted small">{e.from === "library" ? "Deckle" : e.from === "calibre" ? "calibre" : "folder"}</span>
              </label>
            </li>
          ))}
      </ul>
    </div>
  );

  const readerCard = (r: WirelessReader) => {
    const jobs = jobsFor(r, view?.queue ?? []);
    return (
      <li key={r.id} className={`store-item ${r.connected ? "" : "muted"}`}>
        <div className="store-text">
          <div>
            <strong>{r.name}</strong> <span className="muted small">{[seenLine(r), freeLine(r.free), r.books.length === 1 ? "1 book" : `${r.books.length} books`].filter(Boolean).join(" · ")}</span>
          </div>
          {r.books.length > 0 && (
            <ul className="wireless-books">
              {r.books.map((b) => (
                <li key={b.lpath}>
                  {bookLine(b)}{" "}
                  <button className="link" onClick={() => act(() => api.wirelessRemove(r.id, [b.lpath]))}>
                    Remove
                  </button>
                </li>
              ))}
            </ul>
          )}
          {jobs.length > 0 && (
            <ul className="wireless-books">
              {jobs.map((j) => (
                <li key={j.id} className="muted">
                  Waiting: {jobLine(j)}{" "}
                  <button className="link" onClick={() => act(() => api.wirelessCancel(j.id))}>
                    Cancel
                  </button>
                </li>
              ))}
            </ul>
          )}
          {picking === r.id && picker}
        </div>
        {picking !== r.id && (
          <button className="ghost" onClick={() => openPicker(r.id)}>
            Send books…
          </button>
        )}
      </li>
    );
  };

  // Jobs no reader on the list will take: all of them before any has connected.
  const waitingForAny = (view?.queue ?? []).filter((j) => !readers.some((r) => jobsFor(r, [j]).length));
  const enabled = view?.enabled ?? true;

  return (
    <section className="set-card set-pad" aria-label="Over Wi-Fi">
      <h3>Over Wi-Fi (calibre connection)</h3>
      {!enabled && <p className="set-note">Turn on KOReader's calibre connection in Daily → Delivery first.</p>}
      {enabled && readers.length === 0 && (
        <p className="set-note">No reader has connected yet. On the reader: Tools → calibre → Connect. Books you send now go to the first one that does.</p>
      )}
      <ul className="store-list">{readers.map(readerCard)}</ul>
      {waitingForAny.length > 0 && (
        <ul className="wireless-books">
          {waitingForAny.map((j) => (
            <li key={j.id} className="muted">
              Waiting for {jobReader(j, readers)}: {jobLine(j)}{" "}
              <button className="link" onClick={() => act(() => api.wirelessCancel(j.id))}>
                Cancel
              </button>
            </li>
          ))}
        </ul>
      )}
      {enabled && readers.length === 0 && (picking === "any" ? picker : <button className="ghost" onClick={() => openPicker("any")}>Send books…</button>)}
      {view && view.log.length > 0 && <p className="muted small">Last: {view.log[view.log.length - 1]}</p>}
      {error && <p className="warn small">{error}</p>}
    </section>
  );
}
