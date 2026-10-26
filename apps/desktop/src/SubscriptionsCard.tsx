// SPDX-License-Identifier: GPL-3.0-or-later
import { useCallback, useEffect, useState } from "react";
import { api } from "./api";
import { subscriptionLine } from "./subscriptions";
import type { SiteStatus, SubscriptionConfig } from "./types";

/**
 * Newspapers the household subscribes to (publisher subscriptions design §3): a site and the
 * account that signs in to it. The password is write-only and lives in the keychain.
 */
export default function SubscriptionsCard({
  subscriptions,
  dirty,
  onChange,
  onSave,
}: {
  subscriptions: SubscriptionConfig[];
  dirty: boolean;
  onChange: (subs: SubscriptionConfig[]) => void;
  /** Saves the draft; false when it didn't save (the notice says why). */
  onSave: () => Promise<boolean>;
}) {
  const [statuses, setStatuses] = useState<SiteStatus[]>([]);
  const [passwords, setPasswords] = useState<Record<number, string>>({});
  const [busy, setBusy] = useState<number | null>(null);
  const [msg, setMsg] = useState<{ text: string; ok: boolean } | null>(null);

  const refresh = useCallback(() => {
    api.subscriptionsStatus().then(setStatuses).catch(() => {});
  }, []);
  useEffect(refresh, [refresh]);

  const edit = (i: number, p: Partial<SubscriptionConfig>) =>
    onChange(subscriptions.map((s, j) => (j === i ? { ...s, ...p } : s)));
  const statusOf = (s: SubscriptionConfig) => statuses.find((t) => t.site === s.site.trim().toLowerCase().replace(/^https?:\/\//, "").replace(/^www\./, "").replace(/\/$/, ""));
  // The keychain entry and the session are named after the saved site and username.
  const saved = async () => !dirty || (await onSave());

  const savePassword = async (i: number) => {
    const pw = passwords[i];
    if (!pw || busy !== null) return;
    setBusy(i);
    setMsg(null);
    try {
      if (await saved()) {
        const site = subscriptions[i].site;
        setStatuses(await api.subscriptionSetPassword(site, pw));
        setMsg({ text: "Password saved in the system keychain.", ok: true });
      }
    } catch (e) {
      setMsg({ text: String(e), ok: false });
    }
    setPasswords((p) => ({ ...p, [i]: "" }));
    setBusy(null);
  };

  const test = async (i: number) => {
    if (busy !== null) return;
    setBusy(i);
    setMsg(null);
    try {
      if (await saved()) {
        await api.subscriptionTest(subscriptions[i].site);
        refresh();
      }
    } catch (e) {
      setMsg({ text: String(e), ok: false });
    }
    setBusy(null);
  };

  const remove = async (i: number) => {
    const s = subscriptions[i];
    const next = subscriptions.filter((_, j) => j !== i);
    setMsg(null);
    if (!statusOf(s)) {
      onChange(next); // never saved: nothing in the keychain or on disk
      return;
    }
    try {
      setStatuses(await api.subscriptionForget(s.site));
      onChange(next);
    } catch (e) {
      setMsg({ text: String(e), ok: false });
    }
  };

  return (
    <section className="card subscriptions-card" aria-label="Subscriptions">
      <div className="card-head">
        <h3>Subscriptions</h3>
        {dirty && (
          <button className="primary" onClick={() => onSave()}>
            Save
          </button>
        )}
      </div>
      <p className="muted small">
        Newspapers you pay for. Deckle signs in with your account so their stories come into the paper whole. The password is kept
        in the system keychain, never in digest.toml.
      </p>
      {subscriptions.map((s, i) => {
        const st = statusOf(s);
        return (
          <div key={i} className="subscription-row">
            <div className="grid2">
              <label>
                <span className="label">Site</span>
                <input value={s.site} placeholder="thepilot.com" spellCheck={false} onChange={(e) => edit(i, { site: e.target.value })} />
              </label>
              <label>
                <span className="label">Username</span>
                <input value={s.username} placeholder="you@example.com" spellCheck={false} onChange={(e) => edit(i, { username: e.target.value })} />
              </label>
            </div>
            <div className="save-row">
              <input
                type="password"
                autoComplete="new-password"
                value={passwords[i] ?? ""}
                placeholder={st?.has_password ? "Saved in the keychain" : "Password"}
                onChange={(e) => setPasswords((p) => ({ ...p, [i]: e.target.value }))}
              />
              <button className="ghost" disabled={busy !== null || !passwords[i]} onClick={() => savePassword(i)}>
                Save password
              </button>
              <button className="ghost" disabled={busy !== null || !s.site.trim() || !s.username.trim()} onClick={() => test(i)}>
                {busy === i ? "Working…" : "Test sign-in"}
              </button>
              <button className="ghost" disabled={busy !== null} onClick={() => remove(i)}>
                Remove
              </button>
            </div>
            {st && <p className="small">{subscriptionLine(st)}</p>}
            {st?.jar_note && <p className="muted small">{st.jar_note}</p>}
          </div>
        );
      })}
      {msg && <p className={`small ${msg.ok ? "" : "fail"}`}>{msg.text}</p>}
      <div className="save-row">
        <button className="ghost" onClick={() => onChange([...subscriptions, { site: "", username: "" }])}>
          Add a subscription
        </button>
      </div>
    </section>
  );
}
