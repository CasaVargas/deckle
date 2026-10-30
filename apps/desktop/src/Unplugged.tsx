// SPDX-License-Identifier: GPL-3.0-or-later
import { useCapabilities } from "./capabilities";
import { NO_DEVICE_REASON } from "./web/gates";
import { useEffect, useState } from "react";
import { api } from "./api";
import { sentOn } from "./kindleRooms";
import type { SentKindle } from "./types";

/**
 * The Kindle tab with no Kindle plugged in (Kindle tab redesign §7): what Deckle sent to the
 * Kindle it sent to last (or another it picks), said plainly to be only that; or, before any
 * send, what the tab will show and how to connect one.
 */
export default function Unplugged() {
  const caps = useCapabilities();
  const [kindles, setKindles] = useState<SentKindle[] | null>(null);
  const [key, setKey] = useState<string | null>(null);
  const [covers, setCovers] = useState<Record<string, string | null>>({});
  useEffect(() => {
    api
      .librarySentKindles()
      .then((k) => {
        setKindles(k);
        setKey(k[0]?.key ?? null);
      })
      .catch(() => setKindles([]));
  }, []);
  const kindle = kindles?.find((k) => k.key === key) ?? null;
  useEffect(() => {
    if (!kindle) return;
    const paths = kindle.books.map((b) => b.path).filter((p) => !(p in covers));
    if (paths.length === 0) return;
    api
      .libraryCovers(paths.slice(0, 60))
      .then((rows) => setCovers((c) => ({ ...c, ...Object.fromEntries(rows) })))
      .catch(() => {});
  }, [kindle?.key]);

  if (kindles === null) return <main className="kindle-main" />;
  const now = new Date();
  const connect = caps.web
    ? NO_DEVICE_REASON
    : "Plug it in by USB. Newer Kindles connect the way a phone does: if it doesn't show up, unlock it after plugging it in.";

  if (!kindle) {
    return (
      <main className="kindle-main unplugged-empty">
        <div className="unplugged-welcome">
          <h1>No Kindle plugged in</h1>
          <p>Plug one in to see every book on it, how far into each you are, its highlights and looked-up words, how much room is left, and which books Deckle can make better.</p>
          <p className="muted small">{connect}</p>
        </div>
      </main>
    );
  }

  return (
    <main className="kindle-main">
      <section className="unplugged-banner">
        <svg width="40" height="52" viewBox="0 0 44 56" fill="none" stroke="currentColor" strokeWidth="2" aria-hidden="true">
          <rect x="2" y="2" width="40" height="52" rx="5" />
          <rect x="8" y="8" width="28" height="36" rx="1.5" />
          <path d="M18 49h8" strokeLinecap="round" />
        </svg>
        <div className="unplugged-text">
          <h1>{kindle.name || "Your Kindle"} isn't plugged in</h1>
          <p>
            Below {kindle.books.length === 1 ? "is the 1 book" : `are the ${kindle.books.length} books`} Deckle has sent it. That isn't everything on it, and its places and highlights show once it's
            plugged in.
          </p>
          <p className="muted small">
            Deckle last sent to it on {sentOn(kindle.last, now)}. {connect}
          </p>
        </div>
        {kindles.length > 1 && (
          <select className="quiet-select" value={kindle.key} onChange={(e) => setKey(e.target.value)} aria-label="Kindle">
            {kindles.map((k) => (
              <option key={k.key} value={k.key}>
                {k.name || k.key}
              </option>
            ))}
          </select>
        )}
      </section>
      <section aria-label="Sent by Deckle">
        <h2 className="kindle-h2">
          Sent by Deckle <span className="muted">{kindle.books.length} · newest first</span>
        </h2>
        <ul className="kcards">
          {kindle.books.map((b) => (
            <li key={b.path} className="kcard">
              <span className="kcard-open">
                <span className="kcard-cover faded">
                  <span className="kcard-img" aria-hidden="true">
                    {covers[b.path] ? <img src={covers[b.path]!} alt="" /> : <span className="cover-placeholder">{b.title.slice(0, 1)}</span>}
                  </span>
                </span>
                <span className="kcard-title">{b.title}</span>
                <span className="kcard-line">Sent {sentOn(b.at, now)}</span>
              </span>
            </li>
          ))}
        </ul>
      </section>
    </main>
  );
}
