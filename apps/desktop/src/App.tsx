// SPDX-License-Identifier: GPL-3.0-or-later
import { useCallback, useEffect, useRef, useState } from "react";
import { flushSync } from "react-dom";
import { getCurrentWebview } from "@tauri-apps/api/webview";
import { open as openDialog } from "@tauri-apps/plugin-dialog";
import { api, coverRequest } from "./api";
import BookRowView from "./BookRow";
import SettingsView, { type SettingsPage } from "./SettingsView";
import BeforeYouSend from "./BeforeYouSend";
import { presetSettings } from "./presets";
import BindPanel from "./BindPanel";
import Library from "./Library";
import { bindable } from "./bindList";
import ArticleField from "./ArticleField";
import { parseAddresses } from "./articles";
import { runSummary } from "./bookStatus";
import DailyView from "./DailyView";
import Icon from "./Icon";
import KindleView from "./KindleView";
import { checkForUpdate, hasUpdate, type UpdateState } from "./updates";
import type { BookRow, BookState, CatalogView, CoverChoice, DeviceRow, LibraryStatus, PlanRow, PreviewerStatus, RunTarget, Settings, UpgradeOf } from "./types";

const DEFAULT_SETTINGS: Settings = {
  book_presets: {},
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
function toState(row: BookRow): BookState {
  return { ...row, status: "idle", stage: null, delivered: null, pages: null, runWarnings: [], runError: null, shipped: null, xray: null, wordWise: null, cdeOverride: null, asin: null, showReport: false, replaced: [], exported: false, upgradeOf: null, coverPick: null };
}

export default function App() {
  const [version, setVersion] = useState("");
  const [books, setBooks] = useState<BookState[]>([]);
  const [devices, setDevices] = useState<DeviceRow[]>([]);
  const [targetKey, setTargetKey] = useState<string | null>(null);
  const [plans, setPlans] = useState<Record<string, PlanRow>>({});
  const [settings, setSettings] = useState<Settings>(DEFAULT_SETTINGS);
  const [previewer, setPreviewer] = useState<PreviewerStatus | null>(null);
  const [settingsPage, setSettingsPage] = useState<SettingsPage | null>(null);
  /** The settings page last opened this session: the gear returns to it. */
  const lastPage = useRef<SettingsPage>("general");
  const [update, setUpdate] = useState<UpdateState>({ kind: "checking" });
  const [busy, setBusy] = useState(false);
  const [dragging, setDragging] = useState(false);
  const [exportOpen, setExportOpen] = useState(false);
  const [bindOpen, setBindOpen] = useState(false);
  const [binding, setBinding] = useState(false);
  const [notice, setNotice] = useState<string | null>(null);
  const [autoExport, setAutoExport] = useState<string | null>(null);
  const [tab, setTab] = useState<"books" | "daily" | "kindle">("books");
  const [library, setLibrary] = useState<LibraryStatus | null>(null);
  /** The Books tab's library (Books and Kindle design §8): Deckle's, calibre's and your folders' books. */
  const [catalog, setCatalog] = useState<CatalogView | null>(null);
  /** The queued book Before you send is open for. */
  const [beforeFor, setBeforeFor] = useState<string | null>(null);
  const booksRef = useRef(books);
  booksRef.current = books;
  const tabRef = useRef(tab);
  tabRef.current = tab;
  const [articleOpen, setArticleOpen] = useState(false);
  const articleInput = useRef<HTMLInputElement>(null);
  /** Whether ⌘V should open the article field: Books tab showing, nothing running. */
  const pasteReady = useRef(false);
  const settingsOpen = useRef(false);

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

  const loadCatalog = useCallback(async () => {
    try {
      setCatalog(await api.libraryCatalog(targetKey));
    } catch {
      setCatalog(null);
    }
  }, [targetKey]);
  // Again after each send: what's on the Kindle changed.
  useEffect(() => {
    if (!busy) loadCatalog();
  }, [loadCatalog, busy]);
  // The watched folder, looked at every two minutes while Deckle is open.
  const watching = !!catalog?.watch;
  useEffect(() => {
    if (!watching) return;
    const look = async () => {
      try {
        const w = await api.libraryWatchScan();
        if (w.added.length) {
          setNotice(w.added.length === 1 ? `${w.added[0]} was added to your library from the watched folder.` : `${w.added.length} books were added to your library from the watched folder.`);
          loadCatalog();
        }
      } catch {
        // A folder that went away says so in Settings; nothing to do here.
      }
    };
    look();
    const t = window.setInterval(look, 120_000);
    return () => window.clearInterval(t);
  }, [watching, loadCatalog]);

  const setCoverPick = (id: string, cover: CoverChoice | null) => setBooks((prev) => prev.map((b) => (b.id === id ? { ...b, coverPick: cover } : b)));
  // Fixed details show in the queue at once: the row is read again from the library's copy.
  const refreshRow = async (book: BookState) => {
    const [row] = await api.inspectBooks([book.path]);
    if (row) setBooks((prev) => prev.map((b) => (b.id === book.id ? { ...b, title: row.title, authors: row.authors, series: row.series, cover: row.cover ?? b.cover } : b)));
    await loadCatalog();
  };

  const device = devices.find((d) => d.key === targetKey) ?? null;
  const deviceTarget: RunTarget | null = device
    ? device.kind === "mtp"
      ? { kind: "mtp", bus: device.bus, address: device.address }
      : { kind: "device", root: device.root ?? "" }
    : null;

  // Ask the cover rule what each book gets on the current target (the chips).
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
    const unlistenFinished = api.onRunFinished(() => {
      setBusy(false);
      // Settings hide the rows and the footer: say how the run went.
      if (settingsOpen.current) {
        setBooks((prev) => {
          const summary = runSummary(prev);
          if (summary) setNotice(summary);
          return prev;
        });
      }
    });
    // A pasted web address on the Books tab (not into a text field) becomes an article.
    const onPaste = (ev: ClipboardEvent) => {
      const el = ev.target as HTMLElement | null;
      if (!pasteReady.current || el?.closest("input, textarea, select, [contenteditable]")) return;
      const urls = parseAddresses(ev.clipboardData?.getData("text") ?? "");
      if (urls.length === 0) return;
      ev.preventDefault();
      addArticles(urls);
    };
    document.addEventListener("paste", onPaste);
    // The Mac webview sends no paste event unless a text field has focus, so ⌘V (Ctrl+V) on the
    // Books tab first opens and focuses the article field; the paste then lands in it.
    const onKey = (ev: KeyboardEvent) => {
      const el = ev.target as HTMLElement | null;
      if (ev.key.toLowerCase() !== "v" || !(ev.metaKey || ev.ctrlKey) || ev.shiftKey || ev.altKey) return;
      if (!pasteReady.current || el?.closest("input, textarea, select, [contenteditable]")) return;
      flushSync(() => setArticleOpen(true));
      articleInput.current?.focus();
    };
    document.addEventListener("keydown", onKey);
    return () => {
      document.removeEventListener("paste", onPaste);
      document.removeEventListener("keydown", onKey);
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
    const picked = await openDialog({ multiple: true, directory: false, filters: [{ name: "Books, documents and web articles", extensions: ["epub", "kfx", "docx", "txt", "md", "markdown", "pdf", "webloc", "url", "html", "htm"] }] });
    if (picked) addPaths(Array.isArray(picked) ? picked : [picked]);
  };

  const pickNotes = async () => {
    const picked = await openDialog({ directory: true, multiple: false, title: "Choose a folder of Markdown notes" });
    if (typeof picked === "string") addPaths([picked]);
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
  const resendFromLibrary = async (items: { source: string; cde: "pdoc" | "ebok" | null; upgradeOf?: UpgradeOf }[], what: string) => {
    const rows = await api.inspectBooks(items.map((i) => i.source));
    const cdeFor = new Map(items.map((i) => [i.source, i.cde]));
    const upgradeFor = new Map(items.map((i) => [i.source, i.upgradeOf ?? null]));
    if (rows.length === 0) {
      setNotice("The app can't send these; send them with `deckle run`.");
      return;
    }
    setBooks((prev) => {
      const ids = new Set(rows.map((r) => r.id));
      return [...prev.filter((b) => !ids.has(b.id)), ...rows.map((r) => ({ ...toState(r), cdeOverride: cdeFor.get(r.path) ?? null, upgradeOf: upgradeFor.get(r.path) ?? null }))];
    });
    setTab("books");
    const skipped = items.length - rows.length;
    setNotice(`${what} Check each book, then Send to Kindle.${skipped ? ` ${skipped} Word document${skipped > 1 ? "s" : ""} left out: send with \`deckle library reprocess\`.` : ""}`);
  };

  const pickPreviewer = async () => {
    const picked = await openDialog({ multiple: false, directory: false });
    if (typeof picked === "string") saveSettings({ ...settings, previewer_path: picked });
  };

  const runnable = books.filter((b) => b.mode !== "error");
  // Library first, the queue docked beside it; with no library, the queue is the whole tab.
  const docked = !!catalog && catalog.entries.length > 0;
  const opened = books.find((b) => b.id === beforeFor) ?? null;
  const parts = books.filter((b) => bindable(b.path, b.mode));

  // The bound book takes the parts' place in the list (#22).
  const bindBooks = async (chosen: BookState[], title: string, author: string) => {
    setBinding(true);
    setNotice(null);
    try {
      const row = await api.bindBooks(
        chosen.map((b) => b.path),
        title,
        author.trim() || null,
      );
      const used = new Set(chosen.map((b) => b.id));
      setBooks((prev) => [...prev.filter((b) => !used.has(b.id) && b.id !== row.id), toState(row)]);
      setBindOpen(false);
      setNotice(`Bound ${chosen.length} files into “${row.title}”. Check it, then Send to Kindle.`);
    } catch (e) {
      setNotice(String(e));
    } finally {
      setBinding(false);
    }
  };

  const start = async (target: RunTarget) => {
    if (runnable.length === 0 || busy) return;
    // A finished KFX is copied as is; only EPUBs need Kindle Previewer.
    if (!previewer?.found && runnable.some((b) => b.mode !== "ready")) {
      setNotice("Kindle Previewer is not installed. Open Settings to download it or point Deckle at it.");
      openSettings("general");
      return;
    }
    setNotice(null);
    setBusy(true);
    setBooks((prev) =>
      prev.map((b) => (b.mode === "error" ? b : { ...b, status: "queued", exported: target.kind === "folder", stage: null, delivered: null, pages: null, runWarnings: [], runError: null, shipped: null, xray: null, wordWise: null })),
    );
    try {
      await api.runBooks(
        runnable.map((b) => ({ id: b.id, path: b.path, cde: b.cdeOverride, asin: b.asin, upgrade_of: b.upgradeOf, cover: coverRequest(b.coverPick) })),
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
  pasteReady.current = tab === "books" && settingsPage === null && !busy;
  settingsOpen.current = settingsPage !== null;

  const openSettings = (page: SettingsPage = lastPage.current) => setSettingsPage(page);
  const pickPage = (page: SettingsPage) => {
    lastPage.current = page;
    setSettingsPage(page);
  };
  const closeSettings = useCallback(() => setSettingsPage(null), []);

  const noticeBar = notice && (
    <div className="notice" role="status">
      {notice}
      <button className="link" onClick={() => setNotice(null)}>
        dismiss
      </button>
    </div>
  );

  // Settings cover the tabs without unmounting them, so a running Daily send or an unsaved
  // edit is still there on Back.
  return (
    <div className={`app ${dragging ? "dragging" : ""}`}>
      {settingsPage && (
        <>
          {noticeBar}
          <SettingsView
            page={settingsPage}
            onPage={pickPage}
            onBack={closeSettings}
            version={version}
            settings={settings}
            previewer={previewer}
            devices={devices}
            onChange={saveSettings}
            onDeviceOptions={async (key, cde, verified) => {
              await api.setDeviceOptions(key, cde, verified).catch((e) => setNotice(String(e)));
              refreshDevices();
            }}
            update={update}
            onUpdateState={setUpdate}
            busy={busy}
            onPickPreviewer={pickPreviewer}
            library={library}
            onLibrary={setLibraryOptions}
            onPickLibrary={pickLibrary}
          />
        </>
      )}
      <div className="app-main" hidden={settingsPage !== null}>
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
            <button className="pill warn" onClick={() => openSettings("general")}>
              Kindle Previewer not found
            </button>
          )}
          <div className="spacer" />
          {tab === "books" && (
            <button className="ghost with-icon" onClick={pickBooks} disabled={busy}>
              <Icon name="plus" /> Add books
            </button>
          )}
          {tab === "books" && (
            <button className="ghost" onClick={pickNotes} disabled={busy}>Add notes folder</button>
          )}
          {tab === "books" && (
            <button className="ghost with-icon" onClick={() => setArticleOpen(true)} disabled={busy}>
              <Icon name="link" /> Add article
            </button>
          )}
          {tab === "books" && doneCount > 0 && !busy && (
            <button className="ghost" onClick={clearDone}>
              Clear done
            </button>
          )}
          <button className="icon" aria-label="Settings" onClick={() => openSettings()}>
            <Icon name="settings" size={18} />
          </button>
        </header>

        {tab === "daily" && <DailyView onNotice={setNotice} />}
        {tab === "kindle" && <KindleView target={deviceTarget} targetKey={targetKey} busy={busy} onResend={resendFromLibrary} onNotice={setNotice} />}
        {tab === "books" && articleOpen && (
          <ArticleField
            ref={articleInput}
            onAdd={(urls) => {
              setArticleOpen(false);
              addArticles(urls);
            }}
            onCancel={() => setArticleOpen(false)}
          />
        )}
        {tab === "books" && bindOpen && (
          <BindPanel books={parts} busy={binding || busy} onBind={bindBooks} onCancel={() => setBindOpen(false)} />
        )}
        {tab === "books" && (
        <div className={docked ? "books-split" : "books-whole"}>
        {catalog && <Library catalog={catalog} hasKindle={!!device} queued={new Set(books.map((b) => b.path))} busy={busy} onAdd={(paths) => addPaths(paths).catch((e) => setNotice(String(e)))} />}
        <main className={`list-area ${docked ? "queue-dock" : ""} ${books.length === 0 ? "empty" : ""}`} aria-label="To send">
          {docked && <h2 className="dock-head">To send{books.length ? ` · ${books.length}` : ""}</h2>}
          {books.length === 0 && docked ? (
            <div className="dock-empty muted">
              <p>Nothing to send yet.</p>
              <p className="small">Pick books from your library, or drop EPUBs, PDFs and Word documents here.</p>
            </div>
          ) : books.length === 0 ? (
            <div className="dropzone-inner">
              <div className="glyph" aria-hidden="true">
                <Icon name="download" size={40} />
              </div>
              <h1>Drop books or notes here</h1>
              <p>They become Kindle books with covers, page numbers, X-Ray and Word Wise.</p>
              <p className="muted small">EPUBs, PDFs, Word documents, text, Markdown notes, .kfx, web links and saved pages</p>
              <div className="empty-actions">
                <button className="ghost" onClick={pickBooks}>
                  Choose files…
                </button>
                <button className="ghost" onClick={() => setArticleOpen(true)}>
                  Add article
                </button>
              </div>
            </div>
          ) : (
            <ul className="books" aria-label="Books">
              {books.map((b) => (
                <BookRowView
                  key={b.id}
                  book={b}
                  busy={busy}
                  plan={plans[b.id]}
                  settings={presetSettings(settings, b.path)}
                  preset={settings.book_presets[b.path] ?? null}
                  onPreset={(preset) => {
                    const book_presets = { ...settings.book_presets };
                    if (preset) book_presets[b.path] = preset;
                    else delete book_presets[b.path];
                    const next = { ...settings, book_presets };
                    setBusy(true);
                    api.setSettings(next).then(() => setSettings(next))
                      .catch((e) => setNotice(String(e))).finally(() => setBusy(false));
                  }}
                  onOverride={override}
                  onAsin={setAsin}
                  onToggleReport={toggleReport}
                  onRemove={remove}
                  onReveal={(p) => api.reveal(p)}
                  onOpen={(id) => setBeforeFor(id)}
                />
              ))}
            </ul>
          )}
          {dragging && <div className="drop-overlay">Drop to add</div>}
        </main>
        {opened && (
          <BeforeYouSend
            key={opened.id}
            book={opened}
            plan={plans[opened.id]}
            settings={settings}
            entry={catalog?.entries.find((e) => e.path === opened.path) ?? null}
            onCover={setCoverPick}
            onFixed={refreshRow}
            onClose={() => setBeforeFor(null)}
          />
        )}
        </div>
        )}

        {!settingsPage && noticeBar}

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
          </div>
          {tab === "books" && (
          <div className="actions">
            <button className="primary joined" onClick={sendToKindle} disabled={busy || !device || runnable.length === 0}>
              {busy ? "Working…" : `Send to Kindle${runnable.length > 1 ? ` (${runnable.length})` : ""}`}
            </button>
            <div className="split">
              <button className="secondary" aria-label="More actions" onClick={() => setExportOpen((o) => !o)} disabled={busy || runnable.length === 0}>
                <Icon name="chevron-down" />
              </button>
              {exportOpen && (
                <div className="menu up" onMouseLeave={() => setExportOpen(false)}>
                  <button onClick={exportToFolder}>Export to folder…</button>
                  <button
                    disabled={parts.length < 2}
                    title={parts.length < 2 ? "Add at least two files to bind them." : undefined}
                    onClick={() => {
                      setExportOpen(false);
                      setBindOpen(true);
                    }}
                  >
                    Bind into one book…
                  </button>
                </div>
              )}
            </div>
          </div>
          )}
          {hasUpdate(update) ? (
            <button
              className="version link"
              onClick={() => openSettings("about")}
            >
              Deckle {version} · {update.kind === "ready" ? "Restart to update" : "Update available"}
            </button>
          ) : (
            <div className="version">Deckle {version}</div>
          )}
        </footer>
        )}
      </div>
    </div>
  );
}
