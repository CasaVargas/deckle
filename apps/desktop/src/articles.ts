// SPDX-License-Identifier: GPL-3.0-or-later
const WEB_ADDRESS = /https?:\/\/[^\s<>"]+/g;
const TRAILING = /[.,;:!?)\]}>'"]+$/;

/** The web addresses in pasted text: unique, in order, without the punctuation around them. */
export function parseAddresses(text: string): string[] {
  const urls = (text.match(WEB_ADDRESS) ?? []).map((u) => u.replace(TRAILING, ""));
  return [...new Set(urls)];
}
