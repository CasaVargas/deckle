// SPDX-License-Identifier: GPL-3.0-or-later
import { api } from "../api";
import type { LibraryStatus, PreviewerStatus, Settings } from "../types";
import Originals from "./Originals";
import { Link, Row, Switch } from "./Row";

interface Props {
  settings: Settings;
  set: <K extends keyof Settings>(k: K, v: Settings[K]) => void;
  previewer: PreviewerStatus | null;
  onPickPreviewer: () => void;
  library: LibraryStatus | null;
  onLibrary: (root: string | null, autoAdd: boolean) => void;
  onPickLibrary: () => void;
}

export default function General({ settings, set, previewer, onPickPreviewer, library, onLibrary, onPickLibrary }: Props) {
  return (
    <>
      <Row
        label="Library"
        note={
          library?.root ? (
            <>
              {library.books} book{library.books === 1 ? "" : "s"} · <code>{library.root}</code>
            </>
          ) : (
            "Keep a copy of every book you send, to send again or upgrade later"
          )
        }
      >
        <button className="ghost" onClick={onPickLibrary}>
          {library?.root ? "Change…" : "Choose folder…"}
        </button>
        {library?.root && (
          <button className="ghost" onClick={() => api.reveal(library.root!)}>
            Show
          </button>
        )}
        {library?.root && (
          <button className="ghost" onClick={() => onLibrary(null, library.auto_add)}>
            Stop using
          </button>
        )}
      </Row>
      {library?.root && (
        <Row label="Add every book I send">
          <Switch label="Add every book I send" checked={library.auto_add} onChange={(on) => onLibrary(library.root, on)} />
        </Row>
      )}
      <Originals />

    </>
  );
}
