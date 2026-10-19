import zipfile, sys, struct, zlib
out = sys.argv[1]
def png(w,h,rgb):
    raw=b"".join(b"\x00"+bytes(rgb)*w for _ in range(h))
    def chunk(t,d): return struct.pack(">I",len(d))+t+d+struct.pack(">I",zlib.crc32(t+d)&0xffffffff)
    return b"\x89PNG\r\n\x1a\n"+chunk(b"IHDR",struct.pack(">IIBBBBB",w,h,8,2,0,0,0))+chunk(b"IDAT",zlib.compress(raw))+chunk(b"IEND",b"")
chapters=[("Chapter One","It is a truth universally acknowledged, that a single man in possession of a good fortune, must be in want of a wife. "*40),
          ("Chapter Two","Mr. Bennet was among the earliest of those who waited on Mr. Bingley. He had always intended to visit him. "*40),
          ("Chapter Three","Not all that Mrs. Bennet, however, with the assistance of her five daughters, could ask on the subject. "*40)]
x=lambda t,b: f'<?xml version="1.0" encoding="utf-8"?><html xmlns="http://www.w3.org/1999/xhtml" xmlns:epub="http://www.idpf.org/2007/ops"><head><title>{t}</title><link rel="stylesheet" href="style.css"/></head><body>{b}</body></html>'
z=zipfile.ZipFile(out,"w")
z.writestr(zipfile.ZipInfo("mimetype"),"application/epub+zip",compress_type=zipfile.ZIP_STORED)
z.writestr("META-INF/container.xml",'<?xml version="1.0"?><container version="1.0" xmlns="urn:oasis:names:tc:opendocument:xmlns:container"><rootfiles><rootfile full-path="OEBPS/content.opf" media-type="application/oebps-package+xml"/></rootfiles></container>')
items="".join(f'<item id="c{i}" href="c{i}.xhtml" media-type="application/xhtml+xml"/>' for i in range(3))
spine="".join(f'<itemref idref="c{i}"/>' for i in range(3))
z.writestr("OEBPS/content.opf",f'''<?xml version="1.0" encoding="utf-8"?><package xmlns="http://www.idpf.org/2007/opf" version="3.0" unique-identifier="uid" xml:lang="en"><metadata xmlns:dc="http://purl.org/dc/elements/1.1/"><dc:identifier id="uid">urn:uuid:7b0d3f1e-4c2a-4e8b-9c6d-deckle000001</dc:identifier><dc:title>Deckle Mini Fixture</dc:title><dc:creator id="a1">Jane Testwright</dc:creator><meta refines="#a1" property="file-as">Testwright, Jane</meta><dc:language>en</dc:language><meta property="belongs-to-collection" id="s1">Fixture Series</meta><meta refines="#s1" property="collection-type">series</meta><meta refines="#s1" property="group-position">2</meta><meta property="dcterms:modified">2026-10-18T00:00:00Z</meta></metadata><manifest><item id="nav" href="nav.xhtml" media-type="application/xhtml+xml" properties="nav"/><item id="cover" href="cover.png" media-type="image/png" properties="cover-image"/><item id="css" href="style.css" media-type="text/css"/>{items}</manifest><spine>{spine}</spine></package>''')
nav="".join(f'<li><a href="c{i}.xhtml">{t}</a></li>' for i,(t,_) in enumerate(chapters))
z.writestr("OEBPS/nav.xhtml",x("Contents",f'<nav epub:type="toc" id="toc"><h1>Contents</h1><ol>{nav}</ol></nav>'))
z.writestr("OEBPS/style.css","body{font-family:serif} h1{text-align:center} p{text-indent:1.5em;margin:0} .first{text-indent:0}")
for i,(t,b) in enumerate(chapters):
    paras="".join(f'<p{" class=\"first\"" if j==0 else ""}>{b.strip()}</p>' for j in range(6))
    z.writestr(f"OEBPS/c{i}.xhtml",x(t,f'<h1 id="h{i}">{t}</h1>{paras}'))
z.writestr("OEBPS/cover.png",png(600,900,(40,80,160)))
z.close(); print("wrote",out)
