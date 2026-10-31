// SPDX-License-Identifier: GPL-3.0-or-later
import { useState } from "react";
import { api } from "../api";
import { pickAction } from "../koreaderView";
import type { KoreaderView, StoreFound, StorePick, StoreUpdate } from "../types";

/**
 * The plugin store (KOReader plugins design part C): plugins Deckle recommends, each pinned to a
 * reviewed release, and GitHub's koreader-plugin topic, not reviewed. Nothing goes online until
 * the owner opens it. Installing goes through the page's preview and question.
 */
export default function KoreaderStore({
  view,
  folder,
  busy,
  onInstall,
  onUpdates,
}: {
  view: KoreaderView | null;
  folder: string | null;
  busy: boolean;
  /** Shows what installing `source` would do, then asks. */
  onInstall: (source: string) => void;
  onUpdates: (u: StoreUpdate[]) => void;
}) {
  const [open, setOpen] = useState(false);
  const [tab, setTab] = useState<"recommended" | "discover">("recommended");
  const [picks, setPicks] = useState<{ live: boolean; plugins: StorePick[] } | null>(null);
  const [found, setFound] = useState<StoreFound[] | null>(null);
  const [text, setText] = useState("");
  const [updated, setUpdated] = useState(false);
  const [readme, setReadme] = useState<{ repo: string; text: string; latest: string | null } | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [loading, setLoading] = useState(false);

  const run = async (f: () => Promise<void>) => {
    setLoading(true);
    setError(null);
    try {
      await f();
    } catch (e) {
      setError(String(e));
    } finally {
      setLoading(false);
    }
  };

  const openStore = () =>
    run(async () => {
      setOpen(true);
      setPicks(await api.storeList());
      if (view?.listing) onUpdates(await api.storeUpdates(folder));
    });
  const search = () => run(async () => setFound(await api.storeSearch(text, updated)));
  const showReadme = (repo: string) =>
    readme?.repo === repo
      ? setReadme(null)
      : run(async () => {
          const r = await api.storeReadme(repo);
          setReadme({ repo, text: r.readme, latest: r.latest });
        });

  if (!open) {
    return (
      <section className="set-card set-pad" aria-label="Plugin store">
        <h3>Plugin store</h3>
        <p className="set-note">
          Plugins Deckle recommends, each pinned to a release it reviewed, and community plugins from GitHub. It goes online only while
          it's open.
        </p>
        <button className="ghost" disabled={busy || loading} onClick={openStore}>
          {loading ? "Opening…" : "Open the store"}
        </button>
        {error && <p className="warn small">{error}</p>}
      </section>
    );
  }

  const disabled = busy || loading || !view?.listing;
  const readmeFor = (repo: string) =>
    readme?.repo === repo && (
      <div className="store-readme">
        {readme.latest && <p className="muted small">Newest release: {readme.latest}</p>}
        <pre>{readme.text}</pre>
      </div>
    );

  return (
    <section className="set-card set-pad" aria-label="Plugin store">
      <h3>Plugin store</h3>
      <div className="store-tabs" role="tablist">
        <button role="tab" aria-selected={tab === "recommended"} className={tab === "recommended" ? "on" : ""} onClick={() => setTab("recommended")}>
          Recommended
        </button>
        <button role="tab" aria-selected={tab === "discover"} className={tab === "discover" ? "on" : ""} onClick={() => setTab("discover")}>
          Discover
        </button>
      </div>
      {!view?.listing && <p className="set-note">Plug in a reader with KOReader (or choose a folder) to install.</p>}

      {tab === "recommended" && picks && (
        <>
          {!picks.live && <p className="muted small">The list built into this Deckle (the newest couldn't be fetched).</p>}
          <ul className="store-list">
            {picks.plugins.map((p) => {
              const action = pickAction(p, view);
              return (
                <li key={p.repo} className="store-item">
                  <div className="store-text">
                    <div>
                      <strong>{p.name}</strong> <span className="muted small">{p.release.tag}</span>
                      <span className="tag">Reviewed</span>
                    </div>
                    <div className="set-note">{p.why}</div>
                    <div className="muted small">
                      {p.repo} · {p.license}{" "}
                      <button className="link" onClick={() => showReadme(p.repo)}>
                        {readme?.repo === p.repo ? "Hide details" : "Details"}
                      </button>
                    </div>
                    {readmeFor(p.repo)}
                  </div>
                  <button className="ghost" disabled={disabled || action === "installed"} onClick={() => onInstall(p.repo)}>
                    {action === "install" ? "Install" : action === "update" ? "Update" : "Installed"}
                  </button>
                </li>
              );
            })}
          </ul>
        </>
      )}

      {tab === "discover" && (
        <>
          <p className="set-note">Community plugins with the koreader-plugin topic on GitHub. Deckle hasn't reviewed them: read the details first.</p>
          <form
            className="save-row"
            onSubmit={(e) => {
              e.preventDefault();
              search();
            }}
          >
            <input value={text} placeholder="Search, or leave empty for all" spellCheck={false} onChange={(e) => setText(e.target.value)} aria-label="Search plugins" />
            <select value={updated ? "updated" : "stars"} onChange={(e) => setUpdated(e.target.value === "updated")} aria-label="Sort">
              <option value="stars">Most starred</option>
              <option value="updated">Recently updated</option>
            </select>
            <button className="ghost" type="submit" disabled={loading}>
              Search
            </button>
          </form>
          {found && found.length === 0 && <p className="muted small">Nothing found.</p>}
          {found && (
            <ul className="store-list">
              {found.map((f) => (
                <li key={f.repo} className="store-item">
                  <div className="store-text">
                    <div>
                      <strong>{f.repo}</strong>
                      <span className="tag">Not reviewed</span>
                    </div>
                    {f.description && <div className="set-note">{f.description}</div>}
                    <div className="muted small">
                      ★ {f.stars} · updated {f.pushed} · {f.license ?? "no license"}{" "}
                      <button className="link" onClick={() => showReadme(f.repo)}>
                        {readme?.repo === f.repo ? "Hide details" : "Details"}
                      </button>
                    </div>
                    {readmeFor(f.repo)}
                  </div>
                  <button className="ghost" disabled={disabled} onClick={() => onInstall(f.repo)}>
                    Install
                  </button>
                </li>
              ))}
            </ul>
          )}
        </>
      )}
      {loading && <p className="muted small">Asking GitHub…</p>}
      {error && <p className="warn small">{error}</p>}
    </section>
  );
}
