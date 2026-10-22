// SPDX-License-Identifier: GPL-3.0-or-later
const WEB_ADDRESS = /https?:\/\/[^\s<>"]+/g;
const TRAILING = /[.,;:!?\]}>'"]+$/;

const count = (s: string, c: string) => s.split(c).length - 1;

/** Drop punctuation that ends the sentence, not the address: a `)` only when it closes nothing in the address. */
function trim(url: string): string {
  let u = url;
  for (;;) {
    const t = u.replace(TRAILING, "");
    if (t.endsWith(")") && count(t, ")") > count(t, "(")) u = t.slice(0, -1);
    else return t;
  }
}

/** The web addresses in pasted text: unique, in order, without the punctuation around them. */
export function parseAddresses(text: string): string[] {
  return [...new Set((text.match(WEB_ADDRESS) ?? []).map(trim))];
}
