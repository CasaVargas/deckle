// SPDX-License-Identifier: GPL-3.0-or-later
import { expect, test } from "vitest";
import { escStaysInField } from "./settingsKeys";

test("text fields keep Esc", () => {
  for (const type of ["text", "password", "search", "url", ""]) expect(escStaysInField("INPUT", type)).toBe(true);
  expect(escStaysInField("TEXTAREA", "")).toBe(true);
});
test("switches, sliders and selects don't", () => {
  expect(escStaysInField("INPUT", "checkbox")).toBe(false);
  expect(escStaysInField("INPUT", "range")).toBe(false);
  expect(escStaysInField("SELECT", "")).toBe(false);
  expect(escStaysInField("BUTTON", "submit")).toBe(false);
});
