# Clippings fixtures

`variants.txt` is a `My Clippings.txt` in the Kindle's own format (UTF-8 BOM, CRLF), one entry per
case the highlights export handles, in this order:

1. A highlight with page and location range (Walden).
2. A note at a location inside that highlight's range.
3. A bookmark (no text; skipped).
4. A title that contains parentheses: `The Count of Monte Cristo (Illustrated) (Alexandre Dumas)`.
5. A book with no author group.
6. An entry with no page field (`at location …`).
7. A PDF entry: page only, no location.
8. The first highlight extended on the Kindle: a later entry with an overlapping range.
9. A Spanish entry (kind and date not recognised).
10. A highlight of two paragraphs.

`variants.walden.md` is the Markdown Deckle writes for Walden from it.
