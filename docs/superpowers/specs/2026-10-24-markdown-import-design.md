# Markdown files and notes folders → book (#23)

A `.md` or `.markdown` file, or a flat folder of notes, becomes EPUB 3 in the existing import
stage. The CLI, preflight, bind and reader delivery share that stage. The desktop accepts files
through its picker and drag-and-drop and has an Add notes folder picker. Dropping a folder with
visible top-level Markdown files makes one book; other folders retain the existing book walk.

One note is one spine chapter. Folder chapters sort by case-folded filename, exact spelling as
tie-breaker; numeric prefixes give an explicit order. Hidden files, nested notes and symlinked
folder entries are excluded. There is no recursive vault import. Nested folders can hold images.
An empty folder or empty note reports an error. Inputs are UTF-8, at most 1000 notes and 50 MB
including packaged images; individual images are at most 10 MB.

CommonMark with tables, strikethrough and wikilinks is rendered by pulldown-cmark. Headings,
emphasis, lists, blockquotes, fenced code and local PNG/JPEG/GIF images are preserved. Every
H1/H2 appears in the contents beneath its note; H3–H6 stay in the body. Heading IDs are derived
from their text, with numeric suffixes for duplicates. Raw HTML displays as text with a warning.

YAML frontmatter supports `title`, `author`, `authors` (a list), and `language`. Unknown keys warn;
malformed frontmatter fails. A single file's title is frontmatter title, first H1, then filename.
A folder's title is its folder name; authors are the unique authors of its notes, and its language
is the first declared language, defaulting to `en`. Each note's title follows the single-file rule.
The stable book identifier hashes title and authors so editing the body retains its identity.

`[[Note]]`, `[[Note#Heading|label]]`, ordinary links to included `.md` files, and local `#heading`
links resolve within the selected set by filename or stem. Matching note names ignores case;
ambiguous names, missing notes or missing headings leave the label as text and issue a warning.
HTTP(S) and mailto links remain links. Other schemes and paths outside the selected set are
unlinked. Images are packaged once, retain their alt text, and must resolve within the source
folder, including after resolving symlinks. Nothing is fetched from the network. Missing,
unsupported or oversized images retain alt text and warn. Note embeds are unsupported and warn;
image embeds use the same image rules. Callout syntax is ordinary blockquote text. Full vault
semantics, plugins, transclusions, executable HTML, footnotes and remote images are out of scope.

The library stores a self-contained EPUB snapshot rather than copying Markdown without its
assets. Reimporting edited notes updates the same book; re-sending the saved library copy does
not require the original notes folder. Source files and images are left untouched by import.

Tests cover XML validity and link/manifest targets, metadata, headings, formatting, image
packaging, determinism, folder ordering, unresolved/ambiguous links, boundary checks, source
preservation, library snapshots, desktop path expansion, and the import/CLI pipeline with the
fake Previewer. Both fixtures also passed the opt-in `markdown_real_previewer_smoke` test
with Kindle Previewer 4 on macOS on 2026-10-24. Reading on physical hardware remains untested.

Parser syntax reference: https://pulldown-cmark.github.io/pulldown-cmark/specs/wikilinks.html
