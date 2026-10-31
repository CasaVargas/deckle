// SPDX-License-Identifier: GPL-3.0-or-later
import { useEffect, useState } from "react";
import { api } from "../api";
import { canSwitch, confirmLine, deckleState, pluginRows, stateLine, switchOn } from "../koreaderView";
import { platform } from "../platform";
import type { KoreaderPreview, KoreaderView } from "../types";
import { Row, Switch } from "./Row";

/**
 * Settings → KOReader (KOReader plugins design, phase 4): the plugins on the plugged-in reader,
 * or on a folder that's a copy of one, the Deckle plugin that fetches the paper, and patches.
 * Every change answers with the page as it now is.
 */
export default function Koreader() {
  const [folder, setFolder] = useState<string | null>(null);
  const [view, setView] = useState<KoreaderView | null>(null);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [source, setSource] = useState("");
  const [preview, setPreview] = useState<{ source: string; p: KoreaderPreview } | null>(null);

  const load = (f: string | null = folder) => {
    setBusy(true);
    api
      .koreaderStatus(f)
      .then(setView, (e) => setError(String(e)))
      .finally(() => setBusy(false));
  };
  useEffect(() => load(), []); // eslint-disable-line react-hooks/exhaustive-deps

  const act = async (f: () => Promise<KoreaderView>) => {
    setBusy(true);
    setError(null);
    try {
      setView(await f());
    } catch (e) {
      setError(String(e));
    } finally {
      setBusy(false);
    }
  };

  const ask = async (src: string) => {
    if (!src.trim()) return;
    setBusy(true);
    setError(null);
    try {
      setPreview({ source: src, p: await api.koreaderPreview(folder, src) });
    } catch (e) {
      setError(String(e));
    } finally {
      setBusy(false);
    }
  };

  const chooseFolder = async () => {
    const f = await platform.pickFolder({ title: "A copy of a reader's drive" });
    if (f) {
      setFolder(f);
      load(f);
    }
  };
  const chooseFile = async () => {
    const files = await platform.pickFiles({ title: "A plugin (.zip)", filterName: "Zip", extensions: ["zip"] });
    if (files?.[0]) ask(files[0]);
  };

  const deckle = view ? deckleState(view) : null;
  const switching = view ? canSwitch(view) : false;
  const listing = view?.listing;

  return (
    <div className="set-stack koreader-page" aria-busy={busy}>
      <section className="set-card set-pad">
        <h3>Reader</h3>
        {view?.device ? (
          <p className="set-note">
            {view.device}
            {view.root ? ` · ${view.root}` : ""}
          </p>
        ) : null}
        {view?.note && <p className="set-note">{view.note}</p>}
        <div className="paper-actions">
          <button className="ghost" disabled={busy} onClick={() => load()}>
            Look again
          </button>
          <button className="ghost" disabled={busy} onClick={chooseFolder}>
            Use a folder…
          </button>
          {folder && (
            <button
              className="link"
              onClick={() => {
                setFolder(null);
                load(null);
              }}
            >
              Use the plugged-in reader
            </button>
          )}
        </div>
      </section>

      {listing && deckle && (
        <section className="set-card set-pad" aria-label="The Deckle plugin">
          <h3>The Deckle plugin</h3>
          <p className="set-note">
            Fetches the morning paper from KOReader over Wi-Fi when the reader wakes or joins Wi-Fi, keeps the last three, and lets
            Deckle switch plugins on and off.
          </p>
          <Row
            label={deckle.installed ? `Installed${deckle.version ? `, ${deckle.version}` : ""}` : "Not installed"}
            note={view?.deckle_blocked ?? (deckle.outdated ? `This Deckle has ${view?.deckle_version}.` : undefined)}
          >
            <button className="ghost" disabled={busy || !!view?.deckle_blocked || (deckle.installed && !deckle.outdated)} onClick={() => ask("deckle")}>
              {deckle.installed ? "Update" : "Install"}
            </button>
          </Row>
        </section>
      )}

      {preview && (
        <section className="set-card set-pad koreader-confirm" role="alertdialog" aria-label="Install a plugin">
          <p>
            <strong>{confirmLine(preview.p)}</strong>
          </p>
          <p className="set-note">Plugins run code on your reader. Install what you trust. KOReader loads it at its next start.</p>
          <div className="paper-actions">
            <button
              className="primary"
              disabled={busy}
              onClick={() => {
                const src = preview.source;
                setPreview(null);
                setSource("");
                act(() => api.koreaderInstall(folder, src));
              }}
            >
              {preview.p.replaces ? "Replace" : "Install"}
            </button>
            <button className="ghost" onClick={() => setPreview(null)}>
              Cancel
            </button>
          </div>
        </section>
      )}

      {error && <p className="warn small">{error}</p>}

      {listing && (
        <section className="set-card set-pad" aria-label="Plugins">
          <h3>Plugins</h3>
          {!switching && <p className="set-note">Install the Deckle plugin to switch plugins on and off from here; KOReader rewrites its settings when it closes.</p>}
          <ul className="koreader-plugins">
            {pluginRows(view!).map((p) => (
              <li key={p.name} className="koreader-plugin">
                <div className="koreader-plugin-text">
                  <div>
                    <strong>{p.fullname}</strong>
                    {p.version && <span className="muted small"> {p.version}</span>}
                    {p.builtin && <span className="tag">Built in</span>}
                  </div>
                  {p.description && <div className="set-note">{p.description}</div>}
                  <div className="muted small">
                    {stateLine(p)}
                    {p.source ? ` · from ${p.source.from}` : ""}
                  </div>
                  {!p.builtin && (
                    <div className="paper-actions">
                      {p.source && (
                        <button className="link" disabled={busy} onClick={() => act(() => api.koreaderUpdate(folder, p.name))}>
                          Update
                        </button>
                      )}
                      <button className="link" disabled={busy} onClick={() => act(() => api.koreaderRemove(folder, p.name))}>
                        Remove
                      </button>
                      <button className="link" disabled={busy} onClick={() => act(() => api.koreaderRollback(folder, p.name))}>
                        Undo last change
                      </button>
                    </div>
                  )}
                </div>
                <Switch label={`${p.fullname}: on`} checked={switchOn(p)} disabled={busy || !switching} onChange={(on) => act(() => api.koreaderSetEnabled(folder, p.name, on))} />
              </li>
            ))}
          </ul>
        </section>
      )}

      {listing && (
        <section className="set-card set-pad" aria-label="Add a plugin">
          <h3>Add a plugin</h3>
          <form
            className="save-row"
            onSubmit={(e) => {
              e.preventDefault();
              ask(source);
            }}
          >
            <input value={source} placeholder="owner/repo or a GitHub link" spellCheck={false} onChange={(e) => setSource(e.target.value)} aria-label="GitHub repository" />
            <button className="ghost" type="submit" disabled={busy || !source.trim()}>
              Check
            </button>
          </form>
          <div className="paper-actions">
            <button className="link" disabled={busy} onClick={chooseFile}>
              From a .zip…
            </button>
            <button
              className="link"
              disabled={busy}
              onClick={async () => {
                const f = await platform.pickFolder({ title: "A .koplugin folder" });
                if (f) ask(f);
              }}
            >
              From a folder…
            </button>
          </div>
          <p className="set-note">Deckle checks a plugin before anything is written, and keeps what it replaces so Undo brings it back.</p>
        </section>
      )}

      {listing && listing.patches.length > 0 && (
        <section className="set-card set-pad" aria-label="Patches">
          <h3>Patches</h3>
          <Row label={listing.patches.join(", ")} note="Every patch at once; KOReader reads this at its next start.">
            <Switch label="Patches on" checked={!listing.patches_disabled} disabled={busy} onChange={(on) => act(() => api.koreaderSetPatches(folder, on))} />
          </Row>
        </section>
      )}
    </div>
  );
}
