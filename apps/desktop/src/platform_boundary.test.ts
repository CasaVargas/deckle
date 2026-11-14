// SPDX-License-Identifier: GPL-3.0-or-later
// The host is reached through two doors only, so the web build can swap them (transport, platform).
import { expect, test } from "vitest";

const sources = import.meta.glob<string>(["./**/*.ts", "./**/*.tsx", "!./**/*.test.ts"], { query: "?raw", import: "default", eager: true });
const ALLOWED = new Set(["./transport.ts", "./platform/desktop.ts"]);

test("the walk sees the source tree", () => {
  expect(Object.keys(sources)).toContain("./App.tsx");
  expect(Object.keys(sources)).toContain("./settings/About.tsx");
});

test("only transport.ts and platform/desktop.ts import @tauri-apps", () => {
  const offenders = Object.entries(sources)
    .filter(([path, text]) => !ALLOWED.has(path) && /from\s+["']@tauri-apps\/|import\(\s*["']@tauri-apps\//.test(text))
    .map(([path]) => path);
  expect(offenders).toEqual([]);
});

// Tauri's dialog plugin turns window.confirm and window.alert into async functions, so on the
// desktop `!window.confirm(...)` is always false and a removal runs without asking (jloutsch/deckle#88).
// Questions go through platform.confirm, which waits for the answer on both hosts.
test("no view calls window.confirm or window.alert", () => {
  const HOST = new Set(["./platform.ts", "./platform/desktop.ts", "./platform/web.ts"]);
  const offenders = Object.entries(sources)
    .filter(([path, text]) => !HOST.has(path) && /\bwindow\.(confirm|alert)\b|(?<![.\w])(confirm|alert)\s*\(/.test(text))
    .map(([path]) => path);
  expect(offenders).toEqual([]);
});
