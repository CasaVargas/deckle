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
