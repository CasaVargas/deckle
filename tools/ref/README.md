# Reference oracle scripts (dev only, never shipped)

These drive **kfxlib** (GPL-3, John Howell; mirror `github.com/frutadoconde/KFX-Input`) to
produce golden KFX files and dump KPF/KFX internals while porting to Rust. They are not part of
the app, and the app never depends on Python.

Setup (one time):

```sh
git clone --depth 1 https://github.com/frutadoconde/KFX-Input /tmp/KFX-Input   # 57fd6cb3 used for the goldens
python3 -m venv /tmp/kfxvenv && /tmp/kfxvenv/bin/pip install lxml pillow pypdf beautifulsoup4
# Python 3.15: in kfxlib/kpf_container.py change
#   sqlite3.connect(db_filename, KpfContainer.db_timeout)
# to
#   sqlite3.connect(db_filename, timeout=KpfContainer.db_timeout)
```

| Script | Purpose |
|---|---|
| `make_mini_epub.py OUT.epub` | Builds the deterministic mini EPUB fixture |
| `kpf2kfx.py <KFX-Input> IN.kpf OUT.kfx CDE ASIN` | KPF to monolithic KFX with approximate pages; prints fragment type counts |
| `kpf2kfx_opts.py <KFX-Input> IN.kpf OUT.kfx CDE ASIN PAGES\|none` | Same, with page-list control |
| `dump_kpf_raw.py <KFX-Input> IN.kpf` | Pre-fix-up fragment inventory of a KPF (what the Rust KDF reader must produce) |
| `dump_kfx.py <KFX-Input> IN.kfx` | Container header, `$270`/`$419`/`$490`/symbol table, entity order of a KFX |

Run with `python -I` from outside the clone directory.
