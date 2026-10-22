// SPDX-License-Identifier: GPL-3.0-or-later
const NOT_TEXT = new Set(["checkbox", "radio", "range", "button", "submit", "reset", "color", "file"]);

/** Whether Esc belongs to the focused element (a text field) rather than closing Settings. */
export function escStaysInField(tagName: string, type: string): boolean {
  if (tagName === "TEXTAREA") return true;
  return tagName === "INPUT" && !NOT_TEXT.has(type.toLowerCase());
}
