// SPDX-License-Identifier: GPL-3.0-or-later
import { expect, test } from "vitest";
import { isValidElement } from "react";
import en from "./locales/en.json";
import fr from "./locales/fr.json";
import { LOCALES, detectLocale, loadLocale, rich, saveLocale, translate, type Key } from "./i18n";

test("t() gives the chosen language and fills {vars}", () => {
  expect(translate("en", "welcome.skip")).toBe("Skip");
  expect(translate("fr", "welcome.skip")).toBe("Passer");
  expect(translate("fr", "welcome.mini.printedAt", { time: "5:30" })).toBe("Imprimé à 5:30");
  // A var not passed is left as written, never "undefined".
  expect(translate("en", "welcome.mini.printedAt")).toBe("Printed at {time}");
});

test("a key the language lacks falls back to English, never a raw key", () => {
  const key = "welcome.skip" as Key;
  const saved = (fr as Record<string, string>)[key];
  delete (fr as Record<string, string>)[key];
  try {
    expect(translate("fr", key)).toBe("Skip");
  } finally {
    (fr as Record<string, string>)[key] = saved;
  }
});

test("every language has exactly English's keys, none empty", () => {
  const keys = Object.keys(en).sort();
  expect(Object.keys(fr).sort()).toEqual(keys);
  for (const [k, v] of Object.entries(fr)) expect(v.trim(), k).not.toBe("");
  // Every {var} English uses, French uses too.
  for (const k of keys) {
    const vars = (s: string) => (s.match(/\{\w+\}/g) ?? []).sort();
    expect(vars((fr as Record<string, string>)[k]), k).toEqual(vars((en as Record<string, string>)[k]));
  }
});

test("the language comes from the browser until one is chosen", () => {
  expect(detectLocale("fr-CA")).toBe("fr");
  expect(detectLocale("FR")).toBe("fr");
  expect(detectLocale("en-US")).toBe("en");
  expect(detectLocale("de-DE")).toBe("en");
  expect(detectLocale(undefined)).toBe("en");

  const store = new Map<string, string>();
  const storage = { getItem: (k: string) => store.get(k) ?? null, setItem: (k: string, v: string) => void store.set(k, v) };
  expect(loadLocale(storage, "fr-FR")).toBe("fr");
  saveLocale("en", storage);
  expect(loadLocale(storage, "fr-FR")).toBe("en");
  store.set("deckle.locale", "xx");
  expect(loadLocale(storage, "fr-FR")).toBe("fr");
  // Storage that throws is no choice, not a crash.
  const broken = { getItem: () => { throw new Error("denied"); } };
  expect(loadLocale(broken, "en-US")).toBe("en");
});

test("each language is offered under its own name", () => {
  expect(LOCALES).toEqual([
    { id: "en", name: "English" },
    { id: "fr", name: "Français" },
  ]);
});

test("rich() bolds <b> and leaves the rest as text", () => {
  const parts = rich("A <b>bold</b> word and <b>another</b>.");
  expect(parts.filter((p) => typeof p === "string").join("|")).toBe("A | word and |.");
  const bold = parts.filter(isValidElement);
  expect(bold.map((b) => (b as { type: string }).type)).toEqual(["strong", "strong"]);
  expect(rich("plain")).toEqual(["plain"]);
});

const sources = import.meta.glob<string>(["./Welcome.tsx", "./BookmarkletInstall.tsx"], { query: "?raw", import: "default", eager: true });

test("the welcome has no English left in its markup", () => {
  for (const [path, text] of Object.entries(sources)) {
    // Text between tags that holds a letter, outside {expressions}.
    // Code between a generic's ">" and the next "<" (useState<X>(null); …) has code punctuation; prose doesn't.
    const loose = [...text.matchAll(/>([^<>{}]*)</g)]
      .map((m) => m[1].trim())
      .filter((seg) => /[A-Za-z]{2,}/.test(seg) && !/[()[\];=&|`]/.test(seg));
    expect(loose, path).toEqual([]);
    // English in the attributes a reader hears or sees.
    expect(text.match(/(aria-label|title|placeholder|alt)="[^"]*[A-Za-z]/g) ?? [], path).toEqual([]);
  }
});
