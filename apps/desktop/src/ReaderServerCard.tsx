// SPDX-License-Identifier: GPL-3.0-or-later
import { useEffect, useState } from "react";
import { api } from "./api";
import { DEFAULT_READER_SERVER } from "./types";
import type { ReaderServerConfig, ReaderServerStatus } from "./types";

/**
 * KOReader over Wi-Fi (KOReader hub design §A.4): the watcher serves an OPDS catalog of the
 * papers and KOReader's progress sync on the home network. Its on switch is on its row in
 * Delivery; port and username save with the rest of the Daily settings, and the watcher picks
 * them up on its next tick. The password is write-only.
 */
export default function ReaderServerCard({
  server,
  onSave,
}: {
  server: ReaderServerConfig | undefined;
  onSave: (r: ReaderServerConfig, how?: "now" | "typing") => void;
}) {
  const current = server ?? DEFAULT_READER_SERVER;
  const [status, setStatus] = useState<ReaderServerStatus | null>(null);
  const [password, setPassword] = useState("");
  const [copied, setCopied] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    api.readerServerStatus().then(setStatus, () => setStatus(null));
  }, [current.port]);

  const savePassword = async () => {
    if (!password) return;
    setError(null);
    try {
      setStatus(await api.readerServerSetPassword(password));
      setPassword("");
    } catch (e) {
      setError(String(e));
    }
  };

  const copy = async (what: string, text: string) => {
    await navigator.clipboard.writeText(text);
    setCopied(what);
    setTimeout(() => setCopied(null), 1500);
  };

  return (
    <section className="reader-server" aria-label="KOReader over Wi-Fi">
      <p className="muted small">
        KOReader on any e-reader (a Kobo, PocketBook, Boox or a Kindle running it) can download the paper from this computer over
        Wi-Fi, and send back how far you read. It works on your home network, or anywhere over Tailscale.
      </p>
      <div className="export-row">
        <label>
          <span className="label">Username</span>
          <input
            value={current.username}
            spellCheck={false}
            onChange={(e) => onSave({ ...current, username: e.target.value }, "typing")}
          />
        </label>
        <label>
          <span className="label">Port</span>
          <input
            type="number"
            min={1024}
            max={65535}
            value={current.port}
            onChange={(e) => onSave({ ...current, port: Number(e.target.value) || DEFAULT_READER_SERVER.port }, "typing")}
          />
        </label>
        <div>
          <span className="label">Password</span>
          <form
            className="save-row"
            onSubmit={(e) => {
              e.preventDefault();
              savePassword();
            }}
          >
            <input
              type="password"
              autoComplete="new-password"
              value={password}
              placeholder={status?.password_set ? "Set" : "Choose one"}
              onChange={(e) => setPassword(e.target.value)}
            />
            <button className="ghost" type="submit" disabled={!password}>
              {status?.password_set ? "Replace" : "Save"}
            </button>
          </form>
        </div>
      </div>
      {error && <p className="warn small">{error}</p>}
      {current.enabled && status && !status.password_set && <p className="warn small">Set a password to start it.</p>}
      {current.enabled && status?.password_set && (
        <dl className="reader-server-urls">
          <dt>In KOReader's OPDS catalog (Search → OPDS catalog → +)</dt>
          <dd>
            <code>{status.opds}</code>{" "}
            <button className="link" onClick={() => copy("opds", status.opds)}>
              {copied === "opds" ? "Copied" : "Copy"}
            </button>
          </dd>
          <dt>As KOReader's custom sync server (Tools → Progress sync)</dt>
          <dd>
            <code>{status.sync}</code>{" "}
            <button className="link" onClick={() => copy("sync", status.sync)}>
              {copied === "sync" ? "Copied" : "Copy"}
            </button>
          </dd>
        </dl>
      )}
    </section>
  );
}
