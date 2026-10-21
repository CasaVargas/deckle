"""Writes the pop-up notes fixtures (#31) into a directory: a semantic EPUB 3, a Calibre-style
EPUB 2, an InDesign-style endnotes chapter, a Pandoc `section.footnotes` EPUB, and a trap EPUB
whose exponents, numbered lists and one-way links must not change. Standard library only;
the zips are deterministic, so regenerating gives the same bytes."""
import os, sys, zipfile

out_dir = sys.argv[1]
os.makedirs(out_dir, exist_ok=True)
STAMP = (2026, 10, 21, 0, 0, 0)
LOREM = "The press was cold that morning, and the type sat in its cases like rain waiting to fall."


def put(z, name, data, stored=False):
    info = zipfile.ZipInfo(name, STAMP)
    info.compress_type = zipfile.ZIP_STORED if stored else zipfile.ZIP_DEFLATED
    info.external_attr = 0o644 << 16
    z.writestr(info, data)


def xhtml(title, body, epub_ns=True):
    ns = ' xmlns:epub="http://www.idpf.org/2007/ops"' if epub_ns else ""
    return (
        '<?xml version="1.0" encoding="utf-8"?>\n'
        f'<html xmlns="http://www.w3.org/1999/xhtml"{ns} xml:lang="en"><head><title>{title}</title></head>\n'
        f"<body>\n{body}\n</body></html>\n"
    )


def book(name, title, chapters, version="3.0"):
    """chapters: [(href, title, body, epub_ns)] in spine order."""
    path = os.path.join(out_dir, name)
    z = zipfile.ZipFile(path, "w")
    put(z, "mimetype", "application/epub+zip", stored=True)
    put(z, "META-INF/container.xml", '<?xml version="1.0"?><container version="1.0" xmlns="urn:oasis:names:tc:opendocument:xmlns:container"><rootfiles><rootfile full-path="OEBPS/content.opf" media-type="application/oebps-package+xml"/></rootfiles></container>')
    items = "".join(f'<item id="c{i}" href="{h}" media-type="application/xhtml+xml"/>' for i, (h, *_ ) in enumerate(chapters))
    spine = "".join(f'<itemref idref="c{i}"/>' for i in range(len(chapters)))
    uid = f"urn:uuid:7b0d3f1e-4c2a-4e8b-9c6d-{name.split('.')[0][:12]:0>12}"
    if version == "3.0":
        nav = "".join(f'<li><a href="{h}">{t}</a></li>' for h, t, *_ in chapters)
        put(z, "OEBPS/nav.xhtml", xhtml("Contents", f'<nav epub:type="toc" id="toc"><h1>Contents</h1><ol>{nav}</ol></nav>'))
        items = '<item id="nav" href="nav.xhtml" media-type="application/xhtml+xml" properties="nav"/>' + items
        meta = '<meta property="dcterms:modified">2026-10-21T00:00:00Z</meta>'
        toc = ""
    else:
        points = "".join(f'<navPoint id="p{i}" playOrder="{i + 1}"><navLabel><text>{t}</text></navLabel><content src="{h}"/></navPoint>' for i, (h, t, *_) in enumerate(chapters))
        put(z, "OEBPS/toc.ncx", f'<?xml version="1.0" encoding="utf-8"?><ncx xmlns="http://www.daisy.org/z3986/2005/ncx/" version="2005-1"><head><meta name="dtb:uid" content="{uid}"/></head><docTitle><text>{title}</text></docTitle><navMap>{points}</navMap></ncx>')
        items = '<item id="ncx" href="toc.ncx" media-type="application/x-dtbncx+xml"/>' + items
        meta = ""
        toc = ' toc="ncx"'
    put(z, "OEBPS/content.opf", f'<?xml version="1.0" encoding="utf-8"?><package xmlns="http://www.idpf.org/2007/opf" version="{version}" unique-identifier="uid"><metadata xmlns:dc="http://purl.org/dc/elements/1.1/"><dc:identifier id="uid">{uid}</dc:identifier><dc:title>{title}</dc:title><dc:creator>Jane Testwright</dc:creator><dc:language>en</dc:language>{meta}</metadata><manifest>{items}</manifest><spine{toc}>{spine}</spine></package>')
    for h, t, body, epub_ns in chapters:
        put(z, f"OEBPS/{h}", xhtml(t, body, epub_ns))
    z.close()


# 1. Semantic EPUB 3: marked noterefs and notes in forms the Kindle doesn't pop up yet, plus
#    one pair already in the target form (left alone, not counted).
book("semantic.epub", "Semantic Notes", [
    ("ch1.xhtml", "Chapter One", f"""<h1 id="top">Chapter One</h1>
<p>{LOREM}<a epub:type="noteref" href="#n1" id="r1">1</a> And again.</p>
<p>{LOREM}<sup><a role="doc-noteref" href="notes.xhtml#e1" id="r2">2</a></sup></p>
<p>{LOREM}<sup><a epub:type="noteref" href="#n3">*</a></sup></p>
<p>{LOREM}<sup><a epub:type="noteref" href="#n4" id="r4">4</a></sup></p>
<div epub:type="footnote" id="n1"><p><a href="#r1">1</a> The first note.</p></div>
<p epub:type="footnote" id="n3">* A starred note with no way back.</p>
<aside epub:type="footnote" id="n4"><p><a href="#r4">4</a> Already a pop-up.</p></aside>""", True),
    ("notes.xhtml", "Notes", """<section role="doc-endnotes"><h1 id="notes-top">Notes</h1>
<ol start="2"><li role="doc-endnote" id="e1"><p>Endnote two, for the second claim. <a href="ch1.xhtml#r2">Back</a></p></li></ol>
</section>""", True),
])

# 2. Calibre-style EPUB 2: no epub namespace, `footnote-link` refs with a `↩` backlink at the
#    end of the chapter; note 1 is cited twice; ref 3's id is on its wrapping span.
book("calibre.epub", "Calibre Notes", [
    ("ch1.xhtml", "Chapter One", f"""<h1 class="calibre1">Chapter One</h1>
<p class="calibre2">{LOREM}<a class="footnote-link" href="#fn1" id="fnref1"><sup>1</sup></a></p>
<p class="calibre2">{LOREM}<a class="footnote-link" href="#fn2" id="fnref2"><sup>[2]</sup></a></p>
<p class="calibre2">{LOREM}<span id="s3"><a class="footnote-link" href="#fn3"><sup>3</sup></a></span></p>
<p class="calibre2">{LOREM} As noted before.<a class="footnote-link" href="#fn1" id="fnref1b"><sup>1</sup></a></p>
<hr class="calibre3"/>
<p class="calibre4" id="fn1"><a href="#fnref1">↩</a> First note, cited twice.</p>
<p class="calibre4" id="fn2"><a href="#fnref2">↩</a> Second note.</p>
<div class="calibre5" id="fn3"><p class="calibre4"><a href="#s3">↩</a> Third note, in two</p><p class="calibre4">paragraphs.</p></div>""", False),
], version="2.0")

# 3. InDesign-style: refs in two chapters to an endnotes chapter whose anchors link back with
#    the same digit; the Notes chapter has a heading other links point at, and an appendix
#    paragraph one ref reaches without a way back.
def indesign_ref(n, ch):
    return f'<span class="_idGenCharOverride-1"><a class="_idFootnoteLink _idGenColorInherit" href="notes.xhtml#footnote-00{n}" id="footnote-00{n}-backlink">{n}</a></span>'

def indesign_note(n, ch, text):
    return f'<div class="_idFootnote" id="footnote-00{n}"><p class="Endnote-Text"><span class="_idGenCharOverride-1"><a class="_idFootnoteAnchor _idGenColorInherit" href="{ch}#footnote-00{n}-backlink">{n}</a></span>\t{text}</p></div>'

book("indesign.epub", "InDesign Endnotes", [
    ("ch1.xhtml", "Chapter One", f"""<h1 class="Heading">Chapter One</h1>
<p class="Body">{LOREM}{indesign_ref(1, 'ch1.xhtml')}</p>
<p class="Body">{LOREM}{indesign_ref(2, 'ch1.xhtml')} See the <a href="notes.xhtml#notes-top">Notes</a>.</p>""", True),
    ("ch2.xhtml", "Chapter Two", f"""<h1 class="Heading">Chapter Two</h1>
<p class="Body">{LOREM}{indesign_ref(3, 'ch2.xhtml')}</p>
<p class="Body">{LOREM}<sup><a class="_idFootnoteLink" href="notes.xhtml#p-extra">4</a></sup></p>""", True),
    ("notes.xhtml", "Notes", f"""<h1 class="Heading" id="notes-top">Notes</h1>
<div class="_idFootnotes">
{indesign_note(1, 'ch1.xhtml', 'The first endnote.')}
{indesign_note(2, 'ch1.xhtml', 'The second endnote.')}
{indesign_note(3, 'ch2.xhtml', 'The third endnote.')}
</div>
<section class="Appendix"><p class="Body" id="p-extra">An appendix paragraph, not a note.</p></section>""", True),
])

# 4. Pandoc-style: refs with `footnote-ref`, notes in an ordered list under section.footnotes
#    with `↩︎` backlinks; the list numbers the notes.
book("pandoc.epub", "Pandoc Notes", [
    ("ch1.xhtml", "Chapter One", f"""<section id="chapter-one" class="level1"><h1>Chapter One</h1>
<p>{LOREM}<a href="#fn1" class="footnote-ref" id="fnref1"><sup>1</sup></a></p>
<p>{LOREM}<a href="#fn2" class="footnote-ref" id="fnref2"><sup>2</sup></a></p>
<p>{LOREM}<a href="#fn3" class="footnote-ref" id="fnref3"><sup>3</sup></a></p>
</section>
<section class="footnotes" id="footnotes">
<hr />
<ol>
<li id="fn1"><p>Pandoc note one.<a href="#fnref1" class="footnote-back">↩︎</a></p></li>
<li id="fn2"><p>Pandoc note two.<a href="#fnref2" class="footnote-back">↩︎</a></p></li>
<li id="fn3"><p>Pandoc note three.<a href="#fnref3" class="footnote-back">↩︎</a></p></li>
</ol>
</section>""", False),
])

# 5. Trap: exponents, numbered lists and digit links that are not notes. Nothing may change.
book("trap.epub", "Not Notes", [
    ("ch1.xhtml", "Chapter One", f"""<h1 id="s1">Chapter One</h1>
<p>Einstein wrote E = mc<sup>2</sup>, and the area is x<sup>2</sup> + y<sup>2</sup>.</p>
<ol><li>First step.</li><li>Second step.</li><li>Third step.</li></ol>
<p>Jump to section <a href="#s1">1</a> or <a href="#s2">2</a>.</p>
<p>{LOREM} As figure<sup><a href="#fig2">2</a></sup> shows.</p>
<p class="caption" id="fig2">Figure 2. A press, with type in its cases.</p>
<p>The table<a class="ref" href="#tbl1">3</a> lists faces.</p>
<table id="tbl1"><tr><td>Caslon</td></tr></table>
<h2 id="s2">Section Two</h2>
<p>{LOREM}</p>""", True),
])
