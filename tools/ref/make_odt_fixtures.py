"""Writes the OpenDocument fixtures into OUT_DIR (fixtures/odt): a text document exercising every
mapping the importer makes (meta, inherited styles, headings, lists, a table, images, notes,
bookmarks) as zipped .odt and flat .fodt, plus a spreadsheet and a password-protected stub.
Standard library only; the output is deterministic."""
import base64, os, struct, sys, zipfile, zlib
out = sys.argv[1]
os.makedirs(out, exist_ok=True)

def png(w, h, rgb):
    def chunk(kind, data):
        c = kind + data
        return struct.pack(">I", len(data)) + c + struct.pack(">I", zlib.crc32(c) & 0xffffffff)
    raw = b"".join(b"\x00" + bytes(rgb) * w for _ in range(h))
    return (b"\x89PNG\r\n\x1a\n" + chunk(b"IHDR", struct.pack(">IIBBBBB", w, h, 8, 2, 0, 0, 0))
            + chunk(b"IDAT", zlib.compress(raw, 9)) + chunk(b"IEND", b""))

GIF = (b"GIF89a\x01\x00\x01\x00\x80\x00\x00\x00\x00\x00\xff\xff\xff!\xf9\x04\x01\x00\x00\x00\x00"
       b",\x00\x00\x00\x00\x01\x00\x01\x00\x00\x02\x02D\x01\x00;")
PICTURES = {"Pictures/plate.png": png(8, 8, (128, 128, 128)), "Pictures/dot.gif": GIF}

NS = ('xmlns:office="urn:oasis:names:tc:opendocument:xmlns:office:1.0" '
      'xmlns:style="urn:oasis:names:tc:opendocument:xmlns:style:1.0" '
      'xmlns:text="urn:oasis:names:tc:opendocument:xmlns:text:1.0" '
      'xmlns:table="urn:oasis:names:tc:opendocument:xmlns:table:1.0" '
      'xmlns:draw="urn:oasis:names:tc:opendocument:xmlns:drawing:1.0" '
      'xmlns:fo="urn:oasis:names:tc:opendocument:xmlns:xsl-fo-compatible:1.0" '
      'xmlns:xlink="http://www.w3.org/1999/xlink" '
      'xmlns:dc="http://purl.org/dc/elements/1.1/" '
      'xmlns:meta="urn:oasis:names:tc:opendocument:xmlns:meta:1.0" '
      'xmlns:svg="urn:oasis:names:tc:opendocument:xmlns:svg-compatible:1.0" '
      'office:version="1.3"')

META = """<office:meta>
  <dc:title>The Deckle ODT Mini</dc:title>
  <dc:creator>Ada Testwright</dc:creator>
  <meta:initial-creator>Someone Else</meta:initial-creator>
  <dc:language>en-US</dc:language>
  <dc:description>A small document about a print shop.</dc:description>
  <meta:keyword>printing</meta:keyword>
  <meta:keyword>type</meta:keyword>
  <dc:date>2026-10-25T09:30:12.123456</dc:date>
 </office:meta>"""

def heading_style(n):
    return (f'<style:style style:name="Heading_20_{n}" style:display-name="Heading {n}" '
            f'style:family="paragraph" style:parent-style-name="Heading" '
            f'style:default-outline-level="{n}"/>')

STYLES = f"""<office:styles>
  <style:default-style style:family="paragraph"><style:text-properties fo:language="en" fo:country="GB"/></style:default-style>
  <style:style style:name="Standard" style:family="paragraph"/>
  <style:style style:name="Heading" style:family="paragraph" style:parent-style-name="Standard"><style:text-properties fo:font-weight="bold"/></style:style>
  {"".join(heading_style(n) for n in range(1, 5))}
  <style:style style:name="Title" style:family="paragraph" style:parent-style-name="Heading"/>
  <style:style style:name="Subtitle" style:family="paragraph" style:parent-style-name="Heading"/>
  <style:style style:name="Quotations" style:family="paragraph" style:parent-style-name="Standard"/>
  <style:style style:name="Emphasis" style:family="text"><style:text-properties fo:font-style="italic"/></style:style>
  <style:style style:name="Strong_20_Emphasis" style:display-name="Strong Emphasis" style:family="text"><style:text-properties fo:font-weight="bold"/></style:style>
  <style:style style:name="Loud" style:family="text" style:parent-style-name="Strong_20_Emphasis"><style:text-properties fo:font-variant="small-caps"/></style:style>
 </office:styles>"""

AUTO = """<office:automatic-styles>
  <style:style style:name="P1" style:family="paragraph" style:parent-style-name="Standard"><style:paragraph-properties fo:text-align="center"/></style:style>
  <style:style style:name="T1" style:family="text"><style:text-properties fo:font-weight="bold"/></style:style>
  <style:style style:name="T2" style:family="text"><style:text-properties fo:font-style="italic"/></style:style>
  <style:style style:name="T3" style:family="text"><style:text-properties style:text-underline-style="solid"/></style:style>
  <style:style style:name="T4" style:family="text"><style:text-properties style:text-line-through-style="solid"/></style:style>
  <style:style style:name="T5" style:family="text"><style:text-properties style:text-position="super 58%"/></style:style>
  <style:style style:name="T6" style:family="text"><style:text-properties style:text-position="-33% 58%"/></style:style>
  <style:style style:name="T7" style:family="text"><style:text-properties fo:font-variant="small-caps"/></style:style>
  <text:list-style style:name="L1">
   <text:list-level-style-number text:level="1" style:num-format="1"/>
   <text:list-level-style-bullet text:level="2" text:bullet-char="•"/>
  </text:list-style>
 </office:automatic-styles>"""

def frame(name, anchor, href, alt_tag, alt, flat):
    if flat:
        data = base64.b64encode(PICTURES[href]).decode()
        image = f'<draw:image><office:binary-data>{data}</office:binary-data></draw:image>'
    else:
        image = f'<draw:image xlink:href="{href}" xlink:type="simple"/>'
    return (f'<draw:frame draw:name="{name}" text:anchor-type="{anchor}" svg:width="1cm" svg:height="1cm">'
            f'{image}<svg:{alt_tag}>{alt}</svg:{alt_tag}></draw:frame>')

def body(flat):
    return f"""<office:body><office:text>
  <text:tracked-changes><text:changed-region text:id="ct1"><text:deletion><text:p>DELETED-SENTINEL</text:p></text:deletion></text:changed-region></text:tracked-changes>
  <text:sequence-decls><text:sequence-decl text:display-outline-level="0" text:name="Figure"/></text:sequence-decls>
  <text:table-of-content text:name="Contents"><text:index-body><text:p>STORED-TOC-SENTINEL</text:p></text:index-body></text:table-of-content>
  <text:p text:style-name="Title">The Deckle ODT Mini</text:p>
  <text:p text:style-name="Subtitle">A test document</text:p>
  <text:h text:style-name="Heading_20_1" text:outline-level="1">Chapter One</text:h>
  <text:p text:style-name="Standard">Plain <text:span text:style-name="T1">bold</text:span> <text:span text:style-name="T2">italic</text:span> <text:span text:style-name="T3">under</text:span> <text:span text:style-name="T4">struck</text:span> E=mc<text:span text:style-name="T5">2</text:span> H<text:span text:style-name="T6">2</text:span>O <text:span text:style-name="T7">caps</text:span> <text:span text:style-name="Emphasis">emph</text:span> <text:span text:style-name="Loud">loud</text:span>.</text:p>
  <text:p text:style-name="Standard">Gap<text:s text:c="3"/>here<text:tab/>tab<text:line-break/>next line.<text:note text:id="ftn1" text:note-class="footnote"><text:note-citation>1</text:note-citation><text:note-body><text:p text:style-name="Standard">First footnote.</text:p></text:note-body></text:note><office:annotation><text:p>COMMENT-SENTINEL</text:p></office:annotation></text:p>
  <text:p text:style-name="P1">Centred line</text:p>
  <text:h text:style-name="Heading_20_2" text:outline-level="2">Section 1.1</text:h>
  <text:h text:style-name="Heading_20_3" text:outline-level="3">Subsection</text:h>
  <text:h text:style-name="Heading_20_4" text:outline-level="4">Deep heading</text:h>
  <text:list text:style-name="L1">
   <text:list-item text:start-value="3"><text:p text:style-name="Standard">First</text:p></text:list-item>
   <text:list-item><text:p text:style-name="Standard">Second</text:p>
    <text:list><text:list-item><text:p text:style-name="Standard">Nested a</text:p></text:list-item><text:list-item><text:p text:style-name="Standard">Nested b</text:p></text:list-item></text:list>
   </text:list-item>
   <text:list-item><text:p text:style-name="Standard">Third</text:p></text:list-item>
  </text:list>
  <text:p text:style-name="Quotations">A quoted passage.</text:p>
  <text:soft-page-break/>
  <text:h text:style-name="Heading_20_1" text:outline-level="1">Chapter Two</text:h>
  <text:p text:style-name="Standard">A dot {frame("dot", "as-char", "Pictures/dot.gif", "title", "Inline dot", flat)} inline.</text:p>
  <text:p text:style-name="Standard">{frame("plate", "paragraph", "Pictures/plate.png", "desc", "A grey plate", flat)}</text:p>
  <text:p text:style-name="Standard"><text:bookmark-start text:name="Plate One"/>The plate above.<text:bookmark-end text:name="Plate One"/></text:p>
  <table:table table:name="Faces">
   <table:table-column table:number-columns-repeated="2"/>
   <table:table-header-rows><table:table-row><table:table-cell><text:p>Face</text:p></table:table-cell><table:table-cell><text:p>Size</text:p></table:table-cell></table:table-row></table:table-header-rows>
   <table:table-row><table:table-cell><text:p>Caslon</text:p></table:table-cell><table:table-cell><text:p>12</text:p></table:table-cell></table:table-row>
   <table:table-row><table:table-cell table:number-columns-spanned="2"><text:p>Garamond, all sizes</text:p></table:table-cell><table:covered-table-cell/></table:table-row>
  </table:table>
  <text:p text:style-name="Standard">See <text:a xlink:type="simple" xlink:href="#Plate One">the plate</text:a> and <text:a xlink:type="simple" xlink:href="https://example.com/press">the press</text:a>, page <text:page-number text:select-page="current">7</text:page-number>.<text:note text:id="ftn2" text:note-class="footnote"><text:note-citation>2</text:note-citation><text:note-body><text:p text:style-name="Standard">Second footnote.</text:p></text:note-body></text:note><text:note text:id="edn1" text:note-class="endnote"><text:note-citation>i</text:note-citation><text:note-body><text:p text:style-name="Standard">An endnote.</text:p></text:note-body></text:note></text:p>
 </office:text></office:body>"""

TEXT_MIME = "application/vnd.oasis.opendocument.text"

def manifest(extra=""):
    return ('<?xml version="1.0" encoding="UTF-8"?>\n'
            '<manifest:manifest xmlns:manifest="urn:oasis:names:tc:opendocument:xmlns:manifest:1.0" manifest:version="1.3">'
            f'<manifest:file-entry manifest:full-path="/" manifest:media-type="{TEXT_MIME}"/>'
            f'<manifest:file-entry manifest:full-path="content.xml" manifest:media-type="text/xml">{extra}</manifest:file-entry>'
            '</manifest:manifest>')

def write_zip(path, entries):
    with zipfile.ZipFile(path, "w") as z:
        for name, data in entries:
            info = zipfile.ZipInfo(name, date_time=(2026, 10, 25, 0, 0, 0))
            info.compress_type = zipfile.ZIP_STORED if name == "mimetype" else zipfile.ZIP_DEFLATED
            z.writestr(info, data)

decl = '<?xml version="1.0" encoding="UTF-8"?>\n'
content = f"{decl}<office:document-content {NS}>{AUTO}{body(False)}</office:document-content>"
styles = f"{decl}<office:document-styles {NS}>{STYLES}</office:document-styles>"
meta = f"{decl}<office:document-meta {NS}>{META}</office:document-meta>"
write_zip(os.path.join(out, "mini.odt"), [
    ("mimetype", TEXT_MIME),
    ("META-INF/manifest.xml", manifest()),
    ("content.xml", content),
    ("styles.xml", styles),
    ("meta.xml", meta),
    *sorted(PICTURES.items()),
])

flat = (f'{decl}<office:document {NS} office:mimetype="{TEXT_MIME}">'
        f"{META}{STYLES}{AUTO}{body(True)}</office:document>\n")
with open(os.path.join(out, "mini.fodt"), "w", encoding="utf-8", newline="\n") as f:
    f.write(flat)

write_zip(os.path.join(out, "sheet.ods"), [
    ("mimetype", "application/vnd.oasis.opendocument.spreadsheet"),
    ("content.xml", f"{decl}<office:document-content {NS}><office:body/></office:document-content>"),
])

write_zip(os.path.join(out, "encrypted.odt"), [
    ("mimetype", TEXT_MIME),
    ("META-INF/manifest.xml", manifest('<manifest:encryption-data manifest:checksum-type="SHA1/1K" manifest:checksum="AAAA"/>')),
    ("content.xml", b"\x00\x01\x02 not xml, encrypted"),
])
