# Markdown and notes folders

Add a `.md` or `.markdown` file with **Add books**, or choose **Add notes folder** to make one
book from a flat folder. You can also drop the file or folder on the Books tab.

```sh
deckle run essay.md --to device
deckle run ./Travel-notes --to device
```

Each note becomes a chapter, ordered by filename (ignoring case). Use names such as `01 Arrival.md`
and `02 The walk.md` to choose the order. Hidden files and nested notes are not imported. A
folder with no top-level Markdown keeps the desktop's usual behavior of adding individual books.

Headings, bold and italic text, strikethrough, lists, quotes, code blocks, tables and local PNG,
JPEG and GIF images are supported. H1 and H2 headings appear in the contents. Images retain alt
text and can be in subfolders, such as `![The view](images/view.jpg)`. Remote images aren't fetched.

Links between included notes work as `[[02 The walk]]`, `[[02 The walk#The bridge|the bridge]]`,
or `[the walk](02%20The%20walk.md)`. Heading links also work within a note. Duplicate headings get
suffixes such as `the-bridge-2`. Missing or ambiguous links remain readable text and produce a
warning. Image embeds such as `![[images/view.jpg]]` work; embedding whole notes is not supported.

Optional YAML frontmatter sets metadata:

```yaml
---
title: A walk by the river
authors:
  - Ada Testwright
language: en
---
```

A single file uses its frontmatter title, first H1, or filename. A folder uses its folder name
as the book title and collects its notes' authors. `author` is also accepted as a single string.
The first language declared in the book applies throughout; the default is English (`en`).

The library saves an EPUB snapshot containing the notes and images, so it can re-send the book
after the original folder moves. To include edits, add the original notes again. Keep the book
title and authors unchanged to update the same library book.

Raw HTML appears as text with a warning. Obsidian plugins, note transclusion, footnotes and full
vault behavior aren't supported. Files must be UTF-8. The limit is 1000 notes and 50 MB total,
including images; a single image can be up to 10 MB.
