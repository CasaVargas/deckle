// SPDX-License-Identifier: GPL-3.0-or-later
/** The bind panel's list (batch-bind design §6): pure, for the tests. */

/** `list` with item `i` moved by `delta` places, clamped to the ends. */
export function move<T>(list: T[], i: number, delta: number): T[] {
  const to = Math.max(0, Math.min(list.length - 1, i + delta));
  if (i < 0 || i >= list.length || to === i) return list;
  const out = [...list];
  const [item] = out.splice(i, 1);
  out.splice(to, 0, item);
  return out;
}

/** What can be a part: anything the app can read except a finished Kindle book. */
export function bindable(path: string, mode: string): boolean {
  return mode !== "error" && !/\.kfx$/i.test(path);
}
