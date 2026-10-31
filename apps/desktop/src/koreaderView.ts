// SPDX-License-Identifier: GPL-3.0-or-later
import type { KoPlugin, KoreaderPreview, KoreaderView } from "./types";

/** The Deckle plugin on the reader, if it's there, and whether this Deckle has a newer one. */
export function deckleState(view: KoreaderView): { installed: boolean; version: string | null; outdated: boolean } {
  const p = view.listing?.plugins.find((x) => x.name === "deckle");
  if (!p) return { installed: false, version: null, outdated: false };
  const version = p.version ?? null;
  return { installed: true, version, outdated: version !== view.deckle_version };
}

/** Switching plugins needs the Deckle plugin on the reader, on, and not on its way off. */
export function canSwitch(view: KoreaderView): boolean {
  const p = view.listing?.plugins.find((x) => x.name === "deckle");
  return !!p && p.enabled && p.pending !== false;
}

/** What a plugin row says about its state: a waiting change first. */
export function stateLine(p: KoPlugin): string {
  if (p.pending === true) return "On after a restart";
  if (p.pending === false) return "Off after a restart";
  return p.enabled ? "On" : "Off";
}

/** Whether the row's switch shows on: what it will be, when a change waits. */
export function switchOn(p: KoPlugin): boolean {
  return p.pending ?? p.enabled;
}

/** The plugins a reader shows: the Deckle plugin has its own card, so it's left out here. Added
 * plugins first (the ones you can change), then the built-in ones. */
export function pluginRows(view: KoreaderView): KoPlugin[] {
  const all = (view.listing?.plugins ?? []).filter((p) => p.name !== "deckle");
  return [...all.filter((p) => !p.builtin), ...all.filter((p) => p.builtin)];
}

/** The question asked before an install. */
export function confirmLine(p: KoreaderPreview): string {
  const what = `${p.fullname}${p.version ? ` ${p.version}` : ""}`;
  const from = p.from === "deckle" ? "built into Deckle" : `from ${p.from}${p.release ? ` (release ${p.release})` : ""}`;
  return `${p.replaces ? "Replace" : "Install"} ${what}, ${from}?`;
}
