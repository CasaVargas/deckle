"""Writes the RTF fixtures into OUT_DIR (fixtures/rtf): a document exercising every mapping the
importer makes (info, stylesheet headings, character formats, code-page and Unicode text, lists,
a table with merges, pictures as hex and \\bin, fields, bookmarks, notes), a Cyrillic
\\ansicpg1251 document, a Japanese double-byte document, and one missing its final brace.
Standard library only; the output is deterministic. JPEG is the one format the standard library
can't write, so an 8x8 grey JPEG (made once with macOS `sips`) is embedded as hex."""
import os, struct, sys, zlib
out = sys.argv[1]
os.makedirs(out, exist_ok=True)

def png(w, h, rgb):
    def chunk(kind, data):
        c = kind + data
        return struct.pack(">I", len(data)) + c + struct.pack(">I", zlib.crc32(c) & 0xffffffff)
    raw = b"".join(b"\x00" + bytes(rgb) * w for _ in range(h))
    return (b"\x89PNG\r\n\x1a\n" + chunk(b"IHDR", struct.pack(">IIBBBBB", w, h, 8, 2, 0, 0, 0))
            + chunk(b"IDAT", zlib.compress(raw, 9)) + chunk(b"IEND", b""))

JPEG = bytes.fromhex(
    "ffd8ffe000104a46494600010100004800480000ffe1004c4578696600004d4d002a00000008000187690004"
    "000000010000001a000000000003a00100030000000100010000a00200040000000100000008a00300040000"
    "00010000000800000000ffed003850686f746f73686f7020332e30003842494d04040000000000003842494d"
    "0425000000000010d41d8cd98f00b204e9800998ecf8427effc00011080008000803012200021101031101ff"
    "c4001f0000010501010101010100000000000000000102030405060708090a0bffc400b51000020103030204"
    "03050504040000017d01020300041105122131410613516107227114328191a1082342b1c11552d1f0243362"
    "7282090a161718191a25262728292a3435363738393a434445464748494a535455565758595a636465666768"
    "696a737475767778797a838485868788898a92939495969798999aa2a3a4a5a6a7a8a9aab2b3b4b5b6b7b8b9"
    "bac2c3c4c5c6c7c8c9cad2d3d4d5d6d7d8d9dae1e2e3e4e5e6e7e8e9eaf1f2f3f4f5f6f7f8f9faffc4001f01"
    "00030101010101010101010000000000000102030405060708090a0bffc400b5110002010204040304070504"
    "0400010277000102031104052131061241510761711322328108144291a1b1c109233352f0156272d10a1624"
    "34e125f11718191a262728292a35363738393a434445464748494a535455565758595a636465666768696a73"
    "7475767778797a82838485868788898a92939495969798999aa2a3a4a5a6a7a8a9aab2b3b4b5b6b7b8b9bac2"
    "c3c4c5c6c7c8c9cad2d3d4d5d6d7d8d9dae2e3e4e5e6e7e8e9eaf2f3f4f5f6f7f8f9faffdb00430006060606"
    "06060a06060a0e0a0a0a0e120e0e0e0e12171212121212171c1717171717171c1c1c1c1c1c1c1c2222222222"
    "2227272727272c2c2c2c2c2c2c2c2c2cffdb0043010707070b0a0b130a0a132e1f1a1f2e2e2e2e2e2e2e2e2e"
    "2e2e2e2e2e2e2e2e2e2e2e2e2e2e2e2e2e2e2e2e2e2e2e2e2e2e2e2e2e2e2e2e2e2e2e2e2e2e2e2e2effdd00"
    "040001ffda000c03010002110311003f0028a28a00ffd9"
)

def hexlines(data):
    h = data.hex()
    return "\n".join(h[i:i + 64] for i in range(0, len(h), 64))

dot = png(1, 1, (0, 0, 0))

head = r"""{\rtf1\ansi\ansicpg1252\deff0\deflang2057
{\fonttbl{\f0\froman\fcharset0 Times New Roman;}{\f1\fswiss\fcharset0 Arial;}}
{\colortbl;\red0\green0\blue0;}
{\stylesheet{\ql Normal;}{\s1\ql\b\fs32 heading 1;}{\s2\b heading 2;}{\s3\b heading 3;}{\s4\b heading 4;}{\s15\qc\b\fs48 Title;}{\s16 Quote;}{\*\cs10 Default Paragraph Font;}}
{\*\listtable{\list\listtemplateid1{\listlevel\levelnfc0\levelstartat3{\leveltext\'02\'00.;}{\levelnumbers\'01;}}{\listlevel\levelnfc23\levelstartat1{\leveltext\'01\u8226 ?;}{\levelnumbers;}}\listid100}}
{\*\listoverridetable{\listoverride\listid100\listoverridecount0\ls1}}
{\info{\title The Deckle RTF Mini}{\author Ada Testwright}{\subject A small document about a print shop.}{\keywords printing, type; presses}{\revtim\yr2026\mo10\dy25\hr9\min30}}
{\header\pard\plain HEADER-SENTINEL\par}
"""

body = r"""\pard\plain\s15\qc\b\fs48 The Deckle RTF Mini\par
\pard\plain\s1\b\fs32 Chapter One\par
\pard\plain Plain {\b bold} {\i italic} {\ul under} {\strike struck} E=mc{\super 2} H{\sub 2}O {\scaps caps} {\b\ulw loud}{\v HIDDEN-SENTINEL}{\deleted DELETED-SENTINEL}.\par
\pard\plain Caf\'e9 \u8364?uro \lquote single\rquote  \ldblquote double\rdblquote  em\emdash dash en\endash dash\tab tab\line next line{\*\atnid x}{\*\annotation COMMENT-SENTINEL}{\super\chftn}{\footnote\pard\plain{\super\chftn} First footnote.}\par
\pard\plain\qc Centred line\par
\pard\plain\s2\b Section 1.1\par
\pard\plain\s3\b Subsection\par
\pard\plain\s4\b Deep heading\par
{\listtext 3.\tab}\pard\plain\ls1\ilvl0 First\par
{\listtext 4.\tab}\pard\plain\ls1\ilvl0 Second\par
{\listtext \'95\tab}\pard\plain\ls1\ilvl1 Nested a\par
{\listtext \'95\tab}\pard\plain\ls1\ilvl1 Nested b\par
{\listtext 5.\tab}\pard\plain\ls1\ilvl0 Third\par
\pard\plain\s16 A quoted passage.\par
\pard\plain\outlinelevel0\b Chapter Two\par
\pard\plain A dot {\*\shppict{\pict{\*\picprop{\sp{\sn wzDescription}{\sv Inline dot}}}\pngblip\picw1\pich1
DOT_HEX}}{\nonshppict{\pict\wmetafile8 010203}} inline.\par
\pard\plain {\pict\jpegblip\picw8\pich8\binJPEG_LEN JPEG_RAW}\par
\pard\plain {\*\bkmkstart Plate One}The plate above.{\*\bkmkend Plate One}\par
\trowd\trhdr\cellx3000\cellx6000\pard\plain\intbl Face\cell Size\cell\row
\trowd\cellx3000\cellx6000\pard\plain\intbl Caslon\cell 12\cell\row
\trowd\clmgf\cellx3000\clmrg\cellx6000\pard\plain\intbl Garamond, all sizes\cell\cell\row
\pard\plain See {\field{\*\fldinst {HYPERLINK \\l "Plate One"}}{\fldrslt {the plate}}} and {\field{\*\fldinst HYPERLINK "https://example.com/press"}{\fldrslt {\ul the press}}}, page {\field{\*\fldinst PAGE}{\fldrslt 7}}.{\footnote\pard\plain Second footnote.}{\footnote\ftnalt\pard\plain An endnote.}\par
{\field{\*\fldinst TOC \\o "1-3"}{\fldrslt STORED-TOC-SENTINEL\par}}
}
"""

doc = (head + body).encode("ascii")
doc = doc.replace(b"DOT_HEX", hexlines(dot).encode())
doc = doc.replace(b"JPEG_LEN", str(len(JPEG)).encode())
doc = doc.replace(b"JPEG_RAW", JPEG)
with open(os.path.join(out, "mini.rtf"), "wb") as f:
    f.write(doc)

def cp(text, page):
    return "".join(f"\\'{b:02x}" for b in text.encode(page))

cyr = ("{\\rtf1\\ansi\\ansicpg1251\\deff0\\deflang1049\n"
       "{\\fonttbl{\\f0\\froman\\fcharset204 Times New Roman;}}\n"
       "{\\info{\\author " + cp("Иван Наборщиков", "cp1251") + "}}\n"
       "\\pard\\plain " + cp("Утро в типографии было холодным.", "cp1251") + "\\par\n}\n")
with open(os.path.join(out, "cyrillic.rtf"), "w", encoding="ascii", newline="\n") as f:
    f.write(cyr)

ja = ("{\\rtf1\\ansi\\ansicpg1252\\deff0\\deflang1041\n"
      "{\\fonttbl{\\f0\\froman\\fcharset0 Times;}{\\f1\\fnil\\fcharset128 MS Mincho;}}\n"
      "\\pard\\plain Latin, then {\\f1 " + cp("活版印刷", "shift_jis") + "}.\\par\n}\n")
with open(os.path.join(out, "japanese.rtf"), "w", encoding="ascii", newline="\n") as f:
    f.write(ja)

with open(os.path.join(out, "unclosed.rtf"), "w", encoding="ascii", newline="\n") as f:
    f.write("{\\rtf1\\ansi\\deff0\n\\pard Still {\\b readable}\\par\n")
