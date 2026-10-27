// SPDX-License-Identifier: GPL-3.0-or-later
// The Cookbook tab (cookbook design §8.2), built on the Daily tab's frame: the book menu, then
// Add, Recipes and Delivery, and the Book settings sheet over them.
import { useCallback, useEffect, useRef, useState } from "react";
import type { MutableRefObject } from "react";
import { api } from "./api";
import BookMenu from "./BookMenu";
import BookSheet from "./BookSheet";
import type { IllustrateRun } from "./BookSheet";
import CookbookAdd from "./CookbookAdd";
import type { Incoming } from "./CookbookAdd";
import CookbookDeliveryRoom from "./CookbookDelivery";
import CookbookRecipes from "./CookbookRecipes";
import { COOKBOOK_ROOMS, failedDelivery, illustrateSummary, recipeCount } from "./cookbookRooms";
import type { CookbookRoom } from "./cookbookRooms";
import type { CookbookBook, CookbookDelivery } from "./types";

export default function CookbookView({
  onNotice,
  focusAdd,
  focusTaken,
  incoming,
  apiKey,
  onApiKey,
  onDailyDelivery,
}: {
  onNotice: (m: string) => void;
  /** Bumped by the app to land on Add with the link box focused (Books → Add → Recipe…). */
  focusAdd: number;
  /** The last `focusAdd` acted on: it lives in the app, so coming back to the tab doesn't focus again. */
  focusTaken: MutableRefObject<number>;
  /** An address or file pasted or dropped on the tab: it goes to Add. */
  incoming: Incoming | null;
  /** The desktop's `illustrations_api_key` setting (used only for illustrations), and how to change it. */
  apiKey: string;
  onApiKey: (key: string) => Promise<void>;
  /** Go to the Daily tab's Delivery room. */
  onDailyDelivery: () => void;
}) {
  const [room, setRoom] = useState<CookbookRoom>("add");
  const [book, setBook] = useState<CookbookBook | null>(null);
  const [delivery, setDelivery] = useState<CookbookDelivery | null>(null);
  /** The recipe whose row Recipes opens first (set by Add's "See it in Recipes"). */
  const [openRecipe, setOpenRecipe] = useState<string | null>(null);
  const recipeOpened = useCallback(() => setOpenRecipe(null), []);
  const [sheetOpen, setSheetOpen] = useState(false);
  // Illustrate all lives here, so closing the sheet doesn't lose the run
  const [run, setRun] = useState<IllustrateRun | null>(null);
  const [runResult, setRunResult] = useState<string | null>(null);
  // What was pasted or dropped before this tab was shown again is not new
  const taken = useRef(incoming?.seq ?? 0);

  const load = useCallback(
    () =>
      Promise.all([
        api.cookbookBook().then(setBook).catch((e) => onNotice(String(e))),
        api.cookbookDelivery().then(setDelivery).catch((e) => setDelivery(failedDelivery(String(e)))),
      ]).then(() => {}),
    [onNotice],
  );
  useEffect(() => void load(), [load]);

  useEffect(() => {
    const un = api.onCookbookIllustrate((event) => setRun((r) => (r ? { ...r, event } : r)));
    return () => {
      un.then((f) => f());
    };
  }, []);
  const illustrateAll = async () => {
    setRun({ event: null, stopping: false });
    setRunResult(null);
    try {
      setRunResult(illustrateSummary(await api.cookbookIllustrate(null, true, false)));
    } catch (e) {
      setRunResult(String(e));
    }
    setRun(null);
    load();
  };
  const closeSheet = useCallback(() => setSheetOpen(false), []);
  const stopIllustrating = () => {
    setRun((r) => (r ? { ...r, stopping: true } : r));
    api.cookbookIllustrateStop().catch((e) => onNotice(String(e)));
  };

  // Add is the room that takes a paste, a drop or a focus request. The room changes while
  // rendering, so Add is showing by the time its effects run and can focus its link box.
  const [seen, setSeen] = useState({ focus: focusAdd, seq: incoming?.seq ?? 0 });
  if (seen.focus !== focusAdd || seen.seq !== (incoming?.seq ?? 0)) {
    setSeen({ focus: focusAdd, seq: incoming?.seq ?? 0 });
    setRoom("add");
  }

  return (
    <div className="daily-view">
      <nav className="kindle-tabs daily-tabs" aria-label="Cookbook">
        <BookMenu title={book?.config.title ?? "Your cookbook"} count={book ? recipeCount(book.recipes) : null} onSettings={() => setSheetOpen(true)} />
        {COOKBOOK_ROOMS.map((r) => (
          <button key={r.key} className={room === r.key ? "on" : ""} aria-current={room === r.key ? "page" : undefined} onClick={() => setRoom(r.key)}>
            {r.title}
          </button>
        ))}
      </nav>

      <div className="daily">
        {/* Add stays mounted, hidden, so a card in progress survives a look at Recipes */}
        <div hidden={room !== "add"}>
          <CookbookAdd
            book={book}
            delivery={delivery}
            focusAdd={focusAdd}
            focusTaken={focusTaken}
            incoming={incoming}
            taken={taken}
            onChanged={load}
            onNotice={onNotice}
            onRecipe={(id) => {
              setOpenRecipe(id);
              setRoom("recipes");
            }}
            onDelivery={() => setRoom("delivery")}
          />
        </div>
        {room === "recipes" && (
          <CookbookRecipes book={book} initialOpen={openRecipe} onOpened={recipeOpened} drawingAll={run !== null} onChanged={load} onNotice={onNotice} />
        )}
        {room === "delivery" && <CookbookDeliveryRoom book={book} delivery={delivery} onChanged={load} onNotice={onNotice} onDailyDelivery={onDailyDelivery} />}
      </div>
      {sheetOpen && book && (
        <BookSheet
          book={book}
          apiKey={apiKey}
          onApiKey={onApiKey}
          run={run}
          runResult={runResult}
          onIllustrateAll={illustrateAll}
          onStop={stopIllustrating}
          onChanged={load}
          onClose={closeSheet}
        />
      )}
    </div>
  );
}
