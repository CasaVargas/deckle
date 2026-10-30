// SPDX-License-Identifier: GPL-3.0-or-later
// The server-side file picker's data: the roots, a folder's listing, uploads, and what the
// picker shows of them (a pure view, so it's tested without a page).
import { checkSession, CSRF, errorOf } from "../transport/web";

export interface Entry {
  name: string;
  path: string;
  dir: boolean;
  size: number;
}

/** `GET /api/fs/list`: `parent` is null at a root. */
export interface Listing {
  path: string;
  parent: string | null;
  entries: Entry[];
}

export type PickMode = "file" | "folder";

export interface PickerView {
  /** Folders first, then (picking files) the files that match. */
  rows: Entry[];
  /** Where Up goes: a folder, `"roots"` (the list of roots), or nowhere. */
  up: string | "roots" | null;
}

const trimSlash = (p: string) => (p.length > 1 ? p.replace(/\/+$/, "") : p);
const within = (path: string, root: string) => {
  const r = trimSlash(root);
  return path === r || path.startsWith(r === "/" ? "/" : `${r}/`);
};

export function pickerView(listing: Listing, roots: string[], o: { mode: PickMode; extensions?: string[] }): PickerView {
  const exts = o.extensions?.map((e) => e.toLowerCase());
  const matches = (e: Entry) => !exts || exts.length === 0 || exts.some((x) => e.name.toLowerCase().endsWith(`.${x}`));
  const dirs = listing.entries.filter((e) => e.dir);
  const files = o.mode === "file" ? listing.entries.filter((e) => !e.dir && matches(e)) : [];
  const atRoot = roots.some((r) => trimSlash(r) === trimSlash(listing.path));
  let up: PickerView["up"] = null;
  if (atRoot) up = roots.length > 1 ? "roots" : null;
  else if (listing.parent && roots.some((r) => within(listing.parent!, r))) up = listing.parent;
  return { rows: [...dirs, ...files], up };
}

/** Where a saved file goes: `/data/Deckle/Exports/`, or under the first root when `/data` isn't one. */
export function saveTarget(roots: string[], name: string): string | null {
  const root = roots.find((r) => trimSlash(r) === "/data") ?? roots[0];
  if (root === undefined) return null;
  return `${trimSlash(root)}/Deckle/Exports/${name}`;
}

async function getJson<T>(url: string): Promise<T> {
  const res = checkSession(await fetch(url, { credentials: "same-origin" }));
  if (!res.ok) throw await errorOf(res);
  return (await res.json()) as T;
}

export const fetchRoots = () => getJson<{ path: string }[]>("/api/fs/roots").then((rs) => rs.map((r) => r.path));
export const fetchListing = (path: string) => getJson<Listing>(`/api/fs/list?path=${encodeURIComponent(path)}`);

/** Uploads to the server; resolves to where it saved each file. */
export async function upload(files: File[]): Promise<string[]> {
  const form = new FormData();
  for (const f of files) form.append("file", f, f.name);
  const res = checkSession(await fetch("/api/fs/upload", { method: "POST", headers: { ...CSRF }, credentials: "same-origin", body: form }));
  if (!res.ok) throw await errorOf(res);
  return (await res.json()) as string[];
}
