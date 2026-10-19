import sys, struct, logging
sys.path.insert(0, sys.argv[1])
from kfxlib import YJ_Book, set_logger
from kfxlib.ion_text import IonText  # noqa
logging.basicConfig(level=logging.WARNING); set_logger(logging.getLogger("kfxlib"))
data = open(sys.argv[2],"rb").read()
sig, ver, hlen, cio, cil = struct.unpack("<4sHLLL", data[:18])
print("sig", sig, "ver", ver, "header_len", hlen, "cinfo_off", cio, "cinfo_len", cil, "total", len(data))
b = YJ_Book(sys.argv[2]); b.decode_book(pure=True, skip_book_checks=True)
print("entities", len(b.fragments))
for f in b.fragments:
    if f.ftype in ("$270","$419","$593","$ion_symbol_table","$490","$258"):
        s = repr(f.value); print(f.ftype, f.fid, s[:600])
print("ORDER", [ (f.ftype, str(f.fid)[:20]) for f in b.fragments][:40])
