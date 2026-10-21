"""Writes fixtures/docx/deckle-mini.docx: headings, inline styles, lists, a table, two images
and footnotes, built from raw WordprocessingML with the standard library only."""
import zipfile, sys, struct, zlib
out = sys.argv[1]
def png(w,h,rgb):
    raw=b"".join(b"\x00"+bytes(rgb)*w for _ in range(h))
    def chunk(t,d): return struct.pack(">I",len(d))+t+d+struct.pack(">I",zlib.crc32(t+d)&0xffffffff)
    return b"\x89PNG\r\n\x1a\n"+chunk(b"IHDR",struct.pack(">IIBBBBB",w,h,8,2,0,0,0))+chunk(b"IDAT",zlib.compress(raw))+chunk(b"IEND",b"")
W='xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main" xmlns:r="http://schemas.openxmlformats.org/officeDocument/2006/relationships" xmlns:wp="http://schemas.openxmlformats.org/drawingml/2006/wordprocessingDrawing" xmlns:a="http://schemas.openxmlformats.org/drawingml/2006/main" xmlns:pic="http://schemas.openxmlformats.org/drawingml/2006/picture"'
def p(text, style=None, num=None):
    ppr=""
    if style or num:
        ppr="<w:pPr>"+(f'<w:pStyle w:val="{style}"/>' if style else "")+(f'<w:numPr><w:ilvl w:val="{num[1]}"/><w:numId w:val="{num[0]}"/></w:numPr>' if num else "")+"</w:pPr>"
    return f"<w:p>{ppr}{text}</w:p>"
def r(t, rpr=""): return f'<w:r>{"<w:rPr>"+rpr+"</w:rPr>" if rpr else ""}<w:t xml:space="preserve">{t}</w:t></w:r>'
def fn(i): return f'<w:r><w:rPr><w:rStyle w:val="FootnoteReference"/></w:rPr><w:footnoteReference w:id="{i}"/></w:r>'
def img(rid, n, alt): return f'<w:r><w:drawing><wp:inline><wp:extent cx="914400" cy="914400"/><wp:docPr id="{n}" name="Picture {n}" descr="{alt}"/><a:graphic><a:graphicData uri="http://schemas.openxmlformats.org/drawingml/2006/picture"><pic:pic><pic:blipFill><a:blip r:embed="{rid}"/></pic:blipFill></pic:pic></a:graphicData></a:graphic></wp:inline></w:drawing></w:r>'
def cell(t): return f"<w:tc>{p(r(t))}</w:tc>"
lorem="The press was cold that morning, and the type sat in its cases like rain waiting to fall. "
body="".join([
    p(r("Deckle Mini Document"),"Title"),
    p(r("Chapter One"),"Heading1"),
    p(r(lorem*3)+r("Bold words","<w:b/>")+r(", ")+r("italic words","<w:i/>")+r(" and ")+r("both","<w:b/><w:i/>")+r(" in one line.")+fn(1)),
    p(r("A bullet item"),"ListParagraph",(1,0)),
    p(r("A nested bullet"),"ListParagraph",(1,1)),
    p(r("Another bullet"),"ListParagraph",(1,0)),
    p(r("First step"),"ListParagraph",(2,0)),
    p(r("Second step"),"ListParagraph",(2,0)),
    p(r("The Section"),"Heading2"),
    p(img("rIdImg1",1,"A blue square")),
    p(r(lorem*2)+fn(2)),
    "<w:tbl><w:tblPr/><w:tblGrid><w:gridCol/><w:gridCol/></w:tblGrid>"
    f'<w:tr><w:trPr><w:tblHeader/></w:trPr>{cell("Name")}{cell("Weight")}</w:tr>'
    f'<w:tr>{cell("Caslon")}{cell("Regular")}</w:tr></w:tbl>',
    p(r("A Subsection"),"Heading3"),
    p(r(lorem*2)),
    p(r("Chapter Two"),"Heading1"),
    p(r(lorem*3)+fn(3)),
    p(img("rIdImg2",2,"A red square")),
    p(r("Deeper, not in the contents"),"Heading4"),
    p(r(lorem)),
])
doc=f'<?xml version="1.0" encoding="UTF-8" standalone="yes"?><w:document {W}><w:body>{body}<w:sectPr/></w:body></w:document>'
styles=f'''<?xml version="1.0" encoding="UTF-8" standalone="yes"?><w:styles {W}>
<w:docDefaults><w:rPrDefault><w:rPr><w:lang w:val="en-US"/></w:rPr></w:rPrDefault></w:docDefaults>
<w:style w:type="paragraph" w:default="1" w:styleId="Normal"><w:name w:val="Normal"/></w:style>
<w:style w:type="paragraph" w:styleId="Title"><w:name w:val="Title"/><w:basedOn w:val="Normal"/></w:style>
<w:style w:type="paragraph" w:styleId="Heading1"><w:name w:val="heading 1"/><w:basedOn w:val="Normal"/><w:pPr><w:outlineLvl w:val="0"/></w:pPr><w:rPr><w:b/></w:rPr></w:style>
<w:style w:type="paragraph" w:styleId="Heading2"><w:name w:val="heading 2"/><w:basedOn w:val="Normal"/><w:pPr><w:outlineLvl w:val="1"/></w:pPr></w:style>
<w:style w:type="paragraph" w:styleId="Heading3"><w:name w:val="heading 3"/><w:basedOn w:val="Normal"/><w:pPr><w:outlineLvl w:val="2"/></w:pPr></w:style>
<w:style w:type="paragraph" w:styleId="Heading4"><w:name w:val="heading 4"/><w:basedOn w:val="Normal"/><w:pPr><w:outlineLvl w:val="3"/></w:pPr></w:style>
<w:style w:type="paragraph" w:styleId="ListParagraph"><w:name w:val="List Paragraph"/><w:basedOn w:val="Normal"/></w:style>
<w:style w:type="character" w:styleId="FootnoteReference"><w:name w:val="footnote reference"/><w:rPr><w:vertAlign w:val="superscript"/></w:rPr></w:style>
</w:styles>'''
numbering=f'''<?xml version="1.0" encoding="UTF-8" standalone="yes"?><w:numbering {W}>
<w:abstractNum w:abstractNumId="0"><w:lvl w:ilvl="0"><w:numFmt w:val="bullet"/></w:lvl><w:lvl w:ilvl="1"><w:numFmt w:val="bullet"/></w:lvl></w:abstractNum>
<w:abstractNum w:abstractNumId="1"><w:lvl w:ilvl="0"><w:start w:val="1"/><w:numFmt w:val="decimal"/></w:lvl></w:abstractNum>
<w:num w:numId="1"><w:abstractNumId w:val="0"/></w:num><w:num w:numId="2"><w:abstractNumId w:val="1"/></w:num>
</w:numbering>'''
def note(i,runs): return f'<w:footnote w:id="{i}"><w:p><w:r><w:rPr><w:rStyle w:val="FootnoteReference"/></w:rPr><w:footnoteRef/></w:r>{runs}</w:p></w:footnote>'
footnotes=f'<?xml version="1.0" encoding="UTF-8" standalone="yes"?><w:footnotes {W}><w:footnote w:type="separator" w:id="-1"><w:p><w:r><w:separator/></w:r></w:p></w:footnote><w:footnote w:type="continuationSeparator" w:id="0"><w:p><w:r><w:continuationSeparator/></w:r></w:p></w:footnote>{note(1,r(" The first note."))}{note(2,r(" A note with ")+r("emphasis","<w:i/>")+r("."))}{note(3,r(" The last note."))}</w:footnotes>'
R="http://schemas.openxmlformats.org/officeDocument/2006/relationships"
rels=f'''<?xml version="1.0" encoding="UTF-8" standalone="yes"?><Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships">
<Relationship Id="rIdStyles" Type="{R}/styles" Target="styles.xml"/>
<Relationship Id="rIdNum" Type="{R}/numbering" Target="numbering.xml"/>
<Relationship Id="rIdFn" Type="{R}/footnotes" Target="footnotes.xml"/>
<Relationship Id="rIdImg1" Type="{R}/image" Target="media/image1.png"/>
<Relationship Id="rIdImg2" Type="{R}/image" Target="media/image2.png"/>
</Relationships>'''
z=zipfile.ZipFile(out,"w",zipfile.ZIP_DEFLATED)
z.writestr("[Content_Types].xml",'<?xml version="1.0" encoding="UTF-8" standalone="yes"?><Types xmlns="http://schemas.openxmlformats.org/package/2006/content-types"><Default Extension="rels" ContentType="application/vnd.openxmlformats-package.relationships+xml"/><Default Extension="xml" ContentType="application/xml"/><Default Extension="png" ContentType="image/png"/><Override PartName="/word/document.xml" ContentType="application/vnd.openxmlformats-officedocument.wordprocessingml.document.main+xml"/></Types>')
z.writestr("_rels/.rels",f'<?xml version="1.0" encoding="UTF-8" standalone="yes"?><Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="rId1" Type="{R}/officeDocument" Target="word/document.xml"/><Relationship Id="rId2" Type="http://schemas.openxmlformats.org/package/2006/relationships/metadata/core-properties" Target="docProps/core.xml"/></Relationships>')
z.writestr("docProps/core.xml",'<?xml version="1.0" encoding="UTF-8" standalone="yes"?><cp:coreProperties xmlns:cp="http://schemas.openxmlformats.org/package/2006/metadata/core-properties" xmlns:dc="http://purl.org/dc/elements/1.1/" xmlns:dcterms="http://purl.org/dc/terms/" xmlns:xsi="http://www.w3.org/2001/XMLSchema-instance"><dc:title>Deckle Mini Document</dc:title><dc:creator>Jane Testwright</dc:creator><dcterms:modified xsi:type="dcterms:W3CDTF">2026-10-21T00:00:00Z</dcterms:modified></cp:coreProperties>')
z.writestr("word/document.xml",doc)
z.writestr("word/styles.xml",styles)
z.writestr("word/numbering.xml",numbering)
z.writestr("word/footnotes.xml",footnotes)
z.writestr("word/_rels/document.xml.rels",rels)
z.writestr("word/media/image1.png",png(64,64,(40,80,160)))
z.writestr("word/media/image2.png",png(64,64,(160,40,40)))
z.close(); print("wrote",out)
