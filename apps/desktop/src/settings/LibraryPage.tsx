// SPDX-License-Identifier: GPL-3.0-or-later
import { useCapabilities } from "../capabilities";
import { api } from "../api";
import type { LibraryStatus } from "../types";
import Originals from "./Originals";
import { Row, Switch } from "./Row";

interface Props {
  library: LibraryStatus | null;
  onLibrary: (root: string | null, autoAdd: boolean) => void;
  onPickLibrary: () => void;
}

export default function LibraryPage({ library, onLibrary, onPickLibrary }: Props) {
  const caps = useCapabilities();
  return (
    <div className="set-stack">
      <section className="set-card set-pad" aria-label="Library">
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
        {library?.root && caps.reveal && (
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
      </section>
      <section className="set-card set-pad" aria-label="Originals">
        <h3 className="set-card-title">Originals</h3>
        <p className="muted">Where Deckle looks for the originals of your Kindle's books, so Upgrades can rebuild them.</p>
        <Originals />
      </section>
    </div>
  );
}
