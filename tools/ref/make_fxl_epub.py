# A 3-page fixed-layout comic EPUB (1264x1680 grey PNG pages, KCC-style Kindle metadata), the
# input for fixtures/comic/. Usage: python3 -I tools/ref/make_fxl_epub.py out.epub [rtl]
import zipfile, struct, zlib, sys
def png(w,h,shade):
    raw=b''.join(b'\x00'+bytes([shade])*w for _ in range(h))
    def ch(t,d): return struct.pack('>I',len(d))+t+d+struct.pack('>I',zlib.crc32(t+d)&0xffffffff)
    return b'\x89PNG\r\n\x1a\n'+ch(b'IHDR',struct.pack('>IIBBBBB',w,h,8,0,0,0,0))+ch(b'IDAT',zlib.compress(raw))+ch(b'IEND',b'')
W,H=1264,1680
rtl = len(sys.argv)>2 and sys.argv[2]=='rtl'
out=sys.argv[1]
z=zipfile.ZipFile(out,'w')
z.writestr(zipfile.ZipInfo('mimetype'),'application/epub+zip',compress_type=zipfile.ZIP_STORED)
z.writestr('META-INF/container.xml','<?xml version="1.0"?><container version="1.0" xmlns="urn:oasis:names:tc:opendocument:xmlns:container"><rootfiles><rootfile full-path="OEBPS/content.opf" media-type="application/oebps-package+xml"/></rootfiles></container>')
items=[];spine=[]
for i in range(3):
    z.writestr(f'OEBPS/Images/p{i}.png',png(W,H,60+80*i))
    z.writestr(f'OEBPS/Text/p{i}.xhtml',f'<?xml version="1.0" encoding="UTF-8"?><!DOCTYPE html><html xmlns="http://www.w3.org/1999/xhtml" xmlns:epub="http://www.idpf.org/2007/ops"><head><title>p{i}</title><meta name="viewport" content="width={W}, height={H}"/><style>body{{margin:0}}img{{position:absolute;top:0;left:0;width:{W}px;height:{H}px}}</style></head><body><div><img src="../Images/p{i}.png" alt=""/></div></body></html>')
    items.append(f'<item id="p{i}" href="Text/p{i}.xhtml" media-type="application/xhtml+xml"/><item id="i{i}" href="Images/p{i}.png" media-type="image/png"{" properties=\"cover-image\"" if i==0 else ""}/>')
    spine.append(f'<itemref idref="p{i}"/>')
z.writestr('OEBPS/nav.xhtml','<?xml version="1.0" encoding="UTF-8"?><!DOCTYPE html><html xmlns="http://www.w3.org/1999/xhtml" xmlns:epub="http://www.idpf.org/2007/ops"><head><title>nav</title></head><body><nav epub:type="toc"><ol><li><a href="Text/p0.xhtml">Start</a></li></ol></nav></body></html>')
ppd=' page-progression-direction="rtl"' if rtl else ''
z.writestr('OEBPS/content.opf',f'''<?xml version="1.0" encoding="UTF-8"?><package xmlns="http://www.idpf.org/2007/opf" version="3.0" unique-identifier="uid"><metadata xmlns:dc="http://purl.org/dc/elements/1.1/"><dc:identifier id="uid">urn:uuid:12345678-1234-1234-1234-1234567890ab</dc:identifier><dc:title>FXL Probe</dc:title><dc:language>en</dc:language><dc:creator>Deckle</dc:creator><meta property="dcterms:modified">2026-11-01T00:00:00Z</meta><meta property="rendition:layout">pre-paginated</meta><meta property="rendition:spread">landscape</meta><meta name="cover" content="i0"/><meta name="book-type" content="comic"/><meta name="original-resolution" content="{W}x{H}"/><meta name="zero-gutter" content="true"/><meta name="zero-margin" content="true"/><meta name="fixed-layout" content="true"/><meta name="orientation-lock" content="portrait"/>{'<meta name="primary-writing-mode" content="horizontal-rl"/>' if rtl else ''}</metadata><manifest><item id="nav" href="nav.xhtml" media-type="application/xhtml+xml" properties="nav"/>{"".join(items)}</manifest><spine{ppd}>{"".join(spine)}</spine></package>''')
z.close()
