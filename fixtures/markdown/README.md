# Markdown import fixtures

`mini.md` covers frontmatter, headings, inline formatting, lists, a table, code, links and an
image. `images/square.png` is a generated 1×1 blue RGB PNG, with no external source or licence.

`notes/` contains three chapters with wikilinks, ordinary relative links, heading aliases,
duplicate headings and one deliberately missing target. The importer tests verify all EPUB XML,
manifest resources and local link targets. The pipeline also has an opt-in real Previewer test:

```sh
cargo test -p deckle-core --test pipeline markdown_real_previewer_smoke -- --ignored
```

That test passed for both inputs with Kindle Previewer 4 on macOS on 2026-10-24.
