// SPDX-License-Identifier: GPL-3.0-or-later
import { useCallback, useEffect, useState } from "react";
import { api } from "./api";
import { SUBSCRIBED_SLUGS, forgetTested, groupFor, removeGroup, renameGroup, setMaxItems, shownStatus, statusKey, subscriptionLine, toggleGroup, toggleSlug } from "./subscriptions";
import type { Tested } from "./subscriptions";
import type { DailyConfig, SiteStatus, SubscriptionConfig } from "./types";

/**
 * Newspapers the household subscribes to (publisher subscriptions design §3): a site and the
 * account that signs in to it. The password is write-only and lives in the keychain.
 */
export default function SubscriptionsCard({
  subscriptions,
  config,
  dirty,
  onChange,
  onConfig,
  onSave,
}: {
  subscriptions: SubscriptionConfig[];
  /** The paper's draft: the `[[subscribed]]` groups and `sections` ride on it. */
  config: DailyConfig;
  dirty: boolean;
  onChange: (subs: SubscriptionConfig[]) => void;
  /** Replaces the draft with an edited copy (the helpers in subscriptions.ts return one). */
  onConfig: (next: (c: DailyConfig) => DailyConfig) => void;
  /** Saves the draft; false when it didn't save (the notice says why). */
  onSave: () => Promise<boolean>;
}) {
  const [statuses, setStatuses] = useState<SiteStatus[]>([]);
  const [passwords, setPasswords] = useState<Record<number, string>>({});
  // A fresh status only knows what this process has tried, so a test's answer is kept here.
  const [tested, setTested] = useState<Tested>({});
  const [busy, setBusy] = useState<number | null>(null);
  const [msg, setMsg] = useState<{ text: string; ok: boolean } | null>(null);

  const refresh = useCallback(() => {
    api.subscriptionsStatus().then(setStatuses).catch(() => {});
  }, []);
  useEffect(refresh, [refresh]);

  const edit = (i: number, p: Partial<SubscriptionConfig>) => {
    // A test's answer is about the site and account it was run for.
    setTested((t) => forgetTested(t, subscriptions[i].site));
    onChange(subscriptions.map((s, j) => (j === i ? { ...s, ...p } : s)));
    // The paper's group follows the row's site.
    if (p.site !== undefined) onConfig((c) => renameGroup(c, subscriptions[i].site, p.site as string));
  };
  const statusOf = (s: SubscriptionConfig): SiteStatus | undefined => shownStatus(statuses, tested, s.site);
  // The keychain entry and the session are named after the saved site and username.
  const saved = async () => {
    if (!dirty) return true;
    const ok = await onSave();
    if (ok) refresh();
    return ok;
  };

  const savePassword = async (i: number) => {
    const pw = passwords[i];
    if (!pw || busy !== null) return;
    setBusy(i);
    setMsg(null);
    try {
      if (await saved()) {
        const site = subscriptions[i].site;
        setStatuses(await api.subscriptionSetPassword(site, pw));
        setTested((t) => forgetTested(t, site));
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
        const site = subscriptions[i].site;
        const state = await api.subscriptionTest(site);
        refresh();
        setTested((t) => ({ ...t, [statusKey(site)]: state }));
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
    setPasswords({}); // typed passwords are by row, and the rows shift
    if (!statusOf(s)) {
      onChange(next); // never saved: nothing in the keychain or on disk
      onConfig((c) => removeGroup(c, s.site));
      return;
    }
    try {
      setStatuses(await api.subscriptionForget(s.site));
      onChange(next);
      onConfig((c) => removeGroup(c, s.site));
    } catch (e) {
      setMsg({ text: String(e), ok: false });
    }
  };

  return (
    <section className="card subscriptions-card" aria-label="Subscriptions">
      <div className="card-head">
        <h3>Subscriptions</h3>
        {dirty && (
          <button className="primary" onClick={() => saved()}>
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
            {s.site.trim() && (
              <PrintPaper
                site={s.site}
                config={config}
                onConfig={onConfig}
              />
            )}
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

/** "Print this paper": the site's `[[subscribed]]` group, its sections and its stories per section. */
function PrintPaper({ site, config, onConfig }: { site: string; config: DailyConfig; onConfig: (f: (c: DailyConfig) => DailyConfig) => void }) {
  const group = groupFor(config, site);
  return (
    <div className="print-paper">
      <label className="check">
        <input type="checkbox" checked={!!group} onChange={() => onConfig((c) => toggleGroup(c, site))} /> Print this paper
      </label>
      {group && (
        <>
          <div className="print-paper-slugs">
            {SUBSCRIBED_SLUGS.map(({ slug, name }) => (
              <label key={slug} className="check">
                <input type="checkbox" checked={group.sections.includes(slug)} onChange={() => onConfig((c) => toggleSlug(c, site, slug))} /> {name}
              </label>
            ))}
          </div>
          <label className="print-paper-max">
            <span className="label">Stories per section</span>
            <input
              type="number"
              min={1}
              value={group.max_items ?? ""}
              placeholder="Same as Local"
              onChange={(e) => onConfig((c) => setMaxItems(c, site, e.target.value))}
            />
          </label>
        </>
      )}
    </div>
  );
}
