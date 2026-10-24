# Accessibility: language tags and image-alt audit (#32)

This increment repairs language tags during EPUB normalization and reports missing image alt
attributes during inspection. It is always offline. Vision-based generated descriptions remain
a separate follow-up; no image is uploaded and no description or decorative status is inferred.

## Language rules

- Trim surrounding whitespace, replace locale underscores with hyphens, and use `language-tags`
  to parse well-formed BCP 47 tags and normalize their casing (`en_US` → `en-US`, `zh_hant` →
  `zh-Hant`). Preserve scripts, variants, extensions, private-use and grandfathered tags. This is
  syntax normalization, not IANA registry validation or guessing language from prose/names.
- Inspect returns normalized primary language. Preserve the existing warned English fallback
  for missing metadata. Structurally invalid nonempty metadata becomes `und` (undetermined),
  with a warning, rather than guessing English.
- Normalize every metadata language element, respecting its namespace/prefix; add one if absent.
  Empty language elements use the first nonempty book language, or the existing English fallback.
- Every manifest XHTML root, including generated covers and navigation, receives missing `lang`
  and `xml:lang`. An explicit root value takes precedence over the book default. Normalize its
  spelling, preserving its language; an explicit empty attribute means unknown and remains empty.
- Preserve invalid and conflicting root declarations and warn. Do not rewrite inline language
  attributes or replace an explicit chapter language with the package language.
- Use parsed XML byte ranges for edits, preserving unrelated markup and text. Already normalized
  values are unchanged. Invalid/unreadable XHTML is skipped with a warning, not silently repaired.
- `NormalizeReport.language_tags_fixed` counts changed metadata elements/root attributes. Run
  the pass after generated pages and metadata fill, so the completed EPUB is covered.

## Alt audit

Inspect each distinct manifest XHTML document, including non-spine content. Count XHTML `img`
elements without an unqualified `alt` attribute. Preserve both existing text and explicit empty
`alt`; do not infer that an image is decorative. SVG images require different accessibility
semantics and are not counted by this pass.

`BookInfo.images_missing_alt` is the aggregate count. Per-document warnings include the href and
count and flow through existing CLI inspection, desktop book warnings and conversion reports.
Unreadable documents produce an audit-skipped warning, so a zero count is not claimed as proof
of full accessibility. KFX inspection does not perform this EPUB audit.

## Validation

Tests cover language spelling, multilingual chapters/passages, explicit unknown language,
namespace prefixes, self-closing metadata, malformed/conflicting tags, byte preservation and
idempotence. EPUB integration tests cover alt counts, empty/described alt preservation, duplicate
manifest references, non-spine content, audit errors and generated pages. CLI JSON exposes the
count. Existing inspect, normalize, pipeline and desktop tests remain regression coverage.

References:
- https://www.w3.org/TR/xml/#sec-lang-tag
- https://docs.rs/language-tags/0.3.2/language_tags/struct.LanguageTag.html
