"""Writes the FictionBook fixtures into OUT_DIR (fixtures/fb2): a UTF-8 book with every structure
the importer maps (authors, series, cover, parts, chapters, epigraph, poem, table, images, notes,
links), the same book zipped, a short windows-1251 book, and truncated XML. Standard library only;
the output is deterministic."""
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

def b64(data):
    s = base64.b64encode(data).decode()
    return "\n".join(s[i:i + 76] for i in range(0, len(s), 76))

para = ("The press was cold that morning, and the type sat in its cases like rain waiting to fall. "
        "Nobody in the shop said a word about the proofs, which were late again.")

mini = f"""<?xml version="1.0" encoding="utf-8"?>
<FictionBook xmlns="http://www.gribuser.ru/xml/fictionbook/2.0" xmlns:l="http://www.w3.org/1999/xlink">
 <description>
  <title-info>
   <genre>sf_fantasy</genre>
   <genre>prose_classic</genre>
   <author><first-name>Ada</first-name><middle-name>K.</middle-name><last-name>Testwright</last-name></author>
   <author><nickname>Typesetter</nickname></author>
   <book-title>The Deckle FB2 Mini</book-title>
   <annotation><p>A small book about a print shop.</p><p>Second paragraph &amp; more.</p></annotation>
   <coverpage><image l:href="#cover.png"/></coverpage>
   <lang>en</lang>
   <sequence name="Deckle Fixtures" number="2"/>
  </title-info>
  <document-info><id>deckle-fb2-mini</id><version>1.0</version></document-info>
 </description>
 <body>
  <title><p>The Deckle FB2 Mini</p><p>A Novel</p></title>
  <epigraph><p>Ink is patient.</p><text-author>Old proverb</text-author></epigraph>
  <section id="part1">
   <title><p>Part One</p></title>
   <section id="ch1">
    <title><p>Chapter 1</p><p>The Cold Press</p></title>
    <p>{para}</p>
    <p>It was <emphasis>very</emphasis> cold, and <strong>nobody</strong> spoke.<a l:href="#n1" type="note">[1]</a></p>
    <empty-line/>
    <image l:href="#plate.png" alt="A grey plate"/>
    <p>See <a l:href="#ch3">the third chapter</a> or <a l:href="https://example.com/press">the press site</a>.</p>
   </section>
   <section id="ch2">
    <title><p>Chapter 2</p></title>
    <poem>
     <title><p>The Compositor's Song</p></title>
     <stanza><v>Set the line and lock the forme,</v><v>Ink the roller, keep it warm.</v></stanza>
     <stanza><v>Pull the sheet and hang it dry,</v><v>Let the morning paper fly.</v></stanza>
     <text-author>Anonymous</text-author>
    </poem>
    <p>A dot<image l:href="#dot.gif"/> in the middle of a line.<a l:href="#n2">[2]</a></p>
   </section>
  </section>
  <section>
   <title><p>Part Two</p></title>
   <section id="ch3">
    <title><p>Chapter 3</p></title>
    <subtitle>* * *</subtitle>
    <table>
     <tr><th>Face</th><th>Size</th></tr>
     <tr><td>Caslon</td><td align="right">12</td></tr>
     <tr><td colspan="2">Garamond, all sizes</td></tr>
    </table>
    <cite><p>Quoted from the foreman's notebook.</p></cite>
    <p>H<sub>2</sub>O and E=mc<sup>2</sup>, <strikethrough>struck</strikethrough>, <code>lock()</code>.</p>
   </section>
   <section id="ch4">
    <title><p>Chapter 4</p></title>
    <p>{para}</p>
    <p>The end.</p>
   </section>
  </section>
 </body>
 <body name="notes">
  <title><p>Notes</p></title>
  <section id="n1"><title><p>1</p></title><p>Colder than the previous winter.</p></section>
  <section id="n2"><title><p>2</p></title><p>A full stop, cast in lead.</p></section>
 </body>
 <binary id="cover.png" content-type="image/png">{b64(png(60, 90, (40, 60, 90)))}</binary>
 <binary id="plate.png" content-type="image/jpeg">{b64(png(8, 8, (128, 128, 128)))}</binary>
 <binary id="dot.gif" content-type="image/gif">{b64(GIF)}</binary>
 <binary id="unused.png" content-type="image/png">{b64(png(2, 2, (0, 0, 0)))}</binary>
</FictionBook>
"""

with open(os.path.join(out, "mini.fb2"), "w", encoding="utf-8", newline="\n") as f:
    f.write(mini)

info = zipfile.ZipInfo("mini.fb2", date_time=(2026, 10, 25, 0, 0, 0))
info.compress_type = zipfile.ZIP_DEFLATED
with zipfile.ZipFile(os.path.join(out, "mini.fb2.zip"), "w") as z:
    z.writestr(info, mini.encode("utf-8"))

ru = """<?xml version="1.0" encoding="windows-1251"?>
<FictionBook xmlns="http://www.gribuser.ru/xml/fictionbook/2.0" xmlns:l="http://www.w3.org/1999/xlink">
 <description><title-info>
  <genre>prose_rus_classic</genre>
  <author><first-name>Иван</first-name><last-name>Наборщиков</last-name></author>
  <book-title>Типография</book-title>
  <lang>ru</lang>
 </title-info></description>
 <body>
  <section><title><p>Глава первая</p></title><p>Утро в типографии было холодным.</p></section>
  <section><title><p>Глава вторая</p></title><p>Краска терпелива.</p></section>
 </body>
</FictionBook>
"""
with open(os.path.join(out, "cp1251.fb2"), "wb") as f:
    f.write(ru.encode("windows-1251"))

with open(os.path.join(out, "broken.fb2"), "w", encoding="utf-8", newline="\n") as f:
    f.write(mini[:mini.index("<section id=\"ch2\">") + 40])
