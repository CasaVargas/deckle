// SPDX-License-Identifier: GPL-3.0-or-later
import { useEffect, useState } from "react";
import { platform } from "../platform";
import { api } from "../api";
import Icon from "../Icon";
import type { SourcesStatus } from "../types";
import { Row, Switch } from "./Row";

/**
 * Where the Kindle tab looks for originals of books Deckle didn't send (Books and Kindle design
 * §4): a calibre library, read and never written, and folders of EPUBs. Each change saves at once.
 */
export default function Originals() {
  const [status, setStatus] = useState<SourcesStatus | null>(null);
  const [error, setError] = useState<string | null>(null);
  useEffect(() => {
    api.librarySources().then(setStatus).catch((e) => setError(String(e)));
  }, []);
  const save = async (findInCalibre: boolean, calibre: string | null, folders: string[]) => {
    try {
      setStatus(await api.librarySourcesSet(findInCalibre, calibre, folders));
      setError(null);
    } catch (e) {
      setError(String(e));
    }
  };
  if (!status) return error ? <p className="warn small">{error}</p> : null;
  const pickCalibre = async () => {
    const dir = await platform.pickFolder({});
    if (typeof dir === "string") save(true, dir, status.folders);
  };
  const addFolder = async () => {
    const dir = await platform.pickFolder({});
    if (typeof dir === "string" && !status.folders.includes(dir)) save(status.find_in_calibre, status.calibre, [...status.folders, dir]);
  };
  const setWatch = async (dir: string | null) => {
    try {
      const watch = await api.libraryWatchSet(dir);
      setStatus({ ...status, watch });
      setError(null);
    } catch (e) {
      setError(String(e));
    }
  };
  const watchFolder = async () => {
    const dir = await platform.pickFolder({});
    if (typeof dir === "string") setWatch(dir);
  };
  return (
    <>
      <Row
        label="Find originals in calibre"
        note={
          status.find_in_calibre
            ? status.calibre
              ? `${status.calibre} · read only: Deckle never changes it`
              : "No calibre library found. Choose its folder."
            : "Books calibre sent to your Kindle can be upgraded from calibre's EPUBs"
        }
      >
        {status.find_in_calibre && (
          <button className="ghost" onClick={pickCalibre}>
            {status.calibre ? "Change…" : "Choose…"}
          </button>
        )}
        <Switch label="Find originals in calibre" checked={status.find_in_calibre} onChange={(on) => save(on, status.calibre, status.folders)} />
      </Row>
      <Row label="Find originals in folders" note="EPUBs in these folders, a few levels down">
        <button className="ghost" onClick={addFolder}>
          Add folder…
        </button>
      </Row>
      {status.folders.map((f) => (
        <div key={f} className="set-row originals-folder">
          <span className="mono">{f}</span>
          <button className="icon small" aria-label={`Stop looking in ${f}`} onClick={() => save(status.find_in_calibre, status.calibre, status.folders.filter((x) => x !== f))}>
            <Icon name="x" size={14} />
          </button>
        </div>
      ))}
      <Row
        label="Watch a folder"
        note={status.watch ? `New EPUBs in ${status.watch} are added to your library` : "EPUBs that land in a folder, Downloads say, are added to your library"}
      >
        {status.watch && (
          <button className="ghost" onClick={watchFolder}>
            Change…
          </button>
        )}
        <Switch label="Watch a folder" checked={!!status.watch} onChange={(on) => (on ? watchFolder() : setWatch(null))} />
      </Row>
      {error && <p className="warn small">{error}</p>}
    </>
  );
}
