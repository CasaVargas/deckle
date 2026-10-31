// SPDX-License-Identifier: GPL-3.0-or-later
import { expect, test } from "vitest";
import { canSwitch, confirmLine, deckleState, pickAction, pluginRows, stateLine, switchOn, updateLine } from "./koreaderView";
import type { KoPlugin, KoreaderView } from "./types";

const plugin = (name: string, o: Partial<KoPlugin> = {}): KoPlugin => ({ name, fullname: name, builtin: false, enabled: true, ...o });

const view = (plugins: KoPlugin[]): KoreaderView => ({
  device: "Kobo",
  root: "/Volumes/KOBOeReader",
  listing: { data_dir: "/Volumes/KOBOeReader/.adds/koreader", plugins, patches: [], patches_disabled: false },
  deckle_version: "0.11.0",
  deckle_blocked: null,
  note: null,
});

test("the Deckle plugin's state, and whether this Deckle has a newer one", () => {
  expect(deckleState(view([]))).toEqual({ installed: false, version: null, outdated: false });
  expect(deckleState(view([plugin("deckle", { version: "0.10.0" })]))).toEqual({ installed: true, version: "0.10.0", outdated: true });
  expect(deckleState(view([plugin("deckle", { version: "0.11.0" })])).outdated).toBe(false);
});

test("switching needs the Deckle plugin on, and not on its way off", () => {
  expect(canSwitch(view([plugin("zen")]))).toBe(false);
  expect(canSwitch(view([plugin("deckle")]))).toBe(true);
  expect(canSwitch(view([plugin("deckle", { enabled: false })]))).toBe(false);
  expect(canSwitch(view([plugin("deckle", { pending: false })]))).toBe(false);
});

test("a waiting change shows as what it will be", () => {
  expect(stateLine(plugin("zen", { enabled: false, pending: true }))).toBe("On after a restart");
  expect(stateLine(plugin("zen", { pending: false }))).toBe("Off after a restart");
  expect(stateLine(plugin("zen", { enabled: false }))).toBe("Off");
  expect(switchOn(plugin("zen", { enabled: false, pending: true }))).toBe(true);
  expect(switchOn(plugin("zen", { enabled: true }))).toBe(true);
});

test("rows: added plugins first, built-in after, the Deckle plugin left to its own card", () => {
  const rows = pluginRows(view([plugin("kosync", { builtin: true }), plugin("deckle"), plugin("zen")]));
  expect(rows.map((p) => p.name)).toEqual(["zen", "kosync"]);
});

test("the question before an install says what and from where", () => {
  expect(confirmLine({ name: "appstore", fullname: "App Store", version: "1.14.0", replaces: false, from: "omer-faruq/appstore.koplugin", release: "v1.14.0", reviewed: true })).toBe(
    "Install App Store 1.14.0, from omer-faruq/appstore.koplugin (release v1.14.0)?",
  );
  expect(confirmLine({ name: "deckle", fullname: "Deckle", version: "0.11.0", replaces: true, from: "deckle", release: "0.11.0", reviewed: true })).toBe("Replace Deckle 0.11.0, built into Deckle?");
});

test("a Recommended plugin installs, updates to a newer pin, or is installed", () => {
  const pick = { repo: "a/zen.koplugin", name: "zen", kind: "plugin", why: "", license: "GPL-3.0", release: { tag: "v2", asset: "z.zip", sha256: "x" } };
  expect(pickAction(pick, view([]))).toBe("install");
  expect(pickAction(pick, view([plugin("zen", { source: { from: "a/zen.koplugin", release: "v1", installed: "" } })]))).toBe("update");
  expect(pickAction(pick, view([plugin("zen", { source: { from: "a/zen.koplugin", release: "v2", installed: "" } })]))).toBe("installed");
  expect(pickAction(pick, null)).toBe("install");
});

test("the update line says whether the new release was reviewed", () => {
  const u = [{ name: "zen", repo: "a/zen", installed: "v1", latest: "v2", pinned: true }];
  expect(updateLine("zen", u)).toBe("Update available: v2 (reviewed)");
  expect(updateLine("other", u)).toBeNull();
});
