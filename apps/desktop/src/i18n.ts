// SPDX-License-Identifier: GPL-3.0-or-later
// The app in other languages (UI translations design §3): the catalogs, the chosen language and
// t(). English is the source of truth; a key another language lacks falls back to English. The
// choice lives in localStorage, per window, and switching re-renders without a restart.
import { createContext, createElement, useContext, useMemo, useState, type ReactNode } from "react";
import en from "./locales/en.json";
import fr from "./locales/fr.json";

export type Key = keyof typeof en;
export type Locale = "en" | "fr";
export type Vars = Record<string, string | number>;
export type T = (key: Key, vars?: Vars) => string;

/** The languages offered, each named in itself. */
export const LOCALES: { id: Locale; name: string }[] = [
  { id: "en", name: "English" },
  { id: "fr", name: "Français" },
];

const CATALOGS: Record<Locale, Partial<Record<Key, string>>> = { en, fr };

/** The date format each language uses (the welcome's dateline has always been en-GB). */
export const DATE_LOCALE: Record<Locale, string> = { en: "en-GB", fr: "fr-FR" };

const STORE = "deckle.locale";

/** The string for `key` in `locale` (else English, else the key), with `{name}` filled from `vars`. */
export function translate(locale: Locale, key: Key, vars?: Vars): string {
  const s = CATALOGS[locale][key] ?? en[key] ?? key;
  return vars ? s.replace(/\{(\w+)\}/g, (whole, name: string) => (name in vars ? String(vars[name]) : whole)) : s;
}

/** The language a browser asks for: French for any fr-*, English for everything else. */
export function detectLocale(lang: string | undefined | null): Locale {
  return lang?.toLowerCase().startsWith("fr") ? "fr" : "en";
}

/** The saved choice, else the browser's language. Storage that throws counts as no choice. */
export function loadLocale(storage: Pick<Storage, "getItem"> | undefined = globalThis.localStorage, lang = globalThis.navigator?.language): Locale {
  try {
    const saved = storage?.getItem(STORE);
    if (saved === "en" || saved === "fr") return saved;
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
