// SPDX-License-Identifier: GPL-3.0-or-later
// The bookmarklets, installed (welcome design §4.2): on the desktop a button opens the install
// page in the default browser, where each is a link to drag to the bookmarks bar; in the web app
// the links are right here, since this is already a browser. Copy stays for a browser that
// won't take a drag. Used by the welcome, the Reading room and the Cookbook's Add page.
import { useEffect, useRef, useState } from "react";
import { useCapabilities } from "./capabilities";
import { api } from "./api";

type Which = "paper" | "recipe" | "both";

const PAPER = { key: "paper", name: "Save to paper", what: "On any article, it puts the story in tomorrow's paper, whole, with its photos." } as const;
const RECIPE = {
  key: "recipe",
  name: "Save recipe",
  what: "On a recipe page, it sends the recipe to your cookbook, taken from the page you're looking at, so it works on sites that won't let Deckle fetch them.",
} as const;

/** A link to drag to the bookmarks bar. React replaces a `javascript:` href with one that throws,
 * so the address goes onto the element itself, after it's mounted. */
function DragLink({ js, children }: { js: string | undefined; children: string }) {
  const a = useRef<HTMLAnchorElement | null>(null);
  useEffect(() => {
    if (a.current) a.current.setAttribute("href", js ?? "#");
  }, [js]);
  return (
    <a className="bookmarklet" ref={a} draggable onClick={(e) => e.preventDefault()}>
      {children}
    </a>
  );
}

export default function BookmarkletInstall({ which, onNotice }: { which: Which; onNotice: (m: string) => void }) {
  const caps = useCapabilities();
  const [js, setJs] = useState<{ paper?: string; recipe?: string }>({});
  const [copied, setCopied] = useState<string | null>(null);
  useEffect(() => {
    if (which !== "recipe") api.bookmarklet().then((paper) => setJs((j) => ({ ...j, paper }))).catch(() => {});
    if (which !== "paper") api.recipeBookmarklet().then((recipe) => setJs((j) => ({ ...j, recipe }))).catch(() => {});
  }, [which]);
  const items = which === "both" ? [PAPER, RECIPE] : which === "paper" ? [PAPER] : [RECIPE];
  const copy = async (key: string, text: string | undefined) => {
    if (!text) return;
    try {
      await navigator.clipboard.writeText(text);
      setCopied(key);
      setTimeout(() => setCopied(null), 2000);
    } catch (e) {
      onNotice(String(e));
    }
  };
  const each = items.length > 1 ? "each" : "it";

  if (caps.web) {
    return (
      <div className="bookmarklets">
        {items.map((i) => (
          <div className="row-pair" key={i.key}>
            <div>
              <DragLink js={js[i.key]}>{i.name}</DragLink>
              <p className="muted small">{i.what}</p>
            </div>
            <button className="ghost small" onClick={() => copy(i.key, js[i.key])} disabled={!js[i.key]}>
              {copied === i.key ? "Copied" : "Copy"}
            </button>
          </div>
        ))}
        <p className="muted small">
          Drag {each} to your bookmarks bar. If a drag won't take, Copy {items.length > 1 ? "one" : "it"}, make a new bookmark and paste it as the address.
        </p>
      </div>
    );
  }

  return (
    <div className="bookmarklets">
      <div className="row-pair">
        <div>
          <div className="label">{items.map((i) => i.name).join(" and ")}</div>
          {items.map((i) => (
            <p className="muted small" key={i.key}>
              {items.length > 1 && <strong>{i.name}. </strong>}
              {i.what}
            </p>
          ))}
          <p className="muted small">The install page opens in your browser; drag {each} to the bookmarks bar there. They work while Deckle's morning schedule is on.</p>
        </div>
        <div className="bookmarklet-actions">
          <button className="ghost" onClick={() => api.bookmarkletPageOpen().catch((e) => onNotice(String(e)))}>
            Open the install page
          </button>
          {items.map((i) => (
            <button className="link" key={i.key} onClick={() => copy(i.key, js[i.key])} disabled={!js[i.key]}>
              {copied === i.key ? "Copied" : `Copy ${i.name}`}
            </button>
          ))}
        </div>
      </div>
    </div>
  );
}
