// SPDX-License-Identifier: GPL-3.0-or-later
// The web app's door: one password, from DECKLE_WEB_PASSWORD on the server.
import { useState, type FormEvent } from "react";
import { CSRF, errorOf } from "../transport/web";

export default function Login({ onIn }: { onIn: () => void }) {
  const [password, setPassword] = useState("");
  const [error, setError] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);

  const submit = async (e: FormEvent) => {
    e.preventDefault();
    if (!password || busy) return;
    setBusy(true);
    setError(null);
    try {
      const res = await fetch("/api/login", {
        method: "POST",
        headers: { "Content-Type": "application/json", ...CSRF },
        credentials: "same-origin",
        body: JSON.stringify({ password }),
      });
      if (res.ok) {
        setPassword("");
        onIn();
      } else {
        // 429 says so itself: too many login attempts; try again in a moment.
        setError(await errorOf(res));
      }
    } catch (err) {
      setError(String(err));
    } finally {
      setBusy(false);
    }
  };

  return (
    <div className="login-backdrop">
      <form className="login card" onSubmit={submit} aria-label="Log in">
        <div className="brand">Deckle</div>
        {/* No accounts, but password managers and browsers want a user name beside a password. */}
        <input type="text" name="username" autoComplete="username" value="deckle" readOnly hidden />
        <label>
          <span className="label">Password</span>
          <input type="password" autoFocus autoComplete="current-password" value={password} onChange={(e) => setPassword(e.target.value)} />
        </label>
        {error && (
          <p className="warn small" role="alert">
            {error}
          </p>
        )}
        <button className="primary" type="submit" disabled={busy || !password}>
          {busy ? "Logging in…" : "Log in"}
        </button>
        <p className="muted small">The password is the server's DECKLE_WEB_PASSWORD.</p>
      </form>
    </div>
  );
}
