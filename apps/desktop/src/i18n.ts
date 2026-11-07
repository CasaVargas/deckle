// SPDX-License-Identifier: GPL-3.0-or-later
// The app in other languages (UI translations design §3): the catalogs, the chosen language and
// t(). English is the source of truth; a key another language lacks falls back to English. The
// choice lives in localStorage, per window, and switching re-renders without a restart.
import { createContext, createElement, useContext, useEffect, useMemo, useState, type ReactNode } from "react";
import en from "./locales/en.json";

export type Key = keyof typeof en;
export type Vars = Record<string, string | number>;
export type T = (key: Key, vars?: Vars) => string;

/**
 * The languages offered, each named in itself, English first (design §8). A catalog in locales/
 * that isn't listed here is still being translated: it loads, but isn't offered until it's added.
 */
export const LOCALES = [
  { id: "en", name: "English", date: "en-GB" },
  { id: "de", name: "Deutsch", date: "de-DE" },
  { id: "es", name: "Español", date: "es" },
  { id: "fr", name: "Français", date: "fr-FR" },
  { id: "it", name: "Italiano", date: "it-IT" },
  { id: "pt-BR", name: "Português (Brasil)", date: "pt-BR" },
] as const;
export type Locale = (typeof LOCALES)[number]["id"];

const files = import.meta.glob<Partial<Record<Key, string>>>("./locales/*.json", { eager: true, import: "default" });

/** Every catalog in locales/, by file name (a BCP 47 tag: fr, pt-BR), offered or not. */
export const CATALOGS: Record<string, Partial<Record<Key, string>>> = Object.fromEntries(
  Object.entries(files).map(([path, catalog]) => [path.replace(/^.*\/|\.json$/g, ""), catalog]),
);

/** The date format each language uses (the welcome's dateline has always been en-GB). */
export const DATE_LOCALE = Object.fromEntries(LOCALES.map((l) => [l.id, l.date])) as Record<Locale, string>;

const isLocale = (s: unknown): s is Locale => LOCALES.some((l) => l.id === s);

const STORE = "deckle.locale";

/**
 * The string for `key` in `locale` (else English, else the key), with `{name}` filled from `vars`.
 * An empty string counts as missing: a translation tool may leave one for a line not done yet.
 */
export function translate(locale: Locale, key: Key, vars?: Vars): string {
  const s = CATALOGS[locale]?.[key] || en[key] || key;
  return vars ? s.replace(/\{(\w+)\}/g, (whole, name: string) => (name in vars ? String(vars[name]) : whole)) : s;
}

/**
 * The language a browser asks for, from its list of preferences in order: the exact language
 * (pt-BR), else the same language anywhere (es-MX gives es, pt-PT gives pt-BR), else English.
 */
export function detectLocale(langs: string | readonly string[] | undefined | null): Locale {
  const base = (tag: string) => tag.toLowerCase().split(/[-_]/)[0];
  for (const lang of typeof langs === "string" ? [langs] : (langs ?? [])) {
    const want = lang.toLowerCase().replace("_", "-");
    const found = LOCALES.find((l) => l.id.toLowerCase() === want) ?? LOCALES.find((l) => base(l.id) === base(want));
    if (found) return found.id;
  }
  return "en";
}

/** The saved choice, else the browser's language. Storage that throws counts as no choice. */
export function loadLocale(
  storage: Pick<Storage, "getItem"> | undefined = globalThis.localStorage,
  lang: string | readonly string[] | undefined = globalThis.navigator?.languages ?? globalThis.navigator?.language,
): Locale {
  try {
    const saved = storage?.getItem(STORE);
    if (isLocale(saved)) return saved;
  } catch {
    /* private mode or no storage: fall through */
  }
  return detectLocale(lang);
}

export function saveLocale(locale: Locale, storage: Pick<Storage, "setItem"> | undefined = globalThis.localStorage): void {
  try {
    storage?.setItem(STORE, locale);
  } catch {
    /* the choice just won't outlive the window */
  }
}

/** A translated string with `<b>…</b>` turned bold; everything else stays text. */
export function rich(s: string): ReactNode[] {
  return s.split(/(<b>.*?<\/b>)/).map((part, i) => {
    const m = /^<b>(.*)<\/b>$/.exec(part);
    return m ? createElement("strong", { key: i }, m[1]) : part;
  });
}

interface I18n {
  locale: Locale;
  setLocale: (l: Locale) => void;
  t: T;
}

const english: I18n = { locale: "en", setLocale: () => {}, t: (k, v) => translate("en", k, v) };
const Ctx = createContext<I18n>(english);

/** Holds the language at the root; anything reading useT() re-renders when it changes. */
export function I18nProvider({ children, initial }: { children: ReactNode; initial?: Locale }) {
  const [locale, setState] = useState<Locale>(() => initial ?? loadLocale());
  // The page's language follows, so a screen reader speaks German in a German voice.
  useEffect(() => {
    document.documentElement.lang = locale;
  }, [locale]);
  const value = useMemo<I18n>(
    () => ({
      locale,
      setLocale: (l) => {
        saveLocale(l);
        setState(l);
      },
      t: (k, v) => translate(locale, k, v),
    }),
    [locale],
  );
  return createElement(Ctx.Provider, { value }, children);
}

export function useI18n(): I18n {
  return useContext(Ctx);
}

export function useT(): T {
  return useContext(Ctx).t;
}
