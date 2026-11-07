// SPDX-License-Identifier: GPL-3.0-or-later
import { expect, test } from "vitest";
import { isValidElement } from "react";
import en from "./locales/en.json";
import { CATALOGS, DATE_LOCALE, LOCALES, detectLocale, loadLocale, rich, saveLocale, translate, type Key } from "./i18n";
import { dateline } from "./onboarding";

const english = en as Record<string, string>;
const vars = (s: string) => (s.match(/\{\w+\}/g) ?? []).sort();
const bolds = (s: string) => [(s.match(/<b>/g) ?? []).length, (s.match(/<\/b>/g) ?? []).length];

test("t() gives the chosen language and fills {vars}", () => {
  expect(translate("en", "welcome.skip")).toBe("Skip");
  expect(translate("fr", "welcome.skip")).toBe("Passer");
  expect(translate("de", "welcome.skip")).toBe("Überspringen");
  expect(translate("pt-BR", "welcome.skip")).toBe("Pular");
  expect(translate("fr", "welcome.mini.printedAt", { time: "5:30" })).toBe("Imprimé à 5:30");
  // A var not passed is left as written, never "undefined".
  expect(translate("en", "welcome.mini.printedAt")).toBe("Printed at {time}");
});

test("a key the language lacks, or left empty, falls back to English, never a raw key", () => {
  const fr = CATALOGS.fr;
  const key: Key = "welcome.skip";
  const saved = fr[key];
  try {
    delete fr[key];
    expect(translate("fr", key)).toBe("Skip");
    // A translation tool may write "" for a line nobody has done yet.
    fr[key] = "";
    expect(translate("fr", key)).toBe("Skip");
  } finally {
    fr[key] = saved;
  }
});

test("every language offered has all of English's keys, none empty", () => {
  for (const { id } of LOCALES) {
    const catalog = CATALOGS[id] as Record<string, string> | undefined;
    expect(catalog, `locales/${id}.json`).toBeDefined();
    expect(Object.keys(catalog!).sort(), id).toEqual(Object.keys(english).sort());
    for (const [k, v] of Object.entries(catalog!)) expect(v.trim(), `${id} ${k}`).not.toBe("");
  }
});

test("every catalog, offered or still being translated, fits English", () => {
  for (const [id, catalog] of Object.entries(CATALOGS) as [string, Record<string, string>][]) {
    // Named by its BCP 47 tag (fr, pt-BR), not pt_BR or pt-br.
    expect(id).toMatch(/^[a-z]{2,3}(-[A-Z]{2}|-[A-Z][a-z]{3})?$/);
    // No line English doesn't have: a typo, or one English has since dropped.
    expect(Object.keys(catalog).filter((k) => !(k in english)), id).toEqual([]);
    for (const [k, v] of Object.entries(catalog)) {
      if (v === "") continue;
      // Every {var} English uses, the translation uses too; and its <b> tags pair the same.
      expect(vars(v), `${id} ${k}`).toEqual(vars(english[k]));
      expect(bolds(v), `${id} ${k}`).toEqual(bolds(english[k]));
    }
  }
});

test("the language comes from the browser until one is chosen", () => {
  expect(detectLocale("fr-CA")).toBe("fr");
  expect(detectLocale("FR")).toBe("fr");
  expect(detectLocale("en-US")).toBe("en");
  expect(detectLocale("de-DE")).toBe("de");
  expect(detectLocale("es-MX")).toBe("es");
  expect(detectLocale("pt-BR")).toBe("pt-BR");
  expect(detectLocale("pt_BR")).toBe("pt-BR");
  expect(detectLocale("pt-PT")).toBe("pt-BR");
  expect(detectLocale("nl-NL")).toBe("en");
  expect(detectLocale(undefined)).toBe("en");
  // The browser's list, in order: the first one Deckle speaks wins.
  expect(detectLocale(["nl-NL", "it-IT", "en-US"])).toBe("it");
  expect(detectLocale(["en-GB", "de-DE"])).toBe("en");

  const store = new Map<string, string>();
  const storage = { getItem: (k: string) => store.get(k) ?? null, setItem: (k: string, v: string) => void store.set(k, v) };
  expect(loadLocale(storage, "fr-FR")).toBe("fr");
  saveLocale("en", storage);
  expect(loadLocale(storage, "fr-FR")).toBe("en");
  saveLocale("pt-BR", storage);
  expect(loadLocale(storage, "fr-FR")).toBe("pt-BR");
  store.set("deckle.locale", "xx");
  expect(loadLocale(storage, "fr-FR")).toBe("fr");
  // Storage that throws is no choice, not a crash.
  const broken = { getItem: () => { throw new Error("denied"); } };
  expect(loadLocale(broken, "en-US")).toBe("en");
});

test("each language is offered under its own name, English first", () => {
  expect(LOCALES.map((l) => l.name)).toEqual(["English", "Deutsch", "Español", "Français", "Italiano", "Português (Brasil)"]);
});

test("each language dates the welcome its own way", () => {
  const day = new Date(2026, 10, 3, 9);
  expect(dateline(day, DATE_LOCALE.en)).toBe("Tuesday, 3 November 2026");
  expect(dateline(day, DATE_LOCALE.de)).toBe("Dienstag, 3. November 2026");
  expect(dateline(day, DATE_LOCALE.es)).toBe("martes, 3 de noviembre de 2026");
  expect(dateline(day, DATE_LOCALE.it)).toBe("martedì 3 novembre 2026");
  expect(dateline(day, DATE_LOCALE["pt-BR"])).toBe("terça-feira, 3 de novembro de 2026");
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
