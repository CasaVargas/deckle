// SPDX-License-Identifier: GPL-3.0-or-later
import { useEffect } from "react";
import Icon from "./Icon";
import { escStaysInField } from "./settingsKeys";
import About from "./settings/About";
import Books from "./settings/Books";
import General from "./settings/General";
import Kindles from "./settings/Kindles";
import WordWise from "./settings/WordWise";
import XRay from "./settings/XRay";
import type { DeviceRow, LibraryStatus, PreviewerStatus, Settings } from "./types";
import type { UpdateState } from "./updates";

export type SettingsPage = "general" | "books" | "xray" | "wordwise" | "kindles" | "about";

const PAGES: { id: SettingsPage; title: string }[] = [
  { id: "general", title: "General" },
  { id: "books", title: "Books" },
  { id: "xray", title: "X-Ray" },
  { id: "wordwise", title: "Word Wise" },
  { id: "kindles", title: "Kindles" },
  { id: "about", title: "About" },
];

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

/** Settings take over the window: pages in a sidebar, one page at a time. */
export default function SettingsView(props: Props) {
  const { page, onPage, onBack, settings, onChange } = props;

  // Esc goes back, unless a field wants it.
  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      const el = e.target as HTMLInputElement | null;
      if (e.key !== "Escape" || (el && escStaysInField(el.tagName, el.type ?? ""))) return;
      onBack();
    };
    document.addEventListener("keydown", onKey);
    return () => document.removeEventListener("keydown", onKey);
  }, [onBack]);

  const set = <K extends keyof Settings>(k: K, v: Settings[K]) => onChange({ ...settings, [k]: v });
  const title = PAGES.find((p) => p.id === page)!.title;

  return (
    <div className="settings">
      <nav className="settings-nav" aria-label="Settings pages">
        <button className="settings-back" onClick={onBack}>
          <Icon name="arrow-left" /> Back
        </button>
        {PAGES.map((p) => (
          <button key={p.id} className={p.id === page ? "on" : ""} aria-current={p.id === page ? "page" : undefined} onClick={() => onPage(p.id)}>
            {p.title}
          </button>
        ))}
      </nav>
      <main className="settings-page">
        <div className="settings-inner">
          <h2>{title}</h2>
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
          {page === "xray" && <XRay settings={settings} set={set} />}
          {page === "wordwise" && <WordWise settings={settings} set={set} />}
          {page === "kindles" && <Kindles devices={props.devices} onDeviceOptions={props.onDeviceOptions} />}
          {page === "about" && <About version={props.version} update={props.update} onUpdateState={props.onUpdateState} busy={props.busy} />}
        </div>
      </main>
    </div>
  );
}
