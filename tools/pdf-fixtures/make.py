#!/usr/bin/env python3
# SPDX-License-Identifier: GPL-3.0-or-later
"""Writes the PDF fixtures in fixtures/pdf/ (PDF spec §7). Every line is placed by hand, so the
tests know exactly where columns, running heads, broken words and tables are.

    python3 tools/pdf-fixtures/make.py   # needs reportlab
"""
import io
import os
import sys

from reportlab.lib.pagesizes import letter
from reportlab.pdfgen import canvas

OUT = os.path.join(os.path.dirname(__file__), "..", "..", "fixtures", "pdf")
W, H = letter


def doc(name, title=None, author=None):
    c = canvas.Canvas(os.path.join(OUT, name), pagesize=letter, invariant=1)
    if title:
        c.setTitle(title)
    if author:
        c.setAuthor(author)
    return c


def lines(c, x, y, rows, font="Times-Roman", size=10, step=12):
    c.setFont(font, size)
    for r in rows:
        c.drawString(x, y, r)
        y -= step
    return y


def running(c, n):
    c.setFont("Helvetica", 8)
    c.drawString(72, H - 40, "Proceedings of the Deckle Test Society")
    c.drawRightString(W - 72, H - 40, "Vol. 3")
    c.drawCentredString(W / 2, 40, f"Page {n}")


def two_column():
    c = doc("two-column.pdf", title="On Reading Order", author="A. Tester")
    left1 = [
        "Alpha begins the left column with a",
        "sentence that runs over several lines",
        "and carries the age of informa-",
        "tion into the next line, where a",
        "well-known phrase breaks as well-",
        "known across the line end. Left one",
        "ends here.",
    ]
    right1 = [
        "Bravo opens the right column, which",
        "a reader reaches only after the whole",
        "left column. It keeps going for a few",
        "lines so the gutter is plain to see",
        "between the two columns of the page",
        "and ends the first page right here.",
    ]
    running(c, 1)
    c.setFont("Times-Bold", 20)
    c.drawCentredString(W / 2, H - 90, "On Reading Order in Two Columns")
    lines(c, 72, H - 130, left1)
    lines(c, 320, H - 130, right1)
    c.showPage()
    left2 = [
        "Charlie starts the second page in the",
        "left column, again with enough text to",
        "fill a few lines and make the columns",
        "obvious to anyone who looks closely.",
    ]
    right2 = [
        "Delta closes the paper in the right",
        "column of the second page, after all",
        "of the left column has been read and",
        "nothing is left to say but goodbye.",
    ]
    running(c, 2)
    lines(c, 72, H - 100, left2)
    lines(c, 320, H - 100, right2)
    c.showPage()
    c.save()


def tables_and_pictures():
    c = doc("table.pdf", title="Tables and Pictures")
    y = lines(c, 72, H - 80, [
        "Echo introduces a table of fruit, set without rules, and then the text",
        "carries on below it.",
    ])
    rows = [("Fruit", "Count", "Price"), ("Apple", "3", "1.20"), ("Pear", "10", "0.80"), ("Plum", "7", "2.10")]
    y -= 10
    c.setFont("Helvetica", 10)
    for r in rows:
        for j, cell in enumerate(r):
            c.drawString(90 + j * 130, y, cell)
        y -= 14
    y = lines(c, 72, y - 14, ["Foxtrot follows the table. Next comes an equation, set on its own line:"])
    c.setFont("Symbol", 12)
    c.drawString(250, y - 6, "a + b = g")
    y = lines(c, 72, y - 30, ["Golf comes after the equation, and then a picture of a square:"])
    # A small grey picture.
    from reportlab.lib.utils import ImageReader
    import struct, zlib

    def png(w, h, grey):
        raw = b"".join(b"\x00" + bytes([grey]) * w for _ in range(h))
        def chunk(t, d):
            return struct.pack(">I", len(d)) + t + d + struct.pack(">I", zlib.crc32(t + d) & 0xFFFFFFFF)
        return b"\x89PNG\r\n\x1a\n" + chunk(b"IHDR", struct.pack(">IIBBBBB", w, h, 8, 0, 0, 0, 0)) + chunk(b"IDAT", zlib.compress(raw)) + chunk(b"IEND", b"")

    c.drawImage(ImageReader(io.BytesIO(png(60, 60, 128))), 250, y - 110, width=100, height=100)
    lines(c, 72, y - 140, ["Hotel ends the page after the picture."])
    c.showPage()
    c.save()


def book():
    c = doc("book.pdf", title="A Small Book", author="B. Writer")
    for n, (head, body) in enumerate([
        ("Chapter One", [
            "India begins the first chapter. It is short, but it is a chapter.",
            "The second line ends the first paragraph.",
            "",
            "Kilo starts a second paragraph, long enough that the chapter holds",
            "more than a heading and a line or two of text before the next one.",
        ]),
        ("Chapter Two", [
            "Juliet begins the second chapter, which is just as short as the first",
            "one was, and carries on for a while so that it reads like a chapter",
            "and not like a label someone left on a figure. And it ends here.",
        ]),
    ], start=1):
        c.setFont("Times-Bold", 22)
        c.drawString(72, H - 100, head)
        lines(c, 72, H - 140, body)
        c.setFont("Times-Roman", 9)
        c.drawCentredString(W / 2, 40, str(n))
        c.showPage()
    c.save()


def scan():
    c = doc("scan.pdf")
    import struct, zlib
    raw = b"".join(b"\x00" + b"\xf0" * 200 for _ in range(260))
    def chunk(t, d):
        return struct.pack(">I", len(d)) + t + d + struct.pack(">I", zlib.crc32(t + d) & 0xFFFFFFFF)
    data = b"\x89PNG\r\n\x1a\n" + chunk(b"IHDR", struct.pack(">IIBBBBB", 200, 260, 8, 0, 0, 0, 0)) + chunk(b"IDAT", zlib.compress(raw)) + chunk(b"IEND", b"")
    from reportlab.lib.utils import ImageReader
    c.drawImage(ImageReader(io.BytesIO(data)), 0, 0, width=W, height=H)
    c.showPage()
    c.save()


if __name__ == "__main__":
    os.makedirs(OUT, exist_ok=True)
    two_column()
    tables_and_pictures()
    book()
    scan()
    print("wrote", ", ".join(sorted(os.listdir(OUT))), file=sys.stderr)
