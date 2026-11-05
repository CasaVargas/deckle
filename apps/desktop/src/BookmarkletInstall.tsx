// SPDX-License-Identifier: GPL-3.0-or-later
// The bookmarklets, installed (welcome design §4.2): on the desktop a button opens the install
// page in the default browser, where each is a link to drag to the bookmarks bar; in the web app
// the links are right here, since this is already a browser. Copy stays for a browser that
// won't take a drag. Used by the welcome, the Reading room and the Cookbook's Add page.
import { useEffect, useRef, useState } from "react";
import { useCapabilities } from "./capabilities";
import { api } from "./api";
import { useT, type Key } from "./i18n";

type Which = "paper" | "recipe" | "both";

const PAPER = { key: "paper", name: "bookmarklet.paper.name", what: "bookmarklet.paper.what" } as const satisfies { key: string; name: Key; what: Key };
const RECIPE = { key: "recipe", name: "bookmarklet.recipe.name", what: "bookmarklet.recipe.what" } as const satisfies { key: string; name: Key; what: Key };

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
  const t = useT();
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
  const both = items.length > 1;

  if (caps.web) {
    return (
      <div className="bookmarklets">
        {items.map((i) => (
          <div className="row-pair" key={i.key}>
            <div>
              <DragLink js={js[i.key]}>{t(i.name)}</DragLink>
              <p className="muted small">{t(i.what)}</p>
            </div>
            <button className="ghost small" onClick={() => copy(i.key, js[i.key])} disabled={!js[i.key]}>
              {copied === i.key ? t("bookmarklet.copied") : t("bookmarklet.copy")}
            </button>
          </div>
        ))}
        <p className="muted small">{t(both ? "bookmarklet.web.both" : "bookmarklet.web.one")}</p>
      </div>
    );
  }

  return (
    <div className="bookmarklets">
      <div className="row-pair">
        <div>
          <div className="label">{both ? t("bookmarklet.and", { a: t(PAPER.name), b: t(RECIPE.name) }) : t(items[0].name)}</div>
          {items.map((i) => (
            <p className="muted small" key={i.key}>
              {both && <strong>{t(i.name)}. </strong>}
              {t(i.what)}
            </p>
          ))}
          <p className="muted small">{t(both ? "bookmarklet.desk.both" : "bookmarklet.desk.one")}</p>
        </div>
        <div className="bookmarklet-actions">
          <button className="ghost" onClick={() => api.bookmarkletPageOpen().catch((e) => onNotice(String(e)))}>
            {t("bookmarklet.desk.open")}
          </button>
          {items.map((i) => (
            <button className="link" key={i.key} onClick={() => copy(i.key, js[i.key])} disabled={!js[i.key]}>
              {copied === i.key ? t("bookmarklet.copied") : t("bookmarklet.copyName", { name: t(i.name) })}
            </button>
          ))}
        </div>
      </div>
    </div>
  );
}
