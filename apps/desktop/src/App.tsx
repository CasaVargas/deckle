// SPDX-License-Identifier: GPL-3.0-or-later
import { useCallback, useEffect, useRef, useState } from "react";
import { flushSync } from "react-dom";
import { platform } from "./platform";
import { api, coverRequest } from "./api";
import { deviceLabel } from "./deviceLabel";
import { size } from "./kindleRooms";
import BookRowView from "./BookRow";
import SettingsView, { type SettingsPage } from "./SettingsView";
import BookDrawer, { type DrawerTab } from "./BookDrawer";
import { presetSettings } from "./presets";
import BindPanel from "./BindPanel";
import Library from "./Library";
import { bindable } from "./bindList";
import ArticleField from "./ArticleField";
import { parseAddresses } from "./articles";
import { runSummary } from "./bookStatus";
import CookbookView from "./CookbookView";
import type { Incoming } from "./CookbookAdd";
import DailyView from "./DailyView";
import type { Room } from "./dailyRooms";
import Icon from "./Icon";
import KindleView from "./KindleView";
import { checkForUpdate, hasUpdate, type UpdateState } from "./updates";
import { refreshCapabilities, useCapabilities } from "./capabilities";
import { onSessionLost, restartEvents } from "./transport/web";
import Login from "./web/Login";
import Unavailable from "./web/Unavailable";
import { NO_DEVICE_REASON, PREVIEWER_REASON } from "./web/gates";
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
  illustrations_api_key: "",
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

/** The web app asks for its password first, and again when the session ends (a 401). */
export default function App() {
  // The build-time constant, not platform.kind, so the desktop bundle drops WebApp and Login.
  if (import.meta.env.VITE_DECKLE_TARGET !== "web") return <Deckle />;
  return <WebApp />;
}

function WebApp() {
  /** null: not asked yet. `in` stays true once in, so the app below keeps its state behind Login. */
  const [ok, setOk] = useState<boolean | null>(null);
  const [wasIn, setWasIn] = useState(false);
  useEffect(() => {
    fetch("/api/session", { credentials: "same-origin" })
      .then((r) => r.json())
      .then((s: { ok: boolean }) => {
        setOk(s.ok);
        if (s.ok) setWasIn(true);
      })
      .catch(() => setOk(false));
    return onSessionLost(() => setOk(false));
  }, []);
  const loggedIn = () => {
    setOk(true);
    setWasIn(true);
    restartEvents();
    refreshCapabilities();
  };
  if (ok === null) return null;
  return (
    <>
      {wasIn && <Deckle />}
      {!ok && <Login onIn={loggedIn} />}
    </>
  );
}

function Deckle() {
  const caps = useCapabilities();
  const [version, setVersion] = useState("");
  const [books, setBooks] = useState<BookState[]>([]);
  const [devices, setDevices] = useState<DeviceRow[]>([]);
  const [targetKey, setTargetKey] = useState<string | null>(null);
  const [plans, setPlans] = useState<Record<string, PlanRow>>({});
  const [settings, setSettings] = useState<Settings>(DEFAULT_SETTINGS);
  const [previewer, setPreviewer] = useState<PreviewerStatus | null>(null);
  const [settingsPage, setSettingsPage] = useState<SettingsPage | null>(null);
  /** The settings page last opened this session: the gear returns to it. */
  const lastPage = useRef<SettingsPage>("overview");
  const [update, setUpdate] = useState<UpdateState>({ kind: "checking" });
  const [busy, setBusy] = useState(false);
  const [dragging, setDragging] = useState(false);
  const [addOpen, setAddOpen] = useState(false);
  /** The library search in the top bar (Books tab redesign §4). */
  const [query, setQuery] = useState("");
  const searchInput = useRef<HTMLInputElement>(null);
  /** The Kindle tab's search, in the same place (Kindle tab redesign §3). */
  const [kindleQuery, setKindleQuery] = useState("");
  /** The picked Kindle's free space, once the Kindle tab has read it. */
  const [kindleFree, setKindleFree] = useState<number | null>(null);
  const [bindOpen, setBindOpen] = useState(false);
  const [binding, setBinding] = useState(false);
  const [notice, setNotice] = useState<string | null>(null);
  const [autoExport, setAutoExport] = useState<string | null>(null);
  const [tab, setTab] = useState<"books" | "daily" | "cookbook" | "kindle">("books");
  /** The Cookbook tab: a counter that sends focus to Add's link box, and what was pasted or dropped on the tab. */
  const [focusAdd, setFocusAdd] = useState(0);
  const focusTaken = useRef(0);
  const [incoming, setIncoming] = useState<Incoming | null>(null);
  /** The Daily room to land on when that tab is opened from the Cookbook's Delivery note (taken once). */
  const [dailyRoom, setDailyRoom] = useState<Room | null>(null);
  const [library, setLibrary] = useState<LibraryStatus | null>(null);
  /** The Books tab's library (Books and Kindle design §8): Deckle's, calibre's and your folders' books. */
  const [catalog, setCatalog] = useState<CatalogView | null>(null);
  /** The book whose drawer is open (Books tab redesign §5), by path: queued, in the library, or both. */
  const [drawer, setDrawer] = useState<{ path: string; tab: DrawerTab; cover?: string | null } | null>(null);
  const booksRef = useRef(books);
  booksRef.current = books;
  const tabRef = useRef(tab);
  tabRef.current = tab;
  const [articleOpen, setArticleOpen] = useState(false);
  const articleInput = useRef<HTMLInputElement>(null);
  /** Whether ⌘V should open the article field: Books tab showing, nothing running. */
  const pasteReady = useRef(false);
  /** Whether ⌘V should go to the Cookbook tab's Add: that tab showing, settings closed. */
  const cookbookPasteReady = useRef(false);
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
    // A failed look (the web app's session ended) waits for the next one.
    const timer = setInterval(() => refreshDevices().catch(() => {}), 4000);
    const unlistenDrop = platform.onFileDrop({
      over: setDragging,
      failed: setNotice,
      drop: (paths) => {
        // On the Cookbook tab, link files and saved pages are recipes for Add; nothing else belongs
        // there, and never the Books library (not even while Settings covers the tab).
        if (tabRef.current === "cookbook") {
          if (cookbookPasteReady.current) {
            const files = paths.filter((p) => ARTICLE_FILE.test(p));
            if (files.length > 0) setIncoming((c) => ({ seq: (c?.seq ?? 0) + 1, targets: files }));
            else setNotice("Drop .webloc, .url or .html files to add recipes.");
          }
          return;
        }
        addPaths(paths);
      },
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
      if (el?.closest("input, textarea, select, [contenteditable]")) return;
      const cookbook = cookbookPasteReady.current;
      if (!pasteReady.current && !cookbook) return;
      const urls = parseAddresses(ev.clipboardData?.getData("text") ?? "");
      if (urls.length === 0) return;
      ev.preventDefault();
      // On the Cookbook tab a pasted address is a recipe for Add; on Books it's an article.
      if (cookbook) setIncoming((c) => ({ seq: (c?.seq ?? 0) + 1, targets: urls }));
      else addArticles(urls);
    };
    document.addEventListener("paste", onPaste);
    // The Mac webview sends no paste event unless a text field has focus, so ⌘V (Ctrl+V) on the
    // Books tab first opens and focuses the article field; the paste then lands in it.
    const onKey = (ev: KeyboardEvent) => {
      const el = ev.target as HTMLElement | null;
      // ⌘F (Ctrl+F) finds in the library, when there's one to search.
      if (ev.key.toLowerCase() === "f" && (ev.metaKey || ev.ctrlKey) && !ev.shiftKey && !ev.altKey && tabRef.current === "books" && searchInput.current) {
        ev.preventDefault();
        searchInput.current.focus();
        searchInput.current.select();
        return;
      }
      if (ev.key.toLowerCase() !== "v" || !(ev.metaKey || ev.ctrlKey) || ev.shiftKey || ev.altKey) return;
      if (el?.closest("input, textarea, select, [contenteditable]")) return;
      if (cookbookPasteReady.current) {
        // Bring Add forward first (the link box is hidden on the other rooms), then focus it;
        // the paste then lands in it
        flushSync(() => setFocusAdd((n) => n + 1));
        document.getElementById("cookbook-link")?.focus();
        return;
      }
      if (!pasteReady.current) return;
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
    const picked = await platform.pickFiles({ multiple: true, filterName: "Books, documents and web articles", extensions: ["epub", "kfx", "cbz", "docx", "odt", "ott", "fodt", "rtf", "mobi", "azw", "azw3", "prc", "txt", "fb2", "zip", "md", "markdown", "pdf", "webloc", "url", "html", "htm"] });
    if (picked) addPaths(picked);
  };

  const pickNotes = async () => {
    const picked = await platform.pickFolder({ title: "Choose a folder of Markdown notes" });
    if (picked !== null) addPaths([picked]);
  };

  const setLibraryOptions = async (root: string | null, autoAdd: boolean) => {
    try {
      setLibrary(await api.librarySet(root, autoAdd));
    } catch (e) {
      setNotice(String(e));
    }
  };

  const pickLibrary = async () => {
    const dir = await platform.pickFolder({});
    if (dir !== null) setLibraryOptions(dir, library?.auto_add ?? true);
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
    const picked = (await platform.pickFiles({}))?.[0];
    if (picked !== undefined) saveSettings({ ...settings, previewer_path: picked });
  };

  const runnable = books.filter((b) => b.mode !== "error");
  // Library first, the queue docked beside it; with no library, the queue is the whole tab.
  const docked = !!catalog && catalog.entries.length > 0;
  const drawerBook = drawer ? books.find((b) => b.path === drawer.path) ?? null : null;
  const drawerEntry = drawer ? catalog?.entries.find((e) => e.path === drawer.path) ?? null : null;
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
      // No Previewer to download on the server: say why, nothing to open.
      if (caps.web) {
        setNotice(PREVIEWER_REASON);
        return;
      }
      setNotice("Kindle Previewer is not installed. Open Settings to download it or point Deckle at it.");
      openSettings("sending");
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
    const dir = await platform.pickFolder({});
    if (dir !== null) start({ kind: "folder", path: dir });
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
  cookbookPasteReady.current = tab === "cookbook" && settingsPage === null;
  settingsOpen.current = settingsPage !== null;

  const openSettings = (page: SettingsPage = lastPage.current) => setSettingsPage(page);
  const pickPage = (page: SettingsPage) => {
    lastPage.current = page;
    setSettingsPage(page);
  };
  const closeSettings = useCallback(() => setSettingsPage(null), []);

  /** The Kindle sends go to: the Books tray's foot and the Kindle tab's footer. */
  const deviceControl = (
    <div className="device">
      <span className={`dot ${device ? "on" : ""}`} aria-hidden="true" />
      {devices.length === 0 ? (
        caps.web ? NO_DEVICE_REASON : "No Kindle connected"
      ) : devices.length === 1 ? (
        <span title={device?.root ?? undefined}>{device && deviceLabel(device)}</span>
      ) : (
        <select value={targetKey ?? ""} onChange={(e) => setTargetKey(e.target.value)} aria-label="Target Kindle">
          {devices.map((d) => (
            <option key={d.key} value={d.key}>
              {deviceLabel(d)}
            </option>
          ))}
        </select>
      )}
    </div>
  );

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
            <button role="tab" aria-selected={tab === "cookbook"} className={tab === "cookbook" ? "on" : ""} onClick={() => setTab("cookbook")}>
              Cookbook
            </button>
            <button role="tab" aria-selected={tab === "kindle"} className={tab === "kindle" ? "on" : ""} onClick={() => setTab("kindle")}>
              Kindle
            </button>
          </div>
          {!previewer?.found && previewer && !caps.web && (
            <button className="pill warn" onClick={() => openSettings("sending")}>
              Kindle Previewer not found
            </button>
          )}
          {hasUpdate(update) && (
            <button className="pill" onClick={() => openSettings("about")}>
              {update.kind === "ready" ? "Restart to update" : "Update available"}
            </button>
          )}
          <div className="spacer" />
          {tab === "books" && docked && (
            <label className="top-search">
              <span className="visually-hidden">Search your library</span>
              <input ref={searchInput} type="search" placeholder="Search titles, authors, series" value={query} onChange={(e) => setQuery(e.target.value)} onKeyDown={(e) => e.key === "Escape" && setQuery("")} />
              <kbd aria-hidden="true">⌘F</kbd>
            </label>
          )}
          {tab === "kindle" && device && (
            <label className="top-search">
              <span className="visually-hidden">Search this Kindle</span>
              <input type="search" placeholder="Search this Kindle" value={kindleQuery} onChange={(e) => setKindleQuery(e.target.value)} onKeyDown={(e) => e.key === "Escape" && setKindleQuery("")} />
            </label>
          )}
          {tab === "kindle" && (
            <div className="device-pill">
              {deviceControl}
              {device && kindleFree != null && <span className="muted">{size(kindleFree)} free</span>}
            </div>
          )}
          {tab === "books" && (
            <div className="split">
              <button className="ghost with-icon" aria-haspopup="menu" aria-expanded={addOpen} onClick={() => setAddOpen((o) => !o)} disabled={busy}>
                <Icon name="plus" /> Add <Icon name="chevron-down" size={14} />
              </button>
              {addOpen && (
                <div className="menu" role="menu" onMouseLeave={() => setAddOpen(false)}>
                  <button role="menuitem" onClick={() => { setAddOpen(false); pickBooks(); }}>Books and documents…</button>
                  <button role="menuitem" onClick={() => { setAddOpen(false); setArticleOpen(true); }}>Web article…</button>
                  <button role="menuitem" onClick={() => { setAddOpen(false); pickNotes(); }}>Notes folder…</button>
                  <button role="menuitem" onClick={() => { setAddOpen(false); setTab("cookbook"); setFocusAdd((n) => n + 1); }}>Recipe…</button>
                </div>
              )}
            </div>
          )}
          <button className="icon" aria-label="Settings" onClick={() => openSettings()}>
            <Icon name="settings" size={18} />
          </button>
        </header>

        {tab === "daily" && <DailyView onNotice={setNotice} initialRoom={dailyRoom} onRoomTaken={() => setDailyRoom(null)} />}
        {tab === "cookbook" && (
          <CookbookView
            onNotice={setNotice}
            focusAdd={focusAdd}
            focusTaken={focusTaken}
            incoming={incoming}
            apiKey={settings.illustrations_api_key}
            onApiKey={async (illustrations_api_key) => {
              // Only this field is written (in Rust), so a stale copy here can't undo other settings
              await api.setIllustrationsKey(illustrations_api_key).catch((e) => setNotice(String(e)));
              setSettings((s) => ({ ...s, illustrations_api_key }));
            }}
            onDailyDelivery={() => {
              setDailyRoom("delivery");
              setTab("daily");
            }}
          />
        )}
        {tab === "kindle" && <KindleView target={deviceTarget} targetKey={targetKey} device={device ?? null} busy={busy} onResend={resendFromLibrary} onNotice={setNotice} onRenamed={refreshDevices} query={kindleQuery} onFree={setKindleFree} />}
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
        {catalog && <Library catalog={catalog} hasKindle={!!device} queued={new Set(books.map((b) => b.path))} query={query} busy={busy} onAdd={(paths) => addPaths(paths).catch((e) => setNotice(String(e)))} onEdit={(path, cover) => setDrawer({ path, tab: "details", cover })} />}
        <main className={`tray ${docked ? "queue-dock" : ""} ${books.length === 0 ? "empty" : ""}`} aria-label="To send">
          <div className="tray-scroll">
          {(docked || books.length > 0) && (
            <div className="tray-head">
              <h2>To send</h2>
              {books.length > 0 && <span className="muted">{books.length === 1 ? "1 book" : `${books.length} books`}</span>}
              <div className="spacer" />
              {doneCount > 0 && !busy && (
                <button className="quiet" onClick={clearDone}>
                  Clear done
                </button>
              )}
            </div>
          )}
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
              <p className="muted small">EPUBs, PDFs, Word, OpenDocument and RTF files, DRM-free MOBI and AZW3, text, FB2, Markdown notes, .kfx, web links and saved pages</p>
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
                  onToggleReport={toggleReport}
                  onRemove={remove}
                  onReveal={caps.reveal ? (p) => api.reveal(p) : undefined}
                  onOpen={(_, tab) => setDrawer({ path: b.path, tab })}
                />
              ))}
            </ul>
          )}
          </div>
          <div className="send-foot">
            {caps.web && previewer && !previewer.found && <Unavailable reason={PREVIEWER_REASON} />}
            {deviceControl}
            <button className="primary send" onClick={sendToKindle} disabled={busy || !device || runnable.length === 0}>
              {busy ? "Working…" : runnable.length === 0 ? "Send to Kindle" : `Send ${runnable.length === 1 ? "1 book" : `${runnable.length} books`} to Kindle`}
            </button>
            <div className="send-more">
              <button className="quiet" onClick={exportToFolder} disabled={busy || runnable.length === 0}>
                Export to folder…
              </button>
              <button className="quiet" disabled={busy || parts.length < 2} title={parts.length < 2 ? "Add at least two files to bind them." : undefined} onClick={() => setBindOpen(true)}>
                Bind into one book…
              </button>
            </div>
          </div>
          {dragging && <div className="drop-overlay">Drop to add</div>}
        </main>
        {drawer && (drawerBook || drawerEntry) && (
          <BookDrawer
            key={drawer.path}
            book={drawerBook}
            entry={drawerEntry}
            plan={drawerBook ? plans[drawerBook.id] : undefined}
            settings={settings}
            cover={drawerBook?.cover ?? drawer.cover ?? null}
            initialTab={drawer.tab}
            onCover={setCoverPick}
            onAsin={setAsin}
            onFixed={(book) => (book ? refreshRow(book) : loadCatalog())}
            onClose={() => setDrawer(null)}
          />
        )}
        </div>
        )}

        {!settingsPage && noticeBar}

      </div>
    </div>
  );
}
