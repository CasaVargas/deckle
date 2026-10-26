// SPDX-License-Identifier: GPL-3.0-or-later
import type { Draft, Field } from "./detailsDraft";
import type { Suggested } from "./types";

/** One Open Library suggestion for an empty field: what it says, and what Use writes. */
export interface Suggestion {
  field: Field;
  text: string;
  patch: Partial<Draft>;
}

const blank = (s: string) => !s.trim();

/**
 * Suggestions for the fields the form leaves empty now (Books tab redesign §6): one filled in
 * since gets none, so the book's own details and the person's always win.
 */
export function suggestionsFor(d: Draft, s: Suggested): Suggestion[] {
  const out: Suggestion[] = [];
  const text = (field: Field, v: string | null) => {
    const t = v?.trim();
    if (t && blank(d[field] as string)) out.push({ field, text: t, patch: { [field]: t } });
  };
  if (d.authors.length === 0 && s.authors.length) out.push({ field: "authors", text: s.authors.join(", "), patch: { authors: s.authors } });
  if (blank(d.series) && s.series?.name.trim()) {
    const index = s.series.index != null ? String(s.series.index) : "";
    out.push({ field: "series", text: index ? `${s.series.name}, ${index}` : s.series.name, patch: { series: s.series.name, index } });
  }
  text("description", s.description);
  text("publisher", s.publisher);
  text("published", s.published);
  text("language", s.language);
  if (d.tags.length === 0 && s.tags.length) out.push({ field: "tags", text: s.tags.join(", "), patch: { tags: s.tags } });
  return out;
}

/** The form with every suggestion used. */
export function applyAll(d: Draft, list: Suggestion[]): Draft {
  return list.reduce<Draft>((acc, s) => ({ ...acc, ...s.patch }), d);
}
