# Per-book style presets (#33)

## Scope

Three built-in presets cover the issue's fonts, spacing, notes and enhancement stages.
No user-editable preset language or editor is introduced. The existing typography and
pop-up footnote passes do the text and note transformations.

| Preset | Body family | Line height | Paragraph gap / indent | Typography | Pop-up notes | X-Ray |
| --- | --- | --- | --- | --- | --- | --- |
| Academic paper | serif | 1.45 | 0.6em / none | on | on | off |
| Novel | serif | 1.35 | none / 1.2em | on | on | auto |
| Manual | sans-serif | 1.4 | 0.7em / none | off | off | off |

All presets enable automatic page numbers and disable Word Wise by default, avoiding
an implicit dictionary download. X-Ray still requires an eligible EBOK and supported
language. Cover rules, CDE overrides, metadata lookup, LLM and Wikipedia permissions
remain governed by their existing settings. Selecting a preset does not enable a
network permission. Manual preserves existing note markup; it does not undo pop-up
semantics already in a source.

## Styling

Normalize appends a marked inline stylesheet to each XHTML spine document's head,
after publisher styles. It uses generic font families, relative spacing, and targeted
paragraph/list overrides, with no embedded fonts or fixed text size. Code remains
monospace; paragraphs within code, quotes, lists and tables do not gain novel indents.
The reader's font controls and rendering engine can affect the final appearance.
Publisher CSS and content remain intact. Reprocessing replaces Deckle's marked style
instead of accumulating copies. Fixed-layout packages skip style injection with a warning;
this is not a fixed-layout conversion feature. Missing or invalid XHTML is reported.

## Selection and precedence

CLI: `deckle run book.epub --preset academic`. Without a preset all existing defaults
remain. Explicit `--typography[=false]`, `--popup-notes[=false]`, `--pages auto|off|N`,
`--xray auto|off` and `--word-wise auto|off` override preset stage defaults. Presets
apply to Kindle conversion and KFX folder export; the direct Kobo/PocketBook route
rejects a preset rather than silently ignoring it.

Desktop: Book options → Book style. Choices are stored in the existing settings JSON
as `book_presets: {"/absolute/source/path.epub": "novel"}`. Old settings default to
an empty map. The original-style choice removes that entry. Preview and conversion
both resolve the same saved choice, overriding global stage settings for that book.
A save must succeed before the UI shows the choice or permits conversion. Choices
are local to this source path: moving a file, importing a separate library copy, or
using a different computer requires choosing again. KFX files sent as-is cannot be styled.

Inputs imported as reflowable EPUB (including Markdown, DOCX and text PDFs) use the
same pass. This does not add OCR, scanned-PDF support, or new library storage behavior.

## Verification

Tests cover default output without styling, all styles on spine content, namespace-safe
insertion, repeated normalization, explicit CLI overrides, invalid names, persisted
settings and the preflight's actual CSS. Existing pipeline and desktop tests guard
behavior when no preset is selected. Real-device appearance remains a hardware check.
