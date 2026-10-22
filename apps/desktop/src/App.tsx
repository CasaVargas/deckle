// SPDX-License-Identifier: GPL-3.0-or-later
import { useCallback, useEffect, useRef, useState } from "react";
import { getCurrentWebview } from "@tauri-apps/api/webview";
import { open as openDialog } from "@tauri-apps/plugin-dialog";
import { api } from "./api";
import BookRowView from "./BookRow";
import SettingsDrawer from "./SettingsDrawer";
import DailyView from "./DailyView";
import KindleView from "./KindleView";
import { checkForUpdate, hasUpdate, type UpdateState } from "./updates";
import type { BookRow, BookState, DeviceRow, LibraryStatus, PlanRow, PreviewerStatus, RunTarget, Settings } from "./types";

const DEFAULT_SETTINGS: Settings = {
  previewer_path: null,
  cde: "auto",
  pages_auto: true,
  llm_provider: "off",
  llm_model: "",
  llm_endpoint: "",
  llm_api_key: "",
  xray: true,
  word_wise: true,
  word_wise_level: 5,
  wikipedia: false,
  amazon_match: false,
  generated_cover: true,
  metadata_fill: false,
  typography: false,
  popup_notes: false,
};

/** Link files and saved pages become articles (web-article design); everything else is a book. */
const ARTICLE_FILE = /\.(webloc|url|html?)$/i;
const WEB_ADDRESS = /https?:\/\/[^\s<>"]+/g;

function toState(row: BookRow): BookState {
  return { ...row, status: "idle", stage: null, delivered: null, pages: null, runWarnings: [], runError: null, shipped: null, xray: null, wordWise: null, cdeOverride: null, asin: null, showReport: false, replaced: [] };
}

export default function App() {
  const [version, setVersion] = useState("");
  const [books, setBooks] = useState<BookState[]>([]);
  const [devices, setDevices] = useState<DeviceRow[]>([]);
  const [targetKey, setTargetKey] = useState<string | null>(null);
  const [plans, setPlans] = useState<Record<string, PlanRow>>({});
  const [settings, setSettings] = useState<Settings>(DEFAULT_SETTINGS);
  const [previewer, setPreviewer] = useState<PreviewerStatus | null>(null);
  const [settingsOpen, setSettingsOpen] = useState(false);
  const [aboutFocus, setAboutFocus] = useState(false);
  const [update, setUpdate] = useState<UpdateState>({ kind: "checking" });
  const [busy, setBusy] = useState(false);
  const [dragging, setDragging] = useState(false);
  const [exportOpen, setExportOpen] = useState(false);
  const [notice, setNotice] = useState<string | null>(null);
  const [autoExport, setAutoExport] = useState<string | null>(null);
  const [tab, setTab] = useState<"books" | "daily" | "kindle">("books");
  const [library, setLibrary] = useState<LibraryStatus | null>(null);
  const booksRef = useRef(books);
  booksRef.current = books;
  const tabRef = useRef(tab);
  tabRef.current = tab;

  const addRows = useCallback((rows: BookRow[]) => {
    setBooks((prev) => {
      const known = new Set(prev.map((b) => b.id));
      return [...prev, ...rows.filter((r) => !known.has(r.id)).map(toState)];
    });
  }, []);

  // Fetching and extracting takes a few seconds per article; say so while it runs.
  const addArticles = useCallback(
    async (targets: string[]) => {
      if (targets.length === 0) return;
      setNotice(targets.length === 1 ? "Fetching the article…" : `Fetching ${targets.length} articles…`);
      try {
        addRows(await api.importArticles(targets));
        setNotice(null);
      } catch (e) {
        setNotice(String(e));
      }
    },
    [addRows],
  );

  const addPaths = useCallback(
    async (paths: string[]) => {
      const articles = paths.filter((p) => ARTICLE_FILE.test(p));
      const books = paths.filter((p) => !ARTICLE_FILE.test(p));
      if (books.length > 0) addRows(await api.inspectBooks(books));
      await addArticles(articles);
    },
    [addRows, addArticles],
  );

  const refreshDevices = useCallback(async () => {
    const d = await api.listDevices();
    setDevices(d);
    setTargetKey((cur) => (cur && d.some((x) => x.key === cur) ? cur : d[0]?.key ?? null));
  }, []);

  const device = devices.find((d) => d.key === targetKey) ?? null;
  const deviceTarget: RunTarget | null = device
    ? device.kind === "mtp"
      ? { kind: "mtp", bus: device.bus, address: device.address }
      : { kind: "device", root: device.root ?? "" }
    : null;

  // Ask the cover rule what each book gets on the current target (badges).
  useEffect(() => {
    const runnable = books.filter((b) => b.mode !== "error" && b.mode !== "ready");
    if (runnable.length === 0) {
      setPlans({});
      return;
    }
    const target: RunTarget = deviceTarget ?? { kind: "folder", path: "" };
    api
      .planBooks(
        runnable.map((b) => ({ id: b.id, path: b.path, cde: b.cdeOverride, asin: b.asin })),
        target,
      )
      .then((rows) => setPlans(Object.fromEntries(rows.map((r) => [r.id, r]))))
      .catch(() => {});
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [books.map((b) => `${b.id}:${b.cdeOverride ?? ""}:${b.asin ?? ""}`).join("|"), targetKey, devices.length, settings.cde]);

  useEffect(() => {
    api.appVersion().then(setVersion).catch(() => setVersion("dev"));
    checkForUpdate().then(setUpdate);
    api.startTab().then(setTab).catch(() => {});
    api.getSettings().then(setSettings).catch(() => {});
    api.previewerStatus().then(setPreviewer).catch(() => {});
    api.libraryStatus().then(setLibrary).catch(() => {});
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
                shipped:
                  e.status === "done" && e.cde && e.asin
                    ? { cde: e.cde, asin: e.asin, reason: e.reason ?? "", matched: e.matched ?? null }
                    : b.shipped,
                xray: e.status === "done" ? (e.xray ?? null) : b.xray,
                wordWise: e.status === "done" ? (e.word_wise ?? null) : b.wordWise,
                replaced: e.status === "done" ? (e.replaced ?? []) : b.replaced,
              },
        ),
      );
    });
    const unlistenFinished = api.onRunFinished(() => setBusy(false));
    // A pasted web address on the Books tab (not into a text field) becomes an article.
    const onPaste = (ev: ClipboardEvent) => {
      const el = ev.target as HTMLElement | null;
      if (tabRef.current !== "books" || el?.closest("input, textarea, select, [contenteditable]")) return;
      const urls = ev.clipboardData?.getData("text").match(WEB_ADDRESS) ?? [];
      if (urls.length === 0) return;
      ev.preventDefault();
      addArticles([...new Set(urls)]);
    };
    document.addEventListener("paste", onPaste);
    return () => {
      document.removeEventListener("paste", onPaste);
      clearInterval(timer);
      unlistenDrop.then((f) => f());
      unlistenProgress.then((f) => f());
      unlistenFinished.then((f) => f());
    };
  }, [addPaths, addArticles, refreshDevices]);

  const saveSettings = async (s: Settings) => {
    setSettings(s);
    await api.setSettings(s).catch((e) => setNotice(String(e)));
    api.previewerStatus().then(setPreviewer).catch(() => {});
  };

  const pickBooks = async () => {
    const picked = await openDialog({ multiple: true, directory: false, filters: [{ name: "Books and web articles", extensions: ["epub", "kfx", "webloc", "url", "html", "htm"] }] });
    if (picked) addPaths(Array.isArray(picked) ? picked : [picked]);
  };

  const setLibraryOptions = async (root: string | null, autoAdd: boolean) => {
    try {
      setLibrary(await api.librarySet(root, autoAdd));
    } catch (e) {
      setNotice(String(e));
    }
  };

  const pickLibrary = async () => {
    const dir = await openDialog({ directory: true, multiple: false });
    if (typeof dir === "string") setLibraryOptions(dir, library?.auto_add ?? true);
  };

  // Re-send, Upgrade and Update from the Kindle tab: the library copies go on the Books tab
  // for review, each with its chosen mode.
  const resendFromLibrary = async (items: { source: string; cde: "pdoc" | "ebok" | null }[], what: string) => {
    const rows = await api.inspectBooks(items.map((i) => i.source));
    const cdeFor = new Map(items.map((i) => [i.source, i.cde]));
    if (rows.length === 0) {
      setNotice("The app sends EPUBs; send Word documents with `deckle run`.");
      return;
    }
    setBooks((prev) => {
      const ids = new Set(rows.map((r) => r.id));
      return [...prev.filter((b) => !ids.has(b.id)), ...rows.map((r) => ({ ...toState(r), cdeOverride: cdeFor.get(r.path) ?? null }))];
    });
    setTab("books");
    const skipped = items.length - rows.length;
    setNotice(`${what} Check the badges, then Send to Kindle.${skipped ? ` ${skipped} Word document${skipped > 1 ? "s" : ""} left out: send with \`deckle library reprocess\`.` : ""}`);
  };

  const pickPreviewer = async () => {
    const picked = await openDialog({ multiple: false, directory: false });
    if (typeof picked === "string") saveSettings({ ...settings, previewer_path: picked });
  };

  const runnable = books.filter((b) => b.mode !== "error");

  const start = async (target: RunTarget) => {
    if (runnable.length === 0 || busy) return;
    // A finished KFX is copied as is; only EPUBs need Kindle Previewer.
    if (!previewer?.found && runnable.some((b) => b.mode !== "ready")) {
      setNotice("Kindle Previewer is not installed. Open Settings to download it or point Deckle at it.");
      setSettingsOpen(true);
      return;
    }
    setNotice(null);
    setBusy(true);
    setBooks((prev) =>
      prev.map((b) => (b.mode === "error" ? b : { ...b, status: "queued", stage: null, delivered: null, pages: null, runWarnings: [], runError: null, shipped: null, xray: null, wordWise: null })),
    );
    try {
      await api.runBooks(
        runnable.map((b) => ({ id: b.id, path: b.path, cde: b.cdeOverride, asin: b.asin })),
        target,
      );
    } catch (e) {
      setBusy(false);
      setNotice(String(e));
    }
  };

  const sendToKindle = () => {
    if (!deviceTarget) return;
    start(deviceTarget);
  };

  const exportToFolder = async () => {
    setExportOpen(false);
    const dir = await openDialog({ directory: true, multiple: false });
    if (typeof dir === "string") start({ kind: "folder", path: dir });
  };

  // `--export-to DIR` on the command line: run as soon as the books and Previewer status are in.
  useEffect(() => {
    if (!autoExport || !previewer || books.length === 0 || busy) return;
    if (autoExport === "device:") {
      if (!deviceTarget) return; // wait for the device list
      setAutoExport(null);
      start(deviceTarget);
      return;
    }
    const dir = autoExport;
    setAutoExport(null);
    start({ kind: "folder", path: dir });
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [autoExport, previewer, books.length, deviceTarget?.kind, targetKey]);

  const override = (id: string, cde: "pdoc" | "ebok" | null) => setBooks((p) => p.map((b) => (b.id === id ? { ...b, cdeOverride: cde } : b)));
  const setAsin = (id: string, asin: string | null) => setBooks((p) => p.map((b) => (b.id === id ? { ...b, asin } : b)));
  const toggleReport = (id: string) => setBooks((p) => p.map((b) => (b.id === id ? { ...b, showReport: !b.showReport } : b)));
  const remove = (id: string) => setBooks((p) => p.filter((b) => b.id !== id));
  const clearDone = () => setBooks((p) => p.filter((b) => b.status !== "done"));

  const doneCount = books.filter((b) => b.status === "done").length;

  return (
    <div className={`app ${dragging ? "dragging" : ""}`}>
      <header className="topbar">
        <div className="brand">Deckle</div>
        <div className="tabs" role="tablist">
          <button role="tab" aria-selected={tab === "books"} className={tab === "books" ? "on" : ""} onClick={() => setTab("books")}>
            Books
          </button>
          <button role="tab" aria-selected={tab === "daily"} className={tab === "daily" ? "on" : ""} onClick={() => setTab("daily")}>
            Daily
          </button>
          <button role="tab" aria-selected={tab === "kindle"} className={tab === "kindle" ? "on" : ""} onClick={() => setTab("kindle")}>
            Kindle
          </button>
        </div>
        {!previewer?.found && previewer && (
          <button className="pill warn" onClick={() => setSettingsOpen(true)}>
            Kindle Previewer not found
          </button>
        )}
        <div className="spacer" />
        {tab === "books" && (
          <button className="ghost" onClick={pickBooks} disabled={busy}>
            Add books…
          </button>
        )}
        {tab === "books" && doneCount > 0 && !busy && (
          <button className="ghost" onClick={clearDone}>
            Clear done
          </button>
        )}
        <button className="icon" aria-label="Settings" onClick={() => setSettingsOpen((o) => !o)}>
          ⚙
        </button>
      </header>

      {tab === "daily" && <DailyView onNotice={setNotice} />}
      {tab === "kindle" && <KindleView target={deviceTarget} targetKey={targetKey} busy={busy} onResend={resendFromLibrary} onNotice={setNotice} />}
      {tab === "books" && (
      <main className={`list-area ${books.length === 0 ? "empty" : ""}`}>
        {books.length === 0 ? (
          <div className="dropzone-inner">
            <div className="glyph" aria-hidden="true">
              ⬇
            </div>
            <h1>Drop EPUBs here</h1>
            <p>Each book becomes a Kindle book with Enhanced Typesetting, a cover that shows, page numbers, and X-Ray and Word Wise where your Kindle allows them.</p>
            <p className="muted">Finished Kindle books (.kfx) are sent as they are.</p>
            <p className="muted">Web articles too: paste an address, or drop a link (.webloc, .url) or a saved page (.html).</p>
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
                plan={plans[b.id]}
                onOverride={override}
                onAsin={setAsin}
                onToggleReport={toggleReport}
                onRemove={remove}
                onReveal={(p) => api.reveal(p)}
              />
            ))}
          </ul>
        )}
        {dragging && <div className="drop-overlay">Drop to add</div>}
      </main>
      )}

      {notice && (
        <div className="notice" role="status">
          {notice}
          <button className="link" onClick={() => setNotice(null)}>
            dismiss
          </button>
        </div>
      )}

      {(tab === "books" || tab === "kindle") && (
      <footer className="bar">
        <div className="device">
          <span className={`dot ${device ? "on" : ""}`} aria-hidden="true" />
          {devices.length === 0 ? (
            "No Kindle connected"
          ) : devices.length === 1 ? (
            <span title={device?.root ?? undefined}>{device?.model_hint}</span>
          ) : (
            <select value={targetKey ?? ""} onChange={(e) => setTargetKey(e.target.value)} aria-label="Target Kindle">
              {devices.map((d) => (
                <option key={d.key} value={d.key}>
                  {d.model_hint}
                </option>
              ))}
            </select>
          )}
          {device && (
            <span className="muted">
              {" · "}
              {device.kind === "mtp" ? "MTP" : "USB"}
              {device.kind === "mtp" || !device.thumbnails_writable ? " · covers via PDOC" : " · thumbnails"}
            </span>
          )}
        </div>
        {tab === "books" && (
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
        )}
        {hasUpdate(update) ? (
          <button
            className="version link"
            onClick={() => {
              setAboutFocus(true);
              setSettingsOpen(true);
            }}
          >
            Deckle {version} · {update.kind === "ready" ? "Restart to update" : "Update available"}
          </button>
        ) : (
          <div className="version">Deckle {version}</div>
        )}
      </footer>
      )}

      <SettingsDrawer
        open={settingsOpen}
        version={version}
        settings={settings}
        previewer={previewer}
        devices={devices}
        onChange={saveSettings}
        onDeviceOptions={async (key, cde, verified) => {
          await api.setDeviceOptions(key, cde, verified).catch((e) => setNotice(String(e)));
          refreshDevices();
        }}
        onClose={() => {
          setSettingsOpen(false);
          setAboutFocus(false);
        }}
        update={update}
        onUpdateState={setUpdate}
        busy={busy}
        focusAbout={aboutFocus}
        onPickPreviewer={pickPreviewer}
        library={library}
        onLibrary={setLibraryOptions}
        onPickLibrary={pickLibrary}
      />
    </div>
  );
}
