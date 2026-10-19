// SPDX-License-Identifier: GPL-3.0-or-later
import { useCallback, useEffect, useRef, useState } from "react";
import { getCurrentWebview } from "@tauri-apps/api/webview";
import { open as openDialog } from "@tauri-apps/plugin-dialog";
import { api } from "./api";
import BookRowView from "./BookRow";
import SettingsDrawer from "./SettingsDrawer";
import type { BookRow, BookState, DeviceRow, PreviewerStatus, RunTarget, Settings } from "./types";

const DEFAULT_SETTINGS: Settings = {
  previewer_path: null,
  cde: "pdoc",
  pages_auto: true,
  llm_provider: "off",
  llm_model: "",
  llm_endpoint: "",
  word_wise_level: 3,
  wikipedia: false,
  amazon_match: false,
};

function toState(row: BookRow): BookState {
  return { ...row, status: "idle", stage: null, delivered: null, pages: null, runWarnings: [], runError: null, cdeOverride: null, showReport: false };
}

export default function App() {
  const [version, setVersion] = useState("");
  const [books, setBooks] = useState<BookState[]>([]);
  const [devices, setDevices] = useState<DeviceRow[]>([]);
  const [targetRoot, setTargetRoot] = useState<string | null>(null);
  const [settings, setSettings] = useState<Settings>(DEFAULT_SETTINGS);
  const [previewer, setPreviewer] = useState<PreviewerStatus | null>(null);
  const [settingsOpen, setSettingsOpen] = useState(false);
  const [busy, setBusy] = useState(false);
  const [dragging, setDragging] = useState(false);
  const [exportOpen, setExportOpen] = useState(false);
  const [notice, setNotice] = useState<string | null>(null);
  const [autoExport, setAutoExport] = useState<string | null>(null);
  const booksRef = useRef(books);
  booksRef.current = books;

  const addPaths = useCallback(async (paths: string[]) => {
    if (paths.length === 0) return;
    const rows = await api.inspectBooks(paths);
    setBooks((prev) => {
      const known = new Set(prev.map((b) => b.id));
      return [...prev, ...rows.filter((r) => !known.has(r.id)).map(toState)];
    });
  }, []);

  const refreshDevices = useCallback(async () => {
    const d = await api.listDevices();
    setDevices(d);
    setTargetRoot((cur) => (cur && d.some((x) => x.root === cur) ? cur : d[0]?.root ?? null));
  }, []);

  useEffect(() => {
    api.appVersion().then(setVersion).catch(() => setVersion("dev"));
    api.getSettings().then(setSettings).catch(() => {});
    api.previewerStatus().then(setPreviewer).catch(() => {});
    refreshDevices();
    api
      .takePreloadedBooks()
      .then(async (paths) => {
        await addPaths(paths);
        const dir = await api.takePreloadedTarget();
        if (dir) setAutoExport(dir);
      })
      .catch(() => {});
    const timer = setInterval(refreshDevices, 4000);
    const unlistenDrop = getCurrentWebview().onDragDropEvent((ev) => {
      if (ev.payload.type === "enter" || ev.payload.type === "over") setDragging(true);
      else if (ev.payload.type === "leave") setDragging(false);
      else if (ev.payload.type === "drop") {
        setDragging(false);
        addPaths(ev.payload.paths);
      }
    });
    const unlistenProgress = api.onProgress((e) => {
      setBooks((prev) =>
        prev.map((b) =>
          b.id !== e.id
            ? b
            : {
                ...b,
                status: e.status,
                stage: e.stage ?? (e.status === "running" ? b.stage : null),
                delivered: e.delivered ?? b.delivered,
                pages: e.pages ?? b.pages,
                runWarnings: e.status === "running" ? b.runWarnings : e.warnings,
                runError: e.error,
              },
        ),
      );
    });
    const unlistenFinished = api.onRunFinished(() => setBusy(false));
    return () => {
      clearInterval(timer);
      unlistenDrop.then((f) => f());
      unlistenProgress.then((f) => f());
      unlistenFinished.then((f) => f());
    };
  }, [addPaths, refreshDevices]);

  const saveSettings = async (s: Settings) => {
    setSettings(s);
    await api.setSettings(s).catch((e) => setNotice(String(e)));
    api.previewerStatus().then(setPreviewer).catch(() => {});
  };

  const pickBooks = async () => {
    const picked = await openDialog({ multiple: true, directory: false, filters: [{ name: "EPUB", extensions: ["epub"] }] });
    if (picked) addPaths(Array.isArray(picked) ? picked : [picked]);
  };

  const pickPreviewer = async () => {
    const picked = await openDialog({ multiple: false, directory: false });
    if (typeof picked === "string") saveSettings({ ...settings, previewer_path: picked });
  };

  const runnable = books.filter((b) => b.mode !== "error");

  const start = async (target: RunTarget) => {
    if (runnable.length === 0 || busy) return;
    if (!previewer?.found) {
      setNotice("Kindle Previewer is not installed. Open Settings to download it or point Deckle at it.");
      setSettingsOpen(true);
      return;
    }
    setNotice(null);
    setBusy(true);
    setBooks((prev) =>
      prev.map((b) => (b.mode === "error" ? b : { ...b, status: "queued", stage: null, delivered: null, pages: null, runWarnings: [], runError: null })),
    );
    try {
      await api.runBooks(
        runnable.map((b) => ({ id: b.id, path: b.path, cde: b.cdeOverride })),
        target,
      );
    } catch (e) {
      setBusy(false);
      setNotice(String(e));
    }
  };

  const sendToKindle = () => {
    if (!targetRoot) return;
    start({ kind: "device", root: targetRoot });
  };

  const exportToFolder = async () => {
    setExportOpen(false);
    const dir = await openDialog({ directory: true, multiple: false });
    if (typeof dir === "string") start({ kind: "folder", path: dir });
  };

  // `--export-to DIR` on the command line: run as soon as the books and Previewer status are in.
  useEffect(() => {
    if (autoExport && previewer && books.length > 0 && !busy) {
      const dir = autoExport;
      setAutoExport(null);
      start({ kind: "folder", path: dir });
    }
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [autoExport, previewer, books.length]);

  const override = (id: string, cde: "pdoc" | "ebok" | null) => setBooks((p) => p.map((b) => (b.id === id ? { ...b, cdeOverride: cde } : b)));
  const toggleReport = (id: string) => setBooks((p) => p.map((b) => (b.id === id ? { ...b, showReport: !b.showReport } : b)));
  const remove = (id: string) => setBooks((p) => p.filter((b) => b.id !== id));
  const clearDone = () => setBooks((p) => p.filter((b) => b.status !== "done"));

  const device = devices.find((d) => d.root === targetRoot) ?? null;
  const doneCount = books.filter((b) => b.status === "done").length;

  return (
    <div className={`app ${dragging ? "dragging" : ""}`}>
      <header className="topbar">
        <div className="brand">Deckle</div>
        {!previewer?.found && previewer && (
          <button className="pill warn" onClick={() => setSettingsOpen(true)}>
            Kindle Previewer not found
          </button>
        )}
        <div className="spacer" />
        <button className="ghost" onClick={pickBooks} disabled={busy}>
          Add books…
        </button>
        {doneCount > 0 && !busy && (
          <button className="ghost" onClick={clearDone}>
            Clear done
          </button>
        )}
        <button className="icon" aria-label="Settings" onClick={() => setSettingsOpen((o) => !o)}>
          ⚙
        </button>
      </header>

      <main className={`list-area ${books.length === 0 ? "empty" : ""}`}>
        {books.length === 0 ? (
          <div className="dropzone-inner">
            <div className="glyph" aria-hidden="true">
              ⬇
            </div>
            <h1>Drop EPUBs here</h1>
            <p>Each book becomes a Kindle file with Enhanced Typesetting, a cover that shows, and real page numbers.</p>
            <button className="ghost" onClick={pickBooks}>
              or choose files…
            </button>
          </div>
        ) : (
          <ul className="books" aria-label="Books">
            {books.map((b) => (
              <BookRowView
                key={b.id}
                book={b}
                busy={busy}
                defaultCde={settings.cde}
                onOverride={override}
                onToggleReport={toggleReport}
                onRemove={remove}
                onReveal={(p) => api.reveal(p)}
              />
            ))}
          </ul>
        )}
        {dragging && <div className="drop-overlay">Drop to add</div>}
      </main>

      {notice && (
        <div className="notice" role="status">
          {notice}
          <button className="link" onClick={() => setNotice(null)}>
            dismiss
          </button>
        </div>
      )}

      <footer className="bar">
        <div className="device">
          <span className={`dot ${device ? "on" : ""}`} aria-hidden="true" />
          {devices.length === 0 ? (
            "No Kindle connected"
          ) : devices.length === 1 ? (
            <span title={device?.root}>{device?.model_hint}</span>
          ) : (
            <select value={targetRoot ?? ""} onChange={(e) => setTargetRoot(e.target.value)} aria-label="Target Kindle">
              {devices.map((d) => (
                <option key={d.root} value={d.root}>
                  {d.model_hint}
                </option>
              ))}
            </select>
          )}
          {device && !device.thumbnails_writable && <span className="muted"> · covers via PDOC</span>}
        </div>
        <div className="actions">
          <button className="primary" onClick={sendToKindle} disabled={busy || !device || runnable.length === 0}>
            {busy ? "Working…" : `Send to Kindle${runnable.length > 1 ? ` (${runnable.length})` : ""}`}
          </button>
          <div className="split">
            <button className="secondary" aria-label="More actions" onClick={() => setExportOpen((o) => !o)} disabled={busy || runnable.length === 0}>
              ▾
            </button>
            {exportOpen && (
              <div className="menu up" onMouseLeave={() => setExportOpen(false)}>
                <button onClick={exportToFolder}>Export to folder…</button>
              </div>
            )}
          </div>
        </div>
        <div className="version">Deckle {version}</div>
      </footer>

      <SettingsDrawer
        open={settingsOpen}
        settings={settings}
        previewer={previewer}
        onChange={saveSettings}
        onClose={() => setSettingsOpen(false)}
        onPickPreviewer={pickPreviewer}
      />
    </div>
  );
}
