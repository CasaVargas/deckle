// SPDX-License-Identifier: GPL-3.0-or-later
// The Cookbook tab (cookbook design §8.2), built on the Daily tab's frame: the book menu, then
// Add, Recipes and Delivery. Add is here; Recipes, Delivery and the Book settings sheet are
// Task 6.3 and slot in at the marked seams.
import { useCallback, useEffect, useRef, useState } from "react";
import { api } from "./api";
import BookMenu from "./BookMenu";
import CookbookAdd from "./CookbookAdd";
import type { Incoming } from "./CookbookAdd";
import { COOKBOOK_ROOMS, recipeCount } from "./cookbookRooms";
import type { CookbookRoom } from "./cookbookRooms";
import type { CookbookBook, CookbookDelivery } from "./types";

export default function CookbookView({
  onNotice,
  focusAdd,
  incoming,
}: {
  onNotice: (m: string) => void;
  /** Bumped by the app to land on Add with the link box focused (Books → Add → Recipe…). */
  focusAdd: number;
  /** An address or file pasted or dropped on the tab: it goes to Add. */
  incoming: Incoming | null;
}) {
  const [room, setRoom] = useState<CookbookRoom>("add");
  const [book, setBook] = useState<CookbookBook | null>(null);
  const [delivery, setDelivery] = useState<CookbookDelivery | null>(null);
  /** Task 6.3: the recipe whose row Recipes opens first (set by Add's "See it in Recipes"). */
  const [, setOpenRecipe] = useState<string | null>(null);
  /** Task 6.3: the Book settings sheet. */
  const [, setSheetOpen] = useState(false);
  const taken = useRef(0);

  const load = useCallback(() => {
    api.cookbookBook().then(setBook).catch((e) => onNotice(String(e)));
    api.cookbookDelivery().then(setDelivery).catch(() => setDelivery(null));
  }, [onNotice]);
  useEffect(load, [load]);

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
        {/* Task 6.3: <CookbookRecipes …/>, <CookbookDelivery …/> and <BookSheet …/> replace these */}
        {room === "recipes" && <p className="muted">Recipes are listed here.</p>}
        {room === "delivery" && <p className="muted">Where the book goes is shown here.</p>}
      </div>
    </div>
  );
}
