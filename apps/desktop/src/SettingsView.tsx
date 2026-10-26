// SPDX-License-Identifier: GPL-3.0-or-later
import { open as openDialog } from "@tauri-apps/plugin-dialog";
import { useEffect, useState } from "react";
import { api } from "./api";
import Icon from "./Icon";
import { escStaysInField } from "./settingsKeys";
import { PAGES, onlineOn, pageNotes, pageSummary, searchSettings, setupChecks } from "./settingsIndex";
import type { Check, SettingsPage } from "./settingsIndex";
import About from "./settings/About";
import Books from "./settings/Books";
import General from "./settings/General";
import Kindles from "./settings/Kindles";
import Online from "./settings/Online";
import Overview from "./settings/Overview";
import ReadingAids from "./settings/ReadingAids";
import Sending from "./settings/Sending";
import type { DeviceRow, HighlightsStatus, LibraryStatus, PreviewerStatus, Settings, SourcesStatus } from "./types";
import type { UpdateState } from "./updates";

export type { SettingsPage } from "./settingsIndex";

interface Props {
  page: SettingsPage;
  onPage: (p: SettingsPage) => void;
  onBack: () => void;
  version: string;
  settings: Settings;
  previewer: PreviewerStatus | null;
  devices: DeviceRow[];
  onChange: (s: Settings) => void;
  onDeviceOptions: (key: string, cde: "pdoc" | "ebok" | null, verified: boolean) => void;
  onPickPreviewer: () => void;
  library: LibraryStatus | null;
  onLibrary: (root: string | null, autoAdd: boolean) => void;
  onPickLibrary: () => void;
  update: UpdateState;
  onUpdateState: (s: UpdateState) => void;
  /** A conversion or send is running: nothing that closes Deckle may start. */
  busy: boolean;
}

/** Settings take over the window: pages in a sidebar, one page at a time, and a search over every setting (Settings redesign §3). */
export default function SettingsView(props: Props) {
  const { page, onPage, onBack, settings, onChange } = props;
  const [query, setQuery] = useState("");
  // Overview's checklist reads where originals and highlights go; read again on each visit.
  const [sources, setSources] = useState<SourcesStatus | null>(null);
  const [highlights, setHighlights] = useState<HighlightsStatus | null | undefined>(undefined);
  useEffect(() => {
    api.librarySources().then(setSources).catch(() => setSources(null));
    api.highlightsStatus().then(setHighlights).catch(() => setHighlights(undefined));
  }, [page]);
  const checks = setupChecks({ previewer: props.previewer, library: props.library, sources, highlights: highlights === undefined ? undefined : (highlights?.folder ?? null), devices: props.devices });
  const notes = pageNotes(checks, onlineOn(settings));
  const fix = async (c: Check) => {
    if (c.key === "previewer") props.onPickPreviewer();
    else if (c.key === "library") props.onPickLibrary();
    else if (c.key === "highlights") {
      const dir = await openDialog({ directory: true, multiple: false });
      if (typeof dir === "string") setHighlights(await api.highlightsSet(dir, highlights?.clippings ?? false));
    } else onPage(c.page);
  };
  const results = searchSettings(query);

  // Esc goes back, unless a field wants it.
  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      const el = e.target as HTMLInputElement | null;
      if (e.key !== "Escape" || (el && escStaysInField(el.tagName, el.type ?? ""))) return;
      if (query) setQuery("");
      else onBack();
    };
    document.addEventListener("keydown", onKey);
    return () => document.removeEventListener("keydown", onKey);
  }, [onBack, query]);

  const set = <K extends keyof Settings>(k: K, v: Settings[K]) => onChange({ ...settings, [k]: v });
  const title = PAGES.find((p) => p.id === page)!.title;

  return (
    <div className="settings">
      <nav className="settings-nav" aria-label="Settings pages">
        <button className="settings-back" onClick={onBack}>
          <Icon name="arrow-left" /> Back to Deckle
        </button>
        <label className="settings-search">
          <span className="visually-hidden">Search settings</span>
          <input type="search" placeholder="Search settings" value={query} onChange={(e) => setQuery(e.target.value)} />
        </label>
        {PAGES.map((p) => (
          <button
            key={p.id}
            className={p.id === page && !query ? "on" : ""}
            aria-current={p.id === page && !query ? "page" : undefined}
            onClick={() => {
              setQuery("");
              onPage(p.id);
            }}
          >
            <span>{p.title}</span>
            {notes[p.id] && <span className={`nav-note ${p.id === "online" ? "plain" : ""}`}>{notes[p.id]}</span>}
          </button>
        ))}
      </nav>
      <main className="settings-page">
        <div className="settings-inner">
          {query ? (
            <>
              <h2>Search</h2>
              {results.length === 0 ? (
                <p className="muted">No setting matches “{query.trim()}”.</p>
              ) : (
                <ul className="search-results">
                  {results.map((r) => (
                    <li key={r.label}>
                      <button
                        className="search-result"
                        onClick={() => {
                          setQuery("");
                          onPage(r.page);
                        }}
                      >
                        <strong>{r.label}</strong>
                        <span className="muted">{PAGES.find((p) => p.id === r.page)!.title}</span>
                      </button>
                    </li>
                  ))}
                </ul>
              )}
            </>
          ) : (
          <>
          <h2>{title}</h2>
          {page === "overview" && (
            <Overview checks={checks} online={onlineOn(settings)} summary={(p) => pageSummary(p, settings, props.library, props.devices, props.version)} onFix={fix} onPage={onPage} />
          )}
          {page === "sending" && (
            <Sending settings={settings} set={set} previewer={props.previewer} onPickPreviewer={props.onPickPreviewer} devices={props.devices} onKindles={() => onPage("kindles")} />
          )}
          {page === "general" && (
            <General
              settings={settings}
              set={set}
              previewer={props.previewer}
              onPickPreviewer={props.onPickPreviewer}
              library={props.library}
              onLibrary={props.onLibrary}
              onPickLibrary={props.onPickLibrary}
            />
          )}
          {page === "books" && <Books settings={settings} set={set} />}
          {page === "reading" && <ReadingAids settings={settings} set={set} />}
          {page === "kindles" && <Kindles devices={props.devices} onDeviceOptions={props.onDeviceOptions} />}
          {page === "online" && <Online settings={settings} set={set} onPage={onPage} />}
          {page === "about" && <About version={props.version} update={props.update} onUpdateState={props.onUpdateState} busy={props.busy} />}
          </>
          )}
        </div>
      </main>
    </div>
  );
}
