import sys, logging
sys.path.insert(0, sys.argv[1])
from kfxlib import YJ_Book, set_logger
logging.basicConfig(level=logging.WARNING); set_logger(logging.getLogger("kfxlib"))
from collections import Counter
b = YJ_Book(sys.argv[2])
b.decode_book(pure=True, skip_book_checks=True)
c = Counter(f.ftype for f in b.fragments)
print("TOTAL", len(b.fragments))
for k,v in sorted(c.items()): print(k, v)
print("FIDS", sorted(str(f.fid) for f in b.fragments if f.fid)[:60])
print("local_min_id", b.symtab.local_min_id, "nsymbols", len(b.symtab.symbols))
