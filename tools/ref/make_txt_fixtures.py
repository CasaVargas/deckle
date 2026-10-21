"""Writes the plain text fixtures into OUT_DIR (fixtures/txt): a Project Gutenberg style book,
a one-paragraph-per-line Windows-1252 file, verse, UTF-16 LE with a BOM, and a ~500 KB file with
no headings. Standard library only; the output is deterministic."""
import os, sys, textwrap
out = sys.argv[1]
os.makedirs(out, exist_ok=True)

def wrap(text): return textwrap.fill(text, width=70)
press = ("The press was cold that morning, and the type sat in its cases like rain waiting to "
         "fall. Nobody in the shop said a word about the proofs, which were late again, and the "
         "foreman walked the length of the room twice before he sat down. ")
ink = ("Ink is patient. It waits in the tin for weeks and then, on the one day that matters, it "
       "goes on thick and dark and even, and the sheet comes off the platen looking as if it had "
       "always been printed. ")

licence = """The Project Gutenberg eBook of The Deckle Mini Novel

This ebook is for the use of anyone anywhere in the United States and
most other parts of the world at no cost and with almost no restrictions
whatsoever. LICENCE-HEADER-SENTINEL

Title: The Deckle Mini Novel

Author: Ada Testwright

Release date: October 21, 2026 [eBook #99999]

Language: English

Credits: Deckle fixtures

*** START OF THE PROJECT GUTENBERG EBOOK THE DECKLE MINI NOVEL ***
"""
footer = """
*** END OF THE PROJECT GUTENBERG EBOOK THE DECKLE MINI NOVEL ***

Updated editions will replace the previous one--the old editions will
be renamed. LICENCE-FOOTER-SENTINEL
"""
gutenberg = "\n\n".join([
    "THE DECKLE MINI NOVEL",
    "by Ada Testwright",
    "PART ONE",
    "CHAPTER I.",
    wrap(press * 3),
    # A hyphen at a line end between two lowercase letters is kept: well-known.
    "The foreman had a well-\nknown habit of reading every proof twice, once for the\n"
    "words and once for the spaces between them, and he did it now.",
    wrap(ink * 2),
    "Chapter II — The Ink",
    wrap(ink * 3),
    "* * *",
    wrap(press * 2),
    "PART TWO",
    "III.",
    wrap(press * 2),
    # A short-line block inside a hard-wrapped book keeps its breaks.
    "Yours, in haste,\nA. T.",
    "THE LAST PROOF",
    "Chapter 4",
    wrap(ink * 2),
])
with open(os.path.join(out, "gutenberg-mini.txt"), "w", encoding="utf-8", newline="\r\n") as f:
    f.write(licence + "\n\n\n" + gutenberg + "\n\n\n" + footer)

unwrapped = "\n".join([
    "“It’s late,” said the foreman, and he looked at the clock over the door as if it had done something to him personally.",
    press.strip(),
    ink.strip(),
    "“Then we print it tomorrow,” she said. “The words will keep. They always do, whatever the paper thinks.”",
]) + "\n"
with open(os.path.join(out, "unwrapped.txt"), "wb") as f:
    f.write(unwrapped.encode("cp1252"))

verse = """The press is cold, the type is set,
the ink has not been opened yet;
the foreman waits beside the door
and counts the proofs upon the floor.

A sheet goes in, a sheet comes out,
the margins wide, the letters stout,
and every line that leaves the bed
is something somebody once said.
"""
with open(os.path.join(out, "verse.txt"), "w", encoding="utf-8") as f:
    f.write(verse)

utf16 = "Café Notes\n\nThe naïve compositor set “façade” with a cedilla.\n"
with open(os.path.join(out, "utf16.txt"), "wb") as f:
    f.write(b"\xff\xfe" + utf16.encode("utf-16-le"))

paras = []
size = 0
i = 0
while size < 500_000:
    p = wrap(f"Paragraph {i}. " + (press if i % 2 else ink) * 3)
    paras.append(p)
    size += len(p) + 2
    i += 1
with open(os.path.join(out, "noheadings.txt"), "w", encoding="utf-8") as f:
    f.write("\n\n".join(paras) + "\n")
