// SPDX-License-Identifier: GPL-3.0-or-later
// The web app's file and folder picker: it browses the server's folders (the roots it allows),
// and picking files can upload them from this computer instead.
import { useEffect, useRef, useState } from "react";
import { createRoot } from "react-dom/client";
import { fetchListing, fetchRoots, pickerView, upload, type Listing, type PickMode } from "./serverFiles";

interface Props {
  mode: PickMode;
  title?: string;
  extensions?: string[];
  multiple?: boolean;
  /** Where to start; the first root when missing or not allowed. */
  start?: string;
  onDone: (paths: string[] | null) => void;
}

const size = (n: number) => (n >= 1048576 ? `${(n / 1048576).toFixed(1)} MB` : n >= 1024 ? `${Math.round(n / 1024)} KB` : `${n} B`);

export default function ServerPicker({ mode, title, extensions, multiple, start, onDone }: Props) {
  const [roots, setRoots] = useState<string[] | null>(null);
  /** null: the list of roots. */
  const [listing, setListing] = useState<Listing | null>(null);
  const [picked, setPicked] = useState<string[]>([]);
  const [error, setError] = useState<string | null>(null);
  const [uploading, setUploading] = useState(false);
  const fileInput = useRef<HTMLInputElement>(null);

  const go = async (path: string) => {
    try {
      setListing(await fetchListing(path));
      setPicked([]);
      setError(null);
    } catch (e) {
      setError(String(e));
    }
  };

  useEffect(() => {
    fetchRoots()
      .then(async (rs) => {
        setRoots(rs);
        const first = start ?? rs[0];
        if (first === undefined) return;
        try {
          setListing(await fetchListing(first));
        } catch {
          // A start that isn't allowed (or gone) opens the first root instead.
          if (rs[0] !== undefined && first !== rs[0]) setListing(await fetchListing(rs[0]).catch(() => null));
        }
      })
      .catch((e) => setError(String(e)));
    const onKey = (e: KeyboardEvent) => e.key === "Escape" && onDone(null);
    document.addEventListener("keydown", onKey);
    return () => document.removeEventListener("keydown", onKey);
  }, []);

  const view = listing && roots ? pickerView(listing, roots, { mode, extensions }) : null;
  const toggle = (path: string) =>
    setPicked((p) => (p.includes(path) ? p.filter((x) => x !== path) : multiple ? [...p, path] : [path]));

  const sendFiles = async (files: FileList | null) => {
    if (!files || files.length === 0) return;
    setUploading(true);
    setError(null);
    try {
      onDone(await upload(Array.from(files)));
    } catch (e) {
      setError(String(e));
    } finally {
      setUploading(false);
    }
  };

  const heading = title ?? (mode === "folder" ? "Choose a folder on the server" : multiple ? "Choose files on the server" : "Choose a file on the server");
  return (
    <div className="picker-backdrop" onClick={(e) => e.target === e.currentTarget && onDone(null)}>
      <div className="picker card" role="dialog" aria-modal="true" aria-label={heading}>
        <div className="picker-head">
          <h3>{heading}</h3>
          <button className="quiet" onClick={() => onDone(null)} aria-label="Close">
            ✕
          </button>
        </div>
        <div className="picker-where">
          <button className="ghost" disabled={!view?.up} onClick={() => (view?.up === "roots" ? setListing(null) : view?.up && go(view.up))}>
            Up
          </button>
          <span className="mono small">{listing?.path ?? "Server folders"}</span>
        </div>
        <ul className="picker-list" aria-label="Folders and files">
          {!listing &&
            roots?.map((r) => (
              <li key={r}>
                <button className="picker-row" onClick={() => go(r)}>
                  <span aria-hidden="true">▸</span> <span className="mono">{r}</span>
                </button>
              </li>
            ))}
          {view?.rows.map((e) => (
            <li key={e.path}>
              {e.dir ? (
                <button className="picker-row" onClick={() => go(e.path)}>
                  <span aria-hidden="true">▸</span> {e.name}
                </button>
              ) : (
                <button className={`picker-row ${picked.includes(e.path) ? "on" : ""}`} aria-pressed={picked.includes(e.path)} onClick={() => toggle(e.path)} onDoubleClick={() => onDone([e.path])}>
                  <span aria-hidden="true">·</span> {e.name} <span className="muted small">{size(e.size)}</span>
                </button>
              )}
            </li>
          ))}
          {view && view.rows.length === 0 && <li className="muted small picker-empty">{mode === "folder" ? "No folders here." : "Nothing to pick here."}</li>}
        </ul>
        {error && <p className="warn small">{error}</p>}
        <div className="picker-foot">
          {mode === "file" && (
            <>
              <input
                ref={fileInput}
                type="file"
                hidden
                multiple={multiple}
                accept={extensions?.map((x) => `.${x}`).join(",")}
                onChange={(e) => sendFiles(e.target.files)}
              />
              <button className="ghost" disabled={uploading} onClick={() => fileInput.current?.click()}>
                {uploading ? "Uploading…" : "Upload from this computer…"}
              </button>
            </>
          )}
          <div className="spacer" />
          <button className="quiet" onClick={() => onDone(null)}>
            Cancel
          </button>
          {mode === "folder" ? (
            <button className="primary" disabled={!listing} onClick={() => listing && onDone([listing.path])}>
              Choose this folder
            </button>
          ) : (
            <button className="primary" disabled={picked.length === 0} onClick={() => onDone(picked)}>
              Choose
            </button>
          )}
        </div>
      </div>
    </div>
  );
}

/** Opens the picker over the page; resolves when it's closed (null: cancelled). */
export function openServerPicker(o: Omit<Props, "onDone">): Promise<string[] | null> {
  return new Promise((resolve) => {
    const host = document.createElement("div");
    document.body.appendChild(host);
    const root = createRoot(host);
    const done = (paths: string[] | null) => {
      root.unmount();
      host.remove();
      resolve(paths);
    };
    root.render(<ServerPicker {...o} onDone={done} />);
  });
}
