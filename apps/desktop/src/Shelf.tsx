// SPDX-License-Identifier: GPL-3.0-or-later
import { useEffect, useState } from "react";
import { api } from "./api";
import { emailLine, statusLine } from "./shelfView";
import type { ShelfStatus } from "./types";

/** What From Your Shelf last read from a Kindle, and a way to forget it (shelf design §7). */
export default function Shelf() {
  const [status, setStatus] = useState<ShelfStatus | null>(null);
  const [error, setError] = useState<string | null>(null);
  useEffect(() => {
    api.shelfStatus().then(setStatus).catch((e) => setError(String(e)));
  }, []);
  const forget = () =>
    api
      .shelfForget()
      .then((s) => {
        setStatus(s);
        setError(null);
      })
      .catch((e) => setError(String(e)));
  if (!status) return error ? <section className="card shelf"><p className="warn small">{error}</p></section> : null;
  const email = emailLine(status);
  return (
    <section className="card shelf" aria-label="From Your Shelf">
      <h3>From Your Shelf</h3>
      <p className="muted small">{statusLine(status)}</p>
      {!status.enabled && <p className="muted small">This Mac isn't reading your Kindle for it. Turn it on in Settings → Kindles.</p>}
      {email && <p className="warn small">{email}</p>}
      {status.kindle && (
        <button className="link" onClick={forget}>
          Forget what my Kindles shared
        </button>
      )}
      {error && <p className="warn small">{error}</p>}
    </section>
  );
}
